//! Ignore list storage.
//!
//! Stores entities that should be permanently skipped during PII detection.
//! When a user rejects a detection and marks it as "always ignore," it is
//! recorded here so future scans automatically skip it.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A single ignored entity entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IgnoredEntity {
    /// The matched text to ignore.
    pub text: String,

    /// The entity type it was detected as (e.g., "PERSON", "ADDRESS").
    pub entity_type: String,

    /// When this entry was added.
    pub created: DateTime<Utc>,

    /// Optional note explaining why this was ignored.
    pub reason: Option<String>,
}

/// Persistent store of ignored entities.
///
/// Loads from and saves to a JSON file. Entities in this list are
/// automatically filtered out during PII detection review.
#[derive(Debug)]
pub struct IgnoreList {
    entries: Vec<IgnoredEntity>,
    path: PathBuf,
}

impl IgnoreList {
    /// Create an empty ignore list (not backed by a file).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            path: PathBuf::new(),
        }
    }

    /// Load the ignore list from a JSON file.
    ///
    /// If the file does not exist, returns an empty list that will
    /// save to the given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self> {
        let entries = if path.exists() {
            let contents = fs::read_to_string(path).map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })?;
            serde_json::from_str(&contents)?
        } else {
            Vec::new()
        };

        Ok(Self {
            entries,
            path: path.to_path_buf(),
        })
    }

    /// Save all entries to the store's file path.
    ///
    /// Creates parent directories if they do not exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let json = serde_json::to_string_pretty(&self.entries)?;
        fs::write(&self.path, json).map_err(|source| Error::Io {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }

    /// Add an entity to the ignore list and save.
    ///
    /// If an entry with the same text already exists (case-insensitive),
    /// no duplicate is added.
    ///
    /// # Errors
    ///
    /// Returns an error if saving fails.
    pub fn add(&mut self, text: &str, entity_type: &str) -> Result<()> {
        // Don't add duplicates (case-insensitive check)
        if self.is_ignored(text) {
            return Ok(());
        }

        self.entries.push(IgnoredEntity {
            text: text.to_string(),
            entity_type: entity_type.to_string(),
            created: Utc::now(),
            reason: None,
        });

        self.save()
    }

    /// Remove an entry by its text value (case-insensitive).
    ///
    /// Returns `true` if an entry was removed, `false` if not found.
    ///
    /// # Errors
    ///
    /// Returns an error if saving fails.
    pub fn remove(&mut self, text: &str) -> Result<bool> {
        let len_before = self.entries.len();
        let text_lower = text.to_lowercase();
        self.entries.retain(|e| e.text.to_lowercase() != text_lower);
        let removed = self.entries.len() < len_before;

        if removed {
            self.save()?;
        }

        Ok(removed)
    }

    /// Remove all entries from the ignore list.
    ///
    /// # Errors
    ///
    /// Returns an error if saving fails.
    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.save()
    }

    /// Check if a text matches any ignored entry (case-insensitive exact match).
    pub fn is_ignored(&self, text: &str) -> bool {
        let text_lower = text.to_lowercase();
        self.entries
            .iter()
            .any(|e| e.text.to_lowercase() == text_lower)
    }

    /// Return all entries in the ignore list.
    pub fn list(&self) -> &[IgnoredEntity] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_list(dir: &TempDir) -> IgnoreList {
        let path = dir.path().join("ignore-list.json");
        IgnoreList::load(&path).unwrap()
    }

    #[test]
    fn test_load_nonexistent_file_returns_empty_list() {
        let dir = TempDir::new().unwrap();
        let list = test_list(&dir);
        assert!(list.list().is_empty());
    }

    #[test]
    fn test_add_and_is_ignored() {
        let dir = TempDir::new().unwrap();
        let mut list = test_list(&dir);

        list.add("MONROE, WI 53566-8309", "ADDRESS").unwrap();

        assert!(list.is_ignored("MONROE, WI 53566-8309"));
        assert!(!list.is_ignored("something else"));
    }

    #[test]
    fn test_case_insensitive_matching() {
        let dir = TempDir::new().unwrap();
        let mut list = test_list(&dir);

        list.add("John Smith", "PERSON").unwrap();

        assert!(list.is_ignored("John Smith"));
        assert!(list.is_ignored("john smith"));
        assert!(list.is_ignored("JOHN SMITH"));
        assert!(list.is_ignored("jOhN sMiTh"));
    }

    #[test]
    fn test_add_duplicate_is_noop() {
        let dir = TempDir::new().unwrap();
        let mut list = test_list(&dir);

        list.add("John Smith", "PERSON").unwrap();
        list.add("john smith", "PERSON").unwrap(); // case-insensitive duplicate

        assert_eq!(list.list().len(), 1);
    }

    #[test]
    fn test_remove_existing() {
        let dir = TempDir::new().unwrap();
        let mut list = test_list(&dir);

        list.add("MONROE, WI 53566-8309", "ADDRESS").unwrap();
        let removed = list.remove("MONROE, WI 53566-8309").unwrap();

        assert!(removed);
        assert!(!list.is_ignored("MONROE, WI 53566-8309"));
        assert!(list.list().is_empty());
    }

    #[test]
    fn test_remove_case_insensitive() {
        let dir = TempDir::new().unwrap();
        let mut list = test_list(&dir);

        list.add("John Smith", "PERSON").unwrap();
        let removed = list.remove("john smith").unwrap();

        assert!(removed);
        assert!(list.list().is_empty());
    }

    #[test]
    fn test_remove_nonexistent_returns_false() {
        let dir = TempDir::new().unwrap();
        let mut list = test_list(&dir);

        let removed = list.remove("Nobody").unwrap();
        assert!(!removed);
    }

    #[test]
    fn test_clear() {
        let dir = TempDir::new().unwrap();
        let mut list = test_list(&dir);

        list.add("Alice", "PERSON").unwrap();
        list.add("555-1234", "PHONE").unwrap();
        list.clear().unwrap();

        assert!(list.list().is_empty());
    }

    #[test]
    fn test_persistence_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("ignore-list.json");

        {
            let mut list = IgnoreList::load(&path).unwrap();
            list.add("MONROE, WI 53566-8309", "ADDRESS").unwrap();
            list.add("John Smith", "PERSON").unwrap();
        }

        // Load from the same file in a new instance
        let list = IgnoreList::load(&path).unwrap();
        assert_eq!(list.list().len(), 2);
        assert!(list.is_ignored("MONROE, WI 53566-8309"));
        assert!(list.is_ignored("John Smith"));
    }

    #[test]
    fn test_filtering_removes_ignored_detections() {
        let dir = TempDir::new().unwrap();
        let mut list = test_list(&dir);

        list.add("MONROE, WI 53566-8309", "ADDRESS").unwrap();
        list.add("John Smith", "PERSON").unwrap();

        // Simulate a list of detection matched_texts
        let detections = vec![
            "MONROE, WI 53566-8309",
            "Alice Johnson",
            "John Smith",
            "555-123-4567",
        ];

        let filtered: Vec<_> = detections
            .into_iter()
            .filter(|text| !list.is_ignored(text))
            .collect();

        assert_eq!(filtered, vec!["Alice Johnson", "555-123-4567"]);
    }
}
