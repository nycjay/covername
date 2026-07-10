//! Named Entity Recognition (NER) module.
//!
//! Provides trait-based NER detection to supplement regex-based PII detection.
//! The trait allows plugging in different detection backends (dictionary-based,
//! ONNX model, etc.) while presenting a uniform interface to the processing pipeline.

pub mod dictionary;
pub mod model_manager;
#[cfg(feature = "onnx")]
pub mod onnx;

pub use dictionary::DictionaryDetector;
pub use model_manager::{ModelManager, ModelStatus};
#[cfg(feature = "onnx")]
pub use onnx::OnnxDetector;

use crate::detection::Detection;

/// A named entity detector that can identify PII in unstructured text.
///
/// Implementations range from simple heuristic approaches (dictionary-based)
/// to full ML model inference (ONNX). All detectors produce the same
/// `Detection` type used by the regex rule engine, allowing results to be
/// merged seamlessly.
pub trait NerDetector: Send + Sync {
    /// Detect named entities in text, returning Detection objects.
    fn detect(&self, text: &str) -> Vec<Detection>;
    /// Human-readable name of this detector.
    fn name(&self) -> &'static str;
    /// Whether this detector is ready to use (model loaded, etc.)
    fn is_ready(&self) -> bool;
}
