// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — execution. Applies one confirmed format decision.
//!
//! Everything past the picker: the reopen hand-off, the lossy-encoding analysis
//! and its confirmation, the save-encoding and line-ending policy writes, and
//! the invisible-character mode write.
//!
//! ## Where the freshness guards are, and why
//!
//! The save-encoding path is the only one that leaves the GTK thread, and it
//! leaves it **twice**: a chunked buffer capture, then an analysis worker. Both
//! completions are guarded by the same triple — the editor's lossy-analysis
//! generation, its draft-dirty (content) generation, and that the editor is
//! still the active tab. All three are captured *before* dispatch and rechecked
//! at every resumption, including inside the confirmation dialog's response
//! handler, because the user can keep typing while the dialog is open. A stale
//! completion must not rewrite save policy for content it no longer describes.

use std::cell::RefCell;
use std::rc::Rc;

use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use libadwaita::prelude::*;

use crate::config::keys;
use crate::model::encoding::{
    DocumentEncoding, FileHealthFindingKind, InvisibleCharactersMode, LineEnding,
};
use crate::services::editor_io::{self, LossyEncodingPreview};
use crate::ui::buffer_snapshot;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;

use super::super::LushtextWindow;
use super::policy;

const RESPONSE_CONTINUE: &str = "continue";
const RESPONSE_CANCEL: &str = "cancel";

/// Reopen the active document with a different decoding.
///
/// A modified document routes through the discard confirmation first, because
/// reinterpreting the bytes on disk throws away unsaved edits.
pub(super) fn begin_reopen(window: &LushtextWindow, encoding: DocumentEncoding) {
    let Some(editor) = window.active_editor() else {
        return;
    };
    let Some(path) = editor.file_path() else {
        window.publish_status_message(
            "Save the document before using Reopen with Encoding.",
            MessageKind::Warning,
        );
        return;
    };

    if editor.is_modified() {
        let title = editor.title();
        let window_weak = window.downgrade();
        let editor_weak = editor.downgrade();
        window.show_discard_changes_dialog(&title, move |confirmed| {
            if !confirmed {
                return;
            }
            if let Some(window) = window_weak.upgrade()
                && let Some(editor) = editor_weak.upgrade()
            {
                reopen_now(&window, &editor, &path, encoding);
            }
        });
        return;
    }

    reopen_now(window, &editor, &path, encoding);
}

/// Hand the reopen to the document-load workflow, which owns the read.
fn reopen_now(
    window: &LushtextWindow,
    editor: &LushtextEditorPage,
    path: &std::path::Path,
    encoding: DocumentEncoding,
) {
    editor.load_file_async_with_encoding(path, Some(encoding));
    window.publish_status_message(
        &format!("Reopening as {}", encoding.label()),
        MessageKind::Info,
    );
}

/// Change the next-save encoding, warning first if the conversion is lossy.
pub(super) fn begin_save_encoding_change(window: &LushtextWindow, encoding: DocumentEncoding) {
    let Some(editor) = window.active_editor() else {
        return;
    };

    if !editor.size_check().syntax_enabled() {
        // Large-file mode avoids synchronous whole-buffer scans on the GTK
        // thread. The save path still blocks a lossy write and asks for
        // confirmation before any bytes are written, so the check is deferred
        // rather than skipped.
        apply_save_encoding(window, &editor, encoding);
        window.publish_status_message(
            "Lossy conversion will be checked when saving.",
            MessageKind::Warning,
        );
        return;
    }

    let analysis_generation = editor.advance_lossy_analysis_generation();
    let content_generation = editor.draft_dirty_generation();
    let buffer = editor.buffer();
    let editor_weak = editor.downgrade();
    let window_weak = window.downgrade();

    let run_analysis = move |outcome: buffer_snapshot::BufferSnapshotOutcome| {
        let Some(editor) = editor_weak.upgrade() else {
            return;
        };
        // The capture handle is single-use; drop it before the worker starts so a
        // superseding change can install its own.
        editor
            .imp()
            .document_metadata
            .lossy_analysis_snapshot
            .take();
        let buffer_snapshot::BufferSnapshotOutcome::Captured(text) = outcome else {
            return;
        };
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let editor_weak = editor_weak.clone();
        spawn_blocking_then(
            window,
            move || {
                // The document-sized body is coalesced and destroyed on the
                // worker, never on GTK.
                let text = text.into_string_on_worker();
                super::test_policy_delay();
                editor_io::analyze_lossy_encoding(&text, encoding)
            },
            move |window, preview| {
                let Some(editor) = editor_weak.upgrade() else {
                    return;
                };
                if !analysis_is_current(&window, &editor, analysis_generation, content_generation) {
                    return;
                }
                if let Some(preview) = preview {
                    confirm_lossy_encoding_change(
                        &window,
                        &editor,
                        encoding,
                        &preview,
                        analysis_generation,
                        content_generation,
                    );
                    return;
                }

                apply_save_encoding(&window, &editor, encoding);
            },
        );
    };

    if buffer_snapshot::buffer_requires_chunked_snapshot(&buffer) {
        let snapshot = buffer_snapshot::snapshot_buffer_text_async(buffer, run_analysis);
        editor
            .imp()
            .document_metadata
            .lossy_analysis_snapshot
            .replace(Some(snapshot));
    } else {
        run_analysis(buffer_snapshot::BufferSnapshotOutcome::Captured(
            buffer_snapshot::BufferSnapshotPayload::direct(
                buffer_snapshot::snapshot_buffer_text_direct(&buffer),
            ),
        ));
    }
}

/// Whether a lossy analysis result still describes the live document.
///
/// The one predicate both resumption points use, so the dialog's response
/// handler cannot drift from the worker completion's check.
fn analysis_is_current(
    window: &LushtextWindow,
    editor: &LushtextEditorPage,
    analysis_generation: u32,
    content_generation: u64,
) -> bool {
    editor.lossy_analysis_generation() == analysis_generation
        && editor.draft_dirty_generation() == content_generation
        && window.is_active_editor(editor)
}

/// Apply a confirmed save-encoding choice to one editor.
pub(super) fn apply_save_encoding(
    window: &LushtextWindow,
    editor: &LushtextEditorPage,
    encoding: DocumentEncoding,
) {
    editor.set_save_encoding(encoding);
    window.refresh_status_bar();
    window.publish_status_message(
        &format!("Next save will use {}", encoding.label()),
        MessageKind::Info,
    );
}

/// Confirm a lossy conversion before changing save policy.
fn confirm_lossy_encoding_change(
    window: &LushtextWindow,
    editor: &LushtextEditorPage,
    encoding: DocumentEncoding,
    preview: &LossyEncodingPreview,
    analysis_generation: u32,
    content_generation: u64,
) {
    let dialog = libadwaita::AlertDialog::builder()
        .heading("Lossy Encoding Conversion")
        .body(preview.summary())
        .build();
    dialog.add_response(RESPONSE_CANCEL, "_Cancel");
    dialog.add_response(RESPONSE_CONTINUE, "_Use Encoding");
    dialog.set_response_appearance(RESPONSE_CONTINUE, libadwaita::ResponseAppearance::Suggested);
    dialog.set_default_response(Some(RESPONSE_CANCEL));
    dialog.set_close_response(RESPONSE_CANCEL);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    let detail = gtk4::Label::new(Some(
        "These characters cannot be represented and would be replaced on save:",
    ));
    detail.set_wrap(true);
    detail.set_xalign(0.0);
    content.append(&detail);

    for line in preview.detail_lines() {
        let label = gtk4::Label::new(Some(&line));
        label.set_wrap(true);
        label.set_xalign(0.0);
        label.add_css_class("dim-label");
        content.append(&label);
    }

    dialog.set_extra_child(Some(&content));

    let window_weak = window.downgrade();
    let editor_weak = editor.downgrade();
    dialog.connect_response(Some(RESPONSE_CONTINUE), move |_, _| {
        if let (Some(window), Some(editor)) = (window_weak.upgrade(), editor_weak.upgrade()) {
            // Rechecked here, not only at the worker completion: the user can
            // keep typing while this dialog is open.
            if !analysis_is_current(&window, &editor, analysis_generation, content_generation) {
                return;
            }
            apply_save_encoding(&window, &editor, encoding);
        }
    });

    dialog.present(Some(window));
}

/// Apply a new line-ending policy to the active editor.
///
/// Choosing a style on a mixed document also *resolves* the mixed state: the
/// detected value is rewritten and the mixed-line-ending finding is retired, so
/// the warning the user just answered does not reappear.
pub(super) fn apply_line_ending(window: &LushtextWindow, line_ending: LineEnding) {
    let Some(editor) = window.active_editor() else {
        return;
    };

    editor.set_save_line_ending(line_ending);
    if editor.detected_line_ending() == LineEnding::Mixed {
        let mut state = editor.document_encoding_state();
        state.detected_line_ending = line_ending;
        editor.set_document_encoding_state(state);
        editor.set_file_health(
            editor
                .file_health()
                .into_iter()
                .filter(|finding| finding.kind != FileHealthFindingKind::MixedLineEndings)
                .collect(),
        );
    }

    window.dismiss_editor_notifications(&editor);
    window.refresh_status_bar();
    window.publish_status_message(
        &format!("Next save will use {}", line_ending.label()),
        MessageKind::Info,
    );
}

/// Cycle the invisible-character mode in shortcut order.
///
/// With no open tab there is no per-editor state, so the cycle advances the
/// GSettings default instead — the action stays reachable without a document,
/// per the state-extreme rule in `.agents/rules/ui.md`.
pub(super) fn cycle_invisible_characters(window: &LushtextWindow) {
    let Some(editor) = window.active_editor() else {
        let current = InvisibleCharactersMode::from_id(
            window
                .imp()
                .settings
                .string(keys::INVISIBLE_CHARACTERS_MODE)
                .as_str(),
        )
        .unwrap_or_default();
        let next = current.next();
        let _ = window
            .imp()
            .settings
            .set_string(keys::INVISIBLE_CHARACTERS_MODE, next.id());
        window.publish_status_message(
            &format!("Invisible characters: {}", next.label()),
            MessageKind::Info,
        );
        return;
    };

    apply_invisible_mode(window, &editor, editor.invisible_characters_mode().next());
}

/// Apply one invisible-character mode to the active editor.
pub(super) fn apply_invisible_mode_to_active(
    window: &LushtextWindow,
    mode: InvisibleCharactersMode,
) {
    let Some(editor) = window.active_editor() else {
        return;
    };
    apply_invisible_mode(window, &editor, mode);
}

/// Apply one invisible-character mode to a specific editor tab and remember it.
fn apply_invisible_mode(
    window: &LushtextWindow,
    editor: &LushtextEditorPage,
    mode: InvisibleCharactersMode,
) {
    editor.set_invisible_characters_mode(mode);
    editor.apply_invisible_characters_mode();
    let _ = window
        .imp()
        .settings
        .set_string(keys::INVISIBLE_CHARACTERS_MODE, mode.id());

    window.publish_status_message(
        &format!("Invisible characters: {}", mode.label()),
        MessageKind::Info,
    );

    // "All" is the only mode that draws hidden-character markers, so it is the
    // only one worth pointing at File Health — and only when there is something
    // there to see.
    if mode == InvisibleCharactersMode::All && has_hidden_character_findings(editor) {
        window.publish_status_message(
            "Invisible characters: All. Open File Health for zero-width and BOM details.",
            MessageKind::Info,
        );
    }
}

/// Whether the active file-health set includes hidden-character issues.
fn has_hidden_character_findings(editor: &LushtextEditorPage) -> bool {
    editor
        .file_health()
        .iter()
        .any(|finding| policy::is_hidden_character_finding(finding.kind))
}

/// Ask the user whether to proceed with a lossy save, once.
pub(super) fn confirm_lossy_save(
    window: &LushtextWindow,
    editor: &LushtextEditorPage,
    preview: &LossyEncodingPreview,
    retry_save: impl FnOnce() + 'static,
) {
    let dialog = libadwaita::AlertDialog::builder()
        .heading("Lossy Save")
        .body(preview.summary())
        .build();
    dialog.add_response(RESPONSE_CANCEL, "_Cancel");
    dialog.add_response(RESPONSE_CONTINUE, "_Save Anyway");
    dialog.set_response_appearance(
        RESPONSE_CONTINUE,
        libadwaita::ResponseAppearance::Destructive,
    );
    dialog.set_default_response(Some(RESPONSE_CANCEL));
    dialog.set_close_response(RESPONSE_CANCEL);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    for line in preview.detail_lines() {
        let label = gtk4::Label::new(Some(&line));
        label.set_wrap(true);
        label.set_xalign(0.0);
        content.append(&label);
    }
    dialog.set_extra_child(Some(&content));

    // `FnOnce` in a GTK closure needs interior mutability to be taken exactly
    // once; a second response must not retry the save.
    let retry_save = Rc::new(RefCell::new(Some(retry_save)));
    let editor_weak = editor.downgrade();
    dialog.connect_response(Some(RESPONSE_CONTINUE), move |_, _| {
        if let Some(editor) = editor_weak.upgrade() {
            editor.arm_lossy_save_once();
        }
        if let Some(retry_save) = retry_save.borrow_mut().take() {
            retry_save();
        }
    });
    dialog.present(Some(window));
}
