// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared visual-geometry helpers for proof runner phases.
//!
//! The outer runner and same-session live runner consume the same Automation1
//! snapshot shape but differ in their scenario formats. This module keeps the
//! snapshot traversal, rectangle parsing, app-anchor diagnostics, and artifact
//! name normalization in one place while callers keep their local manifest
//! adapters.

use serde_json::{Value, json};

use crate::png;

/// Insets applied to one visual rectangle before a PNG crop is extracted.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Insets {
    left: i64,
    top: i64,
    right: i64,
    bottom: i64,
}

impl Insets {
    /// Parse optional inset values from one scenario JSON row.
    #[must_use]
    pub(crate) fn from_value(value: &Value) -> Self {
        Self {
            left: value.get("left").and_then(Value::as_i64).unwrap_or(0),
            top: value.get("top").and_then(Value::as_i64).unwrap_or(0),
            right: value.get("right").and_then(Value::as_i64).unwrap_or(0),
            bottom: value.get("bottom").and_then(Value::as_i64).unwrap_or(0),
        }
    }
}

/// Integer rectangle reported by the application's visual-geometry snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) struct VisualBox {
    /// Left edge in application snapshot coordinates.
    pub(crate) x: i64,
    /// Top edge in application snapshot coordinates.
    pub(crate) y: i64,
    /// Width in application snapshot coordinates.
    pub(crate) width: i64,
    /// Height in application snapshot coordinates.
    pub(crate) height: i64,
}

impl VisualBox {
    /// Parse a visual rectangle object from Automation1 JSON.
    #[must_use]
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        Some(Self {
            x: value.get("x")?.as_i64()?,
            y: value.get("y")?.as_i64()?,
            width: value.get("width")?.as_i64()?,
            height: value.get("height")?.as_i64()?,
        })
    }
}

/// Return the visual-geometry object from an Automation1 snapshot.
#[must_use]
pub(crate) fn visual_geometry(snapshot: &Value) -> Option<&Value> {
    snapshot.pointer("/window/visual_geometry")
}

/// Find one named surface row in an Automation1 visual-geometry snapshot.
#[must_use]
pub(crate) fn optional_surface<'a>(snapshot: &'a Value, name: &str) -> Option<&'a Value> {
    visual_geometry(snapshot)?
        .get("surfaces")?
        .as_array()?
        .iter()
        .find(|row| row.get("name").and_then(Value::as_str) == Some(name))
}

/// Find one named pixel-anchor row in an Automation1 visual-geometry snapshot.
#[must_use]
pub(crate) fn optional_pixel_anchor<'a>(snapshot: &'a Value, name: &str) -> Option<&'a Value> {
    visual_geometry(snapshot)?
        .get("pixel_anchors")?
        .as_array()?
        .iter()
        .find(|row| row.get("name").and_then(Value::as_str) == Some(name))
}

/// Find one named scroll-anchor row in an Automation1 visual-geometry snapshot.
#[must_use]
pub(crate) fn scroll_anchor<'a>(snapshot: &'a Value, name: &str) -> Option<&'a Value> {
    visual_geometry(snapshot)?
        .get("scroll_anchors")?
        .as_array()?
        .iter()
        .find(|row| row.get("name").and_then(Value::as_str) == Some(name))
}

/// Return one visible surface rectangle or a diagnostic-ready error.
pub(crate) fn surface_box(snapshot: &Value, name: &str) -> Result<VisualBox, String> {
    let row = optional_surface(snapshot, name)
        .ok_or_else(|| format!("visual surface not found: {name}"))?;
    if row.get("visible").and_then(Value::as_bool) != Some(true) {
        return Err(format!("surface {name} is not visible"));
    }
    VisualBox::from_value(row.get("rect").unwrap_or(&Value::Null))
        .ok_or_else(|| format!("surface {name} has malformed rect"))
}

/// Return one visible pixel-anchor rectangle or a diagnostic-ready error.
pub(crate) fn pixel_anchor_box(snapshot: &Value, name: &str) -> Result<VisualBox, String> {
    let row = optional_pixel_anchor(snapshot, name)
        .ok_or_else(|| format!("visual pixel anchor not found: {name}"))?;
    if row.get("visible").and_then(Value::as_bool) != Some(true) {
        return Err(format!("pixel anchor {name} is not visible"));
    }
    VisualBox::from_value(row.get("rect").unwrap_or(&Value::Null))
        .ok_or_else(|| format!("pixel anchor {name} has malformed rect"))
}

/// Apply crop insets to a visual rectangle.
pub(crate) fn inset_box(
    rect: VisualBox,
    insets: Insets,
    empty_message: &str,
) -> Result<VisualBox, String> {
    let width = rect.width - insets.left - insets.right;
    let height = rect.height - insets.top - insets.bottom;
    if width <= 0 || height <= 0 {
        return Err(empty_message.to_string());
    }
    Ok(VisualBox {
        x: rect.x + insets.left,
        y: rect.y + insets.top,
        width,
        height,
    })
}

/// Convert one visual rectangle to the PNG helper's crop rectangle.
pub(crate) fn png_rect(rect: VisualBox) -> Result<png::Rect, String> {
    png_rect_with_message(rect, "PNG crop rectangle is empty")
}

/// Convert one visual rectangle to a PNG crop rectangle with caller-specific diagnostics.
pub(crate) fn png_rect_with_message(
    rect: VisualBox,
    empty_message: &str,
) -> Result<png::Rect, String> {
    if rect.width <= 0 || rect.height <= 0 {
        return Err(empty_message.to_string());
    }
    Ok(png::Rect::new(
        i32::try_from(rect.x).map_err(|error| error.to_string())?,
        i32::try_from(rect.y).map_err(|error| error.to_string())?,
        usize::try_from(rect.width).map_err(|error| error.to_string())?,
        usize::try_from(rect.height).map_err(|error| error.to_string())?,
    ))
}

/// Convert a JSON mask rectangle to the PNG helper's crop rectangle.
pub(crate) fn png_rect_from_value(value: &Value) -> Result<png::Rect, String> {
    png_rect(VisualBox::from_value(value).ok_or_else(|| "mask rectangle is malformed".to_string())?)
}

/// Compute a detected row's offset from the crop top.
#[must_use]
pub(crate) fn row_offset(row_y: Option<i32>, rect: png::Rect) -> Option<i32> {
    row_y.map(|row_y| row_y - rect.y)
}

/// Select stable surface rows from large Automation1 snapshots for summaries.
#[must_use]
pub(crate) fn selected_surface_rows(snapshot: &Value, names: &[&str]) -> Vec<Value> {
    names
        .iter()
        .map(|name| {
            optional_surface(snapshot, name).map_or_else(
                || {
                    json!({
                        "name": name,
                        "visible": false,
                        "absence_reason": "missing-from-snapshot",
                    })
                },
                |row| {
                    json!({
                        "name": row.get("name"),
                        "visible": row.get("visible"),
                        "rect": row.get("rect"),
                        "allocation": row.get("allocation"),
                        "absence_reason": row.get("absence_reason"),
                    })
                },
            )
        })
        .collect()
}

/// Attach app-reported anchor geometry as diagnostics without making it proof.
#[must_use]
pub(crate) fn app_pixel_anchor_geometry(
    before_snapshot: &Value,
    after_snapshot: &Value,
    pixel_anchor_name: &str,
) -> Option<Value> {
    let before = optional_pixel_anchor(before_snapshot, pixel_anchor_name);
    let after = optional_pixel_anchor(after_snapshot, pixel_anchor_name);
    if before.is_none() && after.is_none() {
        return None;
    }
    let before_row = before.and_then(app_anchor_row);
    let after_row = after.and_then(app_anchor_row);
    Some(json!({
        "snapshot_anchor_name": pixel_anchor_name,
        "before": before.map_or_else(|| json!({
            "visible": false,
            "absence_reason": "missing-from-snapshot",
        }), app_anchor_summary),
        "after": after.map_or_else(|| json!({
            "visible": false,
            "absence_reason": "missing-from-snapshot",
        }), app_anchor_summary),
        "before_row_y": before_row,
        "after_row_y": after_row,
        "screen_y_delta": before_row.zip(after_row).map(|(before, after)| (after - before).abs()),
    }))
}

/// Produce a filesystem-safe artifact name while preserving readable words.
#[must_use]
pub(crate) fn safe_name(name: &str) -> String {
    let mut output = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-') {
            output.push(character);
        } else if !output.ends_with('-') {
            output.push('-');
        }
    }
    output.trim_matches('-').to_string()
}

fn app_anchor_row(row: &Value) -> Option<i64> {
    if row.get("visible").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    row.get("rect")?.get("y")?.as_i64()
}

fn app_anchor_summary(row: &Value) -> Value {
    json!({
        "name": row.get("name"),
        "surface": row.get("surface"),
        "visible": row.get("visible"),
        "rect": row.get("rect"),
        "absence_reason": row.get("absence_reason"),
    })
}
