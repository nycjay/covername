//! Document abstraction for text and PDF file processing.
//!
//! Provides wrappers around file paths and content for different document types,
//! used as the input to the detection and processing pipeline.

use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::{Error, Result};

/// The type of a document, determined by file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentType {
    /// Plain text file (.txt, .md, .csv, etc.)
    Text,
    /// PDF document (.pdf)
    Pdf,
    /// Excel spreadsheet (.xlsx) — not yet implemented
    Xlsx,
    /// Image file (.png, .jpg, etc.) — not yet implemented
    Image,
}

/// Detect the document type from a file's extension.
///
/// Defaults to `Text` for unknown extensions.
pub fn detect_file_type(path: &Path) -> DocumentType {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("pdf") => DocumentType::Pdf,
        Some("xlsx" | "xls") => DocumentType::Xlsx,
        Some("png" | "jpg" | "jpeg" | "tiff" | "tif" | "bmp") => DocumentType::Image,
        _ => DocumentType::Text,
    }
}

/// File extensions supported for batch processing.
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "txt", "md", "csv", "pdf", "xlsx", "xls", "png", "jpg", "jpeg", "tiff", "tif",
];

/// Check whether a file has a supported extension for processing.
fn is_supported_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .is_some_and(|ext| SUPPORTED_EXTENSIONS.contains(&ext.as_str()))
}

/// Check whether an extension string is supported for processing.
pub fn is_supported_extension(ext: &str) -> bool {
    SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str())
}

/// Collect files from a path for batch processing.
///
/// - If `path` is a file, returns it in a single-element vector.
/// - If `path` is a directory, collects all supported files (`.txt`, `.pdf`).
/// - If `recursive` is true, walks subdirectories as well.
/// - Returns a sorted list of file paths.
///
/// # Errors
///
/// Returns an error if the path does not exist or the directory cannot be read.
pub fn collect_files(path: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    if !path.exists() {
        return Err(Error::Io {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "path does not exist"),
        });
    }

    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let max_depth = if recursive { usize::MAX } else { 1 };

    let mut files: Vec<PathBuf> = WalkDir::new(path)
        .max_depth(max_depth)
        .into_iter()
        .filter_map(|entry| {
            entry.ok().and_then(|e| {
                let p = e.into_path();
                if p.is_file() && is_supported_file(&p) {
                    Some(p)
                } else {
                    None
                }
            })
        })
        .collect();

    files.sort();
    Ok(files)
}

/// A text document loaded from a file.
///
/// Holds the file path and its full content as a string.
#[derive(Debug, Clone)]
pub struct TextDocument {
    /// Path to the source file.
    path: PathBuf,
    /// Full text content of the file.
    content: String,
}

impl TextDocument {
    /// Load a text document from a file path.
    ///
    /// Reads the entire file into memory as a UTF-8 string.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read (e.g., not found,
    /// permission denied, or invalid UTF-8).
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;

        Ok(Self {
            path: path.to_path_buf(),
            content,
        })
    }

    /// Returns the text content of the document.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Returns the path to the source file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// A PDF document loaded from a file.
///
/// Holds the file path and raw bytes. Text extraction is performed
/// on demand using `pdf_extract`.
#[derive(Debug, Clone)]
pub struct PdfDocument {
    /// Path to the source PDF file.
    path: PathBuf,
    /// Raw file bytes (needed for `pdf_extract`).
    bytes: Vec<u8>,
}

impl PdfDocument {
    /// Load a PDF document from a file path.
    ///
    /// Reads the entire file into memory as raw bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    pub fn from_file(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;

        Ok(Self {
            path: path.to_path_buf(),
            bytes,
        })
    }

    /// Extract all text content from the PDF.
    ///
    /// Uses `pdf_extract::extract_text_from_mem` to pull text from all pages.
    /// Post-processes to fix character duplication artifacts from overlapping
    /// text runs in the PDF.
    ///
    /// # Errors
    ///
    /// Returns an error if the PDF cannot be parsed or text extraction fails.
    pub fn extract_text(&self) -> Result<String> {
        let raw =
            pdf_extract::extract_text_from_mem(&self.bytes).map_err(|e| Error::PdfExtract {
                path: self.path.clone(),
                reason: e.to_string(),
            })?;
        Ok(clean_extracted_text(&raw))
    }

    /// Returns the path to the source PDF file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Fix PDF text extraction artifact where overlapping text runs cause
/// single-character duplication: "V Vanguard" → "Vanguard", "S SIPC" → "SIPC".
///
/// The pattern is: a single uppercase letter or digit at a word boundary, followed
/// by a space, followed by a word that starts with that same character (case-sensitive).
/// We remove the duplicated leading character and space to produce clean text.
///
/// Only applies to uppercase letters and digits to avoid false positives with
/// common lowercase words like "a and" or "I in".
/// Clean extracted text by removing character duplication artifacts.
///
/// PDF text extraction and OCR sometimes produce overlapping text runs
/// where the first character of a word is duplicated (e.g., "V Vanguard" → "Vanguard").
/// Also handles the pattern with a newline separator (e.g., "J\nJASON" → "\nJASON").
pub fn clean_extracted_text(text: &str) -> String {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(len);
    let mut i = 0;

    while i < len {
        // Check for pattern: single uppercase/digit char + space + same char starting a word
        // The single char must be at a word boundary (start of string or preceded by whitespace)
        if i + 2 < len
            && (bytes[i].is_ascii_uppercase() || bytes[i].is_ascii_digit())
            && bytes[i + 1] == b' '
            && bytes[i + 2] == bytes[i]
            && (i + 3 >= len || bytes[i + 3].is_ascii_alphanumeric())
            && (i == 0 || bytes[i - 1].is_ascii_whitespace())
        {
            // Skip the duplicated char and space, the real word follows
            i += 2;
        }
        // Also handle: single char + newline + same char (e.g., "J\nJASON")
        else if i + 2 < len
            && (bytes[i].is_ascii_uppercase() || bytes[i].is_ascii_digit())
            && bytes[i + 1] == b'\n'
            && bytes[i + 2] == bytes[i]
            && (i + 3 >= len || bytes[i + 3].is_ascii_alphanumeric())
            && (i == 0 || bytes[i - 1] == b'\n' || bytes[i - 1].is_ascii_whitespace())
        {
            // Skip the duplicated char, keep the newline
            i += 1;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn test_from_file_reads_content() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "Hello, world!").unwrap();

        let doc = TextDocument::from_file(file.path()).unwrap();
        assert_eq!(doc.content(), "Hello, world!");
        assert_eq!(doc.path(), file.path());
    }

    #[test]
    fn test_from_file_nonexistent_returns_error() {
        let result = TextDocument::from_file(Path::new("/tmp/covername-nonexistent-file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_file_type_txt() {
        assert_eq!(
            detect_file_type(Path::new("report.txt")),
            DocumentType::Text
        );
    }

    #[test]
    fn test_detect_file_type_pdf() {
        assert_eq!(
            detect_file_type(Path::new("document.pdf")),
            DocumentType::Pdf
        );
    }

    #[test]
    fn test_detect_file_type_pdf_uppercase() {
        assert_eq!(
            detect_file_type(Path::new("document.PDF")),
            DocumentType::Pdf
        );
    }

    #[test]
    fn test_detect_file_type_xlsx() {
        assert_eq!(
            detect_file_type(Path::new("spreadsheet.xlsx")),
            DocumentType::Xlsx
        );
    }

    #[test]
    fn test_detect_file_type_image() {
        assert_eq!(
            detect_file_type(Path::new("photo.png")),
            DocumentType::Image
        );
        assert_eq!(detect_file_type(Path::new("pic.jpg")), DocumentType::Image);
        assert_eq!(
            detect_file_type(Path::new("scan.jpeg")),
            DocumentType::Image
        );
    }

    #[test]
    fn test_detect_file_type_unknown_defaults_to_text() {
        assert_eq!(detect_file_type(Path::new("readme.md")), DocumentType::Text);
        assert_eq!(detect_file_type(Path::new("data.csv")), DocumentType::Text);
        assert_eq!(detect_file_type(Path::new("no_ext")), DocumentType::Text);
    }

    #[test]
    fn test_collect_files_single_file() {
        let mut file = NamedTempFile::with_suffix(".txt").unwrap();
        write!(file, "test").unwrap();

        let files = collect_files(file.path(), false).unwrap();
        assert_eq!(files, vec![file.path().to_path_buf()]);
    }

    #[test]
    fn test_collect_files_directory_non_recursive() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "text").unwrap();
        fs::write(dir.path().join("b.pdf"), "pdf").unwrap();
        fs::write(dir.path().join("c.md"), "markdown").unwrap();
        fs::write(dir.path().join("d.jpg"), "image").unwrap();

        let subdir = dir.path().join("sub");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("e.txt"), "nested").unwrap();

        let files = collect_files(dir.path(), false).unwrap();
        assert_eq!(files.len(), 4);
        assert!(files.contains(&dir.path().join("a.txt")));
        assert!(files.contains(&dir.path().join("b.pdf")));
        assert!(files.contains(&dir.path().join("c.md")));
        assert!(files.contains(&dir.path().join("d.jpg")));
    }

    #[test]
    fn test_collect_files_directory_recursive() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "text").unwrap();

        let subdir = dir.path().join("sub");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("b.txt"), "nested").unwrap();
        fs::write(subdir.join("c.pdf"), "pdf").unwrap();
        fs::write(subdir.join("d.md"), "markdown").unwrap();

        let files = collect_files(dir.path(), true).unwrap();
        assert_eq!(files.len(), 4); // includes .md now
        assert!(files.contains(&dir.path().join("a.txt")));
        assert!(files.contains(&subdir.join("b.txt")));
        assert!(files.contains(&subdir.join("c.pdf")));
    }

    #[test]
    fn test_collect_files_sorted() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("z.txt"), "z").unwrap();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::write(dir.path().join("m.txt"), "m").unwrap();

        let files = collect_files(dir.path(), false).unwrap();
        let sorted: Vec<PathBuf> = {
            let mut f = files.clone();
            f.sort();
            f
        };
        assert_eq!(files, sorted);
    }

    #[test]
    fn test_collect_files_nonexistent_path() {
        let result = collect_files(Path::new("/tmp/covername-nonexistent-dir-12345"), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_collect_files_empty_directory() {
        let dir = TempDir::new().unwrap();
        let files = collect_files(dir.path(), false).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_clean_extracted_text_removes_duplicated_chars() {
        assert_eq!(clean_extracted_text("V Vanguard"), "Vanguard");
        assert_eq!(clean_extracted_text("S SIPC"), "SIPC");
        assert_eq!(clean_extracted_text("P Personal"), "Personal");
        assert_eq!(clean_extracted_text("8 877"), "877");
    }

    #[test]
    fn test_clean_extracted_text_preserves_normal_text() {
        assert_eq!(clean_extracted_text("a and b"), "a and b");
        assert_eq!(clean_extracted_text("I saw it"), "I saw it");
        assert_eq!(clean_extracted_text("Hello World"), "Hello World");
    }

    #[test]
    fn test_clean_extracted_text_multiple_artifacts() {
        assert_eq!(
            clean_extracted_text("V Vanguard B Brokerage S Services"),
            "Vanguard Brokerage Services"
        );
    }
}
