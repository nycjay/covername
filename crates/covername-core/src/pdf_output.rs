//! PDF output generation for anonymized documents.
//!
//! Generates a fresh PDF from replaced text content. This is the Phase 1
//! redaction strategy: rather than modifying the original PDF (which is
//! extremely complex with current Rust PDF libraries), we generate a new
//! text-only PDF. This guarantees true redaction because the original PII
//! never appears in the output file.
//!
//! **Trade-off**: The output PDF loses all formatting from the original
//! (fonts, layout, logos, images). Preserving visual fidelity is a Phase 2/3
//! concern for the Tauri document viewer.

use std::fs;
use std::path::Path;

use printpdf::{
    BuiltinFont, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, Pt, TextItem,
    serialize::PdfSaveOptions,
};

use crate::error::{Error, Result};

/// Maximum characters per line before wrapping.
const MAX_CHARS_PER_LINE: usize = 90;

/// Maximum lines per page before creating a new page.
const LINES_PER_PAGE: usize = 50;

/// Font size in points.
const FONT_SIZE: f32 = 10.0;

/// Line height in points.
const LINE_HEIGHT: f32 = 14.0;

/// Page width in mm (A4).
const PAGE_WIDTH: f32 = 210.0;

/// Page height in mm (A4).
const PAGE_HEIGHT: f32 = 297.0;

/// Left margin in points.
const MARGIN_LEFT: f32 = 50.0;

/// Top position for the first line (from bottom, in points).
/// A4 is 841.89 pt tall; start text ~50pt from the top.
const TOP_START: f32 = 780.0;

/// Generate a PDF file from text content.
///
/// Creates a simple A4 PDF using Courier font with the provided text.
/// Lines are wrapped at `MAX_CHARS_PER_LINE` characters and paginated
/// at `LINES_PER_PAGE` lines per page.
///
/// # Errors
///
/// Returns an error if the PDF cannot be written to the output path.
pub fn write_pdf(content: &str, output_path: &Path) -> Result<()> {
    let lines = wrap_and_split(content);
    let pages = paginate(&lines);

    let mut doc = PdfDocument::new("Covername Redacted Document");
    let font = PdfFontHandle::Builtin(BuiltinFont::Courier);

    let pdf_pages: Vec<PdfPage> = pages
        .iter()
        .map(|page_lines| build_page(&font, page_lines))
        .collect();

    doc.pages = pdf_pages;

    let mut warnings = Vec::new();
    let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);

    // Create parent directories if needed
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    fs::write(output_path, bytes).map_err(|source| Error::Io {
        path: output_path.to_path_buf(),
        source,
    })?;

    Ok(())
}

/// Build a single PDF page from lines of text.
fn build_page(font: &PdfFontHandle, lines: &[String]) -> PdfPage {
    let mut ops = vec![
        Op::StartTextSection,
        Op::SetFont {
            font: font.clone(),
            size: Pt(FONT_SIZE),
        },
        Op::SetLineHeight {
            lh: Pt(LINE_HEIGHT),
        },
        Op::SetTextCursor {
            pos: printpdf::Point {
                x: Pt(MARGIN_LEFT),
                y: Pt(TOP_START),
            },
        },
    ];

    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            ops.push(Op::AddLineBreak);
        }
        ops.push(Op::ShowText {
            items: vec![TextItem::Text(line.clone())],
        });
    }

    ops.push(Op::EndTextSection);

    PdfPage::new(Mm(PAGE_WIDTH), Mm(PAGE_HEIGHT), ops)
}

/// Split content into lines, wrapping long lines at the character limit.
fn wrap_and_split(content: &str) -> Vec<String> {
    let mut result = Vec::new();

    for line in content.lines() {
        if line.len() <= MAX_CHARS_PER_LINE {
            result.push(line.to_string());
        } else {
            // Wrap at word boundaries where possible
            let mut remaining = line;
            while !remaining.is_empty() {
                if remaining.len() <= MAX_CHARS_PER_LINE {
                    result.push(remaining.to_string());
                    break;
                }

                // Find a safe byte boundary at or before MAX_CHARS_PER_LINE
                let safe_end = remaining.floor_char_boundary(MAX_CHARS_PER_LINE);
                if safe_end == 0 {
                    // Single char wider than limit (shouldn't happen), take it
                    let ch_len = remaining.chars().next().map_or(1, char::len_utf8);
                    let (chunk, rest) = remaining.split_at(ch_len);
                    result.push(chunk.to_string());
                    remaining = rest;
                    continue;
                }

                // Find the last space within the safe boundary
                let break_at = remaining[..safe_end]
                    .rfind(' ')
                    .map_or(safe_end, |pos| pos + 1);

                let (chunk, rest) = remaining.split_at(break_at);
                result.push(chunk.trim_end().to_string());
                remaining = rest;
            }
        }
    }

    result
}

/// Split lines into groups of `LINES_PER_PAGE` for pagination.
fn paginate(lines: &[String]) -> Vec<Vec<String>> {
    if lines.is_empty() {
        // Always produce at least one (empty) page
        return vec![vec![String::new()]];
    }
    lines
        .chunks(LINES_PER_PAGE)
        .map(<[String]>::to_vec)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_pdf_creates_file() {
        let dir = TempDir::new().unwrap();
        let output = dir.path().join("output.pdf");

        write_pdf("Hello, world!\nThis is a test.", &output).unwrap();

        assert!(output.exists());
        let bytes = fs::read(&output).unwrap();
        // PDF files start with %PDF
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_write_pdf_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let output = dir.path().join("sub").join("dir").join("output.pdf");

        write_pdf("Content here", &output).unwrap();

        assert!(output.exists());
    }

    #[test]
    fn test_write_pdf_empty_content() {
        let dir = TempDir::new().unwrap();
        let output = dir.path().join("empty.pdf");

        write_pdf("", &output).unwrap();

        assert!(output.exists());
        let bytes = fs::read(&output).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_wrap_and_split_short_lines() {
        let lines = wrap_and_split("short line\nanother");
        assert_eq!(lines, vec!["short line", "another"]);
    }

    #[test]
    fn test_wrap_and_split_long_line() {
        let long = "a ".repeat(60); // 120 chars
        let lines = wrap_and_split(&long);
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(line.len() <= MAX_CHARS_PER_LINE);
        }
    }

    #[test]
    fn test_paginate_small_input() {
        let lines: Vec<String> = (0..10).map(|i| format!("line {i}")).collect();
        let pages = paginate(&lines);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].len(), 10);
    }

    #[test]
    fn test_paginate_multi_page() {
        let lines: Vec<String> = (0..120).map(|i| format!("line {i}")).collect();
        let pages = paginate(&lines);
        assert_eq!(pages.len(), 3); // 50 + 50 + 20
        assert_eq!(pages[0].len(), 50);
        assert_eq!(pages[1].len(), 50);
        assert_eq!(pages[2].len(), 20);
    }
}
