//! XLSX document handling for spreadsheet anonymization.
//!
//! Provides text extraction from Excel spreadsheets and the ability to write
//! new spreadsheets with string replacements applied to cell contents.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use calamine::{Reader, Xlsx, open_workbook};
use rust_xlsxwriter::Workbook;

use crate::error::{Error, Result};

/// An XLSX document loaded from a file.
///
/// Holds the file path for deferred reading. Text extraction reads all
/// sheets and concatenates cell values.
#[derive(Debug, Clone)]
pub struct XlsxDocument {
    /// Path to the source XLSX file.
    path: PathBuf,
}

impl XlsxDocument {
    /// Load an XLSX document from a file path.
    ///
    /// Validates that the file can be opened as a valid XLSX workbook.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened or is not a valid XLSX file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let _workbook: Xlsx<_> = open_workbook(path).map_err(|e| Error::Xlsx {
            path: path.to_path_buf(),
            reason: format!("failed to open workbook: {e}"),
        })?;

        Ok(Self {
            path: path.to_path_buf(),
        })
    }

    /// Extract all text content from the XLSX file.
    ///
    /// Iterates over all sheets and cells, producing text with:
    /// - Sheet names as headers (prefixed with `--- Sheet: <name> ---`)
    /// - Tab-separated columns within each row
    /// - Newline-separated rows
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn extract_text(&self) -> Result<String> {
        let mut workbook: Xlsx<_> = open_workbook(&self.path).map_err(|e| Error::Xlsx {
            path: self.path.clone(),
            reason: format!("failed to open workbook: {e}"),
        })?;

        let sheet_names: Vec<String> = workbook.sheet_names().clone();
        let mut output = String::new();

        for (i, name) in sheet_names.iter().enumerate() {
            if i > 0 {
                output.push('\n');
            }
            let _ = writeln!(output, "--- Sheet: {name} ---");

            if let Ok(range) = workbook.worksheet_range(name) {
                for row in range.rows() {
                    let cells: Vec<String> =
                        row.iter().map(std::string::ToString::to_string).collect();
                    output.push_str(&cells.join("\t"));
                    output.push('\n');
                }
            }
        }

        Ok(output)
    }

    /// Returns the path to the source XLSX file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Write an XLSX file with string replacements applied to all cells.
///
/// Opens the original XLSX file, reads all sheets, rows, and cells,
/// applies the provided replacements to each cell's text content, and
/// writes the result to `output_path` using `rust_xlsxwriter`.
///
/// Sheet names and basic structure are preserved. Formatting is not
/// preserved in this implementation.
///
/// # Errors
///
/// Returns an error if the original file cannot be read or the output
/// file cannot be written.
pub fn write_xlsx(
    original_path: &Path,
    replacements: &[(String, String)],
    output_path: &Path,
) -> Result<()> {
    let mut workbook: Xlsx<_> = open_workbook(original_path).map_err(|e| Error::Xlsx {
        path: original_path.to_path_buf(),
        reason: format!("failed to open workbook: {e}"),
    })?;

    let sheet_names: Vec<String> = workbook.sheet_names().clone();
    let mut output_workbook = Workbook::new();

    for name in &sheet_names {
        let worksheet = output_workbook.add_worksheet();
        worksheet.set_name(name.as_str()).map_err(|e| Error::Xlsx {
            path: output_path.to_path_buf(),
            reason: format!("failed to set sheet name: {e}"),
        })?;

        if let Ok(range) = workbook.worksheet_range(name) {
            for (row_idx, row) in range.rows().enumerate() {
                for (col_idx, cell) in row.iter().enumerate() {
                    let cell_text = cell.to_string();
                    if cell_text.is_empty() {
                        continue;
                    }

                    let mut replaced = cell_text;
                    for (original, replacement) in replacements {
                        replaced = replaced.replace(original, replacement);
                    }

                    let row_num = u32::try_from(row_idx).unwrap_or(0);
                    let col_num = u16::try_from(col_idx).unwrap_or(0);

                    worksheet
                        .write_string(row_num, col_num, &replaced)
                        .map_err(|e| Error::Xlsx {
                            path: output_path.to_path_buf(),
                            reason: format!("failed to write cell ({row_num}, {col_num}): {e}"),
                        })?;
                }
            }
        }
    }

    // Create parent directories if needed
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    output_workbook.save(output_path).map_err(|e| Error::Xlsx {
        path: output_path.to_path_buf(),
        reason: format!("failed to save workbook: {e}"),
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper to create a test XLSX file with known content.
    fn create_test_xlsx(path: &Path, sheets: &[(&str, &[&[&str]])]) {
        let mut workbook = Workbook::new();
        for (name, rows) in sheets {
            let worksheet = workbook.add_worksheet();
            worksheet.set_name(*name).unwrap();
            for (row_idx, row) in rows.iter().enumerate() {
                for (col_idx, cell) in row.iter().enumerate() {
                    let row_num = u32::try_from(row_idx).unwrap();
                    let col_num = u16::try_from(col_idx).unwrap();
                    worksheet.write_string(row_num, col_num, *cell).unwrap();
                }
            }
        }
        workbook.save(path).unwrap();
    }

    #[test]
    fn test_xlsx_document_from_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.xlsx");
        create_test_xlsx(&path, &[("Sheet1", &[&["Hello", "World"]])]);

        let doc = XlsxDocument::from_file(&path).unwrap();
        assert_eq!(doc.path(), path);
    }

    #[test]
    fn test_xlsx_document_from_file_nonexistent() {
        let result = XlsxDocument::from_file(Path::new("/tmp/covername-nonexistent.xlsx"));
        assert!(result.is_err());
    }

    #[test]
    fn test_xlsx_extract_text_single_sheet() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.xlsx");
        create_test_xlsx(
            &path,
            &[("Data", &[&["Name", "Phone"], &["Jason Smith", "555-1234"]])],
        );

        let doc = XlsxDocument::from_file(&path).unwrap();
        let text = doc.extract_text().unwrap();

        assert!(text.contains("--- Sheet: Data ---"));
        assert!(text.contains("Name\tPhone"));
        assert!(text.contains("Jason Smith\t555-1234"));
    }

    #[test]
    fn test_xlsx_extract_text_multiple_sheets() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.xlsx");
        create_test_xlsx(
            &path,
            &[
                ("People", &[&["Alice", "Bob"]]),
                ("Places", &[&["New York", "London"]]),
            ],
        );

        let doc = XlsxDocument::from_file(&path).unwrap();
        let text = doc.extract_text().unwrap();

        assert!(text.contains("--- Sheet: People ---"));
        assert!(text.contains("Alice\tBob"));
        assert!(text.contains("--- Sheet: Places ---"));
        assert!(text.contains("New York\tLondon"));
    }

    #[test]
    fn test_write_xlsx_with_replacements() {
        let dir = TempDir::new().unwrap();
        let input_path = dir.path().join("input.xlsx");
        let output_path = dir.path().join("output.xlsx");

        create_test_xlsx(
            &input_path,
            &[(
                "Sheet1",
                &[
                    &["Jason Smith", "jason@example.com"],
                    &["555-123-4567", "data"],
                ],
            )],
        );

        let replacements = vec![
            ("Jason Smith".to_string(), "John Adams".to_string()),
            (
                "jason@example.com".to_string(),
                "john@example.com".to_string(),
            ),
            ("555-123-4567".to_string(), "555-987-6543".to_string()),
        ];

        write_xlsx(&input_path, &replacements, &output_path).unwrap();

        // Verify the output
        let doc = XlsxDocument::from_file(&output_path).unwrap();
        let text = doc.extract_text().unwrap();

        assert!(text.contains("John Adams"));
        assert!(text.contains("john@example.com"));
        assert!(text.contains("555-987-6543"));
        assert!(!text.contains("Jason Smith"));
        assert!(!text.contains("jason@example.com"));
        assert!(!text.contains("555-123-4567"));
    }

    #[test]
    fn test_write_xlsx_round_trip() {
        let dir = TempDir::new().unwrap();
        let input_path = dir.path().join("pii.xlsx");
        let output_path = dir.path().join("clean.xlsx");

        // Create a spreadsheet with PII
        create_test_xlsx(
            &input_path,
            &[(
                "Contacts",
                &[
                    &["Name", "Email", "SSN"],
                    &["Jason Smith", "jason.smith@corp.com", "123-45-6789"],
                    &["Maria Garcia", "maria@company.org", "987-65-4321"],
                ],
            )],
        );

        // Define replacements (simulating what the pipeline would produce)
        let replacements = vec![
            ("Jason Smith".to_string(), "John Adams".to_string()),
            (
                "jason.smith@corp.com".to_string(),
                "john.adams@example.com".to_string(),
            ),
            ("123-45-6789".to_string(), "XXX-XX-XXXX".to_string()),
            ("Maria Garcia".to_string(), "Sarah Johnson".to_string()),
            (
                "maria@company.org".to_string(),
                "sarah.j@example.com".to_string(),
            ),
            ("987-65-4321".to_string(), "XXX-XX-XXXX".to_string()),
        ];

        write_xlsx(&input_path, &replacements, &output_path).unwrap();

        // Verify no PII in output
        let doc = XlsxDocument::from_file(&output_path).unwrap();
        let text = doc.extract_text().unwrap();

        assert!(!text.contains("Jason Smith"));
        assert!(!text.contains("Maria Garcia"));
        assert!(!text.contains("jason.smith@corp.com"));
        assert!(!text.contains("maria@company.org"));
        assert!(!text.contains("123-45-6789"));
        assert!(!text.contains("987-65-4321"));

        // Verify replacements are present
        assert!(text.contains("John Adams"));
        assert!(text.contains("Sarah Johnson"));
    }
}
