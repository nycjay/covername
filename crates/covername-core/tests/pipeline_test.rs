//! Integration tests for the text processing pipeline.

use std::path::Path;

use covername_core::detection::RuleEngine;
use covername_core::document::TextDocument;
use covername_core::mapping::MappingStore;
use covername_core::ner::{DictionaryDetector, NerDetector};
use covername_core::output;
use covername_core::processor;
use covername_core::replacement;

#[test]
fn test_scan_sample_fixture_finds_detections() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("sample.txt");

    let doc = TextDocument::from_file(&fixture_path).unwrap();
    let engine = RuleEngine::new().unwrap();
    let detections = engine.scan(doc.content());

    // Should find at least: SSN, phone, email, account number
    assert!(
        detections.len() >= 4,
        "Expected at least 4 detections, found {}",
        detections.len()
    );

    // Check specific entity types are present
    let entity_types: Vec<&str> = detections.iter().map(|d| d.entity_type.as_str()).collect();
    assert!(
        entity_types.contains(&"SSN"),
        "Should detect SSN, found: {entity_types:?}"
    );
    assert!(
        entity_types.contains(&"PHONE"),
        "Should detect phone, found: {entity_types:?}"
    );
    assert!(
        entity_types.contains(&"EMAIL"),
        "Should detect email, found: {entity_types:?}"
    );
    assert!(
        entity_types.contains(&"ACCOUNT_NUMBER"),
        "Should detect account number, found: {entity_types:?}"
    );
}

#[test]
fn test_full_pipeline_removes_pii_from_output() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("sample.txt");

    let doc = TextDocument::from_file(&fixture_path).unwrap();
    let engine = RuleEngine::new().unwrap();
    let detections = engine.scan(doc.content());

    let dir = tempfile::TempDir::new().unwrap();
    let mapping_path = dir.path().join("mappings.json");
    let store = MappingStore::load(&mapping_path).unwrap();

    let mut resolved = processor::resolve_detections(detections, &store, &|orig, etype| {
        replacement::suggest_replacement(orig, etype)
    });

    // Accept all replacements
    for r in &mut resolved {
        r.accepted = true;
    }

    let output_text = processor::apply_replacements(doc.content(), &resolved);

    // Original PII should not appear in output
    assert!(
        !output_text.contains("123-45-6789"),
        "Output should not contain original SSN"
    );
    assert!(
        !output_text.contains("john.smith@firstnational.com"),
        "Output should not contain original email"
    );

    // Output should still be valid text with content
    assert!(!output_text.is_empty());
    assert!(output_text.contains("First National Bank"));
}

#[test]
fn test_output_path_resolution() {
    let config = covername_core::config::Config::default();
    let input = Path::new("/tmp/test-doc.txt");
    let output_path = output::resolve_output_path(input, &config);

    assert_eq!(
        output_path,
        Path::new("/tmp/test-doc-covered.txt").to_path_buf()
    );
}

#[test]
fn test_ner_detects_person_names_in_sample() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("sample.txt");

    let doc = TextDocument::from_file(&fixture_path).unwrap();
    let detector = DictionaryDetector::new();
    let detections = detector.detect(doc.content());

    // Should find person names (John Smith, Jane Smith) mid-sentence
    let person_detections: Vec<_> = detections
        .iter()
        .filter(|d| d.entity_type == "PERSON")
        .collect();

    assert!(
        !person_detections.is_empty(),
        "NER should detect person names in sample.txt"
    );

    let matched_texts: Vec<&str> = person_detections
        .iter()
        .map(|d| d.matched_text.as_str())
        .collect();

    assert!(
        matched_texts.contains(&"John Smith"),
        "Should detect 'John Smith', found: {matched_texts:?}"
    );
    assert!(
        matched_texts.contains(&"Jane Smith"),
        "Should detect 'Jane Smith', found: {matched_texts:?}"
    );
}

#[test]
fn test_merged_pipeline_detects_all_pii_types() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test-fixtures")
        .join("sample.txt");

    let doc = TextDocument::from_file(&fixture_path).unwrap();

    // Run both detectors
    let engine = RuleEngine::new().unwrap();
    let mut detections = engine.scan(doc.content());

    let ner_detector = DictionaryDetector::new();
    let ner_detections = ner_detector.detect(doc.content());
    detections.extend(ner_detections);

    // Merge
    let merged = processor::merge_detections(detections);

    // Should find regex-based detections
    let entity_types: Vec<&str> = merged.iter().map(|d| d.entity_type.as_str()).collect();
    assert!(entity_types.contains(&"SSN"));
    assert!(entity_types.contains(&"EMAIL"));
    assert!(entity_types.contains(&"PHONE"));

    // Should also find NER-based person detections
    assert!(
        entity_types.contains(&"PERSON"),
        "Merged pipeline should include PERSON detections, found: {entity_types:?}"
    );
}
