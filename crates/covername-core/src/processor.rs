//! Processing pipeline for document anonymization.
//!
//! Coordinates the detection, replacement resolution, and text
//! transformation steps. This module ties together the rule engine,
//! mapping store, and replacement generator into a complete pipeline.

use crate::detection::Detection;
use crate::mapping::MappingStore;
use crate::replacement;

/// A detection with its resolved replacement value.
///
/// Extends a `Detection` with the chosen replacement text and
/// whether the user has accepted this replacement.
#[derive(Debug, Clone)]
pub struct ResolvedDetection {
    /// The original detection from the rule engine.
    pub detection: Detection,
    /// The suggested or chosen replacement text.
    pub replacement: String,
    /// Whether this replacement has been accepted by the user.
    pub accepted: bool,
}

/// The result of processing a document through the pipeline.
///
/// Contains the original text, all resolved detections (accepted or not),
/// and the final output text with accepted replacements applied.
#[derive(Debug, Clone)]
pub struct ProcessingResult {
    /// The original document text.
    pub original_text: String,
    /// All detections with their resolved replacements.
    pub detections: Vec<ResolvedDetection>,
    /// The output text after applying accepted replacements.
    pub output_text: String,
}

/// Apply accepted replacements to produce the output text.
///
/// Processes replacements from end to start to avoid position shifts
/// when earlier replacements change the text length. Only applies
/// detections where `accepted` is `true`.
pub fn apply_replacements(text: &str, detections: &[ResolvedDetection]) -> String {
    let mut result = text.to_string();

    // Collect accepted detections and sort by start position descending
    let mut accepted: Vec<&ResolvedDetection> = detections.iter().filter(|d| d.accepted).collect();
    accepted.sort_by_key(|d| std::cmp::Reverse(d.detection.start));

    for resolved in accepted {
        let start = resolved.detection.start;
        let end = resolved.detection.end;
        result.replace_range(start..end, &resolved.replacement);
    }

    result
}

/// Resolve detections into replacements using the mapping store and generator.
///
/// For each detection, checks the mapping store for an existing replacement.
/// If none exists, generates a suggestion using the provided function.
/// All detections start as not accepted (the user must review them).
pub fn resolve_detections(
    detections: Vec<Detection>,
    mapping_store: &MappingStore,
    suggest_fn: &dyn Fn(&str, &str) -> String,
) -> Vec<ResolvedDetection> {
    detections
        .into_iter()
        .map(|detection| {
            let replacement = if let Some(mapping) = mapping_store.find(&detection.matched_text) {
                mapping.replacement.clone()
            } else {
                suggest_fn(&detection.matched_text, &detection.entity_type)
            };

            ResolvedDetection {
                detection,
                replacement,
                accepted: false,
            }
        })
        .collect()
}

/// Convenience wrapper that uses the default replacement suggestion logic.
pub fn resolve_detections_default(
    detections: Vec<Detection>,
    mapping_store: &MappingStore,
) -> Vec<ResolvedDetection> {
    resolve_detections(detections, mapping_store, &replacement::suggest_replacement)
}

/// Merge detections from multiple sources, removing overlaps.
///
/// When two detections overlap (their byte ranges intersect), the longer
/// detection is kept since it represents a more specific match. This prevents
/// double-detection when both the regex engine and NER detector find the same
/// PII entity.
///
/// Detections are returned sorted by start position.
pub fn merge_detections(mut detections: Vec<Detection>) -> Vec<Detection> {
    if detections.is_empty() {
        return detections;
    }

    // Sort by start position, then by length descending (prefer longer matches)
    detections.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));

    let mut merged: Vec<Detection> = Vec::with_capacity(detections.len());

    for detection in detections {
        if let Some(last) = merged.last() {
            // Check for overlap: current starts before last ends
            if detection.start < last.end {
                // Overlap detected — keep the longer one
                let last_len = last.end - last.start;
                let curr_len = detection.end - detection.start;
                if curr_len > last_len {
                    // Replace with the longer detection
                    merged.pop();
                    merged.push(detection);
                }
                // Otherwise keep the existing (longer or equal) detection
            } else {
                merged.push(detection);
            }
        } else {
            merged.push(detection);
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_detection(text: &str, start: usize, end: usize, entity_type: &str) -> Detection {
        Detection {
            matched_text: text.to_string(),
            entity_type: entity_type.to_string(),
            rule_name: String::from("test"),
            start,
            end,
            context: String::new(),
        }
    }

    #[test]
    fn test_apply_replacements_single() {
        let text = "My SSN is 123-45-6789 and that's it.";
        let detections = vec![ResolvedDetection {
            detection: make_detection("123-45-6789", 10, 21, "SSN"),
            replacement: String::from("900-00-0000"),
            accepted: true,
        }];

        let result = apply_replacements(text, &detections);
        assert_eq!(result, "My SSN is 900-00-0000 and that's it.");
    }

    #[test]
    fn test_apply_replacements_multiple_non_overlapping() {
        let text = "SSN: 123-45-6789, Phone: 555-123-4567";
        let detections = vec![
            ResolvedDetection {
                detection: make_detection("123-45-6789", 5, 16, "SSN"),
                replacement: String::from("900-00-0000"),
                accepted: true,
            },
            ResolvedDetection {
                detection: make_detection("555-123-4567", 25, 37, "PHONE"),
                replacement: String::from("(555) 555-0000"),
                accepted: true,
            },
        ];

        let result = apply_replacements(text, &detections);
        assert_eq!(result, "SSN: 900-00-0000, Phone: (555) 555-0000");
    }

    #[test]
    fn test_apply_replacements_skips_rejected() {
        let text = "SSN: 123-45-6789, Phone: 555-123-4567";
        let detections = vec![
            ResolvedDetection {
                detection: make_detection("123-45-6789", 5, 16, "SSN"),
                replacement: String::from("900-00-0000"),
                accepted: true,
            },
            ResolvedDetection {
                detection: make_detection("555-123-4567", 25, 37, "PHONE"),
                replacement: String::from("(555) 555-0000"),
                accepted: false, // rejected
            },
        ];

        let result = apply_replacements(text, &detections);
        assert_eq!(result, "SSN: 900-00-0000, Phone: 555-123-4567");
    }

    #[test]
    fn test_apply_replacements_empty_detections() {
        let text = "No PII here.";
        let result = apply_replacements(text, &[]);
        assert_eq!(result, text);
    }

    #[test]
    fn test_apply_replacements_different_length_replacement() {
        let text = "Name: Jo, SSN: 123-45-6789";
        let detections = vec![
            ResolvedDetection {
                detection: make_detection("Jo", 6, 8, "PERSON"),
                replacement: String::from("Alexander Blackwood"),
                accepted: true,
            },
            ResolvedDetection {
                detection: make_detection("123-45-6789", 15, 26, "SSN"),
                replacement: String::from("900-00-0000"),
                accepted: true,
            },
        ];

        let result = apply_replacements(text, &detections);
        assert_eq!(result, "Name: Alexander Blackwood, SSN: 900-00-0000");
    }

    #[test]
    fn test_resolve_detections_uses_mapping_store() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mappings.json");
        let mut store = MappingStore::load(&path).unwrap();
        store.add("John Smith", "Jane Doe", "PERSON").unwrap();

        let detections = vec![make_detection("John Smith", 0, 10, "PERSON")];

        let resolved = resolve_detections(detections, &store, &|orig, etype| {
            replacement::suggest_replacement(orig, etype)
        });

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].replacement, "Jane Doe");
        assert!(!resolved[0].accepted);
    }

    #[test]
    fn test_resolve_detections_generates_suggestion_when_no_mapping() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mappings.json");
        let store = MappingStore::load(&path).unwrap();

        let detections = vec![make_detection("123-45-6789", 0, 11, "SSN")];

        let resolved = resolve_detections(detections, &store, &|orig, etype| {
            replacement::suggest_replacement(orig, etype)
        });

        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].replacement.starts_with("900-"));
        assert!(!resolved[0].accepted);
    }

    #[test]
    fn test_merge_detections_no_overlap() {
        let detections = vec![
            make_detection("John Smith", 10, 20, "PERSON"),
            make_detection("123-45-6789", 30, 41, "SSN"),
        ];

        let merged = merge_detections(detections);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].matched_text, "John Smith");
        assert_eq!(merged[1].matched_text, "123-45-6789");
    }

    #[test]
    fn test_merge_detections_overlap_keeps_longer() {
        let detections = vec![
            make_detection("John", 10, 14, "PERSON"),
            make_detection("John Smith", 10, 20, "PERSON"),
        ];

        let merged = merge_detections(detections);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].matched_text, "John Smith");
    }

    #[test]
    fn test_merge_detections_partial_overlap() {
        // Two detections that partially overlap — keep the longer one
        let detections = vec![
            make_detection("John Smith", 10, 20, "PERSON"),
            make_detection("Smith Jr", 15, 23, "PERSON"),
        ];

        let merged = merge_detections(detections);
        assert_eq!(merged.len(), 1);
        // "John Smith" (len 10) vs "Smith Jr" (len 8) — keep "John Smith"
        assert_eq!(merged[0].matched_text, "John Smith");
    }

    #[test]
    fn test_merge_detections_empty() {
        let detections: Vec<Detection> = vec![];
        let merged = merge_detections(detections);
        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_detections_sorted_output() {
        let detections = vec![
            make_detection("123-45-6789", 30, 41, "SSN"),
            make_detection("John Smith", 10, 20, "PERSON"),
        ];

        let merged = merge_detections(detections);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].start, 10);
        assert_eq!(merged[1].start, 30);
    }
}
