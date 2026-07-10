//! Model management infrastructure for NER detectors.
//!
//! Provides status checking, path management, and download guidance for NER
//! model files. The dictionary detector requires no model files, but the
//! ONNX detector needs `model.onnx`, `tokenizer.json`, and `labels.json`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The current status of the NER model installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelStatus {
    /// No ONNX model is installed; using dictionary detector only.
    /// Reserved for future use when explicit "dictionary only" state is needed.
    #[allow(dead_code)]
    DictionaryOnly,
    /// An ONNX model is installed and ready.
    OnnxInstalled {
        /// The installed model version.
        version: String,
        /// Path to the model directory.
        path: PathBuf,
    },
    /// No model is installed at all. Reserved for future use.
    #[allow(dead_code)]
    NotInstalled,
    /// A model is installed and ready (generic).
    Installed {
        /// The installed model version.
        version: String,
        /// Path to the model directory.
        path: PathBuf,
    },
    /// An update is available for the installed model.
    UpdateAvailable {
        /// Currently installed version.
        current: String,
        /// Latest available version.
        latest: String,
    },
}

/// Manages NER model storage, installation status, and paths.
///
/// For the dictionary-based detector, the model is always available since
/// it requires no external files. When the ONNX feature is enabled and
/// model files are present, the ONNX detector takes precedence.
pub struct ModelManager {
    storage_dir: PathBuf,
}

impl ModelManager {
    /// Create a new model manager with the given storage directory.
    #[must_use]
    pub fn new(storage_dir: &Path) -> Self {
        Self {
            storage_dir: storage_dir.to_path_buf(),
        }
    }

    /// Check the current model installation status.
    ///
    /// Reports whether the ONNX model is installed, or if only the
    /// dictionary-based detector is available.
    #[must_use]
    pub fn status(&self) -> ModelStatus {
        if self.is_onnx_installed() {
            let version = self
                .read_manifest_version()
                .unwrap_or_else(|| String::from("unknown"));
            ModelStatus::OnnxInstalled {
                version,
                path: self.model_dir(),
            }
        } else {
            ModelStatus::Installed {
                version: String::from("1.0.0-dictionary"),
                path: self.model_dir(),
            }
        }
    }

    /// Return the path where models are stored.
    #[must_use]
    pub fn model_dir(&self) -> PathBuf {
        self.storage_dir.join("models")
    }

    /// Check whether the ONNX model files are installed.
    ///
    /// Requires `model.onnx` to exist in the model directory.
    #[must_use]
    pub fn is_onnx_installed(&self) -> bool {
        self.model_dir().join("model.onnx").exists()
    }

    /// Check whether any model is installed and ready (always true for dictionary).
    #[must_use]
    pub fn is_installed(&self) -> bool {
        // Dictionary detector is always available
        true
    }

    /// Download the ONNX model from `HuggingFace`.
    ///
    /// Downloads `model.onnx`, `tokenizer.json`, and `config.json` from
    /// the `barflyman/bert-pii-detect-onnx` repository, then generates
    /// `labels.json` from the config's `id2label` mapping.
    ///
    /// # Errors
    ///
    /// Returns an error if the download fails or files cannot be written.
    ///
    /// # Panics
    ///
    /// Panics if the hardcoded progress bar template is invalid (this cannot
    /// happen in practice).
    #[cfg(feature = "download")]
    pub fn download_model(&self) -> Result<()> {
        let model_dir = self.model_dir();
        fs::create_dir_all(&model_dir).map_err(|source| Error::Io {
            path: model_dir.clone(),
            source,
        })?;

        let base_url = "https://huggingface.co/barflyman/bert-pii-detect-onnx/resolve/main";

        let files = [
            (format!("{base_url}/onnx/model.onnx"), "model.onnx"),
            (format!("{base_url}/tokenizer.json"), "tokenizer.json"),
            (format!("{base_url}/config.json"), "config.json"),
        ];

        for (url, filename) in &files {
            let file_path = model_dir.join(filename);
            crate::utils::download_file(url, &file_path, filename)?;
        }

        // Generate labels.json from config.json
        Self::generate_labels_from_config(&model_dir)?;

        // Write manifest
        let manifest = serde_json::json!({
            "version": "1.0.0",
            "type": "onnx",
            "model": "barflyman/bert-pii-detect-onnx",
            "source": "https://huggingface.co/barflyman/bert-pii-detect-onnx"
        });
        let manifest_path = model_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(&manifest_path, manifest_json).map_err(|source| Error::Io {
            path: manifest_path,
            source,
        })?;

        eprintln!("\nModel downloaded successfully!");
        eprintln!("Model directory: {}", model_dir.display());
        eprintln!(
            "\nTo use the ONNX model for inference, rebuild with: cargo build --features onnx"
        );

        Ok(())
    }

    /// Download the ONNX model (stub when download feature is not enabled).
    ///
    /// # Errors
    ///
    /// Always returns an error directing the user to enable the download feature.
    #[cfg(not(feature = "download"))]
    pub fn download_model(&self) -> Result<()> {
        Err(Error::Model {
            reason: String::from("Build with --features download to enable model downloading."),
        })
    }

    /// Generate `labels.json` from the model's `config.json` id2label mapping.
    #[cfg(feature = "download")]
    fn generate_labels_from_config(model_dir: &Path) -> Result<()> {
        let config_path = model_dir.join("config.json");
        let config_contents = fs::read_to_string(&config_path).map_err(|source| Error::Io {
            path: config_path,
            source,
        })?;
        let config: serde_json::Value = serde_json::from_str(&config_contents)?;

        if let Some(id2label) = config.get("id2label").and_then(|v| v.as_object()) {
            let mut labels: Vec<(usize, String)> = id2label
                .iter()
                .filter_map(|(k, v)| {
                    let idx = k.parse::<usize>().ok()?;
                    let label = v.as_str()?.to_string();
                    Some((idx, label))
                })
                .collect();
            labels.sort_by_key(|(idx, _)| *idx);
            let label_list: Vec<String> = labels.into_iter().map(|(_, label)| label).collect();

            let labels_path = model_dir.join("labels.json");
            let labels_json = serde_json::to_string_pretty(&label_list)?;
            fs::write(&labels_path, &labels_json).map_err(|source| Error::Io {
                path: labels_path,
                source,
            })?;
        }

        Ok(())
    }

    /// Remove the ONNX model files from the model directory.
    ///
    /// Removes `model.onnx`, `tokenizer.json`, and `labels.json` if they exist.
    /// The directory itself and `manifest.json` are preserved.
    ///
    /// # Errors
    ///
    /// Returns an error if file removal fails.
    pub fn remove_model(&self) -> Result<()> {
        let model_dir = self.model_dir();

        let files_to_remove = ["model.onnx", "tokenizer.json", "labels.json"];

        for file_name in &files_to_remove {
            let file_path = model_dir.join(file_name);
            if file_path.exists() {
                fs::remove_file(&file_path).map_err(|source| Error::Io {
                    path: file_path,
                    source,
                })?;
            }
        }

        // Update manifest to reflect removal
        let manifest = serde_json::json!({
            "version": "1.0.0-dictionary",
            "type": "dictionary",
            "onnx_model": "not installed"
        });

        let manifest_path = model_dir.join("manifest.json");
        if model_dir.exists() {
            let manifest_json = serde_json::to_string_pretty(&manifest)?;
            fs::write(&manifest_path, manifest_json).map_err(|source| Error::Io {
                path: manifest_path,
                source,
            })?;
        }

        Ok(())
    }

    /// Read the version from the manifest file if it exists.
    fn read_manifest_version(&self) -> Option<String> {
        let manifest_path = self.model_dir().join("manifest.json");
        let contents = fs::read_to_string(&manifest_path).ok()?;
        let manifest: serde_json::Value = serde_json::from_str(&contents).ok()?;
        manifest
            .get("version")
            .and_then(|v| v.as_str())
            .map(String::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_model_manager_reports_dictionary_by_default() {
        let dir = TempDir::new().unwrap();
        let manager = ModelManager::new(dir.path());

        assert!(manager.is_installed());
        assert!(!manager.is_onnx_installed());
        assert!(matches!(manager.status(), ModelStatus::Installed { .. }));
    }

    #[test]
    fn test_model_manager_version() {
        let dir = TempDir::new().unwrap();
        let manager = ModelManager::new(dir.path());

        if let ModelStatus::Installed { version, .. } = manager.status() {
            assert_eq!(version, "1.0.0-dictionary");
        } else {
            panic!("Expected Installed status");
        }
    }

    #[test]
    fn test_model_dir_path() {
        let dir = TempDir::new().unwrap();
        let manager = ModelManager::new(dir.path());

        let model_dir = manager.model_dir();
        assert_eq!(model_dir, dir.path().join("models"));
    }

    #[test]
    fn test_is_onnx_installed_when_model_exists() {
        let dir = TempDir::new().unwrap();
        let manager = ModelManager::new(dir.path());

        // Create model directory and a dummy model.onnx
        let model_dir = manager.model_dir();
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(model_dir.join("model.onnx"), b"dummy").unwrap();

        assert!(manager.is_onnx_installed());
        assert!(matches!(
            manager.status(),
            ModelStatus::OnnxInstalled { .. }
        ));
    }

    #[test]
    fn test_download_model_without_feature() {
        let dir = TempDir::new().unwrap();
        let manager = ModelManager::new(dir.path());

        // Without the download feature, this should return an error
        #[cfg(not(feature = "download"))]
        {
            let result = manager.download_model();
            assert!(result.is_err());
        }

        // With the download feature, it would try to hit the network,
        // so we only test that in integration tests
        #[cfg(feature = "download")]
        {
            // Just verify the method exists and compiles — actual download
            // requires network access and is tested manually
            let _ = &manager;
        }
    }

    #[test]
    fn test_remove_model() {
        let dir = TempDir::new().unwrap();
        let manager = ModelManager::new(dir.path());

        // Set up model files
        let model_dir = manager.model_dir();
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(model_dir.join("model.onnx"), b"dummy").unwrap();
        fs::write(model_dir.join("tokenizer.json"), b"{}").unwrap();
        fs::write(model_dir.join("labels.json"), b"[]").unwrap();

        assert!(manager.is_onnx_installed());

        manager.remove_model().unwrap();

        assert!(!manager.is_onnx_installed());
        assert!(!model_dir.join("model.onnx").exists());
        assert!(!model_dir.join("tokenizer.json").exists());
        assert!(!model_dir.join("labels.json").exists());
    }
}
