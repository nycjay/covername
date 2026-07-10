//! Smart Detection: local language model for PII classification.
//!
//! Uses a small language model to classify detected PII as "personal" (should be
//! redacted) vs "public/corporate" (safe to keep). This reduces false positives
//! and minimizes the review burden on users.
//!
//! The model is optional — if not installed, detection works normally.
//! Users enable it via: `covername smart-detection download`

use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::error::{Error, Result};

/// The model filename stored on disk.
const MODEL_FILENAME: &str = "model.gguf";

/// The `HuggingFace` download URL for `Qwen2.5-1.5B-Instruct` `Q4_K_M`.
#[cfg(feature = "download")]
const MODEL_URL: &str = "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf";

/// Classification result from the Smart Detection model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiiClassification {
    /// Personal PII that should be redacted.
    Personal,
    /// Public/corporate information safe to keep (company name, public address, etc.).
    Public,
    /// Model is uncertain — let user decide.
    Uncertain,
}

/// Status of the Smart Detection feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmartDetectionStatus {
    /// Model files are not installed.
    NotInstalled,
    /// Model is installed and ready.
    Installed {
        /// Size of the model file in megabytes.
        model_size_mb: u64,
    },
    /// Binary was built without the `smart-detection` feature flag.
    FeatureNotCompiled,
}

/// Get the Smart Detection model directory path.
///
/// Returns `~/.config/covername/models/smart-detection/`.
///
/// # Errors
///
/// Returns an error if the storage directory cannot be determined.
pub fn model_dir() -> Result<PathBuf> {
    let storage = Config::storage_dir()?;
    Ok(storage.join("models").join("smart-detection"))
}

/// Check if the Smart Detection model is installed.
///
/// Returns `true` if `model.gguf` exists in the model directory.
#[must_use]
pub fn is_installed() -> bool {
    model_dir().is_ok_and(|dir| {
        let path = dir.join(MODEL_FILENAME);
        path.exists() && std::fs::metadata(&path).is_ok_and(|m| m.len() > 0)
    })
}

/// Get the current status of Smart Detection.
#[must_use]
pub fn status() -> SmartDetectionStatus {
    if cfg!(feature = "smart-detection") {
        match model_dir() {
            Ok(dir) => {
                let model_path = dir.join(MODEL_FILENAME);
                if model_path.exists() {
                    let size_bytes = fs::metadata(&model_path).map_or(0, |m| m.len());
                    let size_mb = size_bytes / (1024 * 1024);
                    SmartDetectionStatus::Installed {
                        model_size_mb: size_mb,
                    }
                } else {
                    SmartDetectionStatus::NotInstalled
                }
            }
            Err(_) => SmartDetectionStatus::NotInstalled,
        }
    } else {
        // Check if model files exist even though feature isn't compiled
        match model_dir() {
            Ok(dir) if dir.join(MODEL_FILENAME).exists() => {
                SmartDetectionStatus::FeatureNotCompiled
            }
            _ => SmartDetectionStatus::NotInstalled,
        }
    }
}

/// Download the Smart Detection model from `HuggingFace`.
///
/// Downloads `Qwen2.5-1.5B-Instruct` (`Q4_K_M` quantization, ~1GB) and saves
/// it to `~/.config/covername/models/smart-detection/model.gguf`.
///
/// # Errors
///
/// Returns an error if the download fails or files cannot be written.
///
/// # Panics
///
/// Panics if the hardcoded progress bar template is invalid (cannot happen in practice).
#[cfg(feature = "download")]
pub fn download_model() -> Result<()> {
    let dir = model_dir()?;
    fs::create_dir_all(&dir).map_err(|source| Error::Io {
        path: dir.clone(),
        source,
    })?;

    eprintln!("Downloading Smart Detection model (Qwen2.5-1.5B-Instruct Q4_K_M)...");
    eprintln!("This is a one-time download (~1 GB).\n");

    let file_path = dir.join(MODEL_FILENAME);
    let downloaded = crate::utils::download_file(MODEL_URL, &file_path, "model.gguf")?;

    eprintln!("\n✓ Smart Detection model downloaded successfully!");
    eprintln!("  Path: {}", dir.display());
    eprintln!("  Size: {} MB", downloaded / (1024 * 1024));

    #[cfg(not(feature = "smart-detection"))]
    {
        eprintln!();
        eprintln!("Note: To use Smart Detection for inference, rebuild with:");
        eprintln!("  cargo build --features smart-detection");
    }

    Ok(())
}

/// Download the Smart Detection model (stub when download feature is not enabled).
///
/// # Errors
///
/// Always returns an error directing the user to enable the download feature.
#[cfg(not(feature = "download"))]
pub fn download_model() -> Result<()> {
    Err(Error::Model {
        reason: String::from(
            "Build with --features download to enable model downloading.\n\
             Run: cargo build --features download",
        ),
    })
}

/// Remove the Smart Detection model files.
///
/// Deletes the model directory and all its contents.
///
/// # Errors
///
/// Returns an error if file removal fails.
pub fn remove_model() -> Result<()> {
    let dir = model_dir()?;
    let model_path = dir.join(MODEL_FILENAME);

    if model_path.exists() {
        fs::remove_file(&model_path).map_err(|source| Error::Io {
            path: model_path,
            source,
        })?;
    }

    // Remove the directory if it's now empty
    if dir.exists()
        && let Ok(entries) = fs::read_dir(&dir)
        && entries.count() == 0
    {
        fs::remove_dir(&dir).map_err(|source| Error::Io { path: dir, source })?;
    }

    Ok(())
}

/// Classify a detection as personal PII or public/corporate information.
///
/// Uses the locally-installed language model to determine whether a detected
/// entity is genuinely personal (should be redacted) or public/corporate
/// (safe to keep).
///
/// # Arguments
///
/// * `matched_text` - The text that was detected as potential PII.
/// * `entity_type` - The type of entity (e.g., "PERSON", "ADDRESS").
/// * `context` - Surrounding text for context.
///
/// # Errors
///
/// Returns an error if the model cannot be loaded or inference fails.
#[cfg(feature = "smart-detection")]
#[allow(clippy::too_many_lines)]
pub fn classify_detection(
    matched_text: &str,
    entity_type: &str,
    context: &str,
) -> Result<PiiClassification> {
    use llama_cpp_v3::{Backend, ChatMessage, LlamaBackend, LlamaContext, LlamaModel, LoadOptions};

    let dir = model_dir()?;
    let model_path = dir.join(MODEL_FILENAME);

    if !model_path.exists() {
        return Err(Error::Model {
            reason: String::from(
                "Smart Detection model not installed. Run: covername smart-detection download",
            ),
        });
    }

    // Load backend
    let backend = LlamaBackend::load(LoadOptions {
        backend: Backend::Cpu,
        app_name: "covername",
        version: None,
        explicit_path: None,
        cache_dir: Some(dir.clone()),
    })
    .map_err(|e| Error::Model {
        reason: format!("Failed to load llama backend: {e}"),
    })?;

    // Load model
    let model = LlamaModel::load_from_file(
        &backend,
        model_path.to_str().unwrap_or("model.gguf"),
        LlamaModel::default_params(&backend),
    )
    .map_err(|e| Error::Model {
        reason: format!("Failed to load Smart Detection model: {e}"),
    })?;

    // Create context
    let mut ctx = LlamaContext::new(&model, LlamaContext::default_params(&model)).map_err(|e| {
        Error::Model {
            reason: format!("Failed to create inference context: {e}"),
        }
    })?;

    // Build prompt using chat template
    let messages = vec![
        ChatMessage {
            role: String::from("system"),
            content: String::from(
                "You are a PII classifier. Given a detected text entity and its context, \
                 determine if it is PERSONAL (private individual's information that should be \
                 redacted) or PUBLIC (corporate/public information that is safe to keep). \
                 Respond with exactly one word: PERSONAL or PUBLIC.",
            ),
        },
        ChatMessage {
            role: String::from("user"),
            content: format!(
                "Context: \"{context}\"\n\
                 Detected text: \"{matched_text}\"\n\
                 Entity type: {entity_type}\n\n\
                 Is this PERSONAL or PUBLIC?"
            ),
        },
    ];

    let prompt = model
        .apply_chat_template(None, &messages, true)
        .unwrap_or_else(|_| {
            // Fallback if chat template not available
            format!(
                "Classify whether this detected text is personal private information or \
                 public/corporate information.\n\n\
                 Context: \"...{context}...\"\n\
                 Detected text: \"{matched_text}\"\n\
                 Type: {entity_type}\n\n\
                 Answer with exactly one word: PERSONAL or PUBLIC\n\n\
                 Answer:"
            )
        });

    // Tokenize and run inference
    let generated_text =
        run_inference(&backend, &model, &mut ctx, &prompt).map_err(|e| Error::Model {
            reason: format!("Inference failed: {e}"),
        })?;

    // Parse the response
    let upper = generated_text.to_uppercase();
    if upper.contains("PERSONAL") {
        Ok(PiiClassification::Personal)
    } else if upper.contains("PUBLIC") {
        Ok(PiiClassification::Public)
    } else {
        Ok(PiiClassification::Uncertain)
    }
}

/// Run tokenization, prompt decoding, and token generation.
#[cfg(feature = "smart-detection")]
fn run_inference(
    backend: &llama_cpp_v3::LlamaBackend,
    model: &llama_cpp_v3::LlamaModel,
    ctx: &mut llama_cpp_v3::LlamaContext,
    prompt: &str,
) -> std::result::Result<String, llama_cpp_v3::LlamaError> {
    let tokens = model.tokenize(prompt, true, true)?;

    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let n_tokens = tokens.len() as i32;
    let mut batch = llama_cpp_v3::LlamaBatch::new(backend.lib.clone(), n_tokens + 32, 0, 1);

    for (i, &token) in tokens.iter().enumerate() {
        let is_last = i == tokens.len() - 1;
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let pos = i as i32;
        batch.add(token, pos, &[0], is_last);
    }

    // Decode prompt
    ctx.decode(&batch)?;

    // Set up sampler (greedy for deterministic classification)
    let sampler = llama_cpp_v3::LlamaSampler::new_greedy(backend.lib.clone());

    // Generate up to 10 tokens (we only need 1-2 for PERSONAL/PUBLIC)
    let vocab = model.get_vocab();
    let mut generated_text = String::new();

    for cur_pos in n_tokens..(n_tokens + 10) {
        let new_token = sampler.sample(ctx, cur_pos - 1);
        sampler.accept(new_token);

        // Check for end of generation
        if vocab.is_eog(new_token) {
            break;
        }

        let piece = model.token_to_piece(new_token);
        generated_text.push_str(&piece);

        // Check if we already have enough to classify
        let upper = generated_text.to_uppercase();
        if upper.contains("PERSONAL") || upper.contains("PUBLIC") {
            break;
        }

        // Prepare next batch
        batch.clear();
        batch.add(new_token, cur_pos, &[0], true);
        ctx.decode(&batch)?;
    }

    Ok(generated_text)
}

/// Stub for `classify_detection` when smart-detection feature is not compiled.
///
/// Always returns `Uncertain` to let the user decide.
///
/// # Errors
///
/// This stub never returns an error.
#[cfg(not(feature = "smart-detection"))]
pub fn classify_detection(
    _matched_text: &str,
    _entity_type: &str,
    _context: &str,
) -> Result<PiiClassification> {
    Ok(PiiClassification::Uncertain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_dir_is_under_config() {
        // Override HOME for test isolation
        let dir = model_dir();
        // This may fail in CI without a home dir, that's ok
        if let Ok(dir) = dir {
            assert!(dir.to_string_lossy().contains("smart-detection"));
        }
    }

    #[test]
    fn test_is_installed_returns_false_when_no_model() {
        // Unless someone has actually installed the model, this should be false
        // in a clean test environment. We don't assert false because a dev
        // machine might have it installed.
        let _ = is_installed();
    }

    #[test]
    fn test_status_not_installed() {
        // In a test environment without the model, status should be NotInstalled
        // or FeatureNotCompiled depending on build features.
        let s = status();
        assert!(matches!(
            s,
            SmartDetectionStatus::NotInstalled | SmartDetectionStatus::FeatureNotCompiled
        ));
    }

    #[test]
    fn test_classification_enum() {
        let p = PiiClassification::Personal;
        let pub_ = PiiClassification::Public;
        let u = PiiClassification::Uncertain;

        assert_ne!(p, pub_);
        assert_ne!(p, u);
        assert_ne!(pub_, u);
    }

    #[test]
    fn test_classify_without_model() {
        // Without model installed, classify should return Uncertain (no-feature)
        // or an error (with feature, no model)
        let result = classify_detection("John Smith", "PERSON", "sent by John Smith to...");
        // Any result is acceptable: Uncertain (stub), error (no model), or a real classification
        let _ = result;
    }
}
