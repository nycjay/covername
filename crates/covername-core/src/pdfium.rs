//! Shared `PDFium` library loading.
//!
//! Centralizes `PDFium` discovery and initialization so both OCR and redaction
//! modules use the same search logic.

use std::path::Path;

use pdfium_render::prelude::*;

use crate::error::{Error, Result};

/// Load the `PDFium` library, trying multiple search locations.
///
/// Search order:
/// 1. `PDFIUM_DYNAMIC_LIB_PATH` environment variable
/// 2. Inside the app bundle (`Contents/Resources/lib/`)
/// 3. `~/lib/pdfium/lib` (installed by `just setup`)
/// 4. System library path
///
/// # Errors
///
/// Returns an error if `PDFium` cannot be found in any location.
pub fn load_pdfium(context_path: &Path) -> Result<Pdfium> {
    // Collect all candidate paths to try
    let mut candidates: Vec<String> = Vec::new();

    // 1. PDFIUM_DYNAMIC_LIB_PATH env var
    if let Ok(path) = std::env::var("PDFIUM_DYNAMIC_LIB_PATH") {
        candidates.push(path);
    }

    // 2. Inside app bundle: exe → ../Resources/lib
    if let Some(contents_dir) = std::env::current_exe()
        .ok()
        .as_ref()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    {
        let bundle_lib = contents_dir.join("Resources").join("lib");
        candidates.push(bundle_lib.to_string_lossy().into_owned());
    }

    // 3. ~/lib/pdfium/lib (CLI / dev)
    if let Some(home) = dirs::home_dir() {
        let home_lib = home.join("lib").join("pdfium").join("lib");
        candidates.push(home_lib.to_string_lossy().into_owned());
    }

    // Try each candidate path
    for path in &candidates {
        if let Ok(lib) = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(path))
        {
            return Ok(Pdfium::new(lib));
        }
    }

    // 4. System library (last resort)
    if let Ok(lib) = Pdfium::bind_to_system_library() {
        return Ok(Pdfium::new(lib));
    }

    Err(Error::Ocr {
        path: context_path.to_path_buf(),
        reason: format!(
            "PDFium library not found. Searched: {}. \
             For CLI use, run 'just setup' or set PDFIUM_DYNAMIC_LIB_PATH.",
            candidates.join(", ")
        ),
    })
}
