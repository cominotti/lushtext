// SPDX-License-Identifier: GPL-3.0-or-later

//! Markdown preview pane: side-by-side and preview-only (Alt+P) toggle modes.
//!
//! The preview pane lives as the end-child of `preview_paned` inside the "tabs"
//! stack page. Three states are managed via the same GtkPaned:
//!
//! - **Editor only** (default): preview hidden, all space to editor
//! - **Side-by-side**: preview visible on right, clamped to max 1/3 window width
//! - **Preview only** (Alt+P): editor hidden, preview takes full width
//!
//! Animation follows the sidebar pattern: `AdwTimedAnimation` + `EaseOutCubic`,
//! 250ms, 1px minimum target (pixman-safe), `shrink-*-child` toggled during
//! animation, `connect_done` snaps visibility.

use crate::config::keys;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::markdown_preview::MarkdownPreviewRenderContext;
use gtk4::prelude::*;
use libadwaita::prelude::AnimationExt;
use sourceview5::prelude::*;

use super::LushtextWindow;

/// Register the preview-related actions on the window.
///
/// Two actions:
/// - `toggle-preview-pane`: shows/hides the side-by-side preview (stateful bool)
/// - `toggle-preview-mode`: Alt+P full-replacement toggle (stateful bool, only
///   works when side-by-side is hidden)
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
            action.set_state(state);
            if let Some(window) = window_weak.upgrade() {
                // If entering side-by-side while preview-only is active, exit preview-only first.
                // Must also cancel any in-flight animation and reset shrink-start-child
                // which animate_preview_mode(true) set to true temporarily.
                if new_visible && window.imp().preview_mode.get() {
                    window.imp().preview_mode.set(false);
                    window.imp().editor_box.set_visible(true);
                    if let Some(anim) = window.imp().preview_animation.take() {
                        anim.pause();
                    }
                    window.imp().preview_paned.set_shrink_start_child(false);
                }
                window.imp().preview_visible.set(new_visible);
                window.animate_preview_pane(new_visible);
                let _ = window
                    .imp()
                    .settings
                    .set_boolean(keys::PREVIEW_PANE_VISIBLE, new_visible);
                if new_visible {
                    window.refresh_preview();
                }
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
                action.set_state(state);
                window.imp().preview_mode.set(new_mode);
                window.animate_preview_mode(new_mode);
                if new_mode {
                    window.refresh_preview();
                }
            }
        });
    }
    window.add_action(&mode_action);
}

impl LushtextWindow {
    fn queue_preview_position_persist(&self, preview_width: i32) {
        let imp = self.imp();
        imp.pending_preview_pos.set(preview_width);

        if imp.last_preview_pos.get() == preview_width {
            return;
        }

        let generation = imp.preview_persist_generation.get().wrapping_add(1);
        imp.preview_persist_generation.set(generation);

        let window_weak = self.downgrade();
        glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let imp = window.imp();
            if imp.preview_persist_generation.get() != generation {
                return;
            }
            let preview_width = imp.pending_preview_pos.get();
            if imp.last_preview_pos.get() == preview_width {
                return;
            }
            if imp
                .settings
                .set_int(keys::PREVIEW_PANE_POSITION, preview_width)
                .is_ok()
            {
                imp.last_preview_pos.set(preview_width);
            }
        });
    }

    fn persist_preview_position_preference(&self) {
        let imp = self.imp();
        let preview_width = if imp.preview_visible.get() {
            imp.preview_paned
                .width()
                .saturating_sub(imp.preview_paned.position())
        } else {
            imp.saved_preview_pos.get()
        };
        self.queue_preview_position_persist(preview_width.max(0));
    }

    /// Animate the side-by-side preview pane show/hide.
    ///
    /// Mirrors the sidebar animation pattern from `animate_sidebar()`:
    /// `shrink-end-child` is temporarily `true`, target is 1px (not 0) on hide,
    /// `connect_done` calls `set_visible(false)` and restores shrink.
    fn animate_preview_pane(&self, show: bool) {
        let imp = self.imp();

        // Cancel any running preview animation.
        if let Some(anim) = imp.preview_animation.take() {
            anim.pause();
        }
        imp.preview_animation_active.set(false);

        let paned = &imp.preview_paned;
        let preview = &imp.markdown_preview;

        let paned_width = paned.width();
        if paned_width <= 0 {
            // Widget not yet realized — skip animation, just toggle visibility.
            if show {
                preview.set_visible(true);
            } else {
                preview.set_visible(false);
            }
            imp.preview_visible.set(show);
            return;
        }

        paned.set_shrink_end_child(true);

        let (from, to) = if show {
            let target_pos = paned_width - imp.saved_preview_pos.get();
            preview.set_visible(true);
            (f64::from(paned.position()), f64::from(target_pos.max(1)))
        } else {
            imp.saved_preview_pos.set(paned_width - paned.position());
            // Animate to full width minus 1px (preview shrinks to nothing).
            (
                f64::from(paned.position()),
                f64::from((paned_width - 1).max(1)),
            )
        };

        let paned_weak = paned.downgrade();
        let anim_target = libadwaita::CallbackAnimationTarget::new(move |value| {
            if let Some(p) = paned_weak.upgrade() {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "Preview pane animation endpoints stay within i32 paned coordinates"
                )]
                p.set_position(value as i32);
            }
        });

        let animation = libadwaita::TimedAnimation::new(
            paned.upcast_ref::<gtk4::Widget>(),
            from,
            to,
            250,
            anim_target,
        );
        animation.set_easing(libadwaita::Easing::EaseOutCubic);

        let preview_weak = if show {
            None
        } else {
            Some(preview.downgrade())
        };
        let done_window_weak = self.downgrade();
        animation.connect_done(move |_| {
            let Some(window) = done_window_weak.upgrade() else {
                return;
            };
            let imp = window.imp();
            if let Some(preview) = preview_weak.as_ref().and_then(glib::WeakRef::upgrade) {
                preview.set_visible(false);
            }
            imp.preview_paned.set_shrink_end_child(false);
            imp.preview_animation_active.set(false);
            window.persist_preview_position_preference();
        });

        imp.preview_animation_active.set(true);
        animation.play();
        imp.preview_animation.replace(Some(animation));
    }

    /// Animate the preview-only mode (Alt+P): editor hidden, preview full-width.
    ///
    /// Enter: show preview, animate paned position to 1px (editor shrinks),
    /// then hide editor_box. Exit: show editor_box, animate back to full width.
    fn animate_preview_mode(&self, enter: bool) {
        let imp = self.imp();

        if let Some(anim) = imp.preview_animation.take() {
            anim.pause();
        }
        imp.preview_animation_active.set(false);

        let paned = &imp.preview_paned;
        paned.set_shrink_start_child(true);

        let paned_width = paned.width().max(1);

        let (from, to) = if enter {
            imp.markdown_preview.set_visible(true);
            (f64::from(paned_width), 1.0)
        } else {
            imp.editor_box.set_visible(true);
            (f64::from(paned.position()), f64::from(paned_width))
        };

        let paned_weak = paned.downgrade();
        let anim_target = libadwaita::CallbackAnimationTarget::new(move |value| {
            if let Some(p) = paned_weak.upgrade() {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "Preview pane animation endpoints stay within i32 paned coordinates"
                )]
                p.set_position(value as i32);
            }
        });

        let animation = libadwaita::TimedAnimation::new(
            paned.upcast_ref::<gtk4::Widget>(),
            from,
            to,
            250,
            anim_target,
        );
        animation.set_easing(libadwaita::Easing::EaseOutCubic);

        let editor_box_weak = if enter {
            Some(imp.editor_box.downgrade())
        } else {
            None
        };
        let preview_weak = if enter {
            None
        } else {
            Some(imp.markdown_preview.downgrade())
        };
        let done_window_weak = self.downgrade();
        animation.connect_done(move |_| {
            let Some(window) = done_window_weak.upgrade() else {
                return;
            };
            let imp = window.imp();
            // After entering preview-only: hide the editor box.
            if let Some(editor_box) = editor_box_weak.as_ref().and_then(glib::WeakRef::upgrade) {
                editor_box.set_visible(false);
            }
            // After exiting preview-only: hide the preview widget.
            if let Some(preview) = preview_weak.as_ref().and_then(glib::WeakRef::upgrade) {
                preview.set_visible(false);
            }
            imp.preview_paned.set_shrink_start_child(false);
            imp.preview_animation_active.set(false);
            window.persist_preview_position_preference();
        });

        imp.preview_animation_active.set(true);
        animation.play();
        imp.preview_animation.replace(Some(animation));
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
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
                let context = MarkdownPreviewRenderContext::new(
                    editor.file_path(),
                    self.current_workspace_directory_roots(),
                );
                preview.render_markdown_with_context(&text, &context);
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
    /// Uses a 300ms generation counter to coalesce rapid edits.
    pub(super) fn refresh_preview_debounced(&self) {
        let imp = self.imp();
        let generation = imp.preview_render_generation.get().wrapping_add(1);
        imp.preview_render_generation.set(generation);

        let window_weak = self.downgrade();
        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            if window.imp().preview_render_generation.get() != generation {
                return;
            }
            window.refresh_preview();
        });
    }

    /// Clamp the preview pane position to at most 1/3 of the paned width
    /// (measured from the right edge). Mirrors `clamp_sidebar_position`
    /// for the right-side pane. Includes debounced GSettings persistence.
    pub(super) fn clamp_preview_position(&self, window_width: i32) {
        let imp = self.imp();
        if window_width <= 0 || !imp.preview_visible.get() {
            return;
        }

        let paned = &imp.preview_paned;
        let paned_width = paned.width();
        if paned_width <= 0 {
            return;
        }

        // Preview width = paned_width - position. Cap at 1/3.
        let max_preview_width = paned_width / 3;
        let min_position = paned_width - max_preview_width;
        let current = paned.position();
        let clamped = current.max(min_position).max(0);

        if clamped != current {
            paned.set_position(clamped);
        }

        if imp.preview_animation_active.get() {
            return;
        }
        let final_pos = paned.position();
        let preview_width = paned_width - final_pos;
        self.queue_preview_position_persist(preview_width);
    }

    /// Get the active editor for preview purposes.
    fn active_editor_for_preview(&self) -> Option<LushtextEditorPage> {
        self.imp()
            .tab_view
            .selected_page()
            .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
    }
}

/// Check whether an editor page contains a Markdown file by querying
/// the GtkSourceView buffer's language ID.
fn is_markdown(editor: &LushtextEditorPage) -> bool {
    editor
        .buffer()
        .language()
        .is_some_and(|lang: sourceview5::Language| lang.id() == "markdown")
}

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
