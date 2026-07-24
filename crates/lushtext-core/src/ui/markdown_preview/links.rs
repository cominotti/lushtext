// SPDX-License-Identifier: GPL-3.0-or-later

//! Markdown-preview link interaction.
//!
//! Resolves Markdown link destinations (local files and remote URLs) against
//! the document and workspace path graph, wires the click and pointer-motion
//! controllers on the preview text view, and launches activated links. Behavior
//! is unchanged from when this lived in `mod.rs`; only the code location moved.
//! The rendered link record types stay in `mod.rs` because the render loop and
//! retirement own them.
//!
//! Accessibility posture: pointer motion only updates the cursor as a hover
//! hint, so hover is a pure enhancement here — link activation is the separate
//! click gesture and does not depend on hover. This read-only preview adds no
//! hover-only command; the link text and destination stay readable regardless,
//! and link styling (`TAG_LINK`) is applied by the render path in `mod.rs`.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use std::path::{Path, PathBuf};

use super::{LushtextMarkdownPreview, MarkdownPreviewRenderContext, PreviewLaunchTarget};

/// Result of trying to resolve a local filesystem path from Markdown content.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LocalPathResolution {
    /// One unambiguous local path candidate was formed.
    Resolved(PathBuf),
    /// No document or workspace base can produce a local path candidate.
    Missing,
    /// More than one workspace-relative path is possible, so link activation should not guess.
    Ambiguous(Vec<PathBuf>),
}

/// Resolve one Markdown link destination into a launchable target, if possible.
pub(super) fn resolve_link_target(
    raw_target: &str,
    context: &MarkdownPreviewRenderContext,
) -> Option<PreviewLaunchTarget> {
    if raw_target.trim().is_empty() {
        return None;
    }

    if let Some(scheme) = glib::Uri::parse_scheme(raw_target) {
        if scheme.as_str() == "file" {
            let file = gio::File::for_uri(raw_target);
            let path = file.path()?;
            return Some(PreviewLaunchTarget {
                uri: raw_target.to_string(),
                local_path: Some(path),
            });
        }

        return Some(PreviewLaunchTarget {
            uri: raw_target.to_string(),
            local_path: None,
        });
    }

    match resolve_local_path(raw_target, context) {
        LocalPathResolution::Resolved(path) => Some(PreviewLaunchTarget {
            uri: gio::File::for_path(&path).uri().to_string(),
            local_path: Some(path),
        }),
        LocalPathResolution::Missing | LocalPathResolution::Ambiguous(_) => None,
    }
}

/// Resolve one Markdown local path against the current document and workspace folders.
fn resolve_local_path(
    raw_target: &str,
    context: &MarkdownPreviewRenderContext,
) -> LocalPathResolution {
    let path = Path::new(raw_target);
    if path.is_absolute() {
        return LocalPathResolution::Resolved(path.to_path_buf());
    }

    if let Some(document_path) = &context.document_path
        && let Some(parent) = document_path.parent()
    {
        return LocalPathResolution::Resolved(parent.join(path));
    }

    let candidates = context
        .workspace_folders
        .iter()
        .map(|folder| folder.join(path))
        .collect::<Vec<_>>();

    match candidates.len() {
        0 => LocalPathResolution::Missing,
        1 => LocalPathResolution::Resolved(
            candidates.into_iter().next().expect("one candidate exists"),
        ),
        _ => LocalPathResolution::Ambiguous(candidates),
    }
}

impl LushtextMarkdownPreview {
    /// Register one callback that overrides the default external link launcher.
    ///
    /// The production window leaves this unset so preview links open through
    /// the desktop's default handler. Widget tests install a callback instead
    /// so they can assert which URI would have been launched.
    pub fn connect_link_activated<F: Fn(&str) + 'static>(&self, f: F) {
        self.imp()
            .link_activation_callback
            .replace(Some(Box::new(move |uri| f(&uri))));
    }

    /// Install click and hover controllers for launchable text-buffer links.
    pub(super) fn setup_link_interaction(&self) {
        let click = gtk4::GestureClick::new();
        let obj_weak = self.downgrade();
        click.connect_pressed(move |_, _press_count, x, y| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.activate_link_at_view_position(x, y);
            }
        });
        self.imp().text_view.add_controller(click);

        let motion = gtk4::EventControllerMotion::new();
        let obj_weak = self.downgrade();
        motion.connect_motion(move |_, x, y| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.update_link_cursor(x, y);
            }
        });
        let obj_weak = self.downgrade();
        motion.connect_leave(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.imp().text_view.set_cursor_from_name(None);
            }
        });
        self.imp().text_view.add_controller(motion);
    }

    /// Try to activate the rendered link at one text-view position.
    fn activate_link_at_view_position(&self, x: f64, y: f64) -> bool {
        let Some(target) = self.link_target_at_view_position(x, y) else {
            return false;
        };
        self.activate_link_target(&target)
    }

    /// Update the pointer cursor based on whether the current position is clickable.
    fn update_link_cursor(&self, x: f64, y: f64) {
        let cursor_name = if self.link_target_at_view_position(x, y).is_some() {
            Some("pointer")
        } else {
            None
        };
        self.imp().text_view.set_cursor_from_name(cursor_name);
    }

    /// Resolve one rendered text link from widget-local coordinates.
    fn link_target_at_view_position(&self, x: f64, y: f64) -> Option<PreviewLaunchTarget> {
        let text_view = self.imp().text_view.get();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Preview hit-testing uses GTK widget coordinates, which fit within i32 here."
        )]
        let (buffer_x, buffer_y) = text_view.window_to_buffer_coords(
            gtk4::TextWindowType::Widget,
            x.round() as i32,
            y.round() as i32,
        );
        let iter = text_view.iter_at_location(buffer_x, buffer_y)?;
        self.link_target_at_buffer_offset(iter.offset())
    }

    /// Resolve one launchable text link from a buffer offset.
    fn link_target_at_buffer_offset(&self, offset: i32) -> Option<PreviewLaunchTarget> {
        self.imp()
            .text_link_targets
            .borrow()
            .iter()
            .find(|link| offset >= link.start_offset && offset < link.end_offset)
            .map(|link| link.target.clone())
    }

    /// Launch one previously resolved preview target.
    pub(super) fn activate_link_target(&self, target: &PreviewLaunchTarget) -> bool {
        if let Some(callback) = self.imp().link_activation_callback.borrow().as_ref() {
            callback(target.uri.clone());
            return true;
        }

        match gio::AppInfo::launch_default_for_uri(
            &target.uri,
            Option::<&gio::AppLaunchContext>::None,
        ) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!("failed to launch preview link '{}': {error}", target.uri);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use crate::ui::markdown_preview::MarkdownPreviewRenderContext;
    use tempfile::tempdir;

    #[test]
    fn test_resolve_local_path_prefers_document_relative_match() {
        let tempdir = tempdir().expect("tempdir");
        let document_dir = tempdir.path().join("docs");
        let workspace_folder = tempdir.path().join("workspace");
        fixture::create_dir_all(&document_dir);
        fixture::create_dir_all(&workspace_folder.join("images"));
        fixture::write_bytes(&document_dir.join("logo.png"), b"doc");
        fixture::write_bytes(&workspace_folder.join("logo.png"), b"workspace");

        let context = MarkdownPreviewRenderContext::new(
            Some(document_dir.join("guide.md")),
            vec![workspace_folder],
        );

        assert_eq!(
            resolve_local_path("logo.png", &context),
            LocalPathResolution::Resolved(document_dir.join("logo.png"))
        );
    }

    #[test]
    fn test_resolve_local_path_defers_existence_checks_to_activation() {
        let tempdir = tempdir().expect("tempdir");
        let document_path = tempdir.path().join("docs/guide.md");
        let context = MarkdownPreviewRenderContext::new(Some(document_path.clone()), Vec::new());

        assert_eq!(
            resolve_local_path("missing.png", &context),
            LocalPathResolution::Resolved(
                document_path
                    .parent()
                    .expect("document path has parent")
                    .join("missing.png")
            )
        );
    }

    #[test]
    fn test_resolve_local_path_reports_ambiguous_workspace_candidates() {
        let tempdir = tempdir().expect("tempdir");
        let folder_a = tempdir.path().join("folder-a");
        let folder_b = tempdir.path().join("folder-b");
        fixture::create_dir_all(&folder_a.join("images"));
        fixture::create_dir_all(&folder_b.join("images"));
        fixture::write_bytes(&folder_a.join("images/logo.png"), b"a");
        fixture::write_bytes(&folder_b.join("images/logo.png"), b"b");

        let context =
            MarkdownPreviewRenderContext::new(None, vec![folder_a.clone(), folder_b.clone()]);

        assert_eq!(
            resolve_local_path("images/logo.png", &context),
            LocalPathResolution::Ambiguous(vec![
                folder_a.join("images/logo.png"),
                folder_b.join("images/logo.png"),
            ])
        );
    }
}
