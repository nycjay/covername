//! Integration test for the PDF pipeline.
//!
//! Tests the full cycle: generate a PDF with known PII content,
//! extract text, run detection, apply replacements, and verify
//! the output PDF does not contain the original PII.

use std::fs;

use tempfile::TempDir;

use covername_core::detection::RuleEngine;
use covername_core::document::{DocumentType, PdfDocument, detect_file_type};
use covername_core::pdf_output;
use covername_core::processor;

/// Generate a test PDF with known PII content using printpdf,
/// then verify the full extraction → detection → replacement pipeline.
#[test]
fn test_pdf_roundtrip_pipeline() {
    let dir = TempDir::new().unwrap();

    // Step 1: Generate a test PDF with known PII content
    let pii_text = "Patient record for John Smith.\n\
                    SSN: 123-45-6789\n\
                    Phone: (555) 867-5309\n\
                    Email: john.smith@example.com\n\
                    Account #: 9876543210\n\
                    This document contains sensitive information.";

    let test_pdf_path = dir.path().join("test_input.pdf");
    pdf_output::write_pdf(pii_text, &test_pdf_path).unwrap();
    assert!(test_pdf_path.exists());

    // Step 2: Read it back and extract text
    let pdf_doc = PdfDocument::from_file(&test_pdf_path).unwrap();
    let extracted = pdf_doc.extract_text().unwrap();

    // Verify that our known PII is present in the extracted text
    assert!(
        extracted.contains("123-45-6789"),
        "Extracted text should contain SSN. Got: {extracted}"
    );
    assert!(
        extracted.contains("555"),
        "Extracted text should contain phone area code. Got: {extracted}"
    );

    // Step 3: Run detection on the extracted text
    let engine = RuleEngine::new().unwrap();
    let detections = engine.scan(&extracted);

    // Should find at least the SSN
    let ssn_detections: Vec<_> = detections
        .iter()
        .filter(|d| d.entity_type == "SSN")
        .collect();
    assert!(
        !ssn_detections.is_empty(),
        "Should detect SSN in extracted PDF text. Detections: {detections:?}"
    );

    // Step 4: Apply replacements
    let merged = processor::merge_detections(detections);
    let resolved: Vec<_> = merged
        .into_iter()
        .map(|d| processor::ResolvedDetection {
            replacement: format!("[REDACTED-{}]", d.entity_type),
            detection: d,
            accepted: true,
        })
        .collect();

    let output_text = processor::apply_replacements(&extracted, &resolved);

    // Verify original PII is no longer in the output text
    assert!(
        !output_text.contains("123-45-6789"),
        "Output text should not contain original SSN"
    );

    // Step 5: Generate output PDF
    let output_pdf_path = dir.path().join("test_output.pdf");
    pdf_output::write_pdf(&output_text, &output_pdf_path).unwrap();
    assert!(output_pdf_path.exists());

    // Step 6: Verify the output PDF bytes don't contain the original PII
    let output_bytes = fs::read(&output_pdf_path).unwrap();
    let output_bytes_str = String::from_utf8_lossy(&output_bytes);
    assert!(
        !output_bytes_str.contains("123-45-6789"),
        "Output PDF bytes should not contain original SSN"
    );
}

/// Test that `detect_file_type` correctly identifies PDF files.
#[test]
fn test_detect_file_type_pdf() {
    assert_eq!(
        detect_file_type(std::path::Path::new("document.pdf")),
        DocumentType::Pdf
    );
    assert_eq!(
        detect_file_type(std::path::Path::new("REPORT.PDF")),
        DocumentType::Pdf
    );
    assert_eq!(
        detect_file_type(std::path::Path::new("notes.txt")),
        DocumentType::Text
    );
}

/// Test that `write_pdf` produces a valid PDF that starts with %PDF.
#[test]
fn test_write_pdf_produces_valid_pdf() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("valid.pdf");

    pdf_output::write_pdf("Hello, this is a test PDF.", &output).unwrap();

    let bytes = fs::read(&output).unwrap();
    assert!(
        bytes.starts_with(b"%PDF"),
        "Generated file should be a valid PDF"
    );
    assert!(bytes.len() > 100, "PDF should not be trivially small");
}

/// Test that a multi-page document is correctly generated.
#[test]
fn test_write_pdf_multipage() {
    let dir = TempDir::new().unwrap();
    let output = dir.path().join("multipage.pdf");

    // Generate enough content for multiple pages (>50 lines)
    let mut content = String::new();
    for i in 0..100 {
        use std::fmt::Write;
        writeln!(content, "Line {i}: This is a test line with some content.").unwrap();
    }

    pdf_output::write_pdf(&content, &output).unwrap();

    let bytes = fs::read(&output).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
}
