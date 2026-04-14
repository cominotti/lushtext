// SPDX-License-Identifier: GPL-3.0-or-later

//! Annotation sidecar persistence, workspace listing, and markdown export.
//!
//! This service keeps annotation file I/O out of the GTK layer. It mirrors the
//! bookmark workflow for load/save/migration, then adds export helpers that
//! read source excerpts on background threads.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::model::annotation::{
    AnnotationDocument, AnnotationId, AnnotationRecord, AnnotationStyle,
};
use crate::model::sidecar_identity::DocumentSidecarIdentity;
use crate::services::json_store;

/// Directory name that stores per-file annotation sidecars.
const ANNOTATIONS_DIR: &str = "annotations";
/// Default export title shown at the top of generated markdown reports.
const EXPORT_TITLE: &str = "# Workspace Annotations";
/// Maximum number of source lines embedded for one annotation excerpt.
const EXPORT_EXCERPT_LINE_CAP: usize = 6;

/// Lightweight workspace-facing annotation row for browse dialogs and export grouping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceAnnotation {
    /// Path of the annotated file.
    pub path: PathBuf,
    /// Annotation identity used for row activation and editor editing.
    pub annotation_id: AnnotationId,
    /// Inclusive zero-based start line.
    pub start_line: u32,
    /// Inclusive zero-based end line.
    pub end_line: u32,
    /// Stored note body.
    pub note_text: String,
    /// Presentation style shown in the highlight and export output.
    pub style: AnnotationStyle,
}

impl WorkspaceAnnotation {
    /// Human-friendly line range label used in annotation browse rows.
    #[must_use]
    pub fn line_range_label(&self) -> String {
        let start = self.start_line.saturating_add(1);
        let end = self.end_line.saturating_add(1);
        if start == end {
            format!("Line {start}")
        } else {
            format!("Lines {start}-{end}")
        }
    }
}

/// Resolve the annotation sidecar directory under the app data home.
#[must_use]
pub fn annotations_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(ANNOTATIONS_DIR)
}

/// Resolve the stable identity for a saved document path.
pub fn resolve_document_identity(path: &Path) -> Result<DocumentSidecarIdentity> {
    let display_path = path.to_path_buf();
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", path.display()))?;
    Ok(DocumentSidecarIdentity::from_paths(
        display_path,
        canonical_path,
    ))
}

/// Load annotations for a saved file, returning an empty document if no sidecar exists yet.
pub fn load_for_path(data_dir: &Path, path: &Path) -> Result<AnnotationDocument> {
    let identity = resolve_document_identity(path)?;
    load_for_identity(data_dir, identity)
}

fn load_for_identity(
    data_dir: &Path,
    identity: DocumentSidecarIdentity,
) -> Result<AnnotationDocument> {
    let filename = sidecar_filename(&identity);
    let path = annotations_dir(data_dir).join(&filename);
    match load_json_file::<AnnotationDocument>(&path) {
        Ok(Some(mut document)) => {
            document.sort_stable();
            Ok(document)
        }
        Ok(None) => Ok(AnnotationDocument::empty(identity)),
        Err(error) => Err(error),
    }
}

/// Save annotations for a document path. Empty annotation sets delete the sidecar file.
pub fn save_for_path(
    data_dir: &Path,
    path: &Path,
    annotations: &[AnnotationRecord],
) -> Result<DocumentSidecarIdentity> {
    let identity = resolve_document_identity(path)?;
    save_document(
        data_dir,
        AnnotationDocument {
            identity: identity.clone(),
            annotations: annotations.to_vec(),
        },
    )?;
    Ok(identity)
}

/// Save a fully shaped annotation document.
pub fn save_document(data_dir: &Path, mut document: AnnotationDocument) -> Result<()> {
    document.sort_stable();

    if document.annotations.is_empty() {
        return delete_sidecar_file(data_dir, &document.identity);
    }

    json_store::save(
        &annotations_dir(data_dir),
        &sidecar_filename(&document.identity),
        &document,
    )
}

/// Delete the annotation sidecar for a saved file path if it exists.
pub fn delete_for_path(data_dir: &Path, path: &Path) -> Result<()> {
    let identity = resolve_document_identity(path)?;
    delete_sidecar_file(data_dir, &identity)
}

fn delete_sidecar_file(data_dir: &Path, identity: &DocumentSidecarIdentity) -> Result<()> {
    let path = annotations_dir(data_dir).join(sidecar_filename(identity));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(anyhow::anyhow!(
            "failed to delete annotation sidecar {}: {}",
            path.display(),
            error
        )),
    }
}

/// Move annotation sidecars after an in-app rename of a file or directory tree.
///
/// Returns the number of annotation documents that were rewritten.
pub fn move_path_tree(data_dir: &Path, old_path: &Path, new_path: &Path) -> Result<usize> {
    let dir = annotations_dir(data_dir);
    if !dir.exists() {
        return Ok(0);
    }

    let mut migrated = 0;
    for entry in
        std::fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("failed to iterate {}", dir.display()))?;
        let sidecar_path = entry.path();
        if sidecar_path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let Some(mut document) = load_json_file::<AnnotationDocument>(&sidecar_path)? else {
            continue;
        };
        let Some((display_path, canonical_path)) =
            rebase_identity_paths(&document.identity, old_path, new_path)
        else {
            continue;
        };

        document.identity = DocumentSidecarIdentity::from_paths(display_path, canonical_path);
        let new_sidecar_path = dir.join(sidecar_filename(&document.identity));
        save_document(data_dir, document)?;
        if entry.path() != new_sidecar_path {
            let _ = std::fs::remove_file(entry.path());
        }
        migrated += 1;
    }

    Ok(migrated)
}

/// Collect all annotations under the current workspace roots for browse dialogs.
pub fn list_workspace_annotations(
    data_dir: &Path,
    workspace_roots: &[PathBuf],
) -> Result<Vec<WorkspaceAnnotation>> {
    let canonical_roots = canonicalize_roots(workspace_roots);
    let dir = annotations_dir(data_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut annotations = Vec::new();
    for entry in
        std::fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let entry = entry.with_context(|| format!("failed to iterate {}", dir.display()))?;
        let Some(document) = load_json_file::<AnnotationDocument>(&entry.path())? else {
            continue;
        };
        if !matches_any_root(&document.identity, &canonical_roots) {
            continue;
        }
        let display_path = document.identity.display_path.clone();

        annotations.extend(document.annotations.into_iter().map(|annotation| {
            WorkspaceAnnotation {
                path: display_path.clone(),
                annotation_id: annotation.id,
                start_line: annotation.start_line,
                end_line: annotation.end_line,
                note_text: annotation.note_text,
                style: annotation.style,
            }
        }));
    }

    annotations.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.start_line.cmp(&right.start_line))
            .then_with(|| left.end_line.cmp(&right.end_line))
            .then_with(|| left.annotation_id.0.cmp(&right.annotation_id.0))
    });
    Ok(annotations)
}

/// Generate a markdown export grouped by file for the current workspace roots.
pub fn export_workspace_markdown(data_dir: &Path, workspace_roots: &[PathBuf]) -> Result<String> {
    let annotations = list_workspace_annotations(data_dir, workspace_roots)?;
    let mut grouped: BTreeMap<PathBuf, Vec<WorkspaceAnnotation>> = BTreeMap::new();
    for annotation in annotations {
        grouped
            .entry(annotation.path.clone())
            .or_default()
            .push(annotation);
    }

    let mut markdown = String::new();
    markdown.push_str(EXPORT_TITLE);
    markdown.push_str("\n\n");

    if grouped.is_empty() {
        markdown.push_str("_No annotations matched the selected workspace._\n");
        return Ok(markdown);
    }

    for (path, mut annotations) in grouped {
        annotations.sort_by(|left, right| {
            left.start_line
                .cmp(&right.start_line)
                .then_with(|| left.end_line.cmp(&right.end_line))
        });

        markdown.push_str(&format!("## {}\n\n", path.display()));
        for annotation in annotations {
            let range_label = annotation.line_range_label();
            markdown.push_str(&format!(
                "### {} · {}\n\n",
                range_label,
                annotation.style.label()
            ));
            markdown.push_str(&annotation.note_text);
            markdown.push_str("\n\n");

            let excerpt = excerpt_for_annotation(&path, annotation.start_line, annotation.end_line)
                .unwrap_or_else(|| "_Source excerpt unavailable._".to_string());
            markdown.push_str("```text\n");
            markdown.push_str(&excerpt);
            if !excerpt.ends_with('\n') {
                markdown.push('\n');
            }
            markdown.push_str("```\n\n");
        }
    }

    Ok(markdown)
}

fn excerpt_for_annotation(path: &Path, start_line: u32, end_line: u32) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let start = usize::try_from(start_line).ok()?;
    if start >= lines.len() {
        return None;
    }

    let end = usize::try_from(end_line)
        .ok()
        .map(|end| end.min(lines.len().saturating_sub(1)))
        .unwrap_or_else(|| lines.len().saturating_sub(1));
    let cap_end = start
        .saturating_add(EXPORT_EXCERPT_LINE_CAP.saturating_sub(1))
        .min(end);

    let mut excerpt = lines[start..=cap_end].join("\n");
    if cap_end < end {
        excerpt.push_str("\n...");
    }
    Some(excerpt)
}

fn sidecar_filename(identity: &DocumentSidecarIdentity) -> String {
    format!("{}.json", identity.sidecar_id)
}

fn canonicalize_roots(workspace_roots: &[PathBuf]) -> Vec<PathBuf> {
    workspace_roots
        .iter()
        .map(|root| root.canonicalize().unwrap_or_else(|_| root.clone()))
        .collect()
}

fn matches_any_root(identity: &DocumentSidecarIdentity, workspace_roots: &[PathBuf]) -> bool {
    workspace_roots.iter().any(|root| {
        identity.canonical_path.starts_with(root) || identity.display_path.starts_with(root)
    })
}

fn rebase_identity_paths(
    identity: &DocumentSidecarIdentity,
    old_path: &Path,
    new_path: &Path,
) -> Option<(PathBuf, PathBuf)> {
    if identity.display_path == old_path || identity.display_path.starts_with(old_path) {
        let suffix = identity
            .display_path
            .strip_prefix(old_path)
            .ok()
            .map(PathBuf::from)
            .unwrap_or_default();
        let display_path = if suffix.as_os_str().is_empty() {
            new_path.to_path_buf()
        } else {
            new_path.join(suffix)
        };
        let canonical_path = display_path
            .canonicalize()
            .unwrap_or_else(|_| display_path.clone());
        return Some((display_path, canonical_path));
    }

    if identity.canonical_path == old_path || identity.canonical_path.starts_with(old_path) {
        let suffix = identity
            .canonical_path
            .strip_prefix(old_path)
            .ok()
            .map(PathBuf::from)
            .unwrap_or_default();
        let display_path = if suffix.as_os_str().is_empty() {
            new_path.to_path_buf()
        } else {
            new_path.join(suffix)
        };
        let canonical_path = display_path
            .canonicalize()
            .unwrap_or_else(|_| display_path.clone());
        return Some((display_path, canonical_path));
    }

    None
}

fn load_json_file<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let value = serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            Ok(Some(value))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(anyhow::anyhow!(
            "failed to read {}: {}",
            path.display(),
            error
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("src/main.rs");
        write_file(&file_path, "fn main() {}\n");

        let annotations = vec![
            AnnotationRecord::new(1, 3, "Refactor this".to_string(), AnnotationStyle::Todo),
            AnnotationRecord::new(5, 5, "Why?".to_string(), AnnotationStyle::Question),
        ];

        save_for_path(dir.path(), &file_path, &annotations).unwrap();
        let loaded = load_for_path(dir.path(), &file_path).unwrap();

        assert_eq!(loaded.annotations.len(), 2);
        assert_eq!(loaded.annotations[0].start_line, 1);
        assert_eq!(loaded.annotations[1].style, AnnotationStyle::Question);
    }

    #[test]
    fn move_path_tree_rewrites_document_identity() {
        let dir = TempDir::new().unwrap();
        let old_file = dir.path().join("workspace/old.rs");
        let new_file = dir.path().join("workspace/new.rs");
        write_file(&old_file, "fn old() {}\n");

        save_for_path(
            dir.path(),
            &old_file,
            &[AnnotationRecord::new(
                2,
                4,
                "Keep this note".to_string(),
                AnnotationStyle::Warning,
            )],
        )
        .unwrap();

        std::fs::rename(&old_file, &new_file).unwrap();
        move_path_tree(dir.path(), &old_file, &new_file).unwrap();

        let loaded = load_for_path(dir.path(), &new_file).unwrap();
        assert_eq!(loaded.identity.display_path, new_file);
        assert_eq!(loaded.annotations.len(), 1);
        assert_eq!(loaded.annotations[0].note_text, "Keep this note");
    }

    #[test]
    fn export_workspace_markdown_groups_by_file_and_includes_excerpt() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("workspace/src/lib.rs");
        write_file(
            &file_path,
            "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\n",
        );

        save_for_path(
            dir.path(),
            &file_path,
            &[AnnotationRecord::new(
                1,
                4,
                "Explain this block".to_string(),
                AnnotationStyle::Note,
            )],
        )
        .unwrap();

        let markdown =
            export_workspace_markdown(dir.path(), &[dir.path().join("workspace")]).unwrap();

        assert!(markdown.contains("# Workspace Annotations"));
        assert!(markdown.contains("## "));
        assert!(markdown.contains("Lines 2-5"));
        assert!(markdown.contains("Explain this block"));
        assert!(markdown.contains("line 2\nline 3\nline 4\nline 5"));
    }
}
