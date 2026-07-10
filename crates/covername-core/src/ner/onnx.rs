//! ONNX-based named entity detector using a `DistilBERT` PII model.
//!
//! This module provides high-accuracy NER detection by running inference
//! on a pre-trained transformer model (e.g., `beki/en_spacy_pii_distilbert`).
//! It uses the ONNX Runtime for efficient model execution and `HuggingFace`
//! tokenizers for text encoding.
//!
//! This module is only compiled when the `onnx` feature is enabled.

use std::cell::UnsafeCell;
use std::path::Path;

use ndarray::{Array2, Axis};
use ort::session::Session;
use ort::value::TensorRef;
use tokenizers::Tokenizer;

use crate::detection::Detection;
use crate::error::{Error, Result};
use crate::ner::NerDetector;

/// An NER detector backed by an ONNX transformer model.
///
/// Loads a pre-trained model and tokenizer from disk, then performs
/// BIO-tagged token classification to identify entities like persons,
/// locations, and organizations.
pub struct OnnxDetector {
    session: UnsafeCell<Session>,
    tokenizer: Tokenizer,
    label_map: Vec<String>,
}

// SAFETY: Session is internally thread-safe via ONNX Runtime's locking.
// The `&mut self` requirement on `Session::run` is for Rust's borrow checker,
// not because the runtime is actually unsafe to call concurrently.
unsafe impl Send for OnnxDetector {}
unsafe impl Sync for OnnxDetector {}

impl OnnxDetector {
    /// Load the ONNX model, tokenizer, and label map from a directory.
    ///
    /// The directory must contain:
    /// - `model.onnx` — the ONNX model file
    /// - `tokenizer.json` — the `HuggingFace` tokenizer configuration
    /// - `labels.json` — a JSON array of BIO label names (e.g., `["O", "B-PER", "I-PER", ...]`)
    ///
    /// # Errors
    ///
    /// Returns an error if any required file is missing or cannot be loaded.
    pub fn load(model_dir: &Path) -> Result<Self> {
        let model_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");
        let labels_path = model_dir.join("labels.json");

        if !model_path.exists() {
            return Err(Error::Io {
                path: model_path,
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "model.onnx not found in model directory",
                ),
            });
        }

        if !tokenizer_path.exists() {
            return Err(Error::Io {
                path: tokenizer_path,
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "tokenizer.json not found in model directory",
                ),
            });
        }

        if !labels_path.exists() {
            return Err(Error::Io {
                path: labels_path,
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "labels.json not found in model directory",
                ),
            });
        }

        let session = Session::builder()
            .and_then(|mut builder| builder.commit_from_file(&model_path))
            .map_err(|e| Error::Model {
                reason: format!("failed to load ONNX session: {e}"),
            })?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| Error::Model {
            reason: format!("failed to load tokenizer: {e}"),
        })?;

        let labels_content = std::fs::read_to_string(&labels_path).map_err(|source| Error::Io {
            path: labels_path.clone(),
            source,
        })?;
        let label_map: Vec<String> =
            serde_json::from_str(&labels_content).map_err(|e| Error::Io {
                path: labels_path,
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()),
            })?;

        Ok(Self {
            session: UnsafeCell::new(session),
            tokenizer,
            label_map,
        })
    }

    /// Map a BIO label to a covername entity type.
    fn label_to_entity_type(label: &str) -> Option<&'static str> {
        let entity = label
            .strip_prefix("B-")
            .or_else(|| label.strip_prefix("I-"))?;

        Some(match entity {
            // Person names
            "PER" | "PERSON" | "GIVENNAME1" | "GIVENNAME2" | "LASTNAME1" | "LASTNAME2"
            | "LASTNAME3" => "PERSON",
            // Email addresses
            "EMAIL" => "EMAIL",
            // Phone numbers
            "TEL" => "PHONE",
            // Physical addresses
            "LOC" | "LOCATION" | "GPE" | "STREET" | "CITY" | "STATE" | "POSTCODE"
            | "SECADDRESS" | "BUILDING" | "COUNTRY" | "GEOCOORD" => "ADDRESS",
            // Social security / national ID numbers
            "SOCIALNUMBER" => "SSN",
            // Dates
            "DATE" | "TIME" | "BOD" => "DATE",
            // Identity documents
            "PASSPORT" | "DRIVERLICENSE" | "IDCARD" => "ID_DOCUMENT",
            // IP addresses
            "IP" => "IP_ADDRESS",
            // Credentials
            "PASS" | "USERNAME" => "CREDENTIAL",
            // Organizations
            "ORG" | "ORGANIZATION" | "NORP" => "ORGANIZATION",
            // Other numeric types
            "CARDINAL" | "QUANTITY" => "NUMBER",
            "MONEY" => "MONEY",
            // Everything else (SEX, TITLE, unknown labels)
            _ => "OTHER",
        })
    }

    /// Check if a label starts a new entity (B- prefix).
    fn is_begin_label(label: &str) -> bool {
        label.starts_with("B-")
    }

    /// Check if a label continues an entity (I- prefix).
    fn is_inside_label(label: &str) -> bool {
        label.starts_with("I-")
    }

    /// Get the entity category from a label (without B-/I- prefix).
    fn label_category(label: &str) -> &str {
        label
            .strip_prefix("B-")
            .or_else(|| label.strip_prefix("I-"))
            .unwrap_or(label)
    }

    /// Decode BIO-tagged tokens into entity spans.
    fn decode_entities(
        &self,
        text: &str,
        predicted_labels: &[usize],
        offsets: &[(usize, usize)],
    ) -> Vec<Detection> {
        let mut detections = Vec::new();
        let mut current_entity: Option<EntitySpan> = None;

        for (idx, (&label_idx, &(char_start, char_end))) in
            predicted_labels.iter().zip(offsets.iter()).enumerate()
        {
            // Skip special tokens (offset 0,0 for [CLS], [SEP], [PAD])
            if char_start == 0 && char_end == 0 && idx > 0 {
                if let Some(entity) = current_entity.take()
                    && let Some(detection) = Self::entity_to_detection(text, &entity)
                {
                    detections.push(detection);
                }
                continue;
            }

            let label = self.label_map.get(label_idx).map_or("O", |s| s.as_str());

            if Self::is_begin_label(label) {
                if let Some(entity) = current_entity.take()
                    && let Some(detection) = Self::entity_to_detection(text, &entity)
                {
                    detections.push(detection);
                }
                current_entity = Some(EntitySpan {
                    category: Self::label_category(label).to_string(),
                    start: char_start,
                    end: char_end,
                });
            } else if Self::is_inside_label(label) {
                if let Some(ref mut entity) = current_entity {
                    if entity.category == Self::label_category(label) {
                        entity.end = char_end;
                    } else {
                        let finished = current_entity.take().unwrap();
                        if let Some(detection) = Self::entity_to_detection(text, &finished) {
                            detections.push(detection);
                        }
                        current_entity = Some(EntitySpan {
                            category: Self::label_category(label).to_string(),
                            start: char_start,
                            end: char_end,
                        });
                    }
                } else {
                    // I- without a preceding B- — treat as B-
                    current_entity = Some(EntitySpan {
                        category: Self::label_category(label).to_string(),
                        start: char_start,
                        end: char_end,
                    });
                }
            } else if let Some(entity) = current_entity.take()
                && let Some(detection) = Self::entity_to_detection(text, &entity)
            {
                detections.push(detection);
            }
        }

        // Flush final entity
        if let Some(entity) = current_entity.take()
            && let Some(detection) = Self::entity_to_detection(text, &entity)
        {
            detections.push(detection);
        }

        detections
    }

    /// Convert an entity span into a Detection.
    fn entity_to_detection(text: &str, entity: &EntitySpan) -> Option<Detection> {
        let entity_type = Self::label_to_entity_type(&format!("B-{}", entity.category))?;

        if entity.start >= text.len() || entity.end > text.len() || entity.start >= entity.end {
            return None;
        }

        let matched_text = &text[entity.start..entity.end];

        if matched_text.trim().is_empty() {
            return None;
        }

        let context = Self::extract_context(text, entity.start, entity.end);

        Some(Detection {
            matched_text: matched_text.to_string(),
            entity_type: entity_type.to_string(),
            rule_name: String::from("NER (ONNX - PII DistilBERT)"),
            start: entity.start,
            end: entity.end,
            context,
        })
    }

    /// Extract a context snippet around a match.
    fn extract_context(text: &str, start: usize, end: usize) -> String {
        let context_chars = 40;
        let ctx_start = start.saturating_sub(context_chars);
        let ctx_end = (end + context_chars).min(text.len());

        let ctx_start = text.floor_char_boundary(ctx_start);
        let ctx_end = text.ceil_char_boundary(ctx_end);

        let mut context = String::new();
        if ctx_start > 0 {
            context.push_str("...");
        }
        context.push_str(&text[ctx_start..ctx_end]);
        if ctx_end < text.len() {
            context.push_str("...");
        }
        context
    }
}

/// A partially-constructed entity span during BIO decoding.
struct EntitySpan {
    category: String,
    start: usize,
    end: usize,
}

impl NerDetector for OnnxDetector {
    fn detect(&self, text: &str) -> Vec<Detection> {
        if text.is_empty() {
            return Vec::new();
        }

        // Tokenize the input text
        let encoding = match self.tokenizer.encode(text, true) {
            Ok(enc) => enc,
            Err(e) => {
                eprintln!("Warning: tokenization failed: {e}");
                return Vec::new();
            }
        };

        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| i64::from(id)).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&m| i64::from(m))
            .collect();
        let token_type_ids: Vec<i64> = encoding
            .get_type_ids()
            .iter()
            .map(|&t| i64::from(t))
            .collect();
        let seq_len = input_ids.len();

        // Create 2D arrays (batch_size=1, seq_len)
        let input_ids_array = match Array2::from_shape_vec((1, seq_len), input_ids) {
            Ok(arr) => arr,
            Err(e) => {
                eprintln!("Warning: failed to create input array: {e}");
                return Vec::new();
            }
        };
        let attention_mask_array = match Array2::from_shape_vec((1, seq_len), attention_mask) {
            Ok(arr) => arr,
            Err(e) => {
                eprintln!("Warning: failed to create attention mask array: {e}");
                return Vec::new();
            }
        };
        let token_type_ids_array = match Array2::from_shape_vec((1, seq_len), token_type_ids) {
            Ok(arr) => arr,
            Err(e) => {
                eprintln!("Warning: failed to create token_type_ids array: {e}");
                return Vec::new();
            }
        };

        // Create tensor references for ort
        let input_ids_tensor = match TensorRef::from_array_view(&input_ids_array) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Warning: failed to create input_ids tensor: {e}");
                return Vec::new();
            }
        };
        let attention_mask_tensor = match TensorRef::from_array_view(&attention_mask_array) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Warning: failed to create attention_mask tensor: {e}");
                return Vec::new();
            }
        };
        let token_type_ids_tensor = match TensorRef::from_array_view(&token_type_ids_array) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Warning: failed to create token_type_ids tensor: {e}");
                return Vec::new();
            }
        };

        // Run inference
        // SAFETY: We have exclusive logical access through &self. The UnsafeCell
        // is necessary because Session::run requires &mut self for Rust's borrow
        // checker, but ONNX Runtime handles concurrency internally.
        let outputs = match unsafe { &mut *self.session.get() }.run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        ]) {
            Ok(outputs) => outputs,
            Err(e) => {
                eprintln!("Warning: ONNX inference failed: {e}");
                return Vec::new();
            }
        };

        // Extract logits from output (shape: [1, seq_len, num_labels])
        let logits = match outputs[0].try_extract_array::<f32>() {
            Ok(tensor) => tensor,
            Err(e) => {
                eprintln!("Warning: failed to extract logits: {e}");
                return Vec::new();
            }
        };

        // Argmax over the label dimension to get predicted label indices
        let predicted_labels: Vec<usize> = logits
            .index_axis(Axis(0), 0) // Remove batch dimension -> [seq_len, num_labels]
            .rows()
            .into_iter()
            .map(|row| {
                row.iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                    .map_or(0, |(idx, _)| idx)
            })
            .collect();

        // Get character offsets from the tokenizer
        let offsets: Vec<(usize, usize)> = encoding.get_offsets().to_vec();

        self.decode_entities(text, &predicted_labels, &offsets)
    }

    fn name(&self) -> &'static str {
        "NER (ONNX - PII DistilBERT)"
    }

    fn is_ready(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_to_entity_type_person() {
        assert_eq!(OnnxDetector::label_to_entity_type("B-PER"), Some("PERSON"));
        assert_eq!(OnnxDetector::label_to_entity_type("I-PER"), Some("PERSON"));
        assert_eq!(
            OnnxDetector::label_to_entity_type("B-PERSON"),
            Some("PERSON")
        );
    }

    #[test]
    fn test_label_to_entity_type_location() {
        assert_eq!(OnnxDetector::label_to_entity_type("B-LOC"), Some("ADDRESS"));
        assert_eq!(OnnxDetector::label_to_entity_type("I-LOC"), Some("ADDRESS"));
        assert_eq!(OnnxDetector::label_to_entity_type("B-GPE"), Some("ADDRESS"));
    }

    #[test]
    fn test_label_to_entity_type_org() {
        assert_eq!(
            OnnxDetector::label_to_entity_type("B-ORG"),
            Some("ORGANIZATION")
        );
        assert_eq!(
            OnnxDetector::label_to_entity_type("I-ORG"),
            Some("ORGANIZATION")
        );
    }

    #[test]
    fn test_label_to_entity_type_outside() {
        assert_eq!(OnnxDetector::label_to_entity_type("O"), None);
    }

    #[test]
    fn test_is_begin_label() {
        assert!(OnnxDetector::is_begin_label("B-PER"));
        assert!(OnnxDetector::is_begin_label("B-LOC"));
        assert!(!OnnxDetector::is_begin_label("I-PER"));
        assert!(!OnnxDetector::is_begin_label("O"));
    }

    #[test]
    fn test_is_inside_label() {
        assert!(OnnxDetector::is_inside_label("I-PER"));
        assert!(OnnxDetector::is_inside_label("I-LOC"));
        assert!(!OnnxDetector::is_inside_label("B-PER"));
        assert!(!OnnxDetector::is_inside_label("O"));
    }

    #[test]
    fn test_label_category() {
        assert_eq!(OnnxDetector::label_category("B-PER"), "PER");
        assert_eq!(OnnxDetector::label_category("I-LOC"), "LOC");
        assert_eq!(OnnxDetector::label_category("O"), "O");
    }

    /// Test decode logic using the standalone helper that mirrors `decode_entities`.
    #[test]
    fn test_decode_entities_basic() {
        let text = "Contact John Smith today";
        let label_map = vec![
            "O".to_string(),
            "B-PER".to_string(),
            "I-PER".to_string(),
            "B-LOC".to_string(),
            "I-LOC".to_string(),
        ];
        // Token offsets: [CLS](0,0), "Contact"(0,7), "John"(8,12), "Smith"(13,18), "today"(19,24), [SEP](0,0)
        let predicted_labels = vec![0, 0, 1, 2, 0, 0];
        let offsets = vec![(0, 0), (0, 7), (8, 12), (13, 18), (19, 24), (0, 0)];

        let detections = decode_entities_standalone(text, &predicted_labels, &offsets, &label_map);

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].matched_text, "John Smith");
        assert_eq!(detections[0].entity_type, "PERSON");
        assert_eq!(detections[0].start, 8);
        assert_eq!(detections[0].end, 18);
    }

    #[test]
    fn test_decode_entities_multiple() {
        let text = "John Smith lives in New York";
        let label_map = vec![
            "O".to_string(),
            "B-PER".to_string(),
            "I-PER".to_string(),
            "B-LOC".to_string(),
            "I-LOC".to_string(),
        ];
        let offsets = vec![
            (0, 0),
            (0, 4),
            (5, 10),
            (11, 16),
            (17, 19),
            (20, 23),
            (24, 28),
            (0, 0),
        ];
        let predicted_labels = vec![0, 1, 2, 0, 0, 3, 4, 0];

        let detections = decode_entities_standalone(text, &predicted_labels, &offsets, &label_map);

        assert_eq!(detections.len(), 2);
        assert_eq!(detections[0].matched_text, "John Smith");
        assert_eq!(detections[0].entity_type, "PERSON");
        assert_eq!(detections[1].matched_text, "New York");
        assert_eq!(detections[1].entity_type, "ADDRESS");
    }

    #[test]
    fn test_decode_entities_empty() {
        let text = "No entities here";
        let label_map = vec!["O".to_string(), "B-PER".to_string()];
        let offsets = vec![(0, 0), (0, 2), (3, 11), (12, 16), (0, 0)];
        let predicted_labels = vec![0, 0, 0, 0, 0];

        let detections = decode_entities_standalone(text, &predicted_labels, &offsets, &label_map);
        assert!(detections.is_empty());
    }

    #[test]
    fn test_decode_entities_i_without_b() {
        let text = "Hello John Smith";
        let label_map = vec!["O".to_string(), "B-PER".to_string(), "I-PER".to_string()];
        let offsets = vec![(0, 0), (0, 5), (6, 10), (11, 16), (0, 0)];
        let predicted_labels = vec![0, 0, 2, 2, 0];

        let detections = decode_entities_standalone(text, &predicted_labels, &offsets, &label_map);

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].matched_text, "John Smith");
        assert_eq!(detections[0].entity_type, "PERSON");
    }

    /// Standalone decode helper that mirrors `OnnxDetector::decode_entities` logic
    /// without needing a real `OnnxDetector` instance.
    fn decode_entities_standalone(
        text: &str,
        predicted_labels: &[usize],
        offsets: &[(usize, usize)],
        label_map: &[String],
    ) -> Vec<Detection> {
        let mut detections = Vec::new();
        let mut current_entity: Option<EntitySpan> = None;

        for (idx, (&label_idx, &(char_start, char_end))) in
            predicted_labels.iter().zip(offsets.iter()).enumerate()
        {
            if char_start == 0 && char_end == 0 && idx > 0 {
                if let Some(entity) = current_entity.take()
                    && let Some(detection) = OnnxDetector::entity_to_detection(text, &entity)
                {
                    detections.push(detection);
                }
                continue;
            }

            let label = label_map.get(label_idx).map_or("O", |s| s.as_str());

            if OnnxDetector::is_begin_label(label) {
                if let Some(entity) = current_entity.take()
                    && let Some(detection) = OnnxDetector::entity_to_detection(text, &entity)
                {
                    detections.push(detection);
                }
                current_entity = Some(EntitySpan {
                    category: OnnxDetector::label_category(label).to_string(),
                    start: char_start,
                    end: char_end,
                });
            } else if OnnxDetector::is_inside_label(label) {
                if let Some(ref mut entity) = current_entity {
                    if entity.category == OnnxDetector::label_category(label) {
                        entity.end = char_end;
                    } else {
                        let finished = current_entity.take().unwrap();
                        if let Some(detection) = OnnxDetector::entity_to_detection(text, &finished)
                        {
                            detections.push(detection);
                        }
                        current_entity = Some(EntitySpan {
                            category: OnnxDetector::label_category(label).to_string(),
                            start: char_start,
                            end: char_end,
                        });
                    }
                } else {
                    current_entity = Some(EntitySpan {
                        category: OnnxDetector::label_category(label).to_string(),
                        start: char_start,
                        end: char_end,
                    });
                }
            } else if let Some(entity) = current_entity.take()
                && let Some(detection) = OnnxDetector::entity_to_detection(text, &entity)
            {
                detections.push(detection);
            }
        }

        if let Some(entity) = current_entity.take()
            && let Some(detection) = OnnxDetector::entity_to_detection(text, &entity)
        {
            detections.push(detection);
        }

        detections
    }
}
