//! Config export and import functionality.
//!
//! Bundles configuration, mappings, and custom rules into a ZIP archive
//! for backup/restore across machines.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use zip::ZipArchive;
use zip::ZipWriter;
use zip::write::FileOptions;

use crate::error::{Error, Result};

/// Files that can be included in a config export.
const EXPORTABLE_FILES: &[&str] = &["config.json", "mappings.json", "custom-rules.json"];

/// The result of importing a config archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    /// Names of files that were successfully imported.
    pub files_imported: Vec<String>,
}

/// Export configuration files to a ZIP archive.
///
/// Bundles `config.json`, `mappings.json`, and `custom-rules.json` from the
/// storage directory into a ZIP file. Files that don't exist are skipped.
///
/// # Errors
///
/// Returns an error if the ZIP file cannot be created or written.
pub fn export_config(storage_dir: &Path, output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let file = fs::File::create(output_path).map_err(|source| Error::Io {
        path: output_path.to_path_buf(),
        source,
    })?;

    let mut zip = ZipWriter::new(file);
    let options =
        FileOptions::<'_, ()>::default().compression_method(zip::CompressionMethod::Deflated);

    for &filename in EXPORTABLE_FILES {
        let file_path = storage_dir.join(filename);
        if file_path.exists() {
            let contents = fs::read(&file_path).map_err(|source| Error::Io {
                path: file_path.clone(),
                source,
            })?;
            zip.start_file(filename, options).map_err(|e| Error::Zip {
                path: output_path.to_path_buf(),
                reason: e.to_string(),
            })?;
            zip.write_all(&contents).map_err(|source| Error::Io {
                path: output_path.to_path_buf(),
                source,
            })?;
        }
    }

    zip.finish().map_err(|e| Error::Zip {
        path: output_path.to_path_buf(),
        reason: e.to_string(),
    })?;

    Ok(())
}

/// Import configuration files from a ZIP archive.
///
/// Extracts `config.json`, `mappings.json`, and `custom-rules.json` from the
/// ZIP file into the storage directory, overwriting any existing files.
///
/// # Errors
///
/// Returns an error if the ZIP file cannot be read or is invalid.
pub fn import_config(zip_path: &Path, storage_dir: &Path) -> Result<ImportResult> {
    let file = fs::File::open(zip_path).map_err(|source| Error::Io {
        path: zip_path.to_path_buf(),
        source,
    })?;

    let mut archive = ZipArchive::new(file).map_err(|e| Error::Zip {
        path: zip_path.to_path_buf(),
        reason: e.to_string(),
    })?;

    fs::create_dir_all(storage_dir).map_err(|source| Error::Io {
        path: storage_dir.to_path_buf(),
        source,
    })?;

    let mut files_imported = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| Error::Zip {
            path: zip_path.to_path_buf(),
            reason: e.to_string(),
        })?;

        let name = entry.name().to_string();

        // Only extract known config files (security: prevent path traversal)
        if !EXPORTABLE_FILES.contains(&name.as_str()) {
            continue;
        }

        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|source| Error::Io {
                path: zip_path.to_path_buf(),
                source,
            })?;

        let output_path = storage_dir.join(&name);
        fs::write(&output_path, &contents).map_err(|source| Error::Io {
            path: output_path,
            source,
        })?;

        files_imported.push(name);
    }

    Ok(ImportResult { files_imported })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_export_creates_valid_zip() {
        let storage = TempDir::new().unwrap();
        let output = TempDir::new().unwrap();
        let zip_path = output.path().join("backup.zip");

        // Create some config files
        fs::write(storage.path().join("config.json"), r#"{"key": "value"}"#).unwrap();
        fs::write(storage.path().join("mappings.json"), r#"{"mappings": []}"#).unwrap();

        export_config(storage.path(), &zip_path).unwrap();

        // Verify ZIP is valid and contains expected files
        let file = fs::File::open(&zip_path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        assert_eq!(archive.len(), 2);

        let mut names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["config.json", "mappings.json"]);
    }

    #[test]
    fn test_export_skips_missing_files() {
        let storage = TempDir::new().unwrap();
        let output = TempDir::new().unwrap();
        let zip_path = output.path().join("backup.zip");

        // Only create one file
        fs::write(storage.path().join("config.json"), r#"{"key": "value"}"#).unwrap();

        export_config(storage.path(), &zip_path).unwrap();

        let file = fs::File::open(&zip_path).unwrap();
        let archive = ZipArchive::new(file).unwrap();
        assert_eq!(archive.len(), 1);
    }

    #[test]
    fn test_import_extracts_files() {
        let storage = TempDir::new().unwrap();
        let output = TempDir::new().unwrap();
        let zip_path = output.path().join("backup.zip");

        // Create and export
        fs::write(storage.path().join("config.json"), r#"{"key": "value"}"#).unwrap();
        fs::write(storage.path().join("mappings.json"), r#"{"mappings": []}"#).unwrap();
        export_config(storage.path(), &zip_path).unwrap();

        // Import to a fresh directory
        let import_dir = TempDir::new().unwrap();
        let result = import_config(&zip_path, import_dir.path()).unwrap();

        assert_eq!(result.files_imported.len(), 2);
        assert!(import_dir.path().join("config.json").exists());
        assert!(import_dir.path().join("mappings.json").exists());

        let content = fs::read_to_string(import_dir.path().join("config.json")).unwrap();
        assert_eq!(content, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_roundtrip_export_import() {
        let storage = TempDir::new().unwrap();
        let output = TempDir::new().unwrap();
        let zip_path = output.path().join("backup.zip");

        let config_content = r#"{"output_pattern": "{name}-covered.{ext}"}"#;
        let mappings_content = r#"{"mappings": [{"original": "John"}]}"#;
        let rules_content = r#"{"rules": [{"name": "test"}]}"#;

        fs::write(storage.path().join("config.json"), config_content).unwrap();
        fs::write(storage.path().join("mappings.json"), mappings_content).unwrap();
        fs::write(storage.path().join("custom-rules.json"), rules_content).unwrap();

        // Export
        export_config(storage.path(), &zip_path).unwrap();

        // Delete originals
        fs::remove_file(storage.path().join("config.json")).unwrap();
        fs::remove_file(storage.path().join("mappings.json")).unwrap();
        fs::remove_file(storage.path().join("custom-rules.json")).unwrap();

        assert!(!storage.path().join("config.json").exists());
        assert!(!storage.path().join("mappings.json").exists());
        assert!(!storage.path().join("custom-rules.json").exists());

        // Import
        let result = import_config(&zip_path, storage.path()).unwrap();
        assert_eq!(result.files_imported.len(), 3);

        // Verify data restored
        assert_eq!(
            fs::read_to_string(storage.path().join("config.json")).unwrap(),
            config_content
        );
        assert_eq!(
            fs::read_to_string(storage.path().join("mappings.json")).unwrap(),
            mappings_content
        );
        assert_eq!(
            fs::read_to_string(storage.path().join("custom-rules.json")).unwrap(),
            rules_content
        );
    }

    #[test]
    fn test_import_missing_zip_returns_error() {
        let storage = TempDir::new().unwrap();
        let result = import_config(
            Path::new("/tmp/nonexistent-covername-backup.zip"),
            storage.path(),
        );
        assert!(result.is_err());
    }
}
