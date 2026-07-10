//! Output file handling for processed documents.
//!
//! Resolves output file paths from configured patterns and writes
//! the anonymized content to disk. Dispatches between text and PDF
//! output based on the input file type.

use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::document::{DocumentType, detect_file_type};
use crate::error::{Error, Result};
use crate::pdf_output;

/// Resolve the output file path from an input path and configuration.
///
/// Applies the `output_pattern` from the config, replacing `{name}` with
/// the input file's stem (filename without extension) and `{ext}` with
/// the input file's extension.
///
/// If `config.output_directory` is set, the output file is placed in that
/// directory. Otherwise, it goes in the same directory as the input file.
pub fn resolve_output_path(input_path: &Path, config: &Config) -> PathBuf {
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let ext = input_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("txt");

    let filename = config
        .output_pattern
        .replace("{name}", stem)
        .replace("{ext}", ext);

    if let Some(ref output_dir) = config.output_directory {
        output_dir.join(filename)
    } else {
        let parent = input_path.parent().unwrap_or_else(|| Path::new("."));
        parent.join(filename)
    }
}

/// Write text content to an output file.
///
/// Creates parent directories if they do not exist.
///
/// # Errors
///
/// Returns an error if the file or its parent directories cannot be created.
pub fn write_text_output(content: &str, output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    fs::write(output_path, content).map_err(|source| Error::Io {
        path: output_path.to_path_buf(),
        source,
    })?;

    Ok(())
}

/// Write output based on the input file type.
///
/// For PDF inputs, generates a new PDF with the replaced text content.
/// For XLSX inputs, this function writes plain text (use `xlsx::write_xlsx`
/// for proper XLSX output with cell-level replacements).
/// For all other inputs, writes plain text.
///
/// # Errors
///
/// Returns an error if the output file cannot be written.
pub fn write_output(content: &str, input_path: &Path, output_path: &Path) -> Result<()> {
    match detect_file_type(input_path) {
        DocumentType::Pdf => pdf_output::write_pdf(content, output_path),
        _ => write_text_output(content, output_path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_output_path_default_pattern() {
        let config = Config::default();
        let input = Path::new("/home/user/docs/report.txt");
        let output = resolve_output_path(input, &config);
        assert_eq!(output, PathBuf::from("/home/user/docs/report-covered.txt"));
    }

    #[test]
    fn test_resolve_output_path_custom_pattern() {
        let config = Config {
            output_pattern: String::from("{name}-anon.{ext}"),
            ..Config::default()
        };
        let input = Path::new("/tmp/data.txt");
        let output = resolve_output_path(input, &config);
        assert_eq!(output, PathBuf::from("/tmp/data-anon.txt"));
    }

    #[test]
    fn test_resolve_output_path_with_output_directory() {
        let config = Config {
            output_directory: Some(PathBuf::from("/tmp/output")),
            ..Config::default()
        };
        let input = Path::new("/home/user/docs/report.txt");
        let output = resolve_output_path(input, &config);
        assert_eq!(output, PathBuf::from("/tmp/output/report-covered.txt"));
    }

    #[test]
    fn test_resolve_output_path_no_extension() {
        let config = Config::default();
        let input = Path::new("/tmp/readme");
        let output = resolve_output_path(input, &config);
        assert_eq!(output, PathBuf::from("/tmp/readme-covered.txt"));
    }

    #[test]
    fn test_resolve_output_path_relative_path() {
        let config = Config::default();
        let input = Path::new("sample.txt");
        let output = resolve_output_path(input, &config);
        // Path::parent() for "sample.txt" returns "" which joins as just the filename
        assert_eq!(output, PathBuf::from("sample-covered.txt"));
    }

    #[test]
    fn test_resolve_output_path_pdf() {
        let config = Config::default();
        let input = Path::new("/tmp/document.pdf");
        let output = resolve_output_path(input, &config);
        assert_eq!(output, PathBuf::from("/tmp/document-covered.pdf"));
    }

    #[test]
    fn test_write_text_output_creates_file() {
        let dir = TempDir::new().unwrap();
        let output_path = dir.path().join("output.txt");

        write_text_output("Hello, anonymous world!", &output_path).unwrap();

        let content = fs::read_to_string(&output_path).unwrap();
        assert_eq!(content, "Hello, anonymous world!");
    }

    #[test]
    fn test_write_text_output_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let output_path = dir.path().join("sub").join("dir").join("output.txt");

        write_text_output("nested content", &output_path).unwrap();

        let content = fs::read_to_string(&output_path).unwrap();
        assert_eq!(content, "nested content");
    }

    #[test]
    fn test_write_output_dispatches_to_text() {
        let dir = TempDir::new().unwrap();
        let output_path = dir.path().join("output.txt");
        let input_path = Path::new("input.txt");

        write_output("text content", input_path, &output_path).unwrap();

        let content = fs::read_to_string(&output_path).unwrap();
        assert_eq!(content, "text content");
    }

    #[test]
    fn test_write_output_dispatches_to_pdf() {
        let dir = TempDir::new().unwrap();
        let output_path = dir.path().join("output.pdf");
        let input_path = Path::new("input.pdf");

        write_output("pdf content", input_path, &output_path).unwrap();

        assert!(output_path.exists());
        let bytes = fs::read(&output_path).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }
}
