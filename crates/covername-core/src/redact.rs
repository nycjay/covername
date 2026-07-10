//! Position-aware PDF redaction.
//!
//! Renders PDF pages to images, uses `Tesseract` hOCR to get word bounding boxes,
//! paints over PII regions, draws replacement text, and assembles a new PDF.
//! The result looks like the original document with only PII regions changed.

#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::cast_precision_loss,
    clippy::collapsible_if,
    clippy::needless_range_loop,
    clippy::too_many_lines
)]

use std::io::IsTerminal;
use std::path::Path;

use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use indicatif::ProgressBar;
use pdfium_render::prelude::*;

use crate::error::{Error, Result};

/// Tracks whether a page needs redaction or can be copied from original.
enum PageResult {
    /// Page was redacted — use this JPEG image.
    Modified(std::path::PathBuf),
    /// No PII found — copy from original PDF.
    Clean,
}

/// A word with its bounding box from hOCR output.
#[derive(Debug, Clone)]
struct OcrWord {
    text: String,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// A replacement to apply on a page image.
#[derive(Debug, Clone)]
pub struct PageReplacement {
    /// The original text to find.
    pub original: String,
    /// The replacement text to draw.
    pub replacement: String,
}

/// Perform position-aware redaction on a PDF.
///
/// For each page:
/// 1. Render to high-res image via `PDFium`
/// 2. Run Tesseract hOCR to get word bounding boxes
/// 3. Find words matching PII, paint over with white
/// 4. Draw replacement text in the same area
/// 5. Save modified page image
///
/// Then assemble all page images into a new PDF.
///
/// # Errors
///
/// Returns an error if rendering, OCR, or image manipulation fails.
pub fn redact_pdf(
    input_path: &Path,
    replacements: &[PageReplacement],
    output_path: &Path,
) -> Result<()> {
    redact_pdf_with_progress(input_path, replacements, output_path, &|_, _| {})
}

/// Same as [`redact_pdf`] but with a progress callback.
///
/// The callback receives `(current_page, total_pages)` after each page is processed.
///
/// # Errors
///
/// Returns an error if rendering, OCR, or image manipulation fails.
pub fn redact_pdf_with_progress(
    input_path: &Path,
    replacements: &[PageReplacement],
    output_path: &Path,
    on_progress: &dyn Fn(u64, u64),
) -> Result<()> {
    // Safety limit: prevent OOM/disk exhaustion on very large PDFs
    const MAX_PAGES: u64 = 500;

    if replacements.is_empty() {
        // No replacements — just copy the file
        std::fs::copy(input_path, output_path).map_err(|source| Error::Io {
            path: output_path.to_path_buf(),
            source,
        })?;
        return Ok(());
    }

    // Create temp directory for page images
    let temp_dir = tempfile::tempdir().map_err(|e| Error::Ocr {
        path: input_path.to_path_buf(),
        reason: format!("failed to create temp directory: {e}"),
    })?;

    // Load PDF with PDFium
    let pdfium = crate::pdfium::load_pdfium(input_path)?;

    let document = pdfium
        .load_pdf_from_file(input_path, None)
        .map_err(|e| Error::Ocr {
            path: input_path.to_path_buf(),
            reason: format!("failed to load PDF: {e}"),
        })?;

    let render_config = PdfRenderConfig::new()
        .set_target_width(2550) // 8.5" at 300 DPI (must match OCR rendering for consistent results)
        .set_maximum_height(3300); // 11" at 300 DPI

    let page_count = u64::from(document.pages().len());

    if page_count > MAX_PAGES {
        return Err(Error::Ocr {
            path: input_path.to_path_buf(),
            reason: format!(
                "PDF has {page_count} pages (limit is {MAX_PAGES}). \
                 Split the document into smaller files before processing."
            ),
        });
    }

    let draw_target = if std::io::stderr().is_terminal() {
        indicatif::ProgressDrawTarget::stderr()
    } else {
        indicatif::ProgressDrawTarget::hidden()
    };
    let pb = ProgressBar::with_draw_target(Some(page_count), draw_target);
    pb.set_style(crate::utils::progress_style());
    pb.set_message("Redacting pages");

    // Track which pages have PII and need image-based redaction
    // Pages without PII will be copied directly from the original PDF
    let mut page_results: Vec<PageResult> = Vec::new();

    for (i, page) in document.pages().iter().enumerate() {
        // Render page to image
        let bitmap = page
            .render_with_config(&render_config)
            .map_err(|e| Error::Ocr {
                path: input_path.to_path_buf(),
                reason: format!("failed to render page {}: {e}", i + 1),
            })?;

        let page_img_path = temp_dir.path().join(format!("page-{:03}.png", i + 1));

        bitmap
            .as_image()
            .as_rgba8()
            .ok_or_else(|| Error::Ocr {
                path: input_path.to_path_buf(),
                reason: format!("failed to convert page {} to RGBA", i + 1),
            })?
            .save(&page_img_path)
            .map_err(|e| Error::Ocr {
                path: input_path.to_path_buf(),
                reason: format!("failed to save page {} image: {e}", i + 1),
            })?;

        // Run hOCR on this page to get word bounding boxes
        let words = run_hocr(&page_img_path)?;

        // Check if any replacements match words on this page
        let has_matches = page_has_matches(&words, replacements);

        if has_matches {
            // Check if text on this page is mostly vertical (rotated 90°)
            // by examining word bounding box dimensions from hOCR
            let vertical_words = words.iter().filter(|w| w.h > w.w * 2).count();
            let is_rotated = vertical_words > words.len() / 3;

            let mut img = image::open(&page_img_path)
                .map_err(|e| Error::Ocr {
                    path: input_path.to_path_buf(),
                    reason: format!("failed to open page {} image: {e}", i + 1),
                })?
                .to_rgba8();

            if is_rotated {
                // Rotate image 90° clockwise so text becomes horizontal
                let rotated_img = image::imageops::rotate90(&img);

                // Re-run hOCR on the rotated image for correct bounding boxes
                let rotated_path = temp_dir.path().join(format!("rotated-{:03}.png", i + 1));
                rotated_img.save(&rotated_path).map_err(|e| Error::Ocr {
                    path: input_path.to_path_buf(),
                    reason: format!("failed to save rotated page {}: {e}", i + 1),
                })?;
                let rotated_words = run_hocr(&rotated_path)?;

                // Apply replacements on the rotated (now horizontal) image
                let mut rotated_img = rotated_img;
                apply_replacements_to_image(&mut rotated_img, &rotated_words, replacements);

                // Rotate back 90° counter-clockwise
                img = image::imageops::rotate270(&rotated_img);
            } else {
                // Normal horizontal text — apply directly
                apply_replacements_to_image(&mut img, &words, replacements);
            }

            // Save modified page
            let modified_path = temp_dir.path().join(format!("modified-{:03}.png", i + 1));
            img.save(&modified_path).map_err(|e| Error::Ocr {
                path: input_path.to_path_buf(),
                reason: format!("failed to save modified page {}: {e}", i + 1),
            })?;

            page_results.push(PageResult::Modified(modified_path));
        } else {
            // No PII on this page — will copy from original PDF
            page_results.push(PageResult::Clean);
        }

        pb.inc(1);
        on_progress(u64::try_from(i + 1).unwrap_or(0), page_count);
    }

    pb.finish_with_message("Redaction done");

    // Assemble hybrid PDF: original pages for clean, JPEG for redacted
    assemble_hybrid_pdf(input_path, &page_results, output_path)?;

    Ok(())
}

/// Check if any replacements match words on a given page.
///
/// Does a case-insensitive sliding-window search through OCR words
/// to see if any replacement's original text appears on this page.
fn page_has_matches(words: &[OcrWord], replacements: &[PageReplacement]) -> bool {
    for replacement in replacements {
        let original_words: Vec<&str> = replacement.original.split_whitespace().collect();
        if original_words.is_empty() {
            continue;
        }

        let mut i = 0;
        while i + original_words.len() <= words.len() {
            let matches = words[i..i + original_words.len()]
                .iter()
                .zip(original_words.iter())
                .all(|(ocr, orig)| ocr.text.eq_ignore_ascii_case(orig));

            if matches {
                return true;
            }
            i += 1;
        }
    }
    false
}

/// Run Tesseract in hOCR mode to get word bounding boxes.
fn run_hocr(image_path: &Path) -> Result<Vec<OcrWord>> {
    let output = crate::ocr::tesseract_command()
        .arg(image_path.as_os_str())
        .arg("stdout")
        .arg("-l")
        .arg("eng")
        .arg("--psm")
        .arg("1") // Auto page segmentation with OSD (handles rotated text)
        .arg("hocr")
        .output()
        .map_err(|e| Error::Ocr {
            path: image_path.to_path_buf(),
            reason: format!("failed to execute tesseract hocr: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Ocr {
            path: image_path.to_path_buf(),
            reason: format!("tesseract hocr failed: {stderr}"),
        });
    }

    let hocr_text = String::from_utf8_lossy(&output.stdout);
    Ok(parse_hocr(&hocr_text))
}

/// Parse hOCR XML to extract word bounding boxes.
fn parse_hocr(hocr: &str) -> Vec<OcrWord> {
    let mut words = Vec::new();

    // hOCR format: <span class='ocrx_word' ... title='bbox x1 y1 x2 y2; ...'>word</span>
    for line in hocr.lines() {
        if !line.contains("ocrx_word") {
            continue;
        }

        // Extract bbox coordinates
        if let Some(bbox_start) = line.find("bbox ") {
            let bbox_str = &line[bbox_start + 5..];
            let bbox_end = bbox_str
                .find(';')
                .or_else(|| bbox_str.find('"'))
                .unwrap_or(bbox_str.len());
            let coords: Vec<u32> = bbox_str[..bbox_end]
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();

            if coords.len() >= 4 {
                // Extract the word text (between > and </span>)
                if let Some(text) = extract_word_text(line) {
                    words.push(OcrWord {
                        text,
                        x: coords[0],
                        y: coords[1],
                        w: coords[2].saturating_sub(coords[0]),
                        h: coords[3].saturating_sub(coords[1]),
                    });
                }
            }
        }
    }

    words
}

/// Extract word text from an hOCR span element.
fn extract_word_text(line: &str) -> Option<String> {
    // Find the content between the last > and </span>
    let end_tag = "</span>";
    let end_pos = line.rfind(end_tag)?;
    let before_end = &line[..end_pos];
    let start_pos = before_end.rfind('>')? + 1;
    let text = &before_end[start_pos..];
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        // Decode HTML entities
        let text = text
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'");
        Some(text)
    }
}

/// Apply replacements to a page image by painting over matched words.
fn apply_replacements_to_image(
    img: &mut RgbaImage,
    words: &[OcrWord],
    replacements: &[PageReplacement],
) {
    let white = Rgba([255u8, 255, 255, 255]);
    let black = Rgba([0u8, 0, 0, 255]);

    // Load a font for drawing replacement text (try system fonts at runtime)
    let font = load_system_font();

    for replacement in replacements {
        // Find consecutive words that match the original text
        let original_words: Vec<&str> = replacement.original.split_whitespace().collect();
        if original_words.is_empty() {
            continue;
        }

        // Sliding window search through OCR words
        let mut i = 0;
        while i + original_words.len() <= words.len() {
            let window: Vec<&str> = words[i..i + original_words.len()]
                .iter()
                .map(|w| w.text.as_str())
                .collect();

            // Case-insensitive comparison
            let matches = window
                .iter()
                .zip(original_words.iter())
                .all(|(ocr, orig)| ocr.eq_ignore_ascii_case(orig));

            if matches {
                // Calculate bounding box spanning all matched words
                // Use min/max to handle any orientation (horizontal or vertical text)
                let matched_words = &words[i..i + original_words.len()];
                let bbox_x = matched_words.iter().map(|w| w.x).min().unwrap_or(0);
                let bbox_y = matched_words.iter().map(|w| w.y).min().unwrap_or(0);
                let bbox_right = matched_words.iter().map(|w| w.x + w.w).max().unwrap_or(0);
                let bbox_bottom = matched_words.iter().map(|w| w.y + w.h).max().unwrap_or(0);
                let bbox_w = bbox_right.saturating_sub(bbox_x);
                let bbox_h = bbox_bottom.saturating_sub(bbox_y).max(1);

                // Paint over with white (erase original text)
                let padding = 4;
                draw_filled_rect_mut(
                    img,
                    Rect::at(bbox_x as i32 - padding, bbox_y as i32 - padding)
                        .of_size(bbox_w + padding as u32 * 2, bbox_h + padding as u32 * 2),
                    white,
                );

                // Draw replacement text if we have a font
                if let Some(ref font) = font {
                    // Use the average height of individual matched words as font size
                    // (not the total bbox height, which spans multiple lines)
                    let matched_words = &words[i..i + original_words.len()];
                    let word_count = u32::try_from(matched_words.len()).unwrap_or(1).max(1);
                    let avg_word_h = matched_words.iter().map(|w| w.h).sum::<u32>() / word_count;
                    let font_scale = avg_word_h as f32 * 0.85;
                    draw_text_mut(
                        img,
                        black,
                        bbox_x as i32,
                        bbox_y as i32,
                        font_scale,
                        font,
                        &replacement.replacement,
                    );
                }

                i += original_words.len();
            } else {
                i += 1;
            }
        }
    }
}

/// Try to load a system font for drawing replacement text.
fn load_system_font() -> Option<ab_glyph::FontArc> {
    let font_paths = [
        // macOS
        "/System/Library/Fonts/Supplemental/Courier New.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/SFNS.ttf",
        // Linux (common distributions)
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/freefont/FreeMono.ttf",
        // Windows
        "C:\\Windows\\Fonts\\cour.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
        "C:\\Windows\\Fonts\\consola.ttf",
    ];

    for path in &font_paths {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = ab_glyph::FontArc::try_from_vec(data) {
                return Some(font);
            }
        }
    }

    tracing::warn!(
        "no system font found for replacement text rendering — PII will be whited out but replacement text won't be visible"
    );
    None
}

/// Assemble page images into a single PDF using JPEG encoding.
///
/// Each page image is JPEG-encoded and embedded as a full-page image
/// in the output PDF. This produces a readable PDF that looks like the original.
fn assemble_images_to_pdf(page_images: &[std::path::PathBuf], output_path: &Path) -> Result<()> {
    use std::io::Cursor;

    if page_images.is_empty() {
        return Err(Error::Ocr {
            path: output_path.to_path_buf(),
            reason: "no page images to assemble".to_string(),
        });
    }

    // Encode each page as JPEG and collect dimensions
    let mut jpeg_pages: Vec<(Vec<u8>, u32, u32)> = Vec::new();
    for img_path in page_images {
        let img = image::open(img_path).map_err(|e| Error::Ocr {
            path: output_path.to_path_buf(),
            reason: format!("failed to open image: {e}"),
        })?;
        let rgb = img.to_rgb8();
        let (w, h) = rgb.dimensions();

        let mut jpeg_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut jpeg_bytes);
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 70);
        encoder
            .encode(rgb.as_raw(), w, h, image::ExtendedColorType::Rgb8)
            .map_err(|e| Error::Ocr {
                path: output_path.to_path_buf(),
                reason: format!("failed to encode JPEG: {e}"),
            })?;

        jpeg_pages.push((jpeg_bytes, w, h));
    }

    // Build PDF with JPEG images
    // Each page needs: image XObject, content stream, page dict
    let num_pages = jpeg_pages.len();

    // Object layout:
    // 1 = Catalog, 2 = Pages, then for each page: (image, content, page)
    let total_objects = 2 + num_pages * 3;

    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");

    let mut offsets = vec![0usize; total_objects + 1];

    // Write objects, track offsets
    // Obj 1: Catalog
    offsets[1] = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    // Obj 2: Pages
    let page_obj_nums: Vec<usize> = (0..num_pages).map(|i| 5 + i * 3).collect();
    let kids = page_obj_nums
        .iter()
        .map(|n| format!("{n} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    offsets[2] = pdf.len();
    let pages_obj =
        format!("2 0 obj\n<< /Type /Pages /Kids [{kids}] /Count {num_pages} >>\nendobj\n");
    pdf.extend_from_slice(pages_obj.as_bytes());

    // For each page: image obj, content obj, page obj
    for (i, (jpeg_data, w, h)) in jpeg_pages.iter().enumerate() {
        let img_obj_num = 3 + i * 3;
        let content_obj_num = 4 + i * 3;
        let page_obj_num = 5 + i * 3;

        // Page dimensions in PDF points (72 DPI)
        let page_w_pt = 612.0_f64; // 8.5"
        let page_h_pt = 792.0_f64; // 11"

        // Image XObject (JPEG)
        offsets[img_obj_num] = pdf.len();
        let img_header = format!(
            "{img_obj_num} 0 obj\n<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpeg_data.len()
        );
        pdf.extend_from_slice(img_header.as_bytes());
        pdf.extend_from_slice(jpeg_data);
        pdf.extend_from_slice(b"\nendstream\nendobj\n");

        // Content stream (draw image scaled to page)
        let content = format!("q {page_w_pt} 0 0 {page_h_pt} 0 0 cm /Im1 Do Q");
        offsets[content_obj_num] = pdf.len();
        let content_obj = format!(
            "{content_obj_num} 0 obj\n<< /Length {} >>\nstream\n{content}\nendstream\nendobj\n",
            content.len()
        );
        pdf.extend_from_slice(content_obj.as_bytes());

        // Page dict
        offsets[page_obj_num] = pdf.len();
        let page_dict = format!(
            "{page_obj_num} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_w_pt} {page_h_pt}] /Contents {content_obj_num} 0 R /Resources << /XObject << /Im1 {img_obj_num} 0 R >> >> >>\nendobj\n"
        );
        pdf.extend_from_slice(page_dict.as_bytes());
    }

    // Cross-reference table
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", total_objects + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \r\n");
    for i in 1..=total_objects {
        pdf.extend_from_slice(format!("{:010} 00000 n \r\n", offsets[i]).as_bytes());
    }

    // Trailer
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF",
            total_objects + 1
        )
        .as_bytes(),
    );

    // Write file
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    std::fs::write(output_path, pdf).map_err(|source| Error::Io {
        path: output_path.to_path_buf(),
        source,
    })?;

    Ok(())
}

/// Assemble a hybrid PDF: clean pages copied from original, redacted pages as JPEG images.
///
/// Uses `lopdf` to load the original PDF and selectively replace pages that
/// contain PII with JPEG-encoded images. Unchanged pages are kept as-is,
/// preserving their original vector quality and small size.
fn assemble_hybrid_pdf(
    original_path: &Path,
    page_results: &[PageResult],
    output_path: &Path,
) -> Result<()> {
    use lopdf::Document;
    use std::io::Cursor;

    // Check if ALL pages need redaction (fall back to full image PDF)
    let has_clean_pages = page_results.iter().any(|r| matches!(r, PageResult::Clean));

    if !has_clean_pages {
        // All pages modified — use the image-only assembly
        let modified_paths: Vec<std::path::PathBuf> = page_results
            .iter()
            .filter_map(|r| match r {
                PageResult::Modified(path) => Some(path.clone()),
                PageResult::Clean => None,
            })
            .collect();
        return assemble_images_to_pdf(&modified_paths, output_path);
    }

    // Hybrid approach: start with the original PDF, replace only modified pages
    let mut doc = Document::load(original_path).map_err(|e| Error::Ocr {
        path: original_path.to_path_buf(),
        reason: format!("failed to load original PDF with lopdf: {e}"),
    })?;

    let page_ids: Vec<_> = doc.page_iter().collect();

    for (i, page_result) in page_results.iter().enumerate() {
        if let PageResult::Modified(img_path) = page_result {
            if i >= page_ids.len() {
                break;
            }

            // Load the JPEG-encoded redacted page
            let img = image::open(img_path).map_err(|e| Error::Ocr {
                path: output_path.to_path_buf(),
                reason: format!("failed to open redacted page image: {e}"),
            })?;
            let rgb = img.to_rgb8();
            let (w, h) = rgb.dimensions();

            // Encode as JPEG
            let mut jpeg_bytes = Vec::new();
            let cursor = Cursor::new(&mut jpeg_bytes);
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(cursor, 70);
            encoder
                .encode(rgb.as_raw(), w, h, image::ExtendedColorType::Rgb8)
                .map_err(|e| Error::Ocr {
                    path: output_path.to_path_buf(),
                    reason: format!("failed to encode JPEG: {e}"),
                })?;

            // Replace the page content in the PDF with the JPEG image
            // Create an image XObject
            let mut img_dict = lopdf::Dictionary::new();
            img_dict.set("Type", lopdf::Object::Name(b"XObject".to_vec()));
            img_dict.set("Subtype", lopdf::Object::Name(b"Image".to_vec()));
            img_dict.set("Width", lopdf::Object::Integer(i64::from(w)));
            img_dict.set("Height", lopdf::Object::Integer(i64::from(h)));
            img_dict.set("ColorSpace", lopdf::Object::Name(b"DeviceRGB".to_vec()));
            img_dict.set("BitsPerComponent", lopdf::Object::Integer(8));
            img_dict.set("Filter", lopdf::Object::Name(b"DCTDecode".to_vec()));
            img_dict.set("Length", lopdf::Object::Integer(jpeg_bytes.len() as i64));

            let img_stream = lopdf::Stream::new(img_dict, jpeg_bytes);
            let img_id = doc.add_object(img_stream);

            // Create a content stream that draws the image full-page
            let page_w = 612.0; // US Letter width in points
            let page_h = 792.0; // US Letter height in points
            let content = format!("q {page_w} 0 0 {page_h} 0 0 cm /Img Do Q");

            let mut content_dict = lopdf::Dictionary::new();
            content_dict.set("Length", lopdf::Object::Integer(content.len() as i64));
            let content_stream = lopdf::Stream::new(content_dict, content.into_bytes());
            let content_id = doc.add_object(content_stream);

            // Update the page dictionary
            let page_id = page_ids[i];
            if let Ok(lopdf::Object::Dictionary(dict)) = doc.get_object_mut(page_id) {
                // Set new content stream
                dict.set("Contents", lopdf::Object::Reference(content_id));
                // Set resources with the image
                let mut xobject_dict = lopdf::Dictionary::new();
                xobject_dict.set("Img", lopdf::Object::Reference(img_id));
                let mut resources_dict = lopdf::Dictionary::new();
                resources_dict.set("XObject", lopdf::Object::Dictionary(xobject_dict));
                dict.set("Resources", lopdf::Object::Dictionary(resources_dict));
                // Set media box
                dict.set(
                    "MediaBox",
                    lopdf::Object::Array(vec![
                        lopdf::Object::Integer(0),
                        lopdf::Object::Integer(0),
                        lopdf::Object::Real(page_w),
                        lopdf::Object::Real(page_h),
                    ]),
                );
            }
        }
        // PageResult::Clean — leave the page as-is in the document
    }

    // Save the modified document
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    doc.save(output_path).map_err(|e| Error::Ocr {
        path: output_path.to_path_buf(),
        reason: format!("failed to save hybrid PDF: {e}"),
    })?;

    Ok(())
}
