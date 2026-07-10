//! OCR support for scanned documents and images.
//!
//! Uses `PDFium` (Apache 2.0, same engine as Chrome) to render PDF pages to
//! images, then `Tesseract` for OCR text extraction. This produces much better
//! results than parsing PDF internal text objects, especially for financial
//! statements with complex layouts.
//!
//! **Prerequisites for development**: `brew install tesseract`
//! **For distribution**: `Tesseract` + `PDFium` bundled inside the .app
//!
//! If `Tesseract` isn't installed, falls back to `pdf-extract`.

use std::path::Path;
use std::process::Command;

use indicatif::ProgressBar;

use crate::error::{Error, Result};

/// Find the tesseract binary — checks app bundle first, then system PATH.
pub fn tesseract_command() -> Command {
    // Check if bundled in app: exe → Contents/MacOS/binary, tessdata at Contents/Resources/
    if let Some(contents_dir) = std::env::current_exe()
        .ok()
        .as_ref()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    {
        let bundled_bin = contents_dir.join("Resources").join("bin").join("tesseract");
        if bundled_bin.exists() {
            let mut cmd = Command::new(&bundled_bin);
            let tessdata = contents_dir.join("Resources").join("tessdata");
            cmd.env("TESSDATA_PREFIX", &tessdata);
            let lib_dir = contents_dir.join("Resources").join("lib");
            cmd.env("DYLD_LIBRARY_PATH", &lib_dir);
            return cmd;
        }
    }

    // Fallback: system tesseract
    Command::new("tesseract")
}

/// Get the path to the bundled tessdata directory, if available.
/// Check if tesseract is available on the system.
pub fn is_tesseract_available() -> bool {
    tesseract_command()
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Check if the OCR pipeline is available (tesseract + `PDFium`).
///
/// `PDFium` is linked as a library (always available when compiled),
/// so we only need to check for tesseract.
pub fn is_ocr_pipeline_available() -> bool {
    is_tesseract_available()
}

/// Extract text from a PDF using OCR (`PDFium` rendering + `Tesseract`).
///
/// Process:
/// 1. Load PDF with `PDFium` and render each page to a PNG image (300 DPI)
/// 2. Run `Tesseract` OCR on each page image
/// 3. Concatenate results with page separators
///
/// # Errors
///
/// Returns an error if Tesseract is not installed or processing fails.
pub fn ocr_pdf_with_images(pdf_path: &Path, language: &str) -> Result<String> {
    if !is_tesseract_available() {
        return Err(Error::TesseractNotFound);
    }

    if !pdf_path.exists() {
        return Err(Error::Ocr {
            path: pdf_path.to_path_buf(),
            reason: "file does not exist".to_string(),
        });
    }

    // Create a temp directory for page images
    let temp_dir = tempfile::tempdir().map_err(|e| Error::Ocr {
        path: pdf_path.to_path_buf(),
        reason: format!("failed to create temp directory: {e}"),
    })?;

    // Step 1: Render PDF pages to PNG images using PDFium
    render_pdf_pages(pdf_path, temp_dir.path())?;

    // Step 2: Find all generated page images and sort them
    let mut page_images: Vec<_> = std::fs::read_dir(temp_dir.path())
        .map_err(|e| Error::Ocr {
            path: pdf_path.to_path_buf(),
            reason: format!("failed to read temp directory: {e}"),
        })?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("png") {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    page_images.sort();

    if page_images.is_empty() {
        return Err(Error::Ocr {
            path: pdf_path.to_path_buf(),
            reason: "PDFium produced no page images".to_string(),
        });
    }

    // Step 3: Run tesseract on each page image and concatenate
    let mut full_text = String::new();

    let pb = ProgressBar::with_draw_target(
        Some(page_images.len() as u64),
        indicatif::ProgressDrawTarget::stderr(),
    );
    pb.set_style(crate::utils::progress_style());
    pb.set_message("Running OCR");

    for (i, image_path) in page_images.iter().enumerate() {
        let page_text = ocr_image(image_path, language)?;
        if i > 0 {
            full_text.push_str("\n\n--- Page Break ---\n\n");
        }
        full_text.push_str(page_text.trim());
        pb.inc(1);
    }

    pb.finish_with_message("OCR complete");

    Ok(full_text)
}

/// Render each page of a PDF to a PNG image using `PDFium`.
///
/// Saves images as `page-001.png`, `page-002.png`, etc. in the output directory.
/// Renders at 300 DPI for good OCR quality.
///
/// # Errors
///
/// Returns an error if `PDFium` cannot load the PDF or render pages.
fn render_pdf_pages(pdf_path: &Path, output_dir: &Path) -> Result<()> {
    use pdfium_render::prelude::*;

    // Safety limit: prevent OOM/disk exhaustion on very large PDFs
    const MAX_PAGES: u64 = 500;

    let pdfium = crate::pdfium::load_pdfium(pdf_path)?;

    let document = pdfium
        .load_pdf_from_file(pdf_path, None)
        .map_err(|e| Error::Ocr {
            path: pdf_path.to_path_buf(),
            reason: format!("failed to load PDF with PDFium: {e}"),
        })?;

    let render_config = PdfRenderConfig::new()
        .set_target_width(2550) // 8.5" at 300 DPI
        .set_maximum_height(3300); // 11" at 300 DPI

    let page_count = u64::from(document.pages().len());

    if page_count > MAX_PAGES {
        return Err(Error::Ocr {
            path: pdf_path.to_path_buf(),
            reason: format!(
                "PDF has {page_count} pages (limit is {MAX_PAGES}). \
                 Split the document into smaller files before processing."
            ),
        });
    }

    let pb =
        ProgressBar::with_draw_target(Some(page_count), indicatif::ProgressDrawTarget::stderr());
    pb.set_style(crate::utils::progress_style());
    pb.set_message("Rendering pages");

    for (i, page) in document.pages().iter().enumerate() {
        let bitmap = page
            .render_with_config(&render_config)
            .map_err(|e| Error::Ocr {
                path: pdf_path.to_path_buf(),
                reason: format!("failed to render page {}: {e}", i + 1),
            })?;

        let image_path = output_dir.join(format!("page-{:03}.png", i + 1));

        bitmap
            .as_image()
            .as_rgba8()
            .ok_or_else(|| Error::Ocr {
                path: pdf_path.to_path_buf(),
                reason: format!("failed to convert page {} to image", i + 1),
            })?
            .save(&image_path)
            .map_err(|e| Error::Ocr {
                path: pdf_path.to_path_buf(),
                reason: format!("failed to save page {} image: {e}", i + 1),
            })?;

        pb.inc(1);
    }

    pb.finish_with_message("Pages rendered");

    Ok(())
}

/// Run OCR on an image file, returning the extracted text.
///
/// Invokes `tesseract <input> stdout -l <language>` and captures the output.
///
/// # Errors
///
/// Returns `Error::TesseractNotFound` if tesseract is not installed.
/// Returns `Error::Ocr` if OCR processing fails.
pub fn ocr_image(image_path: &Path, language: &str) -> Result<String> {
    if !is_tesseract_available() {
        return Err(Error::TesseractNotFound);
    }

    if !image_path.exists() {
        return Err(Error::Ocr {
            path: image_path.to_path_buf(),
            reason: "file does not exist".to_string(),
        });
    }

    let output = tesseract_command()
        .arg(image_path.as_os_str())
        .arg("stdout")
        .arg("-l")
        .arg(language)
        .output()
        .map_err(|e| Error::Ocr {
            path: image_path.to_path_buf(),
            reason: format!("failed to execute tesseract: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Ocr {
            path: image_path.to_path_buf(),
            reason: format!("tesseract exited with error: {stderr}"),
        });
    }

    String::from_utf8(output.stdout).map_err(|e| Error::Ocr {
        path: image_path.to_path_buf(),
        reason: format!("tesseract output is not valid UTF-8: {e}"),
    })
}

/// Minimum character threshold for text extraction from PDF.
/// If `pdf-extract` yields fewer characters than this, the PDF
/// is likely a scanned document and OCR should be attempted.
pub const SCANNED_PDF_THRESHOLD: usize = 50;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tesseract_available_returns_bool() {
        let _available = is_tesseract_available();
    }

    #[test]
    fn test_is_ocr_pipeline_available_returns_bool() {
        let _available = is_ocr_pipeline_available();
    }

    #[test]
    fn test_ocr_image_nonexistent_file() {
        let result = ocr_image(Path::new("/tmp/covername-nonexistent-image.png"), "eng");
        assert!(result.is_err());
    }

    #[test]
    fn test_ocr_pdf_with_images_nonexistent_file() {
        let result = ocr_pdf_with_images(Path::new("/tmp/covername-nonexistent-scan.pdf"), "eng");
        assert!(result.is_err());
    }

    #[test]
    #[ignore = "requires tesseract and PDFium library to be installed"]
    fn test_ocr_pipeline_available() {
        assert!(is_ocr_pipeline_available());
    }
}
