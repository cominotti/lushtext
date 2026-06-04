// SPDX-License-Identifier: GPL-3.0-or-later

//! Encoding toolkit and invisible-character workflows for the main window.
//!
//! This stays in the window shell layer because the workflows coordinate
//! dialogs, status-bar controls, per-tab editor state, and document reload/save
//! behavior. The underlying byte policy still lives in `services/editor_io.rs`.

use crate::config::keys;
use crate::model::encoding::{
    DocumentEncoding, FileHealthFindingKind, InvisibleCharactersMode, LineEnding,
};
use crate::services::{
    async_task,
    editor_io::{self, LossyEncodingPreview},
};
use crate::ui::buffer_snapshot;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicU64, Ordering};

const RESPONSE_CLOSE: &str = "close";
const RESPONSE_CONTINUE: &str = "continue";
const RESPONSE_CANCEL: &str = "cancel";

#[cfg(feature = "test-utils")]
static LOSSY_ENCODING_ANALYSIS_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Configure an artificial lossy-encoding analysis delay for window tests.
#[cfg(feature = "test-utils")]
pub fn set_lossy_encoding_analysis_delay_for_test(delay_ms: u64) {
    LOSSY_ENCODING_ANALYSIS_DELAY_MS.store(delay_ms, Ordering::Release);
}

impl super::LushtextWindow {
    /// Present the summary encoding surface for the active tab.
    pub(super) fn show_encoding_controls_dialog(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };

        let dialog = build_dialog(
            "Text Encoding",
            &format!(
                "Opened as {}. Next save uses {}.",
                editor.opened_encoding().label(),
                editor.save_encoding().label()
            ),
        );
        let content = standard_dialog_content();

        append_action_button_with_sensitivity(
            &content,
            "Reopen with Encoding…",
            editor.file_path().is_some(),
            self.downgrade(),
            dialog.clone(),
            |window| {
                window.show_reopen_encoding_dialog();
            },
        );
        append_action_button(
            &content,
            "Save Using Encoding…",
            self.downgrade(),
            dialog.clone(),
            |window| {
                window.show_save_encoding_dialog();
            },
        );
        append_action_button(
            &content,
            "Invisible Characters…",
            self.downgrade(),
            dialog.clone(),
            |window| {
                window.show_invisible_characters_dialog();
            },
        );

        dialog.set_extra_child(Some(&content));
        dialog.present(Some(self));
    }

    /// Present the chooser for reinterpreting the bytes currently on disk.
    fn show_reopen_encoding_dialog(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };

        let dialog = build_dialog(
            "Reopen with Encoding",
            "Choose how to interpret the bytes currently on disk.",
        );
        let content = standard_dialog_content();
        append_section_label(&content, "Choose Encoding");

        for encoding in DocumentEncoding::COMMON {
            let button = gtk4::Button::with_label(encoding.label());
            button.add_css_class("flat");
            button.set_sensitive(editor.opened_encoding() != encoding);
            let window_weak = self.downgrade();
            let dialog_clone = dialog.clone();
            button.connect_clicked(move |_| {
                if let Some(window) = window_weak.upgrade() {
                    window.request_reopen_with_encoding(encoding);
                }
                dialog_clone.close();
            });
            content.append(&button);
        }

        dialog.set_extra_child(Some(&content));
        dialog.present(Some(self));
    }

    /// Present the chooser for the document's next-save encoding policy.
    fn show_save_encoding_dialog(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };

        let dialog = build_dialog(
            "Save Using Encoding",
            "Choose how the next save should encode this document.",
        );
        let content = standard_dialog_content();
        append_section_label(&content, "Choose Encoding");

        for encoding in DocumentEncoding::COMMON {
            let button = gtk4::Button::with_label(encoding.label());
            button.add_css_class("flat");
            button.set_sensitive(editor.save_encoding() != encoding);
            let window_weak = self.downgrade();
            let dialog_clone = dialog.clone();
            button.connect_clicked(move |_| {
                if let Some(window) = window_weak.upgrade() {
                    window.choose_save_encoding(encoding);
                }
                dialog_clone.close();
            });
            content.append(&button);
        }

        dialog.set_extra_child(Some(&content));
        dialog.present(Some(self));
    }

    /// Present the chooser for invisible-character display mode.
    fn show_invisible_characters_dialog(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };

        let dialog = build_dialog(
            "Invisible Characters",
            "Choose how much whitespace and encoding-adjacent detail the editor should draw.",
        );
        let content = standard_dialog_content();
        append_section_label(&content, "Choose Mode");

        for mode in [
            InvisibleCharactersMode::Off,
            InvisibleCharactersMode::WhitespaceOnly,
            InvisibleCharactersMode::All,
        ] {
            let button = gtk4::Button::with_label(mode.label());
            button.add_css_class("flat");
            button.set_sensitive(editor.invisible_characters_mode() != mode);
            let window_weak = self.downgrade();
            let dialog_clone = dialog.clone();
            button.connect_clicked(move |_| {
                if let Some(window) = window_weak.upgrade() {
                    window.set_invisible_characters_mode_for_active_editor(mode);
                }
                dialog_clone.close();
            });
            content.append(&button);
        }

        dialog.set_extra_child(Some(&content));
        dialog.present(Some(self));
    }

    /// Present line-ending controls for the active tab.
    pub(super) fn show_line_ending_controls_dialog(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };

        let body = if editor.detected_line_ending() == LineEnding::Mixed {
            format!(
                "This document opened with mixed line endings. The next save is currently set to {}.",
                editor.save_line_ending().label()
            )
        } else {
            format!(
                "This document opened with {} line endings. The next save is currently set to {}.",
                editor.detected_line_ending().label(),
                editor.save_line_ending().label()
            )
        };

        let dialog = libadwaita::AlertDialog::builder()
            .heading("Line Endings")
            .body(body)
            .build();
        dialog.add_response(RESPONSE_CLOSE, "_Close");
        dialog.set_default_response(Some(RESPONSE_CLOSE));
        dialog.set_close_response(RESPONSE_CLOSE);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content.set_margin_top(6);
        content.set_margin_bottom(6);
        append_section_label(&content, "Choose Future Save Style");

        for line_ending in LineEnding::SAVE_CHOICES {
            let button = gtk4::Button::with_label(line_ending.label());
            button.add_css_class("flat");
            button.set_sensitive(editor.save_line_ending() != line_ending);
            let window_weak = self.downgrade();
            let dialog_clone = dialog.clone();
            button.connect_clicked(move |_| {
                if let Some(window) = window_weak.upgrade() {
                    window.apply_line_ending_choice(line_ending);
                }
                dialog_clone.close();
            });
            content.append(&button);
        }

        dialog.set_extra_child(Some(&content));
        dialog.present(Some(self));
    }

    /// Present the current file-health findings for the active tab.
    pub(super) fn show_file_health_dialog(&self) {
        let Some(editor) = self.active_editor() else {
            return;
        };

        let dialog = build_dialog(
            "File Health",
            "Review encoding-adjacent findings and any slower follow-up actions for the active document.",
        );
        let content = standard_dialog_content();

        let findings = editor.file_health();
        if findings.is_empty() {
            let label = gtk4::Label::new(Some(
                "No file-health issues are currently recorded for this document.",
            ));
            label.set_wrap(true);
            label.set_xalign(0.0);
            content.append(&label);
        } else {
            for finding in findings {
                let row = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                let title = gtk4::Label::new(Some(&finding.title));
                title.set_xalign(0.0);
                title.add_css_class("heading");
                let body = gtk4::Label::new(Some(&finding.body));
                body.set_wrap(true);
                body.set_xalign(0.0);
                body.add_css_class("dim-label");
                row.append(&title);
                row.append(&body);
                content.append(&row);
            }
        }

        dialog.set_extra_child(Some(&content));
        dialog.present(Some(self));
    }

    /// Cycle the active editor's invisible-character mode in shortcut order.
    pub(super) fn cycle_invisible_characters(&self) {
        let Some(editor) = self.active_editor() else {
            let current = InvisibleCharactersMode::from_id(
                self.imp()
                    .settings
                    .string(keys::INVISIBLE_CHARACTERS_MODE)
                    .as_str(),
            )
            .unwrap_or_default();
            let next = current.next();
            let _ = self
                .imp()
                .settings
                .set_string(keys::INVISIBLE_CHARACTERS_MODE, next.id());
            self.publish_status_message(
                &format!("Invisible characters: {}", next.label()),
                MessageKind::Info,
            );
            return;
        };

        self.set_invisible_characters_mode_for_editor(
            &editor,
            editor.invisible_characters_mode().next(),
        );
    }

    /// Start a reopen-with-encoding flow for the active editor.
    fn request_reopen_with_encoding(&self, encoding: DocumentEncoding) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        let Some(path) = editor.file_path() else {
            self.publish_status_message(
                "Save the document before using Reopen with Encoding.",
                MessageKind::Warning,
            );
            return;
        };

        if editor.is_modified() {
            let title = editor.title();
            let window_weak = self.downgrade();
            let editor_weak = editor.downgrade();
            self.show_discard_changes_dialog(&title, move |confirmed| {
                if !confirmed {
                    return;
                }
                if let (Some(window), Some(editor)) = (window_weak.upgrade(), editor_weak.upgrade())
                {
                    editor.load_file_async_with_encoding(&path, Some(encoding));
                    window.publish_status_message(
                        &format!("Reopening as {}", encoding.label()),
                        MessageKind::Info,
                    );
                }
            });
            return;
        }

        editor.load_file_async_with_encoding(&path, Some(encoding));
        self.publish_status_message(
            &format!("Reopening as {}", encoding.label()),
            MessageKind::Info,
        );
    }

    /// Change the next-save encoding for the active editor, warning if it is lossy.
    fn choose_save_encoding(&self, encoding: DocumentEncoding) {
        let Some(editor) = self.active_editor() else {
            return;
        };

        if !editor.size_check().syntax_enabled() {
            // Large-file mode avoids synchronous whole-buffer scans on the GTK
            // thread; the existing save path will still block lossy writes and
            // ask for confirmation before bytes are written.
            self.apply_save_encoding_choice(&editor, encoding);
            self.publish_status_message(
                "Lossy conversion will be checked when saving.",
                MessageKind::Warning,
            );
            return;
        }

        let analysis_generation = editor.advance_lossy_analysis_generation();
        let content_generation = editor.draft_dirty_generation();
        let buffer = editor.buffer();
        let editor_weak = editor.downgrade();
        let window = self.clone();
        let run_analysis = move |text: String| {
            let editor_weak = editor_weak.clone();
            async_task::spawn_blocking_then(
                window,
                move || {
                    delay_lossy_encoding_analysis_for_test();
                    editor_io::analyze_lossy_encoding(&text, encoding)
                },
                move |window, preview| {
                    let Some(editor) = editor_weak.upgrade() else {
                        return;
                    };
                    if editor.lossy_analysis_generation() != analysis_generation
                        || editor.draft_dirty_generation() != content_generation
                        || !window.is_active_editor(&editor)
                    {
                        return;
                    }
                    if let Some(preview) = preview {
                        window.show_lossy_encoding_dialog(
                            &editor,
                            encoding,
                            &preview,
                            analysis_generation,
                            content_generation,
                        );
                        return;
                    }

                    window.apply_save_encoding_choice(&editor, encoding);
                },
            );
        };

        if buffer_snapshot::buffer_requires_chunked_snapshot(&buffer) {
            buffer_snapshot::snapshot_buffer_text_async(buffer, run_analysis);
        } else {
            run_analysis(buffer_snapshot::snapshot_buffer_text_direct(&buffer));
        }
    }

    /// Apply a confirmed save-encoding choice to one editor.
    fn apply_save_encoding_choice(&self, editor: &LushtextEditorPage, encoding: DocumentEncoding) {
        editor.set_save_encoding(encoding);
        self.refresh_status_bar();
        self.publish_status_message(
            &format!("Next save will use {}", encoding.label()),
            MessageKind::Info,
        );
    }

    /// Show a lossy-conversion confirmation dialog before changing save policy.
    fn show_lossy_encoding_dialog(
        &self,
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
        dialog
            .set_response_appearance(RESPONSE_CONTINUE, libadwaita::ResponseAppearance::Suggested);
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

        let window_weak = self.downgrade();
        let editor_weak = editor.downgrade();
        dialog.connect_response(Some(RESPONSE_CONTINUE), move |_, _| {
            if let (Some(window), Some(editor)) = (window_weak.upgrade(), editor_weak.upgrade()) {
                if editor.lossy_analysis_generation() != analysis_generation
                    || editor.draft_dirty_generation() != content_generation
                    || !window.is_active_editor(&editor)
                {
                    return;
                }
                window.apply_save_encoding_choice(&editor, encoding);
            }
        });

        dialog.present(Some(self));
    }

    /// Apply a new line-ending policy to the active editor.
    pub(super) fn apply_line_ending_choice(&self, line_ending: LineEnding) {
        let Some(editor) = self.active_editor() else {
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

        self.dismiss_editor_notifications(&editor);
        self.refresh_status_bar();
        self.publish_status_message(
            &format!("Next save will use {}", line_ending.label()),
            MessageKind::Info,
        );
    }

    /// Apply one invisible-character mode to the active editor and remember it as the default.
    fn set_invisible_characters_mode_for_active_editor(&self, mode: InvisibleCharactersMode) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        self.set_invisible_characters_mode_for_editor(&editor, mode);
    }

    /// Apply one invisible-character mode to a specific editor tab.
    fn set_invisible_characters_mode_for_editor(
        &self,
        editor: &LushtextEditorPage,
        mode: InvisibleCharactersMode,
    ) {
        editor.set_invisible_characters_mode(mode);
        editor.apply_invisible_characters_mode();
        let _ = self
            .imp()
            .settings
            .set_string(keys::INVISIBLE_CHARACTERS_MODE, mode.id());

        self.publish_status_message(
            &format!("Invisible characters: {}", mode.label()),
            MessageKind::Info,
        );

        if mode == InvisibleCharactersMode::All && has_hidden_character_findings(editor) {
            self.publish_status_message(
                "Invisible characters: All. Open File Health for zero-width and BOM details.",
                MessageKind::Info,
            );
        }
    }

    /// Handle a lossy save failure by asking the user whether to proceed once.
    pub(super) fn confirm_lossy_save(
        &self,
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

        let retry_save = Rc::new(RefCell::new(Some(retry_save)));
        let editor_weak = editor.downgrade();
        let retry_save_closure = retry_save;
        dialog.connect_response(Some(RESPONSE_CONTINUE), move |_, _| {
            if let Some(editor) = editor_weak.upgrade() {
                editor.arm_lossy_save_once();
            }
            if let Some(retry_save) = retry_save_closure.borrow_mut().take() {
                retry_save();
            }
        });
        dialog.present(Some(self));
    }
}

fn delay_lossy_encoding_analysis_for_test() {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = LOSSY_ENCODING_ANALYSIS_DELAY_MS.load(Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
}

/// Build a standard dialog shell for document-local format workflows.
fn build_dialog(heading: &str, body: &str) -> libadwaita::AlertDialog {
    let dialog = libadwaita::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .build();
    dialog.add_response(RESPONSE_CLOSE, "_Close");
    dialog.set_default_response(Some(RESPONSE_CLOSE));
    dialog.set_close_response(RESPONSE_CLOSE);
    dialog
}

/// Create the standard content box used by the encoding toolkit dialogs.
fn standard_dialog_content() -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    content
}

/// Append a compact section heading into an AlertDialog extra child.
fn append_section_label(container: &gtk4::Box, title: &str) {
    let label = gtk4::Label::new(Some(title));
    label.set_xalign(0.0);
    label.add_css_class("heading");
    container.append(&label);
}

/// Append one dialog action button that closes the current dialog before
/// opening the next window-level format surface.
fn append_action_button(
    container: &gtk4::Box,
    label: &str,
    window_weak: glib::WeakRef<super::LushtextWindow>,
    dialog: libadwaita::AlertDialog,
    action: impl Fn(super::LushtextWindow) + 'static,
) {
    append_action_button_with_sensitivity(container, label, true, window_weak, dialog, action);
}

/// Append one dialog action button with an explicit sensitivity override.
fn append_action_button_with_sensitivity(
    container: &gtk4::Box,
    label: &str,
    sensitive: bool,
    window_weak: glib::WeakRef<super::LushtextWindow>,
    dialog: libadwaita::AlertDialog,
    action: impl Fn(super::LushtextWindow) + 'static,
) {
    let button = gtk4::Button::with_label(label);
    button.add_css_class("flat");
    button.set_sensitive(sensitive);
    button.connect_clicked(move |_| {
        dialog.close();
        if let Some(window) = window_weak.upgrade() {
            action(window);
        }
    });
    container.append(&button);
}

/// Return whether the active file-health set includes hidden-character issues.
fn has_hidden_character_findings(editor: &LushtextEditorPage) -> bool {
    editor.file_health().iter().any(|finding| {
        matches!(
            finding.kind,
            FileHealthFindingKind::Utf8Bom
                | FileHealthFindingKind::NonBreakingSpace
                | FileHealthFindingKind::ZeroWidthCharacter
        )
    })
}
