//! Shared utility functions used across the crate.

use indicatif::ProgressStyle;

/// Extract a context snippet around a match (up to 40 chars on each side).
///
/// UTF-8 safe: adjusts boundaries to valid character positions.
pub fn extract_context(text: &str, start: usize, end: usize) -> String {
    let context_chars = 40;
    let ctx_start = start.saturating_sub(context_chars);
    let ctx_end = (end + context_chars).min(text.len());

    // Adjust to char boundaries (UTF-8 safety)
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

/// Create a consistent progress bar style for multi-step processing.
///
/// # Panics
///
/// Panics if the hardcoded progress bar template is invalid (this cannot
/// happen in practice).
pub fn progress_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template("{msg:>15} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("=> ")
}

/// Download a file from a URL to a local path with streaming and progress bar.
///
/// Downloads in 8KB chunks to avoid loading large files into memory.
/// Shows a progress bar on stderr with bytes downloaded and ETA.
/// Suitable for files of any size (tested with 1GB+ model downloads).
///
/// # Errors
///
/// Returns an error if the HTTP request fails, the server returns a non-success
/// status, or the file cannot be written to disk.
///
/// # Panics
///
/// Panics if the hardcoded progress bar template is invalid (cannot happen in practice).
#[cfg(feature = "download")]
pub fn download_file(
    url: &str,
    output_path: &std::path::Path,
    label: &str,
) -> crate::error::Result<u64> {
    use std::io::Write;

    use indicatif::{ProgressBar, ProgressDrawTarget};
    use reqwest::blocking::Client;

    use crate::error::Error;

    let client = Client::builder()
        .timeout(std::time::Duration::from_hours(1)) // 1 hour for large models
        .build()
        .map_err(|e| Error::Model {
            reason: format!("Failed to create HTTP client: {e}"),
        })?;

    eprintln!("Downloading {label}...");

    let response = client.get(url).send().map_err(|e| Error::Model {
        reason: format!("Failed to connect: {e}"),
    })?;

    if !response.status().is_success() {
        return Err(Error::Model {
            reason: format!("HTTP {}: failed to download {label}", response.status()),
        });
    }

    let total_size = response.content_length().unwrap_or(0);
    let pb = ProgressBar::with_draw_target(Some(total_size), ProgressDrawTarget::stderr());
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("=> "),
    );

    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let mut file = std::fs::File::create(output_path).map_err(|source| Error::Io {
        path: output_path.to_path_buf(),
        source,
    })?;

    // Stream in chunks (don't load entire file into memory)
    let mut response = response;
    let mut downloaded: u64 = 0;
    let mut buffer = [0u8; 8192];

    loop {
        use std::io::Read;
        let bytes_read = response.read(&mut buffer).map_err(|e| Error::Model {
            reason: format!("Download interrupted: {e}"),
        })?;
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buffer[..bytes_read])
            .map_err(|source| Error::Io {
                path: output_path.to_path_buf(),
                source,
            })?;
        downloaded += bytes_read as u64;
        pb.set_position(downloaded);
    }

    pb.finish();
    Ok(downloaded)
}
