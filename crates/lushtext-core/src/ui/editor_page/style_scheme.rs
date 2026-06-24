// SPDX-License-Identifier: GPL-3.0-or-later

//! GtkSourceView style-scheme projection for editor tabs.
//!
//! This module owns the runtime-generated opacity-aware child schemes used by
//! transparent tab content. The editor implementation decides when settings
//! changed; this module resolves the active base scheme, writes derived XML off
//! the GTK thread when needed, and reapplies the scheme after GtkSourceView
//! rescans the generated directory.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use sourceview5::prelude::*;

use crate::config::keys;
use crate::services::{
    filesystem::{WriteLabel, read as fs_read, write as fs_write},
    json_store,
};

use super::LushtextEditorPage;

/// Derived style-scheme IDs currently being generated on background threads.
///
/// Multiple open tabs can request the same opacity/base-scheme pair at once. A
/// process-wide registry keeps those tabs from launching duplicate durable writes
/// while still allowing later retries if the first write fails.
static TRANSPARENCY_STYLE_SCHEME_GENERATIONS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Apply the active user style scheme and document-surface opacity to one editor.
pub(super) fn apply_color_scheme_to_editor(editor: &LushtextEditorPage) {
    editor.imp().document_surface_opacity.set(
        editor
            .imp()
            .settings
            .double(keys::TAB_CONTENT_OPACITY)
            .clamp(0.0, 1.0),
    );
    let applied = apply_color_scheme(editor);
    *editor.imp().applied_style_scheme_id.borrow_mut() = applied;
    editor.queue_minimap_draw();
}

fn apply_color_scheme(editor: &LushtextEditorPage) -> Option<String> {
    let buffer = editor.buffer();
    let settings = &editor.imp().settings;
    let scheme_manager = sourceview5::StyleSchemeManager::default();
    let base_scheme = crate::ui::theme::active_sourceview_scheme(settings)?;
    let opacity = settings.double(keys::TAB_CONTENT_OPACITY).clamp(0.0, 1.0);

    if opacity >= 1.0 - f64::EPSILON {
        let applied_id = base_scheme.id().to_string();
        buffer.set_style_scheme(Some(&base_scheme));
        return Some(applied_id);
    }

    let spec = transparency_style_scheme_spec(&base_scheme, settings);
    ensure_transparency_style_scheme_search_path(&scheme_manager, &spec.scheme_dir);
    let scheme = scheme_manager.scheme(&spec.derived_id).or_else(|| {
        schedule_transparency_style_scheme_generation(editor, spec);
        Some(base_scheme.clone())
    })?;
    let applied_id = scheme.id().to_string();
    buffer.set_style_scheme(Some(&scheme));
    Some(applied_id)
}

/// Runtime-generated opacity style scheme that can be written off the GTK thread.
struct TransparencyStyleSchemeSpec {
    /// GtkSourceView style-scheme ID used after the manager rescans the file.
    derived_id: String,
    /// User data subdirectory that holds generated style schemes.
    scheme_dir: PathBuf,
    /// Destination XML file for this specific opacity/base-scheme pair.
    file_path: PathBuf,
    /// Complete style-scheme XML content to write atomically.
    xml: String,
}

/// Background write result returned to the GTK main loop.
struct TransparencyStyleSchemeWriteResult {
    /// GtkSourceView style-scheme ID whose generation finished.
    derived_id: String,
    /// Directory that may need to be added to the manager search path.
    scheme_dir: PathBuf,
    /// Destination file, used only for precise warning messages.
    file_path: PathBuf,
    /// Result from the durable filesystem write.
    result: std::io::Result<()>,
}

/// Build the opacity-aware child scheme derived from the active base scheme.
fn transparency_style_scheme_spec(
    base_scheme: &sourceview5::StyleScheme,
    settings: &gio::Settings,
) -> TransparencyStyleSchemeSpec {
    let palette = crate::ui::theme::resolve_tab_content_palette(settings);
    let base_id = base_scheme.id().to_string();
    let sanitized_base = sanitize_style_scheme_component(&base_id);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "tab-content opacity is clamped to 0..1 before converting to a 0..100 scheme suffix"
    )]
    let opacity_percent = (palette.opacity * 100.0).round() as u32;
    let derived_id = format!("lushtext-opacity-{sanitized_base}-{opacity_percent}");

    let scheme_dir = json_store::data_dir().join("style-schemes");
    let file_path = scheme_dir.join(format!("{derived_id}.xml"));
    let text_bg = crate::ui::theme::sourceview_rgba_with_alpha(&palette.text_bg, palette.opacity);
    let line_numbers_bg =
        crate::ui::theme::sourceview_rgba_with_alpha(&palette.line_numbers_bg, palette.opacity);
    let current_line_bg =
        crate::ui::theme::sourceview_rgba_with_alpha(&palette.current_line_bg, palette.opacity);
    let current_line_number_bg = crate::ui::theme::sourceview_rgba_with_alpha(
        &palette.current_line_number_bg,
        palette.opacity,
    );
    let right_margin_bg =
        crate::ui::theme::sourceview_rgba_with_alpha(&palette.right_margin_bg, palette.opacity);
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<style-scheme id="{derived_id}" _name="LushText Transparency" version="1.0" parent-scheme="{base_id}">
  <author>LushText</author>
  <_description>Opacity-aware derived scheme for LushText tab content</_description>
  <style name="text" background="{text_bg}"/>
  <style name="background-pattern" background="{text_bg}"/>
  <style name="current-line" background="{current_line_bg}"/>
  <style name="line-numbers" background="{line_numbers_bg}"/>
  <style name="current-line-number" background="{current_line_number_bg}"/>
  <style name="right-margin" background="{right_margin_bg}"/>
</style-scheme>
"#
    );
    TransparencyStyleSchemeSpec {
        derived_id,
        scheme_dir,
        file_path,
        xml,
    }
}

fn ensure_transparency_style_scheme_search_path(
    manager: &sourceview5::StyleSchemeManager,
    scheme_dir: &Path,
) {
    let scheme_dir_str = scheme_dir.to_string_lossy();
    if !manager
        .search_path()
        .iter()
        .any(|path| path.as_str() == scheme_dir_str.as_ref())
    {
        manager.prepend_search_path(&scheme_dir_str);
    }
}

/// Replace punctuation in runtime-generated style-scheme IDs so the file name stays stable.
fn sanitize_style_scheme_component(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn try_mark_transparency_style_scheme_generation(derived_id: &str) -> bool {
    TRANSPARENCY_STYLE_SCHEME_GENERATIONS
        .lock()
        .map_or(true, |mut generations| {
            generations.insert(derived_id.to_string())
        })
}

fn clear_transparency_style_scheme_generation(derived_id: &str) {
    if let Ok(mut generations) = TRANSPARENCY_STYLE_SCHEME_GENERATIONS.lock() {
        generations.remove(derived_id);
    }
}

fn schedule_transparency_style_scheme_generation(
    editor: &LushtextEditorPage,
    spec: TransparencyStyleSchemeSpec,
) {
    // Another tab may already be writing the same derived scheme. Coalesce the
    // durable write and retry applying after the first writer has had time to rescan.
    if !try_mark_transparency_style_scheme_generation(&spec.derived_id) {
        schedule_transparency_style_scheme_apply_retry(editor);
        return;
    }

    let editor_weak = editor.downgrade();
    spawn_blocking_then(
        editor_weak,
        move || {
            let result = write_transparency_style_scheme_if_needed(
                &spec.scheme_dir,
                &spec.file_path,
                &spec.xml,
            );
            TransparencyStyleSchemeWriteResult {
                derived_id: spec.derived_id,
                scheme_dir: spec.scheme_dir,
                file_path: spec.file_path,
                result,
            }
        },
        move |editor_weak, write_result| {
            clear_transparency_style_scheme_generation(&write_result.derived_id);
            if let Err(error) = write_result.result {
                tracing::warn!(
                    "Failed to write derived style scheme {}: {error}",
                    write_result.file_path.display()
                );
                return;
            }

            let manager = sourceview5::StyleSchemeManager::default();
            ensure_transparency_style_scheme_search_path(&manager, &write_result.scheme_dir);
            manager.force_rescan();

            if let Some(editor) = editor_weak.upgrade() {
                apply_color_scheme_to_editor(&editor);
            }
        },
    );
}

fn schedule_transparency_style_scheme_apply_retry(editor: &LushtextEditorPage) {
    let editor_weak = editor.downgrade();
    glib::timeout_add_local_once(std::time::Duration::from_millis(120), move || {
        if let Some(editor) = editor_weak.upgrade() {
            apply_color_scheme_to_editor(&editor);
        }
    });
}

fn write_transparency_style_scheme_if_needed(
    scheme_dir: &Path,
    file_path: &Path,
    xml: &str,
) -> std::io::Result<()> {
    if fs_read::text(file_path).is_ok_and(|existing| existing == xml) {
        return Ok(());
    }

    fs_write::create_dir_all_durable(scheme_dir)?;
    fs_write::atomic_replace(file_path, WriteLabel::from("style-scheme"), xml.as_bytes()).map_err(
        |error| match error {
            fs_write::DurableWriteError::BeforeRename(source)
            | fs_write::DurableWriteError::AfterRename(source) => source,
        },
    )
}

#[cfg(test)]
mod tests {
    use crate::services::filesystem::{DirectoryScanPolicy, fixture, tree as fs_tree};
    use tempfile::TempDir;

    use super::write_transparency_style_scheme_if_needed;

    #[test]
    fn transparency_style_scheme_rewrites_corrupt_existing_file() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let scheme_dir = dir.path().join("style-schemes");
        fixture::create_dir_all(&scheme_dir);
        let file_path = scheme_dir.join("lushtext-opacity-test.xml");
        fixture::write_text(&file_path, "<truncated");
        let xml = "<?xml version=\"1.0\"?><style-scheme id=\"ok\"/>";

        write_transparency_style_scheme_if_needed(&scheme_dir, &file_path, xml)
            .expect("style-scheme rewrite should succeed");

        assert_eq!(fixture::read_text(&file_path), xml);
        assert!(
            fs_tree::scan_directory(&scheme_dir, DirectoryScanPolicy::visible_workspace())
                .expect("expected operation to succeed")
                .into_iter()
                .all(|entry| !entry.file_name.contains(".style-scheme."))
        );
    }
}
