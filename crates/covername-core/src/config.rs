//! Application configuration management.
//!
//! Handles reading, writing, and locating the Covername configuration file.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Application configuration.
///
/// Controls output behavior, model settings, and OCR language.
/// Persisted as JSON in the application storage directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Pattern for naming output files. Supports `{name}` and `{ext}` placeholders.
    pub output_pattern: String,

    /// Optional override for output file directory. `None` means same directory as input.
    pub output_directory: Option<PathBuf>,

    /// The NER model version to use for detection.
    pub model_version: String,

    /// Whether to check for model updates on startup.
    pub model_update_check: bool,

    /// Language code for OCR (Tesseract language data).
    pub ocr_language: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_pattern: String::from("{name}-covered.{ext}"),
            output_directory: None,
            model_version: String::from("ner-v1.0"),
            model_update_check: true,
            ocr_language: String::from("eng"),
        }
    }
}

impl Config {
    /// Load configuration from a JSON file at the given path.
    ///
    /// If the file does not exist, returns the default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let config: Self = serde_json::from_str(&contents)?;
        Ok(config)
    }

    /// Save the configuration as JSON to the given path.
    ///
    /// Creates parent directories if they do not exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }

    /// Returns the application storage directory.
    ///
    /// Respects `XDG_CONFIG_HOME` if set, otherwise defaults to `~/.config/covername/`.
    ///
    /// Note: This method uses `storage_dir` (abbreviated) while the struct field
    /// is `output_directory` (full word). The field name matches the JSON config key
    /// on disk; the method name is shorter for ergonomic code use. This is intentional.
    ///
    /// # Errors
    ///
    /// Returns an error if the home directory cannot be determined.
    pub fn storage_dir() -> Result<PathBuf> {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg).join("covername"));
        }
        let home = dirs::home_dir().ok_or(Error::NoDataDir)?;
        Ok(home.join(".config").join("covername"))
    }

    /// Ensures the storage directory exists, creating it if necessary.
    ///
    /// Creates with restrictive permissions (0700 on Unix) since the directory
    /// contains sensitive data (PII mappings, ignore lists).
    ///
    /// Returns the path to the storage directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be determined or created.
    pub fn ensure_storage_dir() -> Result<PathBuf> {
        let dir = Self::storage_dir()?;
        fs::create_dir_all(&dir).map_err(|source| Error::Io {
            path: dir.clone(),
            source,
        })?;

        // Set restrictive permissions on Unix (owner-only access)
        // This directory contains PII (mappings, ignore list)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o700);
            let _ = fs::set_permissions(&dir, perms);
        }

        Ok(dir)
    }

    /// Return the path to the configuration JSON file.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage directory cannot be determined or created.
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::ensure_storage_dir()?.join("config.json"))
    }

    /// Return the path to the mappings JSON file.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage directory cannot be determined or created.
    pub fn mappings_path() -> Result<PathBuf> {
        Ok(Self::ensure_storage_dir()?.join("mappings.json"))
    }

    /// Return the path to the custom rules JSON file.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage directory cannot be determined or created.
    pub fn rules_path() -> Result<PathBuf> {
        Ok(Self::ensure_storage_dir()?.join("custom-rules.json"))
    }

    /// Return the path to the ignore list JSON file.
    ///
    /// # Errors
    ///
    /// Returns an error if the storage directory cannot be determined or created.
    pub fn ignore_list_path() -> Result<PathBuf> {
        Ok(Self::ensure_storage_dir()?.join("ignore-list.json"))
    }

    /// Set a configuration field by key name.
    ///
    /// # Errors
    ///
    /// Returns `Error::UnknownConfigKey` if the key is not recognized.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "output_pattern" => self.output_pattern = value.to_string(),
            "output_directory" => {
                if value.is_empty() || value == "null" {
                    self.output_directory = None;
                } else {
                    self.output_directory = Some(PathBuf::from(value));
                }
            }
            "model_version" => self.model_version = value.to_string(),
            "model_update_check" => {
                self.model_update_check = value.parse::<bool>().unwrap_or(true);
            }
            "ocr_language" => self.ocr_language = value.to_string(),
            _ => return Err(Error::UnknownConfigKey(key.to_string())),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_values() {
        let config = Config::default();
        assert_eq!(config.output_pattern, "{name}-covered.{ext}");
        assert_eq!(config.output_directory, None);
        assert_eq!(config.model_version, "ner-v1.0");
        assert!(config.model_update_check);
        assert_eq!(config.ocr_language, "eng");
    }

    #[test]
    fn test_serialization_roundtrip() {
        let config = Config {
            output_pattern: String::from("{name}-redacted.{ext}"),
            output_directory: Some(PathBuf::from("/tmp/output")),
            model_version: String::from("ner-v2.0"),
            model_update_check: false,
            ocr_language: String::from("deu"),
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_load_missing_file_returns_default() {
        let path = Path::new("/tmp/covername-test-nonexistent-config.json");
        let config = Config::load(path).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();

        let config = Config {
            output_pattern: String::from("{name}-anon.{ext}"),
            output_directory: None,
            model_version: String::from("ner-v1.0"),
            model_update_check: true,
            ocr_language: String::from("eng"),
        };

        config.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(config, loaded);

        // Keep file alive
        drop(file);
    }

    #[test]
    fn test_set_known_keys() {
        let mut config = Config::default();

        config.set("output_pattern", "{name}-clean.{ext}").unwrap();
        assert_eq!(config.output_pattern, "{name}-clean.{ext}");

        config.set("output_directory", "/tmp/out").unwrap();
        assert_eq!(config.output_directory, Some(PathBuf::from("/tmp/out")));

        config.set("output_directory", "null").unwrap();
        assert_eq!(config.output_directory, None);

        config.set("model_version", "ner-v3.0").unwrap();
        assert_eq!(config.model_version, "ner-v3.0");

        config.set("model_update_check", "false").unwrap();
        assert!(!config.model_update_check);

        config.set("ocr_language", "fra").unwrap();
        assert_eq!(config.ocr_language, "fra");
    }

    #[test]
    fn test_set_unknown_key_returns_error() {
        let mut config = Config::default();
        let result = config.set("nonexistent_key", "value");
        assert!(result.is_err());
    }
}
