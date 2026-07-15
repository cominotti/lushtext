// SPDX-License-Identifier: GPL-3.0-or-later

//! Markdown preview presentation: editor-only, side-by-side, and preview-only.
//!
//! The preview shell is Adwaita-native: a `MultiLayoutView` swaps the same
//! Markdown preview widget between an end-position `OverlaySplitView` sidebar
//! and a full-content preview-only layout. The public actions keep their
//! existing meanings while the implementation avoids app-owned paned animation.

use crate::config::keys;
use crate::services::markdown_render::MAX_MARKDOWN_SOURCE_BYTES;
use crate::ui::accessibility::{self, AnnouncementLane};
use crate::ui::buffer_snapshot::{self, BufferSnapshotOutcome};
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::markdown_preview::MarkdownPreviewRenderContext;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::{glib, prelude::*};
use sourceview5::prelude::*;

use super::LushtextWindow;
use super::imp::{
    PREVIEW_DEFAULT_WIDTH_SP, PREVIEW_LAYOUT_EDITOR, PREVIEW_LAYOUT_PREVIEW,
    PREVIEW_MAX_WIDTH_FRACTION, PREVIEW_MIN_WIDTH_SP, PREVIEW_SETTLE_DELAY_MS,
};

/// Register the preview-related actions on the window.
///
/// Stateful toggles back the visible UI, while parameterized target-state
/// actions let automation and smoke tests request a final state without
/// depending on toggle parity.
pub fn setup_preview_actions(window: &LushtextWindow) {
    let imp = window.imp();

    // Side-by-side preview pane toggle. Always start hidden — the spec's
    // "Never: automatic preview activation on app launch" constraint means
    // we don't restore from GSettings. Reset the persisted key to stay consistent.
    let pane_action =
        gtk4::gio::SimpleAction::new_stateful("toggle-preview-pane", None, &false.to_variant());
    let _ = imp.settings.set_boolean(keys::PREVIEW_PANE_VISIBLE, false);
    {
        let window_weak = window.downgrade();
        pane_action.connect_change_state(move |action, state| {
            let Some(state) = state else { return };
            let Some(new_visible) = state.get::<bool>() else {
                return;
            };
            if let Some(window) = window_weak.upgrade() {
                if window.is_focus_mode_active() {
                    window.mark_focus_mode_preview_changed();
                    return;
                }
                action.set_state(state);
                if new_visible && window.imp().preview_mode.get() {
                    window.exit_preview_only_mode_now();
                }
                window.imp().preview_visible.set(new_visible);
                window.apply_preview_shell_state();
                let _ = window
                    .imp()
                    .settings
                    .set_boolean(keys::PREVIEW_PANE_VISIBLE, new_visible);
                if new_visible {
                    window.refresh_preview();
                }
                window.announce_workflow_update(
                    AnnouncementLane::StatusUpdate,
                    if new_visible {
                        "preview-pane-shown"
                    } else {
                        "preview-pane-hidden"
                    },
                    if new_visible {
                        "Markdown preview pane shown"
                    } else {
                        "Markdown preview pane hidden"
                    },
                );
            }
        });
    }
    window.add_action(&pane_action);

    // Preview-only mode (Alt+P): replaces editor with full-width preview.
    // Only activates when the side-by-side pane is NOT visible.
    let mode_action =
        gtk4::gio::SimpleAction::new_stateful("toggle-preview-mode", None, &false.to_variant());
    {
        let window_weak = window.downgrade();
        mode_action.connect_change_state(move |action, state| {
            let Some(state) = state else { return };
            let Some(new_mode) = state.get::<bool>() else {
                return;
            };
            if let Some(window) = window_weak.upgrade() {
                // No-op if side-by-side is visible.
                if window.imp().preview_visible.get() {
                    return;
                }
                if window.is_focus_mode_active() {
                    window.mark_focus_mode_preview_changed();
                }
                action.set_state(state);
                window.imp().preview_mode.set(new_mode);
                window.apply_preview_shell_state();
                window.refresh_focus_mode_preview_column();
                if new_mode {
                    window.refresh_preview();
                }
                window.announce_workflow_update(
                    AnnouncementLane::StatusUpdate,
                    if new_mode {
                        "preview-mode-on"
                    } else {
                        "preview-mode-off"
                    },
                    if new_mode {
                        "Preview mode on"
                    } else {
                        "Preview mode off"
                    },
                );
            }
        });
    }
    window.add_action(&mode_action);

    window.add_action_entries([
        gtk4::gio::ActionEntry::builder("set-preview-pane-visible")
            .parameter_type(Some(glib::VariantTy::BOOLEAN))
            .activate(|window: &LushtextWindow, _, parameter| {
                let Some(visible) = parameter.and_then(glib::Variant::get::<bool>) else {
                    tracing::error!("set-preview-pane-visible: expected bool parameter");
                    return;
                };
                window.change_preview_action_state("toggle-preview-pane", visible);
            })
            .build(),
        gtk4::gio::ActionEntry::builder("set-preview-mode")
            .parameter_type(Some(glib::VariantTy::BOOLEAN))
            .activate(|window: &LushtextWindow, _, parameter| {
                let Some(enabled) = parameter.and_then(glib::Variant::get::<bool>) else {
                    tracing::error!("set-preview-mode: expected bool parameter");
                    return;
                };
                window.set_preview_mode_target(enabled);
            })
            .build(),
    ]);
}

impl LushtextWindow {
    /// Apply the requested preview state to the Adwaita presentation widgets.
    ///
    /// The editor layout owns both editor-only and side-by-side modes through
    /// `preview_split_view.show-sidebar`. Preview-only switches the slot layout
    /// so the same Markdown preview widget fills the content area.
    pub(super) fn apply_preview_shell_state(&self) {
        let imp = self.imp();
        let preview_only = imp.preview_mode.get();
        let side_by_side = imp.preview_visible.get();
        let preview_active = preview_only || side_by_side;

        self.sync_preview_width_constraints(self.width());

        if imp.editor_box.is_visible() == preview_only {
            imp.editor_box.set_visible(!preview_only);
        }
        if imp.markdown_preview.is_visible() != preview_active {
            imp.markdown_preview.set_visible(preview_active);
        }

        if preview_only {
            if imp.preview_split_view.shows_sidebar() {
                imp.preview_split_view.set_show_sidebar(false);
            }
            if imp.preview_layout_view.layout_name().as_deref() != Some(PREVIEW_LAYOUT_PREVIEW) {
                imp.preview_layout_view
                    .set_layout_name(PREVIEW_LAYOUT_PREVIEW);
            }
        } else {
            if imp.preview_layout_view.layout_name().as_deref() != Some(PREVIEW_LAYOUT_EDITOR) {
                imp.preview_layout_view
                    .set_layout_name(PREVIEW_LAYOUT_EDITOR);
            }
            if imp.preview_split_view.shows_sidebar() != side_by_side {
                imp.preview_split_view.set_show_sidebar(side_by_side);
            }
        }
        accessibility::set_hidden(&*imp.markdown_preview, !preview_active);
        if !preview_active {
            imp.markdown_preview.clear_source_snapshot();
            imp.markdown_preview.clear();
        }

        if let Some(editor) = self.active_editor_for_preview() {
            editor.set_preview_only_accessibility(preview_only);
        }
        self.queue_preview_layout_settle();
    }

    /// Clamp and apply the side-by-side preview width as split-view constraints.
    ///
    /// `preview-pane-position` is intentionally kept as a legacy key, but its
    /// value is now interpreted as the preferred preview width from the right
    /// edge rather than a `GtkPaned` divider coordinate.
    pub(super) fn sync_preview_width_constraints(&self, window_width: i32) {
        let imp = self.imp();
        let available_width = effective_preview_available_width(self, window_width);
        let preferred_width = preferred_preview_width(imp.preferred_preview_width.get());
        let preview_width = clamped_preview_width(preferred_width, available_width);
        let changed = set_preview_split_fixed_width(&imp.preview_split_view, preview_width);

        if changed && (imp.preview_visible.get() || imp.preview_mode.get()) {
            self.queue_preview_layout_settle();
        }
    }

    /// Mark preview presentation work as pending until layout and code blocks settle.
    ///
    /// Automation still exposes the compatibility `preview-animation` blocker,
    /// but the latch now tracks shell-neutral layout switching and embedded
    /// widget repair rather than a custom paned animation.
    pub(super) fn queue_preview_layout_settle(&self) {
        let imp = self.imp();
        if !imp.preview_visible.get() && !imp.preview_mode.get() {
            let _ = imp.preview_transition_settle.clear();
            return;
        }

        imp.preview_transition_settle.schedule(
            self,
            std::time::Duration::from_millis(PREVIEW_SETTLE_DELAY_MS),
            move |window, handle| {
                let imp = window.imp();
                if imp.preview_visible.get() || imp.preview_mode.get() {
                    let window_weak = window.downgrade();
                    imp.markdown_preview
                        .queue_code_block_width_refresh_after(move || {
                            let Some(window) = window_weak.upgrade() else {
                                return;
                            };
                            if window.imp().preview_transition_settle.pending() {
                                handle.finish_if_current();
                            }
                        });
                } else {
                    handle.finish_if_current();
                }
            },
        );
    }

    /// Test seam for readiness coverage that needs a pending preview settle.
    #[cfg(feature = "test-utils")]
    pub fn set_preview_transition_pending_for_test(&self, pending: bool) {
        if pending {
            self.imp().preview_transition_settle.schedule(
                self,
                std::time::Duration::from_secs(60),
                move |_, handle| handle.finish_if_current(),
            );
        } else {
            let _ = self.imp().preview_transition_settle.clear();
        }
    }

    /// Test seam exposing the helper-backed preview settle state.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn preview_transition_pending_for_test(&self) -> bool {
        self.imp().preview_transition_settle.pending()
    }

    /// Show or hide side-by-side preview as a temporary Focus Mode effect.
    ///
    /// This intentionally avoids writing the preview visibility GSettings key so
    /// Focus Mode can suppress the pane without changing the user's preference.
    pub(super) fn set_preview_pane_visible_for_focus_mode(&self, show: bool) {
        let imp = self.imp();
        if imp.preview_visible.get() == show {
            return;
        }
        if show && imp.preview_mode.get() {
            self.exit_preview_only_mode_now();
        }
        imp.preview_visible.set(show);
        self.set_preview_action_state("toggle-preview-pane", show);
        self.apply_preview_shell_state();
        if show {
            self.refresh_preview();
        }
    }

    /// Toggle preview-only mode from Focus Mode cleanup without recording user intent.
    ///
    /// Normal `Alt+P` activation still goes through the action path, where the
    /// window records that preview state changed while focused.
    pub(super) fn set_preview_mode_for_focus_mode(&self, enabled: bool) {
        let imp = self.imp();
        if imp.preview_mode.get() == enabled {
            return;
        }
        imp.preview_mode.set(enabled);
        self.set_preview_action_state("toggle-preview-mode", enabled);
        self.apply_preview_shell_state();
        self.refresh_focus_mode_preview_column();
        if enabled {
            self.refresh_preview();
        }
    }

    /// Request a preview state through the normal GAction path.
    ///
    /// Target-state automation remains a thin request layer; Focus Mode
    /// bookkeeping, GSettings writes, refreshes, and layout switching stay on
    /// the existing preview action workflow.
    pub(super) fn change_preview_action_state(&self, action_name: &str, enabled: bool) {
        let Some(action) = self.lookup_action(action_name) else {
            return;
        };
        if action
            .state()
            .and_then(|state| state.get::<bool>())
            .is_some_and(|current| current == enabled)
        {
            return;
        }
        action.change_state(&enabled.to_variant());
    }

    /// Apply preview-only target state without creating a second preview policy path.
    ///
    /// Side-by-side and preview-only remain mutually exclusive, so a request
    /// for preview-only first exits side-by-side through the same action system.
    fn set_preview_mode_target(&self, enabled: bool) {
        if enabled && self.imp().preview_visible.get() {
            self.change_preview_action_state("toggle-preview-pane", false);
        }
        self.change_preview_action_state("toggle-preview-mode", enabled);
    }

    fn set_preview_action_state(&self, action_name: &str, enabled: bool) {
        let Some(action) = self.lookup_action(action_name) else {
            return;
        };
        let Some(action) = action.downcast_ref::<gtk4::gio::SimpleAction>() else {
            return;
        };
        action.set_state(&enabled.to_variant());
    }

    /// Leave preview-only mode immediately and restore the source-editor shell.
    ///
    /// New-document creation and side-by-side preview transitions need a
    /// synchronous reset because delayed focus handoff can run before the next
    /// layout-settle tick. Keeping this in the preview workflow prevents action
    /// state, layout state, and widget visibility from drifting apart.
    pub(super) fn exit_preview_only_mode_now(&self) {
        let imp = self.imp();
        if !imp.preview_mode.get() {
            return;
        }

        imp.preview_mode.set(false);
        self.set_preview_action_state("toggle-preview-mode", false);
        self.apply_preview_shell_state();
        self.refresh_focus_mode_preview_column();
    }

    /// Refresh the preview content for the active tab.
    ///
    /// Uses a 300ms generation-counter debounce. If the active file is Markdown
    /// (detected via GtkSourceView language ID), renders its buffer content.
    /// Otherwise shows a placeholder message.
    pub(super) fn refresh_preview(&self) {
        let imp = self.imp();

        // Only refresh if some form of preview is visible.
        if !imp.preview_visible.get() && !imp.preview_mode.get() {
            return;
        }

        let editor = self.active_editor_for_preview();
        let preview = &imp.markdown_preview;

        match editor {
            Some(editor) if is_markdown(&editor) => {
                let buffer = editor.buffer();
                let char_count = usize::try_from(buffer.char_count()).unwrap_or(usize::MAX);
                if char_count > MAX_MARKDOWN_SOURCE_BYTES {
                    preview.show_placeholder(
                        "Markdown preview paused because the source exceeds 4 MiB",
                    );
                    return;
                }
                let context = MarkdownPreviewRenderContext::new(
                    editor.file_path(),
                    self.current_workspace_folder_paths(),
                );
                if markdown_snapshot_requires_chunked(
                    char_count,
                    buffer_snapshot::buffer_requires_chunked_snapshot(&buffer),
                ) {
                    let window_weak = self.downgrade();
                    let editor_weak = editor.downgrade();
                    let expected_dirty_generation = editor.draft_dirty_generation();
                    let expected_load_generation = editor.load_generation();
                    let expected_path = editor.file_path();
                    preview.show_content_placeholder("Preparing Markdown preview…");
                    let snapshot = buffer_snapshot::snapshot_buffer_text_async_budgeted(
                        buffer,
                        u64::try_from(MAX_MARKDOWN_SOURCE_BYTES)
                            .expect("Markdown source budget fits u64"),
                        move |outcome| {
                            let Some(window) = window_weak.upgrade() else {
                                return;
                            };
                            let Some(editor) = editor_weak.upgrade() else {
                                return;
                            };
                            window.imp().markdown_preview.clear_source_snapshot();
                            if !window.imp().preview_visible.get()
                                && !window.imp().preview_mode.get()
                            {
                                return;
                            }
                            if window.active_editor_for_preview().as_ref() != Some(&editor)
                                || editor.draft_dirty_generation() != expected_dirty_generation
                                || editor.load_generation() != expected_load_generation
                                || editor.file_path() != expected_path
                            {
                                return;
                            }
                            match outcome {
                                BufferSnapshotOutcome::Captured(text) => {
                                    window
                                        .imp()
                                        .markdown_preview
                                        .render_markdown_with_context(&text, &context);
                                }
                                BufferSnapshotOutcome::ExceededLimit { .. } => {
                                    window.imp().markdown_preview.show_placeholder(
                                        "Markdown preview paused because the source exceeds 4 MiB",
                                    );
                                }
                                BufferSnapshotOutcome::Cancelled(_) => {}
                            }
                        },
                    );
                    preview.replace_source_snapshot(Some(snapshot));
                } else {
                    preview.clear_source_snapshot();
                    let text = buffer_snapshot::snapshot_buffer_text_direct(&buffer);
                    preview.render_markdown_with_context(&text, &context);
                    preview.refresh_embedded_code_block_layouts();
                }
            }
            Some(_) => {
                preview.show_placeholder("Not a Markdown file");
            }
            None => {
                preview.show_placeholder("Open a Markdown file to see a rendered preview");
            }
        }
    }

    /// Debounced version of `refresh_preview` for buffer change events.
    /// Uses a 300ms debounce to coalesce rapid edits.
    pub(super) fn refresh_preview_debounced(&self) {
        let imp = self.imp();
        imp.preview_render_debounce.schedule(
            self,
            std::time::Duration::from_millis(300),
            move |window, _| {
                window.refresh_preview();
            },
        );
    }

    /// Get the active editor for preview purposes.
    fn active_editor_for_preview(&self) -> Option<LushtextEditorPage> {
        self.imp()
            .tab_view
            .selected_page()
            .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
    }
}

fn markdown_snapshot_requires_chunked(
    char_count: usize,
    buffer_policy_requires_chunking: bool,
) -> bool {
    buffer_policy_requires_chunking || char_count.saturating_mul(4) > MAX_MARKDOWN_SOURCE_BYTES
}

fn effective_preview_available_width(window: &LushtextWindow, window_width: i32) -> i32 {
    let content_width = window.imp().content_box.width();
    if content_width > 0 {
        content_width
    } else if window_width > 0 {
        window_width
    } else {
        window.width().max(1)
    }
}

fn preferred_preview_width(width: i32) -> i32 {
    if width > 0 {
        width
    } else {
        PREVIEW_DEFAULT_WIDTH_SP
    }
}

fn clamped_preview_width(preferred_width: i32, available_width: i32) -> f64 {
    let max_width = (f64::from(available_width.max(1)) * PREVIEW_MAX_WIDTH_FRACTION)
        .floor()
        .max(PREVIEW_MIN_WIDTH_SP);
    f64::from(preferred_width)
        .max(PREVIEW_MIN_WIDTH_SP)
        .min(max_width)
}

fn set_preview_split_fixed_width(
    split_view: &libadwaita::OverlaySplitView,
    preview_width: f64,
) -> bool {
    let mut changed = false;
    let current_min = split_view.min_sidebar_width();
    let current_max = split_view.max_sidebar_width();

    if preview_width > current_max && (current_max - preview_width).abs() > f64::EPSILON {
        split_view.set_max_sidebar_width(preview_width);
        changed = true;
    }
    if (current_min - preview_width).abs() > f64::EPSILON {
        split_view.set_min_sidebar_width(preview_width);
        changed = true;
    }
    if preview_width <= current_max && (current_max - preview_width).abs() > f64::EPSILON {
        split_view.set_max_sidebar_width(preview_width);
        changed = true;
    }

    changed
}

/// Check whether an editor page contains a Markdown file by querying
/// the GtkSourceView buffer's language ID.
fn is_markdown(editor: &LushtextEditorPage) -> bool {
    editor
        .buffer()
        .language()
        .is_some_and(|lang: sourceview5::Language| lang.id() == "markdown")
}

#[cfg(test)]
mod tests {
    use super::markdown_snapshot_requires_chunked;
    use crate::services::markdown_render::MAX_MARKDOWN_SOURCE_BYTES;

    #[test]
    fn multibyte_sized_markdown_uses_budgeted_snapshot_before_byte_cap() {
        let emoji_chars = MAX_MARKDOWN_SOURCE_BYTES / 2;

        assert!(markdown_snapshot_requires_chunked(emoji_chars, false));
        assert!(!markdown_snapshot_requires_chunked(
            MAX_MARKDOWN_SOURCE_BYTES / 4,
            false
        ));
    }
}
