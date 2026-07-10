//! Unified processing pipeline.
//!
//! This module provides the single entry point for scanning documents.
//! Both the CLI and Tauri app call these functions, ensuring consistent
//! behavior regardless of the interface.

use std::path::Path;

use tracing::{debug, info};

use crate::config::Config;
use crate::detection::{Detection, RuleEngine};
use crate::document::{DocumentType, PdfDocument, TextDocument, detect_file_type};
use crate::error::Result;
use crate::ignore::IgnoreList;
use crate::ner::{DictionaryDetector, NerDetector};
use crate::processor;

/// Result of scanning a document for PII.
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// The extracted text content of the document.
    pub text: String,
    /// Detected PII items (after ignore list filtering).
    pub detections: Vec<Detection>,
}

/// Write a redacted PDF where only PII regions are modified.
///
/// Uses position-aware redaction: renders each page to an image,
/// finds PII word positions via OCR, paints over them, draws
/// replacement text, and assembles into a new PDF. The rest of
/// the document looks identical to the original.
///
/// # Errors
///
/// Returns an error if `PDFium`, `Tesseract`, or image processing fails.
pub fn write_redacted_pdf(
    input_path: &Path,
    replacements: &[(String, String)],
    output_path: &Path,
) -> Result<()> {
    write_redacted_pdf_with_progress(input_path, replacements, output_path, &|_, _| {})
}

/// Same as [`write_redacted_pdf`] but with a progress callback.
///
/// The callback receives `(current_page, total_pages)` after each page is processed.
///
/// # Errors
///
/// Returns an error if `PDFium`, `Tesseract`, or image processing fails.
pub fn write_redacted_pdf_with_progress(
    input_path: &Path,
    replacements: &[(String, String)],
    output_path: &Path,
    on_progress: &dyn Fn(u64, u64),
) -> Result<()> {
    let page_replacements: Vec<crate::redact::PageReplacement> = replacements
        .iter()
        .map(|(original, replacement)| crate::redact::PageReplacement {
            original: original.clone(),
            replacement: replacement.clone(),
        })
        .collect();

    crate::redact::redact_pdf_with_progress(
        input_path,
        &page_replacements,
        output_path,
        on_progress,
    )
}

/// Extract text from a file, using the best available method.
///
/// For PDFs: prefers OCR (`PDFium` + `Tesseract`) when available, falls back
/// to direct text extraction via `pdf-extract`.
///
/// # Errors
///
/// Returns an error if the file cannot be read or text extraction fails.
pub fn extract_text(file: &Path) -> Result<String> {
    info!(path = %file.display(), "extracting text from file");
    let file_type = detect_file_type(file);

    match file_type {
        DocumentType::Pdf => {
            // Prefer OCR pipeline (PDFium + Tesseract) for better accuracy
            if crate::ocr::is_ocr_pipeline_available() {
                match crate::ocr::ocr_pdf_with_images(file, "eng") {
                    Ok(ocr_text) if !ocr_text.trim().is_empty() => {
                        debug!(path = %file.display(), chars = ocr_text.len(), "OCR extraction complete");
                        return Ok(crate::document::clean_extracted_text(&ocr_text));
                    }
                    _ => {} // OCR failed or empty, fall through
                }
            }

            // Fallback: direct text extraction
            let doc = PdfDocument::from_file(file)?;
            let text = doc.extract_text()?;

            // If very little text, suggest OCR
            if text.trim().len() < crate::ocr::SCANNED_PDF_THRESHOLD
                && !crate::ocr::is_ocr_pipeline_available()
            {
                eprintln!(
                    "Note: {} may be a scanned document or has complex layout.",
                    file.display()
                );
                eprintln!("  For better results, install Tesseract: brew install tesseract");
            }

            Ok(text)
        }
        DocumentType::Xlsx => {
            let doc = crate::xlsx::XlsxDocument::from_file(file)?;
            doc.extract_text()
        }
        DocumentType::Image => {
            if crate::ocr::is_tesseract_available() {
                crate::ocr::ocr_image(file, "eng")
            } else {
                Err(crate::error::Error::TesseractNotFound)
            }
        }
        DocumentType::Text => {
            let doc = TextDocument::from_file(file)?;
            Ok(doc.content().to_string())
        }
    }
}

/// Run PII detection on text using all available detectors.
///
/// Combines regex rules, custom rules, and NER detection, then merges
/// overlapping detections. When the `onnx` feature is enabled and the
/// ONNX model is installed, uses the ONNX detector; otherwise falls back
/// to the dictionary-based detector.
///
/// # Errors
///
/// Returns an error if the rule engine cannot be initialized.
pub fn detect_pii(text: &str, storage_dir: &Path) -> Result<Vec<Detection>> {
    debug!(text_len = text.len(), "running PII detection");
    let mut engine = RuleEngine::new()?;

    // Load custom rules if available
    let rules_path = storage_dir.join("custom-rules.json");
    if let Err(e) = engine.load_custom_rules(&rules_path) {
        tracing::warn!(path = %rules_path.display(), error = %e, "failed to load custom rules");
    }

    // Run regex detection
    let mut detections = engine.scan(text);

    // Run NER detection — prefer ONNX if available, fall back to dictionary
    let manager = crate::ner::ModelManager::new(storage_dir);

    #[cfg(feature = "onnx")]
    {
        if manager.is_onnx_installed() {
            match crate::ner::OnnxDetector::load(&manager.model_dir()) {
                Ok(detector) => {
                    let ner_detections = detector.detect(text);
                    detections.extend(ner_detections);
                    let merged = processor::merge_detections(detections);
                    info!(count = merged.len(), "PII detection complete (ONNX)");
                    return Ok(merged);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to load ONNX model, falling back to dictionary");
                }
            }
        }
    }

    // Suppress unused variable warning when onnx feature is not enabled
    let _ = &manager;

    // Fallback: dictionary detector
    let ner = DictionaryDetector::new();
    detections.extend(ner.detect(text));

    // Merge overlapping detections
    let merged = processor::merge_detections(detections);
    info!(count = merged.len(), "PII detection complete");
    Ok(merged)
}

/// Scan a file for PII: extract text, detect, and filter through ignore list.
///
/// This is the single entry point that both CLI and Tauri should use.
///
/// # Errors
///
/// Returns an error if text extraction or detection fails.
pub fn scan_file(file: &Path) -> Result<ScanResult> {
    info!(path = %file.display(), "scanning file");
    let storage_dir = Config::ensure_storage_dir()?;

    // Extract text
    let text = extract_text(file)?;

    // Detect PII
    let detections = detect_pii(&text, &storage_dir)?;

    // Filter through ignore list
    let ignore_path = storage_dir.join("ignore-list.json");
    let ignore_list = match IgnoreList::load(&ignore_path) {
        Ok(list) => list,
        Err(e) => {
            tracing::warn!(path = %ignore_path.display(), error = %e, "failed to load ignore list, using empty");
            IgnoreList::empty()
        }
    };

    let detections: Vec<_> = detections
        .into_iter()
        .filter(|d| !ignore_list.is_ignored(&d.matched_text))
        .collect();

    // If Smart Detection is installed, classify each detection
    #[cfg(feature = "smart-detection")]
    let detections: Vec<_> = if crate::smart_detection::is_installed() {
        detections
            .into_iter()
            .filter(|d| {
                match crate::smart_detection::classify_detection(
                    &d.matched_text,
                    &d.entity_type,
                    &d.context,
                ) {
                    Ok(crate::smart_detection::PiiClassification::Public) => false,
                    Ok(
                        crate::smart_detection::PiiClassification::Uncertain
                        | crate::smart_detection::PiiClassification::Personal,
                    )
                    | Err(_) => true,
                }
            })
            .collect()
    } else {
        detections
    };

    info!(path = %file.display(), detections = detections.len(), "scan complete");

    Ok(ScanResult { text, detections })
}
