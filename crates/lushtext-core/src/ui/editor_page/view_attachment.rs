// SPDX-License-Identifier: GPL-3.0-or-later

//! Which buffer the editor's view shows: the document, or an empty
//! placeholder while the tab is hidden (`defer-background-tab-layout`).
//!
//! **Cross-cutting, no role.** Every workflow that reads or writes the
//! document goes through [`LushtextEditorPage::buffer`], which is the
//! document buffer whatever the view shows; this module only decides what
//! the view lays out.
//!
//! Why: a `GtkSourceView` has a text layout whether or not its tab was ever
//! shown, and a view on a hidden `AdwTabView` page validates its whole buffer
//! from its incremental idle (ledger A82, A80). Restoring ten 8 MiB documents
//! spent most of the application's CPU laying out tabs nobody was looking at,
//! and because GLib co-dispatches equal-priority sources (A81), every hidden
//! view still validating lengthened every iteration of the visible one. Giving
//! the view an empty buffer stops that at once and leaves the document buffer
//! untouched (A83).
//!
//! The rule:
//!
//! - **From construction until the tab is first shown**, the view holds the
//!   placeholder. A background tab's document is loaded, drafted, searched and
//!   saved through the document buffer as before; it is simply never laid out.
//! - **On map** (the tab is shown) the view gets the document back before its
//!   children map, and scrolls to the top line it kept, if any, on its first
//!   layout (A83: a reattached view starts at the buffer start).
//! - **While unmapped** the view stays attached, so switching back to a tab
//!   that finished laying out costs nothing. If it is seen laying out while
//!   hidden — its adjustment's `upper` changes while unmapped, which every
//!   2000 px validation batch does (A83, A31) — it is detached from an idle at
//!   the completion priority, before its next batch, keeping its top line.
//!
//! The placeholder takes the document's style scheme before each detach, so
//! a detach or reattach never changes the view's CSS.

use gtk4::glib;
use gtk4::glib::translate::IntoGlib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use super::LushtextEditorPage;

/// What the editor's view currently shows.
#[derive(Default)]
pub(crate) struct ViewAttachmentState {
    /// The document buffer, captured from the template at construction.
    pub(super) document: std::cell::OnceCell<sourceview5::Buffer>,
    /// The empty buffer the view shows while detached.
    pub(super) placeholder: std::cell::OnceCell<sourceview5::Buffer>,
    /// The line the view showed at its top when it was detached, or the
    /// scroll target a restore asked for while detached: a mark in the
    /// document buffer, applied on the next attach.
    kept_top: std::cell::RefCell<Option<gtk4::TextMark>>,
    /// The hidden-validation watch on the view's adjustments, while unmapped
    /// and attached.
    watch: std::cell::RefCell<Vec<(gtk4::Adjustment, glib::SignalHandlerId)>>,
    /// A detach is queued for the completion-priority idle.
    detach_queued: std::cell::Cell<bool>,
    /// The kept top line a reattached view is being held at while it lays
    /// the document out again (A83).
    restoring: std::cell::RefCell<Option<TopLineRestore>>,
    /// How many times the view was detached and attached (evidence).
    detaches: std::cell::Cell<u64>,
    attaches: std::cell::Cell<u64>,
}

/// A reattached view held at its kept top line until it is laid out.
struct TopLineRestore {
    mark: gtk4::TextMark,
    adjustment: gtk4::Adjustment,
    handler: glib::SignalHandlerId,
    sentinel: Option<glib::SourceId>,
    since_us: i64,
}

/// One step below GtkTextView's incremental validation idle
/// (`GTK_TEXT_VIEW_PRIORITY_VALIDATE`, one below the completion priority): a
/// source here dispatches only once no attached view has lines left to
/// validate (ledger A80).
fn laid_out_priority() -> glib::Priority {
    glib::Priority::from(gtk_lush_tasks::completion_priority().into_glib() + 2)
}

impl LushtextEditorPage {
    /// Capture the document buffer and give the view the placeholder.
    ///
    /// Called at the end of `constructed`, after every handler that connects
    /// to the document buffer was installed.
    pub(super) fn setup_view_attachment(&self) {
        let state = &self.imp().view_attachment;
        let placeholder = sourceview5::Buffer::new(None);
        placeholder.set_highlight_syntax(false);
        placeholder.set_highlight_matching_brackets(false);
        let _ = state.placeholder.set(placeholder);
        self.detach_view_now();
    }

    /// The document buffer while the page's template children are alive, for
    /// evidence and teardown readers that must not panic on a disposed page
    /// (GTK4 clears template children in `dispose()`).
    #[must_use]
    pub(crate) fn live_document_buffer(&self) -> Option<sourceview5::Buffer> {
        let view = self.imp().source_view.try_get()?;
        self.imp()
            .view_attachment
            .document
            .get()
            .cloned()
            .or_else(|| view.buffer().downcast().ok())
    }

    /// Whether the view lays out the document (it is attached).
    #[must_use]
    pub fn view_shows_document(&self) -> bool {
        let Some(view) = self.imp().source_view.try_get() else {
            return false;
        };
        self.imp()
            .view_attachment
            .document
            .get()
            .is_none_or(|document| view.buffer() == *document.upcast_ref::<gtk4::TextBuffer>())
    }

    /// Whether a paced install or replacement into this editor follows the
    /// view's layout (`load::policy::install_slice_pacing`): the view shows
    /// the document, or the tab is its window's selected tab and is about to
    /// be shown (a window still composing). Only a background tab's paced
    /// slices run ahead of validation, so the selected document installs
    /// exactly as before.
    #[must_use]
    pub(crate) fn install_follows_layout(&self) -> bool {
        self.view_shows_document()
            || self
                .root()
                .and_downcast::<crate::ui::window::LushtextWindow>()
                .is_some_and(|window| window.is_selected_editor(self))
    }

    /// How often the view was detached and attached, for tests and evidence.
    #[must_use]
    pub fn view_attachment_counts(&self) -> (u64, u64) {
        let state = &self.imp().view_attachment;
        (state.detaches.get(), state.attaches.get())
    }

    /// A window that is not mapped yet changed its selected tab: the selected
    /// editor's view lays the document out now, as it will be shown when the
    /// window maps (its install and layout then overlap, as for any shown
    /// tab), and every other editor that was never shown shows its
    /// placeholder again. Called from the window's `selected-page` handler.
    pub(crate) fn follow_unmapped_window_selection(&self, selected: bool) {
        if self.is_mapped() || self.in_destruction() {
            return;
        }
        if selected {
            self.attach_view();
        } else {
            self.detach_view_now();
        }
    }

    /// Give the view its document back, scrolled to the kept top line.
    ///
    /// Called from `map` before the children map, so the first frame of a
    /// shown tab lays out the document (only its visible part, at once).
    pub(super) fn attach_view(&self) {
        self.stop_hidden_validation_watch();
        let state = &self.imp().view_attachment;
        state.detach_queued.set(false);
        if self.view_shows_document() {
            // Shown again before it was detached: the line kept at the hide
            // is the one the view still shows.
            self.forget_kept_top();
            return;
        }
        let Some(document) = state.document.get().cloned() else {
            return;
        };
        let view = self.source_view();
        crate::ui::responsiveness_probe::span("view_attach", || {
            self.set_view_buffer(view, &document);
        });
        state.attaches.set(state.attaches.get().saturating_add(1));
        if self.is_protected_view() {
            // Protected View keeps GTK's primary-selection provider off the
            // document (`set_protected_primary_selection_off`); `set_buffer`
            // just added it back.
            self.set_protected_primary_selection_off(true);
        }
        let kept = state.kept_top.borrow_mut().take();
        if let Some(mark) = kept
            && !mark.is_deleted()
        {
            view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
            self.hold_top_line_while_laying_out(mark);
        }
        self.schedule_minimap_refresh();
    }

    /// Keep a reattached view at `mark` while it lays the document out again:
    /// a scroll queued before the lines above the target have heights lands
    /// elsewhere, so the view scrolls to it again after every validation
    /// batch (each grows `upper`), and once more when it is laid out (A83).
    /// The first input after the reattach hands the position to the user.
    fn hold_top_line_while_laying_out(&self, mark: gtk4::TextMark) {
        self.stop_holding_top_line();
        let Some(adjustment) = self.source_view().vadjustment() else {
            self.buffer().delete_mark(&mark);
            return;
        };
        let since_us = glib::monotonic_time();
        let editor_weak = self.downgrade();
        let handler = adjustment.connect_upper_notify(move |_| {
            if let Some(editor) = editor_weak.upgrade() {
                editor.reassert_top_line(false);
            }
        });
        let editor_weak = self.downgrade();
        let sentinel = glib::idle_add_local_full(laid_out_priority(), move || {
            if let Some(editor) = editor_weak.upgrade() {
                if let Some(restore) = editor.imp().view_attachment.restoring.borrow_mut().as_mut()
                {
                    restore.sentinel = None;
                }
                editor.reassert_top_line(true);
            }
            glib::ControlFlow::Break
        });
        *self.imp().view_attachment.restoring.borrow_mut() = Some(TopLineRestore {
            mark,
            adjustment,
            handler,
            sentinel: Some(sentinel),
            since_us,
        });
    }

    fn reassert_top_line(&self, laid_out: bool) {
        let (mark, since_us) = {
            let restoring = self.imp().view_attachment.restoring.borrow();
            let Some(restore) = restoring.as_ref() else {
                return;
            };
            (restore.mark.clone(), restore.since_us)
        };
        let user_moved =
            crate::ui::input_quiet::last_input_us().is_some_and(|input| input > since_us);
        if user_moved || mark.is_deleted() || !self.view_shows_document() {
            self.stop_holding_top_line();
            return;
        }
        let view = self.source_view();
        let now = glib::monotonic_time();
        crate::ui::responsiveness_probe::record_span("view_hold_scroll", now, now);
        if view.is_mapped() {
            let mut target = self.buffer().iter_at_mark(&mark);
            view.scroll_to_iter(&mut target, 0.0, true, 0.0, 0.0);
        } else {
            // Not allocated yet (a window still composing): queue it for the
            // first allocation instead.
            view.scroll_to_mark(&mark, 0.0, true, 0.0, 0.0);
        }
        if laid_out {
            self.stop_holding_top_line();
        }
    }

    fn stop_holding_top_line(&self) {
        let restore = self.imp().view_attachment.restoring.borrow_mut().take();
        if let Some(restore) = restore {
            let now = glib::monotonic_time();
            crate::ui::responsiveness_probe::record_span("view_hold_end", now, now);
            restore.adjustment.disconnect(restore.handler);
            if let Some(sentinel) = restore.sentinel {
                sentinel.remove();
            }
            if !restore.mark.is_deleted() {
                self.buffer().delete_mark(&restore.mark);
            }
        }
    }

    /// The tab was hidden: watch for the view laying out while unmapped.
    pub(super) fn watch_hidden_validation(&self) {
        if !self.view_shows_document() || self.in_destruction() {
            return;
        }
        self.stop_hidden_validation_watch();
        let view = self.source_view();
        // Keep the top line now, while the layout still describes what the
        // view showed: a later re-layout while hidden moves it (A83). A view
        // still being held at a kept line keeps that line.
        let held = self
            .imp()
            .view_attachment
            .restoring
            .borrow()
            .as_ref()
            .and_then(|restore| {
                (!restore.mark.is_deleted()).then(|| self.buffer().iter_at_mark(&restore.mark))
            });
        self.stop_holding_top_line();
        let top = held.unwrap_or_else(|| view.line_at_y(view.visible_rect().y()).0);
        self.keep_top(&self.buffer(), &top);
        let mut watch = Vec::new();
        for adjustment in [view.vadjustment(), view.hadjustment()]
            .into_iter()
            .flatten()
        {
            let editor_weak = self.downgrade();
            let id = adjustment.connect_upper_notify(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.queue_hidden_detach();
                }
            });
            watch.push((adjustment, id));
        }
        *self.imp().view_attachment.watch.borrow_mut() = watch;
    }

    fn stop_hidden_validation_watch(&self) {
        for (adjustment, id) in self.imp().view_attachment.watch.borrow_mut().drain(..) {
            adjustment.disconnect(id);
        }
    }

    /// Detach from an idle one step above the validation idle, so the view
    /// lays out at most the batch that revealed it.
    fn queue_hidden_detach(&self) {
        let state = &self.imp().view_attachment;
        if state.detach_queued.replace(true) {
            return;
        }
        let editor_weak = self.downgrade();
        glib::idle_add_local_full(gtk_lush_tasks::completion_priority(), move || {
            if let Some(editor) = editor_weak.upgrade() {
                let state = &editor.imp().view_attachment;
                if state.detach_queued.replace(false)
                    && !editor.is_mapped()
                    && !editor.in_destruction()
                {
                    editor.detach_view_now();
                }
            }
            glib::ControlFlow::Break
        });
    }

    /// Give the view the placeholder. The top line was kept at the hide.
    fn detach_view_now(&self) {
        self.stop_hidden_validation_watch();
        self.stop_holding_top_line();
        let state = &self.imp().view_attachment;
        let (Some(document), Some(placeholder)) = (state.document.get(), state.placeholder.get())
        else {
            return;
        };
        if !self.view_shows_document() {
            return;
        }
        let view = self.source_view();
        placeholder.set_style_scheme(document.style_scheme().as_ref());
        crate::ui::responsiveness_probe::span("view_detach", || {
            self.set_view_buffer(view, placeholder);
        });
        state.detaches.set(state.detaches.get().saturating_add(1));
    }

    /// `gtk_text_view_set_buffer`, keeping Protected View's primary-selection
    /// bookkeeping true: a realized view removes the primary-selection
    /// clipboard from its old buffer and adds it to the new one, and removing
    /// one the document no longer has (Protected View took it off) is a
    /// `Gtk-CRITICAL`. So it is put back first; the attach takes it off again.
    fn set_view_buffer(&self, view: &sourceview5::View, buffer: &sourceview5::Buffer) {
        self.set_protected_primary_selection_off(false);
        view.set_buffer(Some(buffer));
    }

    fn forget_kept_top(&self) {
        let kept = self.imp().view_attachment.kept_top.borrow_mut().take();
        if let Some(mark) = kept
            && !mark.is_deleted()
        {
            self.buffer().delete_mark(&mark);
        }
    }

    fn keep_top(&self, document: &sourceview5::Buffer, top: &gtk4::TextIter) {
        let mark = document.create_mark(None, top, true);
        if let Some(old) = self
            .imp()
            .view_attachment
            .kept_top
            .borrow_mut()
            .replace(mark)
            && !old.is_deleted()
        {
            document.delete_mark(&old);
        }
    }

    /// Scroll the view to `iter` as `gtk_text_view_scroll_to_mark` would:
    /// at once when the view shows the document; else the line becomes the
    /// kept top line, applied on the next attach (a detached view shows the
    /// placeholder, so a document mark cannot be scrolled to, A83).
    pub fn scroll_to_document_iter(
        &self,
        iter: &gtk4::TextIter,
        within_margin: f64,
        use_align: bool,
        xalign: f64,
        yalign: f64,
    ) {
        let document = self.buffer();
        if self.view_shows_document() {
            let mark = document.create_mark(None, iter, true);
            self.source_view()
                .scroll_to_mark(&mark, within_margin, use_align, xalign, yalign);
            document.delete_mark(&mark);
            if self.imp().view_attachment.kept_top.borrow().is_some() {
                // Hidden but still attached: a later detach must not bring
                // back the line kept at the hide instead of this one.
                self.keep_top(&document, iter);
            }
        } else {
            self.keep_top(&document, iter);
        }
    }

    /// The document line at the top of the view, without laying anything
    /// out for a detached view: the kept top line, else the first line.
    #[must_use]
    pub(crate) fn detached_top_line(&self) -> Option<u32> {
        if self.view_shows_document() {
            return None;
        }
        let line = self
            .imp()
            .view_attachment
            .kept_top
            .borrow()
            .as_ref()
            .filter(|mark| !mark.is_deleted())
            .map_or(0, |mark| self.buffer().iter_at_mark(mark).line());
        Some(u32::try_from(line).unwrap_or(0))
    }
}
