//! Replacement mapping storage.
//!
//! Stores the persistent mappings between original PII values and their
//! cover identity replacements. Mappings are consistent across documents
//! and sessions.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A single PII-to-replacement mapping.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Mapping {
    /// The original PII value.
    pub original: String,

    /// The replacement (cover identity) value.
    pub replacement: String,

    /// The entity type (e.g., "PERSON", "`ACCOUNT_NUMBER`").
    pub entity_type: String,

    /// When this mapping was first created.
    pub created: DateTime<Utc>,

    /// When this mapping was last used in a processing run.
    pub last_used: DateTime<Utc>,
}

/// Persistent store of replacement mappings.
///
/// Loads from and saves to a JSON file. Provides CRUD operations
/// for managing mappings.
#[derive(Debug)]
pub struct MappingStore {
    mappings: Vec<Mapping>,
    path: PathBuf,
}

impl MappingStore {
    /// Create an empty in-memory store (won't persist unless a path is set).
    pub fn empty() -> Self {
        Self {
            mappings: Vec::new(),
            path: PathBuf::new(),
        }
    }

    /// Load mappings from a JSON file.
    ///
    /// If the file does not exist, returns an empty store that will
    /// save to the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self> {
        let mappings = if path.exists() {
            let contents = fs::read_to_string(path).map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })?;
            serde_json::from_str(&contents)?
        } else {
            Vec::new()
        };

        Ok(Self {
            mappings,
            path: path.to_path_buf(),
        })
    }

    /// Save all mappings to the store's file path.
    ///
    /// Creates parent directories if they do not exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self) -> Result<()> {
        // Skip persistence for in-memory-only stores (created via empty())
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let json = serde_json::to_string_pretty(&self.mappings)?;
        fs::write(&self.path, json).map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }

    /// Add a new mapping or update an existing one.
    ///
    /// If a mapping with the same `original` value already exists, its
    /// `replacement`, `entity_type`, and `last_used` fields are updated.
    ///
    /// # Errors
    ///
    /// Returns an error if saving fails.
    pub fn add(&mut self, original: &str, replacement: &str, entity_type: &str) -> Result<()> {
        let now = Utc::now();

        if let Some(existing) = self.mappings.iter_mut().find(|m| m.original == original) {
            existing.replacement = replacement.to_string();
            existing.entity_type = entity_type.to_string();
            existing.last_used = now;
        } else {
            self.mappings.push(Mapping {
                original: original.to_string(),
                replacement: replacement.to_string(),
                entity_type: entity_type.to_string(),
                created: now,
                last_used: now,
            });
        }

        self.save()
    }

    /// Remove a mapping by its original value.
    ///
    /// Returns `true` if a mapping was removed, `false` if not found.
    ///
    /// # Errors
    ///
    /// Returns an error if saving fails.
    pub fn remove(&mut self, original: &str) -> Result<bool> {
        let len_before = self.mappings.len();
        self.mappings.retain(|m| m.original != original);
        let removed = self.mappings.len() < len_before;

        if removed {
            self.save()?;
        }

        Ok(removed)
    }

    /// Find a mapping by its original value.
    pub fn find(&self, original: &str) -> Option<&Mapping> {
        self.mappings.iter().find(|m| m.original == original)
    }

    /// Return all mappings.
    pub fn list(&self) -> &[Mapping] {
        &self.mappings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store(dir: &TempDir) -> MappingStore {
        let path = dir.path().join("mappings.json");
        MappingStore::load(&path).unwrap()
    }

    #[test]
    fn test_load_nonexistent_file_returns_empty_store() {
        let dir = TempDir::new().unwrap();
        let store = test_store(&dir);
        assert!(store.list().is_empty());
    }

    #[test]
    fn test_add_and_find() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(&dir);

        store.add("Alice Smith", "Jane Doe", "PERSON").unwrap();

        let mapping = store.find("Alice Smith").unwrap();
        assert_eq!(mapping.replacement, "Jane Doe");
        assert_eq!(mapping.entity_type, "PERSON");
    }

    #[test]
    fn test_add_duplicate_updates_existing() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(&dir);

        store.add("Alice Smith", "Jane Doe", "PERSON").unwrap();
        let created_time = store.find("Alice Smith").unwrap().created;

        // Small delay not needed — we just check the fields change
        store.add("Alice Smith", "Mary Johnson", "PERSON").unwrap();

        assert_eq!(store.list().len(), 1);
        let mapping = store.find("Alice Smith").unwrap();
        assert_eq!(mapping.replacement, "Mary Johnson");
        // Created time should remain the same
        assert_eq!(mapping.created, created_time);
    }

    #[test]
    fn test_remove_existing() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(&dir);

        store.add("Alice Smith", "Jane Doe", "PERSON").unwrap();
        let removed = store.remove("Alice Smith").unwrap();

        assert!(removed);
        assert!(store.find("Alice Smith").is_none());
        assert!(store.list().is_empty());
    }

    #[test]
    fn test_remove_nonexistent_returns_false() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(&dir);

        let removed = store.remove("Nobody").unwrap();
        assert!(!removed);
    }

    #[test]
    fn test_list_returns_all_mappings() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(&dir);

        store.add("Alice", "Jane", "PERSON").unwrap();
        store.add("555-1234", "555-0000", "PHONE").unwrap();
        store.add("123-45-6789", "900-00-0000", "SSN").unwrap();

        assert_eq!(store.list().len(), 3);
    }

    #[test]
    fn test_persistence_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mappings.json");

        {
            let mut store = MappingStore::load(&path).unwrap();
            store.add("Alice Smith", "Jane Doe", "PERSON").unwrap();
            store.add("555-1234", "555-0000", "PHONE").unwrap();
        }

        // Load from the same file in a new store instance
        let store = MappingStore::load(&path).unwrap();
        assert_eq!(store.list().len(), 2);
        assert_eq!(store.find("Alice Smith").unwrap().replacement, "Jane Doe");
        assert_eq!(store.find("555-1234").unwrap().replacement, "555-0000");
    }
}
