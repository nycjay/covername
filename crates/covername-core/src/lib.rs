//! Covername core library.
//!
//! Provides the foundational logic for document anonymization:
//! configuration management, PII-to-replacement mapping storage,
//! detection via rule engine, document processing, and output generation.

pub mod config;
pub mod detection;
pub mod document;
pub mod error;
pub mod export;
pub mod ignore;
pub mod mapping;
pub mod ner;
pub mod ocr;
pub mod output;
pub mod pdf_output;
pub mod pdfium;
pub mod pipeline;
pub mod processor;
pub mod redact;
pub mod replacement;
pub mod smart_detection;
pub mod utils;
pub mod xlsx;
