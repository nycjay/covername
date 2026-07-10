//! Covername Tauri desktop app.
//!
//! This is the thin bridge between the Svelte frontend and `covername-core`.
//! Each `#[tauri::command]` exposes a core function to the UI via IPC.
//!
//! All document processing goes through `covername_core::pipeline` to ensure
//! CLI and GUI always behave identically.

use std::path::PathBuf;

use covername_core::config::Config;
use covername_core::mapping::MappingStore;
use covername_core::output;
use covername_core::pipeline;
use covername_core::replacement;
use serde::Serialize;
use tauri::{Emitter, Manager};

/// A detection result sent to the frontend.
#[derive(Serialize, Clone)]
struct UiDetection {
    matched_text: String,
    entity_type: String,
    replacement: String,
    start: usize,
    end: usize,
    context: String,
}

/// A replacement instruction from the frontend, including position info.
#[derive(serde::Deserialize)]
struct UiReplacement {
    original: String,
    replacement: String,
    start: usize,
    end: usize,
}

/// Result of scanning a file.
#[derive(Serialize)]
struct UiScanResult {
    text: String,
    detections: Vec<UiDetection>,
}

/// Progress event payload sent to the frontend.
#[derive(Serialize, Clone)]
struct ProgressEvent {
    /// What phase: "scan", "generate"
    phase: String,
    /// Current step (e.g., page number)
    current: u64,
    /// Total steps
    total: u64,
    /// Human-readable message
    message: String,
}

/// Scan a file for PII and return detections with suggested replacements.
///
/// Uses the unified pipeline from covername-core (same logic as CLI).
/// Converts byte offsets from Rust to char offsets for the JavaScript frontend.
#[tauri::command]
async fn scan_file(app: tauri::AppHandle, path: String) -> Result<UiScanResult, String> {
    let result = tokio::task::spawn_blocking(move || {
        let path = PathBuf::from(&path);

        // Security: validate the path before processing
        validate_path(&path)?;

        // Emit scan start
        let _ = app.emit("progress", ProgressEvent {
            phase: "scan".into(),
            current: 0,
            total: 0,
            message: "Scanning for personal information…".into(),
        });

        let result = pipeline::scan_file(&path).map_err(|e| e.to_string())?;

        // Build a byte-to-char offset map for the extracted text.
        // JavaScript uses UTF-16 char indices for string.slice(), but Rust
        // detection positions are byte offsets into the UTF-8 string.
        // Build a byte-to-UTF16 offset map.
        // JavaScript's string.slice() uses UTF-16 code units, not Unicode codepoints.
        // Characters outside the BMP (emoji, etc.) are 2 UTF-16 units but 1 Rust char.
        let byte_to_char: Vec<usize> = {
            let mut map = vec![0usize; result.text.len() + 1];
            let mut utf16_idx = 0;
            for (byte_idx, c) in result.text.char_indices() {
                map[byte_idx] = utf16_idx;
                utf16_idx += c.len_utf16();
            }
            map[result.text.len()] = utf16_idx;
            map
        };

        // Resolve replacements for each detection
        let mapping_store = load_mapping_store();
        let ui_detections: Vec<UiDetection> = result
            .detections
            .into_iter()
            .map(|d| {
                let replacement_text = mapping_store
                    .as_ref()
                    .ok()
                    .and_then(|store| store.find(&d.matched_text))
                    .map(|m| m.replacement.clone())
                    .unwrap_or_else(|| {
                        replacement::suggest_replacement(&d.matched_text, &d.entity_type)
                    });

                // Convert byte offsets to char offsets for the frontend
                let char_start = byte_to_char.get(d.start).copied().unwrap_or(d.start);
                let char_end = byte_to_char.get(d.end).copied().unwrap_or(d.end);

                UiDetection {
                    matched_text: d.matched_text,
                    entity_type: d.entity_type,
                    replacement: replacement_text,
                    start: char_start,
                    end: char_end,
                    context: d.context,
                }
            })
            .collect();

        Ok(UiScanResult {
            text: result.text,
            detections: ui_detections,
        })
    })
    .await
    .map_err(|e| format!("task failed: {e}"))?;

    result
}

/// Generate an output file with replacements applied.
///
/// Uses the same output logic as the CLI: position-aware PDF redaction for PDFs,
/// XLSX replacement for spreadsheets, and text replacement for everything else.
#[tauri::command]
async fn generate_output(
    app: tauri::AppHandle,
    path: String,
    replacements: Vec<serde_json::Value>,
) -> Result<String, String> {
    let result = tokio::task::spawn_blocking(move || {
        use covername_core::detection::Detection;
        use covername_core::document::{DocumentType, detect_file_type};
        use covername_core::processor::{self, ResolvedDetection};

        let input_path = PathBuf::from(&path);

        // Security: validate the path before processing
        validate_path(&input_path)?;

        // Load config for output path
        let config = Config::load(
            &Config::config_path().map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        let output_path = output::resolve_output_path(&input_path, &config);
        let file_type = detect_file_type(&input_path);

        // Parse replacements from the frontend — fail if any are malformed
        let ui_replacements: Vec<UiReplacement> = replacements
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                serde_json::from_value(r)
                    .map_err(|e| format!("Invalid replacement at index {i}: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        if ui_replacements.is_empty() {
            return Ok("No replacements to apply — original file unchanged".into());
        }

        // Build (original, replacement) pairs — shared by PDF and XLSX paths
        let pairs: Vec<(String, String)> = ui_replacements
            .iter()
            .map(|r| (r.original.clone(), r.replacement.clone()))
            .collect();

        // Route to the appropriate output method based on file type
        if file_type == DocumentType::Pdf && covername_core::ocr::is_ocr_pipeline_available() {
            // Emit progress start
            let app_handle = app.clone();
            let _ = app.emit("progress", ProgressEvent {
                phase: "generate".into(),
                current: 0,
                total: 0,
                message: "Generating redacted PDF…".into(),
            });

            pipeline::write_redacted_pdf_with_progress(
                &input_path,
                &pairs,
                &output_path,
                &|current, total| {
                    let _ = app_handle.emit("progress", ProgressEvent {
                        phase: "generate".into(),
                        current,
                        total,
                        message: format!("Processing page {current} of {total}…"),
                    });
                },
            )
            .map_err(|e| e.to_string())?;
        } else if file_type == DocumentType::Xlsx {
            covername_core::xlsx::write_xlsx(&input_path, &pairs, &output_path)
                .map_err(|e| e.to_string())?;
        } else {
            // Text-based replacement using positions
            let text = pipeline::extract_text(&input_path).map_err(|e| e.to_string())?;

            // Build UTF16-to-byte offset map (reverse of scan_file's map).
            // Index by UTF-16 code unit position, value is the byte offset.
            let utf16_to_byte: Vec<usize> = {
                let mut map: Vec<usize> = Vec::new();
                for (byte_idx, c) in text.char_indices() {
                    for _ in 0..c.len_utf16() {
                        map.push(byte_idx);
                    }
                }
                map.push(text.len()); // one past the end
                map
            };

            let resolved: Vec<ResolvedDetection> = ui_replacements
                .iter()
                .map(|r| {
                    let byte_start = utf16_to_byte.get(r.start).copied().unwrap_or(r.start);
                    let byte_end = utf16_to_byte.get(r.end).copied().unwrap_or(r.end);

                    ResolvedDetection {
                        detection: Detection {
                            matched_text: r.original.clone(),
                            entity_type: String::from("USER"),
                            rule_name: String::from("ui"),
                            start: byte_start,
                            end: byte_end,
                            context: String::new(),
                        },
                        replacement: r.replacement.clone(),
                        accepted: true,
                    }
                })
                .collect();

            let result_text = processor::apply_replacements(&text, &resolved);
            output::write_output(&result_text, &input_path, &output_path)
                .map_err(|e| e.to_string())?;
        }

        // Save mappings for consistency across documents
        if let Ok(mut store) = load_mapping_store() {
            for r in &ui_replacements {
                let _ = store.add(&r.original, &r.replacement, "USER");
            }
        }

        // Emit completion event
        let _ = app.emit("progress", ProgressEvent {
            phase: "complete".into(),
            current: 1,
            total: 1,
            message: format!(
                "Done! Saved as {}",
                output_path.file_name().map(|f| f.to_string_lossy()).unwrap_or_default()
            ),
        });

        Ok(output_path.display().to_string())
    })
    .await
    .map_err(|e| format!("task failed: {e}"))?;

    result
}

/// Result of processing a single file in a batch.
#[derive(Serialize, Clone)]
struct BatchFileResult {
    path: String,
    status: String, // "success", "error", "skipped"
    detections: usize,
    output_path: Option<String>,
    error: Option<String>,
}

/// Process multiple files in batch mode.
///
/// Scans each file, applies existing mappings automatically, and generates
/// output files. Emits progress events per file. New detections without
/// existing mappings get auto-generated replacements.
#[tauri::command]
async fn batch_process(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<Vec<BatchFileResult>, String> {
    let result = tokio::task::spawn_blocking(move || {
        let total = paths.len() as u64;
        let mut results: Vec<BatchFileResult> = Vec::new();

        let config = Config::load(
            &Config::config_path().map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        // Load mapping store once — shared across all files for consistency
        let mut mapping_store = load_mapping_store().unwrap_or_else(|_| MappingStore::empty());

        for (i, path_str) in paths.iter().enumerate() {
            let file_path = PathBuf::from(path_str);
            let filename = file_path.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| path_str.clone());

            // Emit progress
            let _ = app.emit("progress", ProgressEvent {
                phase: "batch".into(),
                current: (i as u64) + 1,
                total,
                message: format!("Processing {} ({}/{})", filename, i + 1, total),
            });

            // Validate path
            if let Err(e) = validate_path(&file_path) {
                results.push(BatchFileResult {
                    path: path_str.clone(),
                    status: "error".into(),
                    detections: 0,
                    output_path: None,
                    error: Some(e),
                });
                continue;
            }

            // Scan
            let scan_result = match pipeline::scan_file(&file_path) {
                Ok(r) => r,
                Err(e) => {
                    results.push(BatchFileResult {
                        path: path_str.clone(),
                        status: "error".into(),
                        detections: 0,
                        output_path: None,
                        error: Some(e.to_string()),
                    });
                    continue;
                }
            };

            let detection_count = scan_result.detections.len();

            if detection_count == 0 {
                results.push(BatchFileResult {
                    path: path_str.clone(),
                    status: "skipped".into(),
                    detections: 0,
                    output_path: None,
                    error: None,
                });
                continue;
            }

            // Resolve replacements using existing mappings or generate new ones
            let replacements: Vec<(String, String)> = scan_result
                .detections
                .iter()
                .map(|d| {
                    let rep = mapping_store
                        .find(&d.matched_text)
                        .map(|m| m.replacement.clone())
                        .unwrap_or_else(|| {
                            let r = replacement::suggest_replacement(
                                &d.matched_text,
                                &d.entity_type,
                            );
                            let _ = mapping_store.add(&d.matched_text, &r, &d.entity_type);
                            r
                        });
                    (d.matched_text.clone(), rep)
                })
                .collect();

            // Generate output
            let output_path = output::resolve_output_path(&file_path, &config);
            let file_type = covername_core::document::detect_file_type(&file_path);

            let gen_result = if file_type == covername_core::document::DocumentType::Pdf
                && covername_core::ocr::is_ocr_pipeline_available()
            {
                pipeline::write_redacted_pdf(&file_path, &replacements, &output_path)
            } else if file_type == covername_core::document::DocumentType::Xlsx {
                covername_core::xlsx::write_xlsx(&file_path, &replacements, &output_path)
            } else {
                // Text-based replacement
                let text = &scan_result.text;
                let resolved: Vec<covername_core::processor::ResolvedDetection> = scan_result
                    .detections
                    .iter()
                    .map(|d| {
                        let rep = replacements
                            .iter()
                            .find(|(orig, _)| orig == &d.matched_text)
                            .map(|(_, r)| r.clone())
                            .unwrap_or_default();
                        covername_core::processor::ResolvedDetection {
                            detection: d.clone(),
                            replacement: rep,
                            accepted: true,
                        }
                    })
                    .collect();
                let result_text = covername_core::processor::apply_replacements(text, &resolved);
                output::write_output(&result_text, &file_path, &output_path)
            };

            match gen_result {
                Ok(()) => {
                    results.push(BatchFileResult {
                        path: path_str.clone(),
                        status: "success".into(),
                        detections: detection_count,
                        output_path: Some(output_path.display().to_string()),
                        error: None,
                    });
                }
                Err(e) => {
                    results.push(BatchFileResult {
                        path: path_str.clone(),
                        status: "error".into(),
                        detections: detection_count,
                        output_path: None,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        // Emit completion
        let success_count = results.iter().filter(|r| r.status == "success").count();
        let _ = app.emit("progress", ProgressEvent {
            phase: "complete".into(),
            current: 1,
            total: 1,
            message: format!("Done! {} of {} documents processed", success_count, total),
        });

        Ok(results)
    })
    .await
    .map_err(|e| format!("task failed: {e}"))?;

    result
}

/// List files in a directory that have supported extensions.
#[tauri::command]
fn list_supported_files(path: String) -> Result<Vec<String>, String> {
    use covername_core::document::collect_files;

    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {path}"));
    }

    let files = collect_files(&dir, true).map_err(|e| e.to_string())?;
    let mut result: Vec<String> = files.iter().map(|p| p.display().to_string()).collect();
    result.sort();
    Ok(result)
}

/// Return the current configuration.
#[tauri::command]
fn get_config() -> Result<serde_json::Value, String> {
    let config_path = Config::config_path().map_err(|e| e.to_string())?;
    let config = Config::load(&config_path).map_err(|e| e.to_string())?;
    serde_json::to_value(&config).map_err(|e| e.to_string())
}

/// Return all stored mappings.
#[tauri::command]
fn get_mappings() -> Result<serde_json::Value, String> {
    let store = load_mapping_store().map_err(|e| e.to_string())?;
    let mappings: Vec<serde_json::Value> = store
        .list()
        .iter()
        .map(|m| {
            serde_json::json!({
                "original": m.original,
                "replacement": m.replacement,
                "entity_type": m.entity_type,
                "last_used": m.last_used.format("%Y-%m-%d %H:%M").to_string()
            })
        })
        .collect();
    Ok(serde_json::Value::Array(mappings))
}

/// Return storage usage breakdown for the config directory.
#[tauri::command]
fn get_storage_usage() -> Result<serde_json::Value, String> {
    let storage_dir = Config::ensure_storage_dir().map_err(|e| e.to_string())?;

    let mut config_size: u64 = 0;
    let mut models_size: u64 = 0;
    let mut logs_size: u64 = 0;

    if storage_dir.exists() {
        for entry in walkdir(&storage_dir) {
            let path = entry.path();
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

            if path.starts_with(storage_dir.join("models")) {
                models_size += size;
            } else if path.starts_with(storage_dir.join("logs")) {
                logs_size += size;
            } else {
                config_size += size;
            }
        }
    }

    let total = config_size + models_size + logs_size;

    Ok(serde_json::json!({
        "path": storage_dir.display().to_string(),
        "config_bytes": config_size,
        "models_bytes": models_size,
        "logs_bytes": logs_size,
        "total_bytes": total,
    }))
}

/// Walk a directory and yield all file entries.
fn walkdir(dir: &std::path::Path) -> Vec<std::fs::DirEntry> {
    let mut entries = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                entries.extend(walkdir(&path));
            } else {
                entries.push(entry);
            }
        }
    }
    entries
}

/// Return app version info to the frontend.
#[tauri::command]
fn get_app_info() -> serde_json::Value {
    let version = env!("CARGO_PKG_VERSION");
    let git_hash = option_env!("COVERNAME_GIT_HASH").unwrap_or("unknown");
    let is_dev = cfg!(debug_assertions);

    serde_json::json!({
        "version": version,
        "git_hash": git_hash,
        "is_dev": is_dev
    })
}

/// Check if this is the first time the app has been launched.
///
/// Returns true if the config file doesn't exist yet.
#[tauri::command]
fn is_first_run() -> bool {
    Config::config_path()
        .map(|p| !p.exists())
        .unwrap_or(true)
}

/// Mark onboarding as complete by creating the initial config file.
#[tauri::command]
fn complete_onboarding() -> Result<(), String> {
    let config_path = Config::config_path().map_err(|e| e.to_string())?;
    if !config_path.exists() {
        let config = Config::default();
        config.save(&config_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Uninstall Covername: remove config/data, optionally models, and move .app to Trash.
///
/// Returns a summary of what was removed.
#[tauri::command]
fn uninstall(remove_models: bool) -> Result<String, String> {
    let mut removed: Vec<String> = Vec::new();

    // Remove config directory (~/.config/covername/)
    let config_dir = Config::ensure_storage_dir().map_err(|e| e.to_string())?;
    if remove_models {
        // Remove everything
        if config_dir.exists() {
            std::fs::remove_dir_all(&config_dir).map_err(|e| e.to_string())?;
            removed.push(format!("Removed {}", config_dir.display()));
        }
    } else {
        // Keep models, remove config/mappings/ignore list only
        let files_to_remove = ["config.json", "mappings.json", "ignore-list.json", "custom-rules.json"];
        for filename in &files_to_remove {
            let path = config_dir.join(filename);
            if path.exists() {
                let _ = std::fs::remove_file(&path);
            }
        }
        // Remove logs directory
        let logs_dir = config_dir.join("logs");
        if logs_dir.exists() {
            let _ = std::fs::remove_dir_all(&logs_dir);
        }
        removed.push("Removed configuration and mappings".into());
        removed.push(format!("Kept models in {}", config_dir.display()));
    }

    // Move .app to Trash using macOS `trash` command
    if let Ok(exe_path) = std::env::current_exe() {
        // The exe is inside Covername.app/Contents/MacOS/covername-tauri
        // Walk up to find the .app bundle
        if let Some(app_bundle) = exe_path
            .ancestors()
            .find(|p| p.extension().is_some_and(|ext| ext == "app"))
        {
            let app_path = app_bundle.to_path_buf();
            // Use AppleScript to move to Trash (works without elevated permissions)
            let script = format!(
                "tell application \"Finder\" to delete POSIX file \"{}\"",
                app_path.display()
            );
            let _ = std::process::Command::new("osascript")
                .args(["-e", &script])
                .output();
            removed.push(format!("Moved {} to Trash", app_path.display()));
        }
    }

    Ok(removed.join("\n"))
}

/// Reveal a file in Finder (macOS).
#[tauri::command]
fn reveal_in_finder(path: String) -> Result<(), String> {
    std::process::Command::new("open")
        .args(["-R", &path])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Gather debug information into a zip file on the Desktop.
///
/// Includes: system info, config, log files, model status.
/// Excludes: mappings (may contain PII), actual document content.
#[tauri::command]
fn gather_debug_logs() -> Result<String, String> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let storage_dir = Config::ensure_storage_dir().map_err(|e| e.to_string())?;

    // Output to Desktop
    let desktop = dirs::desktop_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    let zip_path = desktop.join("covername-debug.zip");

    let file = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default();

    // 1. System info
    let version = env!("CARGO_PKG_VERSION");
    let sys_info = format!(
        "covername_version: {}\n\
         os: {}\n\
         arch: {}\n\
         timestamp: {}\n",
        version,
        std::env::consts::OS,
        std::env::consts::ARCH,
        chrono::Utc::now().to_rfc3339(),
    );
    zip.start_file("system-info.txt", options).map_err(|e| e.to_string())?;
    zip.write_all(sys_info.as_bytes()).map_err(|e| e.to_string())?;

    // 2. Config file (if exists)
    let config_path = storage_dir.join("config.json");
    if config_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&config_path) {
            zip.start_file("config.json", options).map_err(|e| e.to_string())?;
            zip.write_all(contents.as_bytes()).map_err(|e| e.to_string())?;
        }
    }

    // 3. Log files
    let logs_dir = storage_dir.join("logs");
    if logs_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&logs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let name = format!("logs/{}", path.file_name().unwrap_or_default().to_string_lossy());
                    if let Ok(contents) = std::fs::read(&path) {
                        let _ = zip.start_file(&name, options);
                        let _ = zip.write_all(&contents);
                    }
                }
            }
        }
    }

    // 4. Model status
    let models_dir = storage_dir.join("models");
    let mut model_info = String::from("Installed models:\n");
    if models_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&models_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let size = path.metadata().map(|m| m.len()).unwrap_or(0);
                model_info.push_str(&format!(
                    "  {} ({} bytes)\n",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    size
                ));
            }
        }
    } else {
        model_info.push_str("  (none)\n");
    }
    zip.start_file("models.txt", options).map_err(|e| e.to_string())?;
    zip.write_all(model_info.as_bytes()).map_err(|e| e.to_string())?;

    // 5. Ignore list (non-sensitive — just entity names the user chose to skip)
    let ignore_path = storage_dir.join("ignore-list.json");
    if ignore_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&ignore_path) {
            zip.start_file("ignore-list.json", options).map_err(|e| e.to_string())?;
            zip.write_all(contents.as_bytes()).map_err(|e| e.to_string())?;
        }
    }

    zip.finish().map_err(|e| e.to_string())?;

    Ok(zip_path.display().to_string())
}

/// Validate that a file path is safe to process.
///
/// Checks:
/// - Path exists
/// - Path is a regular file (not a directory, symlink to sensitive location, etc.)
/// - Has a supported file extension
fn validate_path(path: &std::path::Path) -> Result<(), String> {
    use covername_core::document::detect_file_type;

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    if !path.is_file() {
        return Err(format!("Not a regular file: {}", path.display()));
    }

    // Canonicalize to resolve symlinks and prevent traversal
    let canonical = path.canonicalize().map_err(|e| {
        format!("Cannot resolve path {}: {e}", path.display())
    })?;

    // Reject paths that resolve to sensitive system directories
    let canonical_str = canonical.to_string_lossy();
    let blocked_prefixes = ["/etc", "/usr", "/System", "/Library/Preferences", "/Library/LaunchDaemons"];
    for prefix in &blocked_prefixes {
        if canonical_str.starts_with(prefix) {
            return Err(format!("Access denied: {}", path.display()));
        }
    }

    // Validate supported file type
    let file_type = detect_file_type(path);
    if file_type == covername_core::document::DocumentType::Text {
        // Text is the default fallback — check extension is actually supported
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !ext.is_empty() && !covername_core::document::is_supported_extension(ext) {
            return Err(format!("Unsupported file type: .{ext}"));
        }
    }

    Ok(())
}

fn load_mapping_store() -> covername_core::error::Result<MappingStore> {
    let path = Config::mappings_path()?;
    MappingStore::load(&path)
}

fn main() {
    let version = env!("CARGO_PKG_VERSION");
    let git_hash = option_env!("COVERNAME_GIT_HASH").unwrap_or("unknown");

    let title = if cfg!(debug_assertions) {
        format!("Covername (Dev) — v{version} ({git_hash})")
    } else {
        String::from("Covername")
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            scan_file,
            generate_output,
            batch_process,
            list_supported_files,
            uninstall,
            is_first_run,
            complete_onboarding,
            gather_debug_logs,
            reveal_in_finder,
            get_app_info,
            get_config,
            get_mappings,
            get_storage_usage
        ])
        .setup(move |app| {
            use tauri::menu::{MenuBuilder, SubmenuBuilder, MenuItemBuilder, PredefinedMenuItem};

            // Initialize file logging
            {
                use tracing_appender::rolling;
                use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

                let log_dir = dirs::home_dir()
                    .unwrap_or_default()
                    .join(".config/covername/logs");
                let file_appender = rolling::daily(&log_dir, "covername.log");

                let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    if cfg!(debug_assertions) {
                        EnvFilter::new("debug")
                    } else {
                        EnvFilter::new("info")
                    }
                });

                let _ = tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt::layer().with_writer(file_appender).with_ansi(false))
                    .try_init();
            }

            tracing::info!("covername desktop app starting");

            if let Some(window) = app.webview_windows().values().next() {
                let _ = window.set_title(&title);
            }

            // Build native macOS menu
            let about = MenuItemBuilder::with_id("about", "About Covername").build(app)?;
            let check_update = MenuItemBuilder::with_id("check_update", "Check for Updates").build(app)?;
            let debug_logs = MenuItemBuilder::with_id("debug_logs", "Gather Debug Logs").build(app)?;
            let uninstall_item = MenuItemBuilder::with_id("uninstall", "Uninstall Covername").build(app)?;

            let app_menu = SubmenuBuilder::new(app, "Covername")
                .item(&about)
                .separator()
                .item(&PredefinedMenuItem::hide(app, None)?)
                .item(&PredefinedMenuItem::hide_others(app, None)?)
                .item(&PredefinedMenuItem::show_all(app, None)?)
                .separator()
                .item(&PredefinedMenuItem::quit(app, None)?)
                .build()?;

            let file_menu = SubmenuBuilder::new(app, "File")
                .item(&MenuItemBuilder::with_id("open", "Open…")
                    .accelerator("CmdOrCtrl+O")
                    .build(app)?)
                .item(&PredefinedMenuItem::close_window(app, None)?)
                .build()?;

            let edit_menu = SubmenuBuilder::new(app, "Edit")
                .item(&PredefinedMenuItem::undo(app, None)?)
                .item(&PredefinedMenuItem::redo(app, None)?)
                .separator()
                .item(&PredefinedMenuItem::cut(app, None)?)
                .item(&PredefinedMenuItem::copy(app, None)?)
                .item(&PredefinedMenuItem::paste(app, None)?)
                .item(&PredefinedMenuItem::select_all(app, None)?)
                .build()?;

            let help_menu = SubmenuBuilder::new(app, "Help")
                .item(&about)
                .item(&check_update)
                .item(&debug_logs)
                .separator()
                .item(&uninstall_item)
                .build()?;

            let menu = MenuBuilder::new(app)
                .item(&app_menu)
                .item(&file_menu)
                .item(&edit_menu)
                .item(&help_menu)
                .build()?;

            app.set_menu(menu)?;

            // Handle menu events
            app.on_menu_event(move |app_handle, event| {
                match event.id().as_ref() {
                    "about" => {
                        let _ = app_handle.emit("menu-event", "about");
                    }
                    "check_update" => {
                        let _ = app_handle.emit("menu-event", "check_update");
                    }
                    "uninstall" => {
                        let _ = app_handle.emit("menu-event", "uninstall");
                    }
                    "debug_logs" => {
                        let _ = app_handle.emit("menu-event", "debug_logs");
                    }
                    "open" => {
                        let _ = app_handle.emit("menu-event", "open");
                    }
                    _ => {}
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Covername");
}
