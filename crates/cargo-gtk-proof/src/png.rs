// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded PNG primitives used by the compatibility corpus.
//!
//! This module intentionally avoids image-library policy decisions: the visual
//! proof runner needs deterministic byte-level crop, mask, and anchor behavior
//! that can be compared against the existing Python oracle.

use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use crc32fast::Hasher;
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use serde::Serialize;
use serde_json::Value;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
// The Rust live runner is not authoritative yet, but the corpus code should
// already reject oversized or hostile artifacts before decoding them in CI.
const MAX_COMPRESSED_PNG_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DECODED_PNG_BYTES: usize = 64 * 1024 * 1024;
const MAX_PNG_PIXELS: usize = 4096 * 4096;

type PngResult<T> = Result<T, String>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PngImage {
    width: usize,
    height: usize,
    bit_depth: u8,
    color_type: u8,
    bpp: usize,
    rows: Vec<Vec<u8>>,
}

/// Integer rectangle used for deterministic PNG crop and anchor evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Rect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

impl Rect {
    /// Build a rectangle without clamping; callers clamp against each image.
    pub(crate) const fn new(x: i32, y: i32, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Result of running one pixel-anchor detector against one bounded crop.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub(crate) struct PixelAnchorDetection {
    pub(crate) name: String,
    pub(crate) detector: String,
    pub(crate) status: String,
    pub(crate) row_y: Option<i32>,
    pub(crate) rect: Rect,
    pub(crate) matched_pixels: usize,
    pub(crate) required_pixels: usize,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct CropComparison {
    status: String,
    before_rect: Rect,
    after_rect: Rect,
    mask_rects: Vec<Rect>,
    allowed_changing_regions: Vec<Rect>,
    compared_pixels: usize,
    diff_pixels: usize,
    first_difference: Option<DifferencePoint>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct DifferencePoint {
    x: usize,
    y: usize,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct RenderedAnchorPairReport {
    name: String,
    detector: String,
    status: String,
    before: PixelAnchorDetection,
    after: PixelAnchorDetection,
    screen_y_delta: Option<i32>,
    max_screen_y_delta: Option<i32>,
    app_geometry: Option<AppRenderedAnchorGeometry>,
    diagnostics: Vec<AppVsRenderedDisagreement>,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct AppRenderedAnchorGeometry {
    screen_y_delta: i32,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
struct AppVsRenderedDisagreement {
    name: String,
    status: String,
    app_screen_y_delta: i32,
    rendered_screen_y_delta: i32,
    max_screen_y_delta: i32,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct PngCorpusStats {
    pub(crate) compared: u64,
    pub(crate) failed: u64,
    pub(crate) detail: String,
}

pub(crate) fn run_embedded_png_corpus() -> PngCorpusStats {
    match run_embedded_png_corpus_inner() {
        Ok(compared) => PngCorpusStats {
            compared,
            failed: 0,
            detail: "embedded PNG corpus passed".to_string(),
        },
        Err(error) => PngCorpusStats {
            compared: 1,
            failed: 1,
            detail: error,
        },
    }
}

fn run_embedded_png_corpus_inner() -> PngResult<u64> {
    let root = tempfile::Builder::new()
        .prefix("cargo-gtk-proof-png-corpus-")
        .tempdir()
        .map_err(|error| error.to_string())?;

    run_png_corpus_at(root.path())
}

fn run_png_corpus_at(root: &Path) -> PngResult<u64> {
    let before = root.join("before.png");
    let after = root.join("after.png");
    let black = (0, 0, 0, 255);
    let red = (255, 0, 0, 255);
    write_rgba(&before, &[vec![black, black], vec![black, black]])?;
    write_rgba(&after, &[vec![black, red], vec![black, black]])?;

    let failed = compare_crops(&before, &after, Rect::new(0, 0, 2, 2), None, &[], None)?;
    if failed.status != "failed" || failed.diff_pixels != 1 {
        return Err("exact crop comparison did not detect the changed pixel".to_string());
    }

    let masked = compare_crops(
        &before,
        &after,
        Rect::new(0, 0, 2, 2),
        None,
        &[Rect::new(1, 0, 1, 1)],
        None,
    )?;
    if masked.status != "passed" || masked.diff_pixels != 0 {
        return Err("masked crop comparison did not ignore the changed pixel".to_string());
    }
    if masked.allowed_changing_regions != vec![Rect::new(1, 0, 1, 1)] {
        return Err("masked crop comparison did not report allowed-changing regions".to_string());
    }

    let clamped = clamp_rect(&read_png(&before)?, Rect::new(-1, 0, 4, 2))?;
    if clamped != Rect::new(0, 0, 2, 2) {
        return Err("clamped rectangle did not match image bounds".to_string());
    }

    let anchor_path = root.join("anchors.png");
    write_rgba(&anchor_path, &native_minimap_anchor_rows())?;
    let anchor_image = read_png(&anchor_path)?;
    let edge = detect_pixel_anchor(
        &anchor_image,
        "minimap-native-viewport-top-edge",
        Rect::new(0, 0, 40, 12),
        "native-minimap-viewport-top-edge-row",
        12,
    )?;
    let content = detect_pixel_anchor(
        &anchor_image,
        "minimap-first-content-row",
        Rect::new(0, 0, 40, 12),
        "minimap-content-row",
        20,
    )?;
    if edge.status != "passed" || edge.row_y != Some(3) {
        return Err("native minimap edge detector missed the synthetic edge".to_string());
    }
    if content.status != "passed" || content.row_y != Some(6) {
        return Err("minimap content detector missed the synthetic content row".to_string());
    }

    let before_anchor = root.join("before-anchor.png");
    let after_anchor = root.join("after-anchor.png");
    write_rgba(&before_anchor, &anchor_drift_rows(2))?;
    write_rgba(&after_anchor, &anchor_drift_rows(3))?;
    let rendered = compare_rendered_anchor_pair(RenderedAnchorPairInput {
        before_path: &before_anchor,
        after_path: &after_anchor,
        name: "minimap-native-viewport-top-edge",
        before_rect: Rect::new(0, 0, 16, 8),
        after_rect: Rect::new(0, 0, 16, 8),
        detector: "native-minimap-viewport-top-edge-row",
        min_pixels: 8,
        max_screen_y_delta: Some(0),
        app_screen_y_delta: Some(0),
    })?;
    if rendered.status != "failed"
        || rendered.screen_y_delta != Some(1)
        || rendered.diagnostics.is_empty()
    {
        return Err("rendered-anchor drift did not fail despite stable app geometry".to_string());
    }

    Ok(6)
}

fn native_minimap_anchor_rows() -> Vec<Vec<(u8, u8, u8, u8)>> {
    let bg = (29, 29, 32, 255);
    let fill = (61, 61, 63, 255);
    let edge = (150, 150, 151, 255);
    let cyan = (51, 178, 164, 255);
    let mut rows = vec![vec![bg; 40]; 12];
    for pixel in &mut rows[3][8..21] {
        *pixel = edge;
    }
    for pixel in &mut rows[4][8..29] {
        *pixel = fill;
    }
    for pixel in &mut rows[6][4..24] {
        *pixel = cyan;
    }
    rows
}

fn anchor_drift_rows(edge_row: usize) -> Vec<Vec<(u8, u8, u8, u8)>> {
    let bg = (29, 29, 32, 255);
    let edge = (150, 150, 151, 255);
    let mut rows = vec![vec![bg; 16]; 8];
    for pixel in &mut rows[edge_row][3..13] {
        *pixel = edge;
    }
    rows
}

fn write_rgba(path: &Path, pixels: &[Vec<(u8, u8, u8, u8)>]) -> PngResult<()> {
    let Some(first_row) = pixels.first() else {
        return Err("cannot write empty RGBA image".to_string());
    };
    let rows: Vec<Vec<u8>> = pixels
        .iter()
        .map(|row| {
            row.iter()
                .flat_map(|pixel| [pixel.0, pixel.1, pixel.2, pixel.3])
                .collect()
        })
        .collect();
    let image = PngImage {
        width: first_row.len(),
        height: pixels.len(),
        bit_depth: 8,
        color_type: 6,
        bpp: 4,
        rows: rows.clone(),
    };
    write_png(path, &image, &rows)
}

/// Write a tiny RGBA PNG fixture for tests that exercise live proof reports.
#[cfg(test)]
pub(crate) fn write_rgba_fixture(path: &Path, pixels: &[Vec<(u8, u8, u8, u8)>]) -> PngResult<()> {
    write_rgba(path, pixels)
}

fn read_png(path: &Path) -> PngResult<PngImage> {
    let size = fs::metadata(path).map_err(|error| error.to_string())?.len();
    if size > MAX_COMPRESSED_PNG_BYTES {
        return Err(format!(
            "PNG {} exceeds compressed byte limit of {}",
            path.display(),
            MAX_COMPRESSED_PNG_BYTES
        ));
    }
    let data = fs::read(path).map_err(|error| error.to_string())?;
    let (ihdr, compressed) = read_chunks(&data)?;
    if ihdr.bit_depth != 8 {
        return Err(format!("unsupported PNG bit depth {}", ihdr.bit_depth));
    }
    let bpp = bytes_per_pixel(ihdr.color_type)?;
    let row_bytes = ihdr
        .width
        .checked_mul(bpp)
        .ok_or_else(|| "PNG row byte size overflow".to_string())?;
    let pixel_count = ihdr
        .width
        .checked_mul(ihdr.height)
        .ok_or_else(|| "PNG pixel count overflow".to_string())?;
    if pixel_count > MAX_PNG_PIXELS {
        return Err(format!(
            "PNG {} exceeds pixel limit of {}",
            path.display(),
            MAX_PNG_PIXELS
        ));
    }
    let decoded_bytes = row_bytes
        .checked_add(1)
        .and_then(|stride| stride.checked_mul(ihdr.height))
        .ok_or_else(|| "PNG decoded byte size overflow".to_string())?;
    if decoded_bytes > MAX_DECODED_PNG_BYTES {
        return Err(format!(
            "PNG {} exceeds decoded byte limit of {}",
            path.display(),
            MAX_DECODED_PNG_BYTES
        ));
    }
    let decoder = ZlibDecoder::new(compressed.as_slice());
    let mut raw = Vec::with_capacity(decoded_bytes);
    decoder
        .take(u64::try_from(decoded_bytes + 1).unwrap_or(u64::MAX))
        .read_to_end(&mut raw)
        .map_err(|error| error.to_string())?;
    if raw.len() != decoded_bytes {
        return Err(format!(
            "PNG {} decoded to {} bytes, expected {}",
            path.display(),
            raw.len(),
            decoded_bytes
        ));
    }
    let rows = unfilter_rows(&raw, ihdr.width, ihdr.height, bpp)?;
    Ok(PngImage {
        width: ihdr.width,
        height: ihdr.height,
        bit_depth: ihdr.bit_depth,
        color_type: ihdr.color_type,
        bpp,
        rows,
    })
}

#[derive(Debug, Eq, PartialEq)]
struct Ihdr {
    width: usize,
    height: usize,
    bit_depth: u8,
    color_type: u8,
}

fn read_chunks(data: &[u8]) -> PngResult<(Ihdr, Vec<u8>)> {
    if !data.starts_with(PNG_SIGNATURE) {
        return Err("not a PNG file".to_string());
    }

    let mut offset = PNG_SIGNATURE.len();
    let mut ihdr = None;
    let mut idat = Vec::new();
    while offset < data.len() {
        if offset + 8 > data.len() {
            return Err("truncated PNG chunk header".to_string());
        }
        let length = read_be_u32(data, offset)? as usize;
        let chunk_type = data
            .get(offset + 4..offset + 8)
            .ok_or_else(|| "truncated PNG chunk type".to_string())?;
        offset += 8;
        let chunk_data = data
            .get(offset..offset + length)
            .ok_or_else(|| "truncated PNG chunk data".to_string())?;
        offset = offset
            .checked_add(length)
            .and_then(|value| value.checked_add(4))
            .ok_or_else(|| "PNG chunk offset overflow".to_string())?;

        match chunk_type {
            b"IHDR" => ihdr = Some(parse_ihdr(chunk_data)?),
            b"IDAT" => idat.extend_from_slice(chunk_data),
            b"IEND" => break,
            _ => {}
        }
    }

    let Some(ihdr) = ihdr else {
        return Err("missing IHDR".to_string());
    };
    if idat.is_empty() {
        return Err("missing IDAT".to_string());
    }
    Ok((ihdr, idat))
}

fn parse_ihdr(data: &[u8]) -> PngResult<Ihdr> {
    if data.len() != 13 {
        return Err("invalid IHDR length".to_string());
    }
    Ok(Ihdr {
        width: read_be_u32(data, 0)? as usize,
        height: read_be_u32(data, 4)? as usize,
        bit_depth: data[8],
        color_type: data[9],
    })
}

fn read_be_u32(data: &[u8], offset: usize) -> PngResult<u32> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| "truncated big-endian integer".to_string())?;
    let array: [u8; 4] = bytes
        .try_into()
        .map_err(|_| "invalid big-endian integer".to_string())?;
    Ok(u32::from_be_bytes(array))
}

fn bytes_per_pixel(color_type: u8) -> PngResult<usize> {
    match color_type {
        0 => Ok(1),
        2 => Ok(3),
        4 => Ok(2),
        6 => Ok(4),
        other => Err(format!("unsupported PNG color type {other}")),
    }
}

fn unfilter_rows(raw: &[u8], width: usize, height: usize, bpp: usize) -> PngResult<Vec<Vec<u8>>> {
    let stride = width
        .checked_mul(bpp)
        .ok_or_else(|| "PNG row stride overflow".to_string())?;
    let mut rows = Vec::with_capacity(height);
    let mut offset = 0usize;
    let mut previous = vec![0; stride];
    for _ in 0..height {
        if offset + stride + 1 > raw.len() {
            return Err("decompressed image data is truncated".to_string());
        }
        let filter_type = raw[offset];
        offset += 1;
        let encoded = &raw[offset..offset + stride];
        offset += stride;
        let mut decoded = vec![0; stride];
        for (index, value) in encoded.iter().copied().enumerate() {
            let left = if index >= bpp {
                decoded[index - bpp]
            } else {
                0
            };
            let above = previous[index];
            let upper_left = if index >= bpp {
                previous[index - bpp]
            } else {
                0
            };
            decoded[index] = match filter_type {
                0 => value,
                1 => value.wrapping_add(left),
                2 => value.wrapping_add(above),
                3 => {
                    let average = u8::try_from(u16::midpoint(u16::from(left), u16::from(above)))
                        .map_err(|error| error.to_string())?;
                    value.wrapping_add(average)
                }
                4 => value.wrapping_add(paeth(left, above, upper_left)),
                other => return Err(format!("unsupported PNG filter {other}")),
            };
        }
        previous = decoded.clone();
        rows.push(decoded);
    }
    Ok(rows)
}

fn paeth(left: u8, above: u8, upper_left: u8) -> u8 {
    let left_i = i32::from(left);
    let above_i = i32::from(above);
    let upper_left_i = i32::from(upper_left);
    let estimate = left_i + above_i - upper_left_i;
    let dist_left = (estimate - left_i).abs();
    let dist_above = (estimate - above_i).abs();
    let dist_upper_left = (estimate - upper_left_i).abs();
    if dist_left <= dist_above && dist_left <= dist_upper_left {
        left
    } else if dist_above <= dist_upper_left {
        above
    } else {
        upper_left
    }
}

fn write_png(path: &Path, image: &PngImage, rows: &[Vec<u8>]) -> PngResult<()> {
    let Some(parent) = path.parent() else {
        return Err(format!("PNG path has no parent: {}", path.display()));
    };
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let height = rows.len();
    let width = rows.first().map_or(0, |row| row.len() / image.bpp);
    let mut raw = Vec::new();
    for row in rows {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&raw).map_err(|error| error.to_string())?;
    let compressed = encoder.finish().map_err(|error| error.to_string())?;

    let mut ihdr = Vec::with_capacity(13);
    let width = u32::try_from(width).map_err(|error| error.to_string())?;
    let height = u32::try_from(height).map_err(|error| error.to_string())?;
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, image.color_type, 0, 0, 0]);

    let mut data = Vec::new();
    data.extend_from_slice(PNG_SIGNATURE);
    data.extend_from_slice(&png_chunk(*b"IHDR", &ihdr)?);
    data.extend_from_slice(&png_chunk(*b"IDAT", &compressed)?);
    data.extend_from_slice(&png_chunk(*b"IEND", &[])?);
    fs::write(path, data).map_err(|error| error.to_string())
}

fn png_chunk(kind: [u8; 4], data: &[u8]) -> PngResult<Vec<u8>> {
    let mut chunk = Vec::with_capacity(data.len() + 12);
    let length = u32::try_from(data.len()).map_err(|error| error.to_string())?;
    chunk.extend_from_slice(&length.to_be_bytes());
    chunk.extend_from_slice(&kind);
    chunk.extend_from_slice(data);
    let mut hasher = Hasher::new();
    hasher.update(&kind);
    hasher.update(data);
    chunk.extend_from_slice(&hasher.finalize().to_be_bytes());
    Ok(chunk)
}

fn validate_rect(image: &PngImage, rect: Rect) -> PngResult<()> {
    if rect.width == 0 || rect.height == 0 {
        return Err(format!("invalid empty crop: {rect:?}"));
    }
    if rect.x < 0 || rect.y < 0 {
        return Err(format!("crop starts outside image: {rect:?}"));
    }
    let x = usize::try_from(rect.x).map_err(|error| error.to_string())?;
    let y = usize::try_from(rect.y).map_err(|error| error.to_string())?;
    if x + rect.width > image.width || y + rect.height > image.height {
        return Err(format!(
            "crop exceeds image bounds {}x{}: {rect:?}",
            image.width, image.height
        ));
    }
    Ok(())
}

fn clamp_rect(image: &PngImage, rect: Rect) -> PngResult<Rect> {
    let x0 = rect.x.max(0);
    let y0 = rect.y.max(0);
    let x1 = (rect.x + i32::try_from(rect.width).map_err(|error| error.to_string())?)
        .min(i32::try_from(image.width).map_err(|error| error.to_string())?);
    let y1 = (rect.y + i32::try_from(rect.height).map_err(|error| error.to_string())?)
        .min(i32::try_from(image.height).map_err(|error| error.to_string())?);
    let clamped = Rect::new(
        x0,
        y0,
        usize::try_from(x1 - x0).map_err(|error| error.to_string())?,
        usize::try_from(y1 - y0).map_err(|error| error.to_string())?,
    );
    validate_rect(image, clamped)?;
    Ok(clamped)
}

fn crop_rows(image: &PngImage, rect: Rect) -> PngResult<Vec<Vec<u8>>> {
    validate_rect(image, rect)?;
    let x = usize::try_from(rect.x).map_err(|error| error.to_string())?;
    let y = usize::try_from(rect.y).map_err(|error| error.to_string())?;
    let start = x * image.bpp;
    let end = (x + rect.width) * image.bpp;
    Ok(image.rows[y..y + rect.height]
        .iter()
        .map(|row| row[start..end].to_vec())
        .collect())
}

fn pixel_rgba(image: &PngImage, x: usize, y: usize) -> PngResult<(u8, u8, u8, u8)> {
    validate_rect(
        image,
        Rect::new(
            i32::try_from(x).map_err(|error| error.to_string())?,
            i32::try_from(y).map_err(|error| error.to_string())?,
            1,
            1,
        ),
    )?;
    let start = x * image.bpp;
    let values = &image.rows[y][start..start + image.bpp];
    match image.color_type {
        6 => Ok((values[0], values[1], values[2], values[3])),
        2 => Ok((values[0], values[1], values[2], 255)),
        4 => Ok((values[0], values[0], values[0], values[1])),
        0 => Ok((values[0], values[0], values[0], 255)),
        other => Err(format!("unsupported PNG color type {other}")),
    }
}

fn color_distance(first: (u8, u8, u8, u8), second: (u8, u8, u8, u8)) -> u8 {
    first
        .0
        .abs_diff(second.0)
        .max(first.1.abs_diff(second.1))
        .max(first.2.abs_diff(second.2))
        .max(first.3.abs_diff(second.3))
}

fn is_neutral(pixel: (u8, u8, u8, u8), tolerance: u8) -> bool {
    let (red, green, blue, alpha) = pixel;
    alpha > 0 && red.max(green).max(blue) - red.min(green).min(blue) <= tolerance
}

fn is_minimap_content_pixel(pixel: (u8, u8, u8, u8)) -> bool {
    let (red, green, blue, alpha) = pixel;
    if alpha == 0 {
        return false;
    }
    let high = red.max(green).max(blue);
    let low = red.min(green).min(blue);
    let average = (u16::from(red) + u16::from(green) + u16::from(blue)) / 3;
    let chroma = high - low;
    (high >= 120 && chroma >= 24) || average <= 252
}

fn is_viewport_highlight_fill_pixel(pixel: (u8, u8, u8, u8)) -> bool {
    let (red, green, blue, alpha) = pixel;
    if alpha == 0 {
        return false;
    }
    let high = red.max(green).max(blue);
    let low = red.min(green).min(blue);
    let average = (u16::from(red) + u16::from(green) + u16::from(blue)) / 3;
    high - low <= 24 && (45..=230).contains(&average)
}

fn is_native_minimap_viewport_edge_pixel(pixel: (u8, u8, u8, u8)) -> bool {
    let (red, green, blue, alpha) = pixel;
    if alpha == 0 || !is_neutral(pixel, 16) {
        return false;
    }
    let average = (u16::from(red) + u16::from(green) + u16::from(blue)) / 3;
    (72..=210).contains(&average)
}

fn is_minimap_search_marker_pixel(pixel: (u8, u8, u8, u8)) -> bool {
    let (red, green, blue, alpha) = pixel;
    alpha > 0
        && red >= 180
        && (70..=190).contains(&green)
        && blue <= 100
        && red > green
        && green > blue
}

fn detect_pixel_anchor(
    image: &PngImage,
    name: &str,
    rect: Rect,
    detector: &str,
    min_pixels: usize,
) -> PngResult<PixelAnchorDetection> {
    validate_rect(image, rect)?;
    let y_start = usize::try_from(rect.y).map_err(|error| error.to_string())?;
    let y_end = y_start + rect.height;
    let mut best_row_y = None;
    let mut best_count = 0;
    for y in y_start..y_end {
        let count = match detector {
            "horizontal-neutral-edge-row" => horizontal_edge_count(image, rect, y)?,
            "native-minimap-viewport-top-edge-row" => {
                native_minimap_viewport_edge_count(image, rect, y)?
            }
            "minimap-content-row" => minimap_content_count(image, rect, y, min_pixels)?,
            "minimap-search-marker-row" => {
                row_pixel_count(image, rect, y, is_minimap_search_marker_pixel)?
            }
            "viewport-highlight-fill-row" => {
                row_pixel_count(image, rect, y, is_viewport_highlight_fill_pixel)?
            }
            "non-background-row" => non_background_count(image, rect, y)?,
            other => return Err(format!("unsupported pixel anchor detector: {other}")),
        };
        if count > best_count {
            best_count = count;
            best_row_y = Some(i32::try_from(y).map_err(|error| error.to_string())?);
        }
        if count >= min_pixels {
            return Ok(PixelAnchorDetection {
                name: name.to_string(),
                detector: detector.to_string(),
                status: "passed".to_string(),
                row_y: Some(i32::try_from(y).map_err(|error| error.to_string())?),
                rect,
                matched_pixels: count,
                required_pixels: min_pixels,
            });
        }
    }
    Ok(PixelAnchorDetection {
        name: name.to_string(),
        detector: detector.to_string(),
        status: "failed".to_string(),
        row_y: best_row_y,
        rect,
        matched_pixels: best_count,
        required_pixels: min_pixels,
    })
}

/// Detect one anchor from a PNG file and optionally write the evaluated crop.
pub(crate) fn detect_pixel_anchor_in_file(
    image_path: &Path,
    name: &str,
    rect: Rect,
    detector: &str,
    min_pixels: usize,
    crop_path: Option<&Path>,
) -> PngResult<PixelAnchorDetection> {
    let image = read_png(image_path)?;
    let rect = clamp_rect(&image, rect)?;
    let detection = detect_pixel_anchor(&image, name, rect, detector, min_pixels)?;
    if let Some(crop_path) = crop_path {
        write_png(crop_path, &image, &crop_rows(&image, rect)?)?;
    }
    Ok(detection)
}

/// Compare two PNG crops from files and optionally write the evaluated crops.
pub(crate) fn compare_crops_in_files(
    before_path: &Path,
    after_path: &Path,
    before_rect: Rect,
    after_rect: Option<Rect>,
    masks: &[Rect],
    artifact_prefix: Option<&Path>,
) -> PngResult<Value> {
    let before_image = read_png(before_path)?;
    let after_image = read_png(after_path)?;
    let before_rect = clamp_rect(&before_image, before_rect)?;
    let after_rect = after_rect
        .map(|rect| clamp_rect(&after_image, rect))
        .transpose()?;
    let report = compare_crops(
        before_path,
        after_path,
        before_rect,
        after_rect,
        masks,
        artifact_prefix,
    )?;
    serde_json::to_value(report).map_err(|error| error.to_string())
}

fn native_minimap_viewport_edge_count(image: &PngImage, rect: Rect, y: usize) -> PngResult<usize> {
    longest_row_run_count(image, rect, y, is_native_minimap_viewport_edge_pixel)
}

fn minimap_content_count(
    image: &PngImage,
    rect: Rect,
    y: usize,
    min_pixels: usize,
) -> PngResult<usize> {
    let chrome_run_threshold = 8;
    if native_minimap_viewport_edge_count(image, rect, y)? >= chrome_run_threshold {
        return Ok(0);
    }

    let x_start = usize::try_from(rect.x).map_err(|error| error.to_string())?;
    let pixels: Vec<_> = (x_start..x_start + rect.width)
        .map(|x| pixel_rgba(image, x, y))
        .collect::<PngResult<_>>()?;
    if pixels.is_empty() {
        return Ok(0);
    }
    let saturated_count = pixels
        .iter()
        .filter(|pixel| {
            let high = pixel.0.max(pixel.1).max(pixel.2);
            let low = pixel.0.min(pixel.1).min(pixel.2);
            pixel.3 > 0 && high >= 120 && high - low >= 24
        })
        .count();
    if saturated_count >= min_pixels {
        return Ok(saturated_count);
    }
    let background = most_common_pixel(&pixels);
    let count = pixels
        .iter()
        .filter(|pixel| {
            color_distance(**pixel, background) >= 4 && is_minimap_content_pixel(**pixel)
        })
        .count();
    let fill_run = longest_row_run_count(image, rect, y, is_viewport_highlight_fill_pixel)?;
    if fill_run >= chrome_run_threshold && count < 20.max(min_pixels * 2) {
        return Ok(0);
    }
    Ok(count)
}

fn most_common_pixel(pixels: &[(u8, u8, u8, u8)]) -> (u8, u8, u8, u8) {
    let mut best_pixel = pixels[0];
    let mut best_count = 0;
    for candidate in pixels {
        let count = pixels.iter().filter(|pixel| *pixel == candidate).count();
        if count > best_count {
            best_count = count;
            best_pixel = *candidate;
        }
    }
    best_pixel
}

fn longest_row_run_count(
    image: &PngImage,
    rect: Rect,
    y: usize,
    predicate: fn((u8, u8, u8, u8)) -> bool,
) -> PngResult<usize> {
    let x_start = usize::try_from(rect.x).map_err(|error| error.to_string())?;
    let mut longest = 0;
    let mut current = 0;
    for x in x_start..x_start + rect.width {
        if predicate(pixel_rgba(image, x, y)?) {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    Ok(longest)
}

fn horizontal_edge_count(image: &PngImage, rect: Rect, y: usize) -> PngResult<usize> {
    let Some(above_y) = y.checked_sub(1) else {
        return Ok(0);
    };
    let below_y = y + 1;
    if below_y >= image.height {
        return Ok(0);
    }
    let x_start = usize::try_from(rect.x).map_err(|error| error.to_string())?;
    let mut count = 0;
    for x in x_start..x_start + rect.width {
        let pixel = pixel_rgba(image, x, y)?;
        let contrasts = [above_y, below_y].map(|compare_y| {
            let other = pixel_rgba(image, x, compare_y)?;
            Ok((is_neutral(pixel, 18) || is_neutral(other, 18))
                && color_distance(pixel, other) >= 6)
        });
        if contrasts
            .into_iter()
            .collect::<PngResult<Vec<_>>>()?
            .into_iter()
            .all(|contrast| contrast)
        {
            count += 1;
        }
    }
    Ok(count)
}

fn row_pixel_count(
    image: &PngImage,
    rect: Rect,
    y: usize,
    predicate: fn((u8, u8, u8, u8)) -> bool,
) -> PngResult<usize> {
    let x_start = usize::try_from(rect.x).map_err(|error| error.to_string())?;
    (x_start..x_start + rect.width).try_fold(0, |count, x| {
        Ok(count + usize::from(predicate(pixel_rgba(image, x, y)?)))
    })
}

fn non_background_count(image: &PngImage, rect: Rect, y: usize) -> PngResult<usize> {
    let x_start = usize::try_from(rect.x).map_err(|error| error.to_string())?;
    let pixels: Vec<_> = (x_start..x_start + rect.width)
        .map(|x| pixel_rgba(image, x, y))
        .collect::<PngResult<_>>()?;
    if pixels.is_empty() {
        return Ok(0);
    }
    let background = most_common_pixel(&pixels);
    Ok(pixels
        .iter()
        .filter(|pixel| color_distance(**pixel, background) >= 4)
        .count())
}

fn pixel_masked(x: usize, y: usize, masks: &[Rect]) -> bool {
    masks.iter().any(|mask| {
        let Ok(mask_x) = usize::try_from(mask.x) else {
            return false;
        };
        let Ok(mask_y) = usize::try_from(mask.y) else {
            return false;
        };
        x >= mask_x && x < mask_x + mask.width && y >= mask_y && y < mask_y + mask.height
    })
}

fn compare_crops(
    before_path: &Path,
    after_path: &Path,
    before_rect: Rect,
    after_rect: Option<Rect>,
    masks: &[Rect],
    artifact_prefix: Option<&Path>,
) -> PngResult<CropComparison> {
    let before = read_png(before_path)?;
    let after = read_png(after_path)?;
    let after_rect = after_rect.unwrap_or(before_rect);
    if before.bpp != after.bpp || before.color_type != after.color_type {
        return Err("PNG color formats differ".to_string());
    }
    if before_rect.width != after_rect.width || before_rect.height != after_rect.height {
        return Err("crop sizes differ".to_string());
    }

    let before_rows = crop_rows(&before, before_rect)?;
    let after_rows = crop_rows(&after, after_rect)?;
    let mut diff_pixels = 0;
    let mut compared_pixels = 0;
    let mut first_difference = None;
    for (y, (before_row, after_row)) in before_rows.iter().zip(after_rows.iter()).enumerate() {
        for x in 0..before_rect.width {
            if pixel_masked(x, y, masks) {
                continue;
            }
            compared_pixels += 1;
            let start = x * before.bpp;
            let end = start + before.bpp;
            if before_row[start..end] != after_row[start..end] {
                diff_pixels += 1;
                first_difference.get_or_insert(DifferencePoint { x, y });
            }
        }
    }

    if let Some(prefix) = artifact_prefix {
        let before_artifact = prefix.with_file_name(format!(
            "{}-before.png",
            prefix
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("crop")
        ));
        let after_artifact = prefix.with_file_name(format!(
            "{}-after.png",
            prefix
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("crop")
        ));
        write_png(&before_artifact, &before, &before_rows)?;
        write_png(&after_artifact, &after, &after_rows)?;
    }

    Ok(CropComparison {
        status: if diff_pixels == 0 { "passed" } else { "failed" }.to_string(),
        before_rect,
        after_rect,
        mask_rects: masks.to_vec(),
        allowed_changing_regions: masks.to_vec(),
        compared_pixels,
        diff_pixels,
        first_difference,
    })
}

#[derive(Clone, Copy)]
struct RenderedAnchorPairInput<'a> {
    before_path: &'a Path,
    after_path: &'a Path,
    name: &'a str,
    before_rect: Rect,
    after_rect: Rect,
    detector: &'a str,
    min_pixels: usize,
    max_screen_y_delta: Option<i32>,
    app_screen_y_delta: Option<i32>,
}

fn compare_rendered_anchor_pair(
    input: RenderedAnchorPairInput<'_>,
) -> PngResult<RenderedAnchorPairReport> {
    let RenderedAnchorPairInput {
        before_path,
        after_path,
        name,
        before_rect,
        after_rect,
        detector,
        min_pixels,
        max_screen_y_delta,
        app_screen_y_delta,
    } = input;
    let before_image = read_png(before_path)?;
    let after_image = read_png(after_path)?;
    let before = detect_pixel_anchor(&before_image, name, before_rect, detector, min_pixels)?;
    let after = detect_pixel_anchor(&after_image, name, after_rect, detector, min_pixels)?;
    let screen_y_delta = before.row_y.zip(after.row_y).map(|(before_y, after_y)| {
        let delta = after_y - before_y;
        delta.abs()
    });
    let app_geometry =
        app_screen_y_delta.map(|screen_y_delta| AppRenderedAnchorGeometry { screen_y_delta });
    let mut diagnostics = Vec::new();
    let mut status = if before.status == "passed" && after.status == "passed" {
        "passed"
    } else {
        "failed"
    };

    if let Some(maximum) = max_screen_y_delta
        && let Some(rendered_delta) = screen_y_delta
        && rendered_delta > maximum
    {
        status = "failed";
        if let Some(app_delta) = app_screen_y_delta
            && app_delta <= maximum
        {
            diagnostics.push(AppVsRenderedDisagreement {
                name: name.to_string(),
                status: "app-vs-rendered-anchor-disagreement".to_string(),
                app_screen_y_delta: app_delta,
                rendered_screen_y_delta: rendered_delta,
                max_screen_y_delta: maximum,
            });
        }
    }

    Ok(RenderedAnchorPairReport {
        name: name.to_string(),
        detector: detector.to_string(),
        status: status.to_string(),
        before,
        after,
        screen_y_delta,
        max_screen_y_delta,
        app_geometry,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_png_corpus_passes() {
        let stats = run_embedded_png_corpus();

        assert_eq!(stats.failed, 0);
        assert!(stats.compared >= 4);
    }

    #[test]
    fn pixel_anchor_detectors_match_python_synthetic_fixture() {
        let root = tempfile::Builder::new()
            .prefix("cargo-gtk-proof-png-test-")
            .tempdir()
            .expect("test dir");
        let path = root.path().join("anchors.png");
        write_rgba(&path, &native_minimap_anchor_rows()).expect("write image");
        let image = read_png(&path).expect("read image");

        let edge = detect_pixel_anchor(
            &image,
            "minimap-native-viewport-top-edge",
            Rect::new(0, 0, 40, 12),
            "native-minimap-viewport-top-edge-row",
            12,
        )
        .expect("edge detection");
        let content = detect_pixel_anchor(
            &image,
            "minimap-first-content-row",
            Rect::new(0, 0, 40, 12),
            "minimap-content-row",
            20,
        )
        .expect("content detection");

        assert_eq!(edge.status, "passed");
        assert_eq!(edge.row_y, Some(3));
        assert_eq!(content.status, "passed");
        assert_eq!(content.row_y, Some(6));
        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn detector_regression_fixture_covers_all_python_detector_modes() {
        let root = tempfile::Builder::new()
            .prefix("cargo-gtk-proof-png-detectors-")
            .tempdir()
            .expect("test dir");
        let path = root.path().join("detectors.png");
        let bg = (29, 29, 32, 255);
        let edge = (78, 78, 78, 255);
        let fill = (60, 60, 63, 255);
        let cyan = (0, 190, 180, 255);
        let orange = (245, 132, 28, 255);
        let mut rows = vec![vec![bg; 12]; 10];
        for pixel in &mut rows[1][2..10] {
            *pixel = edge;
        }
        for pixel in &mut rows[3][2..10] {
            *pixel = fill;
        }
        for pixel in &mut rows[6][2..10] {
            *pixel = edge;
        }
        for pixel in &mut rows[8][2..7] {
            *pixel = cyan;
        }
        for pixel in &mut rows[9][8..11] {
            *pixel = orange;
        }
        write_rgba(&path, &rows).expect("write image");
        let image = read_png(&path).expect("read image");

        let checks = [
            (
                "native",
                Rect::new(2, 0, 8, 4),
                "native-minimap-viewport-top-edge-row",
                6,
                1,
            ),
            (
                "legacy-neutral-edge",
                Rect::new(2, 0, 8, 4),
                "horizontal-neutral-edge-row",
                6,
                1,
            ),
            (
                "content",
                Rect::new(2, 7, 8, 3),
                "minimap-content-row",
                4,
                8,
            ),
            (
                "fill",
                Rect::new(2, 2, 8, 3),
                "viewport-highlight-fill-row",
                6,
                3,
            ),
            (
                "bottom-edge",
                Rect::new(2, 5, 8, 3),
                "horizontal-neutral-edge-row",
                6,
                6,
            ),
            (
                "generic",
                Rect::new(0, 8, 12, 1),
                "non-background-row",
                4,
                8,
            ),
            (
                "search-marker",
                Rect::new(0, 8, 12, 2),
                "minimap-search-marker-row",
                2,
                9,
            ),
        ];

        for (name, rect, detector, min_pixels, expected_row) in checks {
            let detection =
                detect_pixel_anchor(&image, name, rect, detector, min_pixels).expect(detector);
            assert_eq!(detection.status, "passed", "{detector}");
            assert_eq!(detection.row_y, Some(expected_row), "{detector}");
        }

        let error = detect_pixel_anchor(&image, "unknown", Rect::new(0, 0, 2, 2), "bogus", 1)
            .expect_err("unsupported detector rejected");
        assert!(error.contains("unsupported pixel anchor detector"));
    }

    #[test]
    fn detector_failure_keeps_best_row_for_diagnostics() {
        let root = tempfile::Builder::new()
            .prefix("cargo-gtk-proof-png-detector-failure-")
            .tempdir()
            .expect("test dir");
        let path = root.path().join("weak-edge.png");
        let bg = (29, 29, 32, 255);
        let edge = (78, 78, 78, 255);
        let mut rows = vec![vec![bg; 12]; 6];
        for pixel in &mut rows[2][3..7] {
            *pixel = edge;
        }
        write_rgba(&path, &rows).expect("write image");
        let image = read_png(&path).expect("read image");

        let detection = detect_pixel_anchor(
            &image,
            "minimap-native-viewport-top-edge",
            Rect::new(0, 0, 12, 6),
            "native-minimap-viewport-top-edge-row",
            8,
        )
        .expect("detection");

        assert_eq!(detection.status, "failed");
        assert_eq!(detection.row_y, Some(2));
        assert_eq!(detection.matched_pixels, 4);
    }

    #[test]
    fn comparison_supports_masks_and_different_crop_origins() {
        let root = tempfile::Builder::new()
            .prefix("cargo-gtk-proof-png-compare-")
            .tempdir()
            .expect("test dir");
        let before = root.path().join("before.png");
        let after = root.path().join("after.png");
        let black = (0, 0, 0, 255);
        let red = (255, 0, 0, 255);
        let blue = (0, 0, 255, 255);
        write_rgba(&before, &[vec![black, red], vec![black, black]]).expect("before image");
        write_rgba(&after, &[vec![blue, black, red], vec![blue, black, black]])
            .expect("after image");

        let shifted = compare_crops(
            &before,
            &after,
            Rect::new(0, 0, 2, 2),
            Some(Rect::new(1, 0, 2, 2)),
            &[],
            None,
        )
        .expect("shifted comparison");
        let masked = compare_crops(
            &before,
            &after,
            Rect::new(0, 0, 2, 2),
            Some(Rect::new(1, 0, 2, 2)),
            &[Rect::new(1, 0, 1, 1)],
            None,
        )
        .expect("masked comparison");

        assert_eq!(shifted.status, "passed");
        assert_eq!(masked.status, "passed");
        assert_eq!(masked.compared_pixels, 3);
        assert_eq!(masked.allowed_changing_regions, vec![Rect::new(1, 0, 1, 1)]);
    }

    #[test]
    fn protected_region_mismatch_reports_first_difference_and_crops() {
        let root = tempfile::Builder::new()
            .prefix("cargo-gtk-proof-png-mismatch-")
            .tempdir()
            .expect("test dir");
        let before = root.path().join("before.png");
        let after = root.path().join("after.png");
        let prefix = root.path().join("artifacts/header");
        let black = (0, 0, 0, 255);
        let red = (255, 0, 0, 255);
        write_rgba(&before, &[vec![black, black], vec![black, black]]).expect("before image");
        write_rgba(&after, &[vec![black, red], vec![black, black]]).expect("after image");

        let report = compare_crops(
            &before,
            &after,
            Rect::new(0, 0, 2, 2),
            None,
            &[],
            Some(&prefix),
        )
        .expect("comparison");

        assert_eq!(report.status, "failed");
        assert_eq!(report.diff_pixels, 1);
        assert_eq!(
            report.first_difference,
            Some(DifferencePoint { x: 1, y: 0 })
        );
        assert!(root.path().join("artifacts/header-before.png").is_file());
        assert!(root.path().join("artifacts/header-after.png").is_file());
        let before_crop = read_png(&root.path().join("artifacts/header-before.png"))
            .expect("before crop readable");
        assert_eq!(before_crop.width, 2);
        assert_eq!(before_crop.height, 2);
    }

    #[test]
    fn rendered_anchor_drift_fails_even_when_app_geometry_is_stable() {
        let root = tempfile::Builder::new()
            .prefix("cargo-gtk-proof-rendered-anchor-")
            .tempdir()
            .expect("test dir");
        let before = root.path().join("before.png");
        let after = root.path().join("after.png");
        write_rgba(&before, &anchor_drift_rows(2)).expect("before image");
        write_rgba(&after, &anchor_drift_rows(3)).expect("after image");

        let report = compare_rendered_anchor_pair(RenderedAnchorPairInput {
            before_path: &before,
            after_path: &after,
            name: "minimap-native-viewport-top-edge",
            before_rect: Rect::new(0, 0, 16, 8),
            after_rect: Rect::new(0, 0, 16, 8),
            detector: "native-minimap-viewport-top-edge-row",
            min_pixels: 8,
            max_screen_y_delta: Some(0),
            app_screen_y_delta: Some(0),
        })
        .expect("rendered anchor comparison");

        assert_eq!(report.status, "failed");
        assert_eq!(report.screen_y_delta, Some(1));
        assert_eq!(
            report.app_geometry,
            Some(AppRenderedAnchorGeometry { screen_y_delta: 0 })
        );
        assert_eq!(
            report.diagnostics,
            vec![AppVsRenderedDisagreement {
                name: "minimap-native-viewport-top-edge".to_string(),
                status: "app-vs-rendered-anchor-disagreement".to_string(),
                app_screen_y_delta: 0,
                rendered_screen_y_delta: 1,
                max_screen_y_delta: 0,
            }]
        );
    }

    #[test]
    fn malformed_and_oversized_pngs_are_rejected() {
        let root = tempfile::Builder::new()
            .prefix("cargo-gtk-proof-png-invalid-")
            .tempdir()
            .expect("test dir");
        let malformed = root.path().join("malformed.png");
        fs::write(&malformed, b"not a png").expect("malformed fixture");
        let oversized = root.path().join("oversized.png");
        fs::File::create(&oversized)
            .expect("oversized fixture")
            .set_len(MAX_COMPRESSED_PNG_BYTES + 1)
            .expect("set len");

        let malformed_error = read_png(&malformed).expect_err("malformed rejected");
        let oversized_error = read_png(&oversized).expect_err("oversized rejected");

        assert!(malformed_error.contains("not a PNG file"));
        assert!(oversized_error.contains("exceeds compressed byte limit"));
    }
}
