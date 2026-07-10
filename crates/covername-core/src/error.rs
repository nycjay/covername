//! Error types for the covername-core library.

use std::path::PathBuf;

/// Errors that can occur in covername-core operations.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failed to read or write a file.
    #[error("I/O error at {path}: {source}")]
    Io {
        /// The path that caused the error.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },

    /// Failed to serialize or deserialize JSON.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Failed to determine the storage directory.
    #[error("could not determine application data directory")]
    NoDataDir,

    /// An invalid configuration key was provided.
    #[error("unknown config key: {0}")]
    UnknownConfigKey(String),

    /// A regex pattern failed to compile.
    #[error("invalid pattern `{pattern}`: {reason}")]
    InvalidPattern {
        /// The pattern that failed.
        pattern: String,
        /// Why it failed.
        reason: String,
    },

    /// Failed to extract text from a PDF file.
    #[error("PDF extraction failed for {path}: {reason}")]
    PdfExtract {
        /// The PDF file path.
        path: PathBuf,
        /// Why extraction failed.
        reason: String,
    },

    /// Failed to generate a PDF output file.
    #[error("PDF generation failed for {path}: {reason}")]
    PdfGenerate {
        /// The output file path.
        path: PathBuf,
        /// Why generation failed.
        reason: String,
    },

    /// Failed to read or write a ZIP archive.
    #[error("ZIP error at {path}: {reason}")]
    Zip {
        /// The ZIP file path.
        path: PathBuf,
        /// Why the operation failed.
        reason: String,
    },

    /// A directory walk failed.
    #[error("failed to traverse directory {path}: {reason}")]
    DirectoryWalk {
        /// The directory path.
        path: PathBuf,
        /// Why the walk failed.
        reason: String,
    },

    /// The ONNX model failed to load or run inference.
    #[error("NER model error: {reason}")]
    Model {
        /// What went wrong.
        reason: String,
    },

    /// Failed to read or write an XLSX file.
    #[error("XLSX error at {path}: {reason}")]
    Xlsx {
        /// The XLSX file path.
        path: PathBuf,
        /// Why the operation failed.
        reason: String,
    },

    /// OCR processing failed.
    #[error("OCR failed for {path}: {reason}")]
    Ocr {
        /// The file path that was being processed.
        path: PathBuf,
        /// Why OCR failed.
        reason: String,
    },

    /// Tesseract is not installed on the system.
    #[error("tesseract not found. Install tesseract for OCR: brew install tesseract")]
    TesseractNotFound,
}

/// A convenience type alias for results in this crate.
pub type Result<T> = std::result::Result<T, Error>;
