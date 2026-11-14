// SPDX-License-Identifier: GPL-3.0-or-later

//! Called presentation surface: the unified notes-browser dialog.
//!
//! # Role
//!
//! This module carries **no role**. It projects `WFR-NOTES-BOOKMARKS` onto the
//! browser dialog's widgets — the adaptive shell, the sectioned, virtualized
//! `GtkListView` and its row and header factories, the preview stack, and the per-session state those widgets need —
//! so under `gtk-adapter-module-boundaries` it is a called presentation surface:
//! outside the five-name role taxonomy, taking none of those names, and owning no
//! `policy.rs` and no `evidence.rs`. Named in the `WFR-NOTES-BOOKMARKS` matrix
//! row. Every decision it renders comes from `policy`.
//!
//! # Why coordination state lives here anyway
//!
//! `NotesBrowserState` holds the dialog's `search_debounce` and its three
//! coordinators (source refresh, query, and closed-file excerpt preview). That is
//! deliberate and is the same shape a `GtkWidget` subclass's `imp.rs` takes: this
//! module is the dialog's subclass-equivalent state home, while the modules that
//! *drive* those coordinators are the roles — `source_execution` advances the
//! source generation and `begin_mode`, `query_execution` owns the query flight,
//! and `bookmark_execution` owns the excerpt preview. A presentation surface may
//! **hold** a workflow's GTK-side state; what it may not do is coordinate an
//! ordered stage, and it does not.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::sync::Arc;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_settle::Debounce;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, NavigationPageExt};

use crate::model::palette::{PaletteNoteEntry, PaletteNoteTarget};
use crate::services::palette as palette_service;
use crate::ui::accessibility;
use crate::ui::markdown_preview::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};
use crate::ui::row_text::LushtextRowText;
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;
use super::bookmark_execution::{ensure_raw_preview_target_tag, open_editor_at_line};
use super::chrome::{
    build_dialog_close_button, empty_browser_label, focus_after_present,
    install_dialog_escape_close,
};
use super::policy::{NotesBrowserListedCount, NotesBrowserModeExt as _};
use super::query_execution::schedule_notes_browser_search;
use super::row_model::{NotesBrowserRow, NotesBrowserRowModel};
use super::source_execution::OpenEditorCaptureOwner;

/// Stack child name for Markdown/status bookmark and note previews.
pub(super) const NOTES_PREVIEW_MARKDOWN_CHILD: &str = "markdown";
/// Stack child name for raw-text bookmark previews.
pub(super) const NOTES_PREVIEW_RAW_CHILD: &str = "raw";
/// Horizontal inset inside raw bookmark previews.
pub(super) const NOTES_RAW_PREVIEW_TEXT_MARGIN_HORIZONTAL_SP: i32 = 12;
/// Vertical inset inside raw bookmark previews.
pub(super) const NOTES_RAW_PREVIEW_TEXT_MARGIN_VERTICAL_SP: i32 = 10;

/// One entry shown in the unified notes browser.
pub(super) type NotesBrowserEntry = PaletteNoteEntry;

/// Space between a browser row's icon, text and Open button.
const NOTES_ROW_SPACING_SP: i32 = 12;
/// Vertical inset of a browser row's content, so two text lines read as one row.
const NOTES_ROW_MARGIN_VERTICAL_SP: i32 = 3;
/// Start inset of a browser row's content, matching `AdwSidebar`'s rows.
const NOTES_ROW_MARGIN_START_SP: i32 = 6;
/// Start inset of a section title, matching `AdwSidebar`'s section headers.
const NOTES_HEADER_MARGIN_START_SP: i32 = 10;
/// Stack child name for the results list.
const NOTES_RESULTS_LIST_CHILD: &str = "list";
/// Stack child name for the results empty state.
const NOTES_RESULTS_EMPTY_CHILD: &str = "empty";

/// State for one open unified notes browser dialog.
pub(super) struct NotesBrowserState {
    /// Window that owns the browser and receives follow-up actions.
    pub(super) window: LushtextWindow,
    /// Dialog containing the browser widgets.
    pub(super) dialog: libadwaita::Dialog,
    /// Adaptive split view used for wide and narrow layouts.
    pub(super) split_view: libadwaita::NavigationSplitView,
    /// Navigation page whose title follows the active inventory mode.
    pub(super) sidebar_page: libadwaita::NavigationPage,
    /// Search field driving the current filtered row set.
    pub(super) search_entry: gtk4::SearchEntry,
    /// Virtualized, sectioned browse rail for bookmarks, folder notes, and
    /// document notes (`bound-notes-browser-by-budget` D1).
    pub(super) list_view: gtk4::ListView,
    /// Single selection over the rows; selection drives the preview.
    pub(super) selection: gtk4::SingleSelection,
    /// The lazy list and section model the list shows.
    pub(super) rows: NotesBrowserRowModel,
    /// Switches between the list and its empty state.
    pub(super) results_stack: gtk4::Stack,
    /// Empty-state status label ("No notes match that search", "No notes yet").
    pub(super) empty_label: gtk4::Label,
    /// Visible notice when a byte budget kept rows out of the source.
    pub(super) limit_label: gtk4::Label,
    /// Header label for the selected note.
    pub(super) preview_title: gtk4::Label,
    /// Secondary metadata label for the selected note.
    pub(super) preview_meta: gtk4::Label,
    /// Stack switching between Markdown/status previews and raw bookmark excerpts.
    pub(super) preview_stack: gtk4::Stack,
    /// Shared markdown preview widget reused for notes and Markdown bookmark excerpts.
    pub(super) markdown_preview: LushtextMarkdownPreview,
    /// Backing buffer for raw bookmark excerpts.
    pub(super) raw_preview_buffer: gtk4::TextBuffer,
    /// Open action for the selected note.
    pub(super) open_button: gtk4::Button,
    /// Back button shown when the split view collapses.
    pub(super) back_button: gtk4::Button,
    /// Complete set of notes covered by this browser session.
    pub(super) all_entries:
        RefCell<Arc<crate::ui::plain_disposal::DisposalOwned<Box<[NotesBrowserEntry]>>>>,
    /// Generation of the published source; rows carry it, so a row of a
    /// replaced source can neither render nor open.
    pub(super) source_generation: Cell<u64>,
    /// Rows listed against rows found for the published source.
    pub(super) listed_count: Cell<NotesBrowserListedCount>,
    /// Source generation whose limit was last announced.
    pub(super) limit_announced_generation: Cell<Option<u64>>,
    /// Limit announcements raised by this dialog (evidence).
    pub(super) limit_announcements: Cell<u64>,
    /// True while a query publish selects programmatically, so the selection
    /// handler does not treat it as the user's navigation.
    pub(super) publishing: Cell<bool>,
    /// The sliced open-editor capture: the capture in progress, its turn
    /// source, and its evidence (turns, restarts, rows counted but not copied,
    /// the longest turn).
    pub(super) snapshot_capture: Rc<OpenEditorCaptureOwner>,
    /// The `content-changed` handler each bound row and header holds on its
    /// row object, keyed by the list item or header, so a retargeted row
    /// rebinds in place and an unbound one stops listening. Bounded by the
    /// realized rows (A1).
    pub(super) bound_rows: RefCell<BoundRowHandlers>,
    /// The staged first publish growing one step per frame
    /// (`lighten-notes-browser-rows` D5), while it grows.
    pub(super) row_growth: RefCell<Option<crate::ui::list_growth::RowGrowth>>,
    /// Debounce used to rebuild browser search rows after typing settles.
    pub(super) search_debounce: Debounce,
    /// One-active/one-latest ownership for background full-source matching.
    pub(super) query_runtime: RefCell<palette_service::NotesBrowserQueryCoordinator>,
    /// Generation owner for the initial bounded source construction.
    pub(super) source_refreshes: RefCell<palette_service::NoteSourceRefreshCoordinator>,
    /// Active compact source request waiting for disposal admission before sidecar I/O.
    pub(super) source_admission: RefCell<Option<palette_service::NoteSourceRefreshStart>>,
    /// One paced capacity wakeup for the browser source.
    pub(super) source_capacity_wakeup: crate::ui::plain_disposal::ProgressDisposalCapacityWakeup,
    /// Typed source omissions (content-free evidence).
    pub(super) source_truncation: RefCell<Vec<palette_service::NoteSourceTruncationReason>>,
    /// Whether bounded source construction has published this dialog's source.
    pub(super) source_ready: Cell<bool>,
    /// Whether dialog teardown has invalidated all source and query publication.
    pub(super) disposed: Cell<bool>,
    /// Inventory mode that owns the current source/query generations.
    pub(super) mode: Cell<palette_service::NotesBrowserMode>,
    /// One-active/one-latest ownership for closed-file bookmark excerpt loads.
    pub(super) preview_loads:
        RefCell<crate::services::bookmark_excerpt::BookmarkExcerptPreviewCoordinator>,
}

/// `content-changed` handlers of bound rows and headers, by the address of
/// the `GtkListItem` or `GtkListHeader` that binds them.
#[derive(Default)]
pub(super) struct BoundRowHandlers {
    handlers: std::collections::HashMap<usize, (NotesBrowserRow, glib::SignalHandlerId)>,
}

impl BoundRowHandlers {
    fn insert(&mut self, owner: usize, row: NotesBrowserRow, handler: glib::SignalHandlerId) {
        if let Some((old_row, old_handler)) = self.handlers.insert(owner, (row, handler)) {
            old_row.disconnect(old_handler);
        }
    }

    fn remove(&mut self, owner: usize) {
        if let Some((row, handler)) = self.handlers.remove(&owner) {
            row.disconnect(handler);
        }
    }

    pub(super) fn clear(&mut self) {
        for (_, (row, handler)) in self.handlers.drain() {
            row.disconnect(handler);
        }
    }
}

/// Content-free evidence of the sliced open-editor capture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpenEditorSnapshotEvidence {
    /// Capture turns run for this dialog.
    pub turns: u64,
    /// Editors captured again because their bookmarks changed mid-capture.
    pub restarts: u64,
    /// Bookmark rows counted past the byte cut without being copied.
    pub rows_counted_not_retained: u64,
    /// The longest capture turn, in microseconds.
    pub worst_turn_us: u64,
}

/// Scalar bounded-source and query-ownership evidence.
///
/// Not feature-gated: `evidence::NotesEvidence` carries it in **both** feature
/// configurations, so gating the type would compile only under `test-utils` —
/// the exact default-feature break slot 4 recorded. Only the *re-export* for the
/// external widget harness is gated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NotesBrowserRuntimeSnapshot {
    /// Rows retained by the immutable admitted source.
    pub source_entries: usize,
    /// Whether source construction reported any omission reason.
    pub source_truncated: bool,
    /// Whether bounded source construction has completed.
    pub source_ready: bool,
    /// Inventory mode owning the current source and query generations.
    pub mode: palette_service::NotesBrowserMode,
    /// One-active/one-latest query ownership counters.
    pub query: palette_service::PaletteSearchCoordinatorSnapshot,
    /// Initial bounded-source ownership counters.
    pub source: palette_service::NoteSourceRefreshCoordinatorSnapshot,
    /// Closed-file bookmark excerpt ownership counters.
    pub preview: crate::services::bookmark_excerpt::BookmarkExcerptPreviewCoordinatorSnapshot,
    /// Rows the list shows for the current query, including the rows a
    /// staged first publish is still listing (`lighten-notes-browser-rows`
    /// D5): the final length from the first publish on.
    pub listed_rows: u32,
    /// Rows a staged first publish has yet to list; 0 once the list holds
    /// every row (tests and automation wait for it).
    pub growing_rows: u32,
    /// Rows the source retained (X of "Showing X of Y").
    pub source_rows: usize,
    /// Rows construction found, retained or not (Y).
    pub rows_found: usize,
    /// Whether `rows_found` is only a floor.
    pub found_is_lower_bound: bool,
    /// Whether a byte budget kept rows out of the source.
    pub limited: bool,
    /// Polite limit announcements raised by this dialog.
    pub limit_announcements: u64,
    /// The sliced open-editor capture's evidence.
    pub snapshot: OpenEditorSnapshotEvidence,
    /// Whether a capture is still running.
    pub snapshot_capturing: bool,
    /// Row widgets the list has realized (bounded by GTK's cap, ledger A1).
    pub realized_row_widgets: usize,
    /// Source index of the selected row, if any.
    pub selected_source_index: Option<u32>,
}

/// Weak handle to the currently visible unified notes browser.
///
/// Window actions use this to drive the same search, selection, and Open button
/// behavior a user sees in the dialog without keeping a closed dialog alive.
#[derive(Clone)]
pub(in crate::ui::window) struct ActiveNotesBrowser {
    state: Weak<NotesBrowserState>,
}

impl ActiveNotesBrowser {
    /// Track one newly presented notes browser dialog.
    pub(super) fn new(state: &Rc<NotesBrowserState>) -> Self {
        Self {
            state: Rc::downgrade(state),
        }
    }

    /// Return whether this handle still points to the same browser state.
    pub(super) fn same_target(&self, other: &Self) -> bool {
        self.state.ptr_eq(&other.state)
    }

    /// Return whether the dialog state still exists.
    pub(super) fn is_alive(&self) -> bool {
        self.state.upgrade().is_some()
    }

    pub(super) fn state(&self) -> Option<Rc<NotesBrowserState>> {
        self.state.upgrade()
    }

    /// Filter the visible notes browser through its normal search entry.
    pub(super) fn set_query(&self, query: &str) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        state.search_entry.set_text(query);
        true
    }

    /// Select one visible row by zero-based sidebar index.
    pub(super) fn select_visible_row(&self, index: u32) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        if index >= state.rows.full_len() {
            return false;
        }
        if index >= state.rows.len() {
            state.finish_row_growth();
        }
        state.selection.set_selected(index);
        true
    }

    /// Activate the same Open workflow as the visible notes browser button.
    pub(super) fn open_selected(&self) -> bool {
        let Some(state) = self.state.upgrade() else {
            return false;
        };
        if state.selected_entry_index().is_none() {
            return false;
        }
        state.open_selected();
        true
    }

    pub(super) fn runtime_snapshot(&self) -> Option<NotesBrowserRuntimeSnapshot> {
        let state = self.state.upgrade()?;
        // Every derived scalar is computed and every `Ref` dropped **before** the
        // struct literal. Borrows taken inside a literal live for the whole
        // literal, so a future field that needs a `borrow_mut()` of the same cell
        // would panic — the evidence-surface reentrancy constraint, applied to the
        // nested snapshot this surface folds in.
        let source_entries = state.all_entries.borrow().len();
        let source_truncated = !state.source_truncation.borrow().is_empty();
        let query = state.query_runtime.borrow().snapshot();
        let source = state.source_refreshes.borrow().snapshot();
        let preview = state.preview_loads.borrow().snapshot();
        let snapshot_capturing = state.snapshot_capture.is_capturing();
        let count = state.listed_count.get();
        let selected_source_index = state.selected_source_index();
        Some(NotesBrowserRuntimeSnapshot {
            source_entries,
            source_truncated,
            source_ready: state.source_ready.get(),
            mode: state.mode.get(),
            query,
            source,
            preview,
            listed_rows: state.rows.full_len(),
            growing_rows: state.rows.growing_rows(),
            source_rows: count.listed,
            rows_found: count.found,
            found_is_lower_bound: count.found_is_lower_bound,
            limited: count.is_limited(),
            limit_announcements: state.limit_announcements.get(),
            snapshot: state.snapshot_capture.evidence(),
            snapshot_capturing,
            realized_row_widgets: realized_row_widgets(&state.list_view),
            selected_source_index,
        })
    }
}

/// Fixed notes browser width.
const NOTES_BROWSER_WIDTH_SP: i32 = 980;
/// Fixed notes browser height.
const NOTES_BROWSER_HEIGHT_SP: i32 = 700;

impl LushtextWindow {
    /// Browse notes across the current workspace scope.
    pub(in crate::ui::window) fn show_notes_dialog(&self) {
        self.show_notes_browser_mode(palette_service::NotesBrowserMode::AllNotes);
    }

    /// Present or retarget the one unified browser to a bounded inventory mode.
    pub(super) fn show_notes_browser_mode(&self, mode: palette_service::NotesBrowserMode) {
        if let Some(browser) = self.current_notes_browser()
            && let Some(state) = browser.state()
        {
            let mode_changed = state.mode.get() != mode;
            state.dialog.present(Some(self));
            focus_after_present(&state.search_entry);
            if mode_changed {
                state.begin_mode(mode);
                self.submit_notes_browser_source(&state, mode);
            }
            return;
        }

        let state = self.present_notes_browser(Vec::new(), mode);
        self.submit_notes_browser_source(&state, mode);
    }
    /// Present the unified notes browser for the current workspace scope.
    fn present_notes_browser(
        &self,
        entries: Vec<NotesBrowserEntry>,
        mode: palette_service::NotesBrowserMode,
    ) -> Rc<NotesBrowserState> {
        let dialog = libadwaita::Dialog::builder()
            .title(mode.title())
            .content_width(NOTES_BROWSER_WIDTH_SP)
            .content_height(NOTES_BROWSER_HEIGHT_SP)
            .follows_content_size(false)
            .build();

        let search_entry = gtk4::SearchEntry::new();
        crate::ui::entry_paste_cap::cap_single_line_inserts(&search_entry);
        install_dialog_escape_close(&dialog, &search_entry);
        search_entry.set_placeholder_text(Some("Search Notes..."));
        accessibility::set_labelled_description(
            &search_entry,
            "Search notes",
            "Filter bookmarks, document notes, and folder notes",
        );

        let rows = NotesBrowserRowModel::default();
        let selection = gtk4::SingleSelection::new(Some(rows.clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(false);
        let list_view = gtk4::ListView::new(Some(selection.clone()), None::<gtk4::ListItemFactory>);
        list_view.add_css_class("navigation-sidebar");
        list_view.set_tab_behavior(gtk4::ListTabBehavior::Item);
        list_view.set_vexpand(true);
        accessibility::set_role(&list_view, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &list_view,
            "Notes results",
            "Choose a bookmark, document note, or folder note",
        );
        let empty_label = empty_browser_label("No notes match that search");
        empty_label.set_valign(gtk4::Align::Center);
        empty_label.set_wrap(true);
        let results_stack = gtk4::Stack::new();
        results_stack.set_vexpand(true);
        results_stack.add_named(
            &gtk4::ScrolledWindow::builder()
                .hscrollbar_policy(gtk4::PolicyType::Never)
                .hexpand(true)
                .vexpand(true)
                .child(&list_view)
                .build(),
            Some(NOTES_RESULTS_LIST_CHILD),
        );
        results_stack.add_named(&empty_label, Some(NOTES_RESULTS_EMPTY_CHILD));
        results_stack.set_visible_child_name(NOTES_RESULTS_EMPTY_CHILD);
        let limit_label = gtk4::Label::new(None);
        limit_label.set_halign(gtk4::Align::Start);
        limit_label.set_xalign(0.0);
        limit_label.set_wrap(true);
        limit_label.add_css_class("caption");
        limit_label.add_css_class("dim-label");
        limit_label.set_visible(false);
        accessibility::set_role(&limit_label, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &limit_label,
            "Notes result limit",
            "Shown when the memory budget keeps some notes out of the browser",
        );

        let preview_title = gtk4::Label::new(Some("Select a note"));
        preview_title.set_halign(gtk4::Align::Start);
        preview_title.set_xalign(0.0);
        preview_title.add_css_class("title-4");
        // A bookmark label can be kilobytes long; a single unellipsized line
        // made the dialog request tens of thousands of pixels
        // (`AdwFloatingSheet … exceeds AdwBreakpointBin width`).
        preview_title.set_ellipsize(gtk4::pango::EllipsizeMode::End);

        let preview_meta = gtk4::Label::new(Some(
            "Choose a bookmark, folder note, or document note to preview it here.",
        ));
        preview_meta.set_halign(gtk4::Align::Start);
        preview_meta.set_xalign(0.0);
        preview_meta.set_wrap(true);
        preview_meta.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        preview_meta.add_css_class("dim-label");

        let markdown_preview = LushtextMarkdownPreview::new();
        markdown_preview.set_hexpand(true);
        markdown_preview.set_vexpand(true);
        markdown_preview.show_placeholder("Select a note to preview its details.");

        let raw_preview_buffer = gtk4::TextBuffer::new(None);
        ensure_raw_preview_target_tag(&raw_preview_buffer);
        let raw_preview_view = gtk4::TextView::with_buffer(&raw_preview_buffer);
        raw_preview_view.set_editable(false);
        raw_preview_view.set_cursor_visible(false);
        crate::ui::font_zoom::follow_editor_font_monospace(&raw_preview_view);
        raw_preview_view.set_wrap_mode(gtk4::WrapMode::None);
        raw_preview_view.set_left_margin(NOTES_RAW_PREVIEW_TEXT_MARGIN_HORIZONTAL_SP);
        raw_preview_view.set_right_margin(NOTES_RAW_PREVIEW_TEXT_MARGIN_HORIZONTAL_SP);
        raw_preview_view.set_top_margin(NOTES_RAW_PREVIEW_TEXT_MARGIN_VERTICAL_SP);
        raw_preview_view.set_bottom_margin(NOTES_RAW_PREVIEW_TEXT_MARGIN_VERTICAL_SP);
        accessibility::set_labelled_description(
            &raw_preview_view,
            "Bookmark source preview",
            "Read-only source excerpt around the selected bookmark",
        );
        accessibility::set_read_only(&raw_preview_view, true);
        accessibility::set_multi_line(&raw_preview_view, true);

        let raw_preview_scroll = gtk4::ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .propagate_natural_width(false)
            .propagate_natural_height(false)
            .child(&raw_preview_view)
            .build();

        let preview_stack = gtk4::Stack::new();
        preview_stack.set_hexpand(true);
        preview_stack.set_vexpand(true);
        preview_stack.set_hhomogeneous(true);
        preview_stack.set_vhomogeneous(true);
        preview_stack.add_named(&markdown_preview, Some(NOTES_PREVIEW_MARKDOWN_CHILD));
        preview_stack.add_named(&raw_preview_scroll, Some(NOTES_PREVIEW_RAW_CHILD));
        preview_stack.set_visible_child_name(NOTES_PREVIEW_MARKDOWN_CHILD);
        accessibility::set_role(&preview_stack, gtk4::AccessibleRole::Group);
        accessibility::set_labelled_description(
            &preview_stack,
            "Notes preview",
            "Read-only preview for the selected bookmark, document note, or folder note",
        );
        accessibility::set_value_text(&preview_stack, "No note selected");

        let open_button = gtk4::Button::with_label("Open");
        open_button.add_css_class("suggested-action");
        open_button.set_sensitive(false);
        accessibility::set_labelled_description(
            &open_button,
            "Open selected note",
            "Open the selected bookmark, document note, or folder note",
        );
        accessibility::set_disabled(&open_button, true);
        accessibility::set_value_text(&open_button, "No note selected");

        let back_button = gtk4::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back to Notes")
            .visible(false)
            .build();
        accessibility::set_labelled_description(
            &back_button,
            "Back to notes",
            "Return to the notes result list in compact layouts",
        );

        let split_view = libadwaita::NavigationSplitView::new();
        split_view.set_hexpand(true);
        split_view.set_vexpand(true);
        split_view.set_min_sidebar_width(260.0);
        split_view.set_max_sidebar_width(340.0);
        let sidebar_page = libadwaita::NavigationPage::new(
            &build_notes_sidebar(&dialog, &search_entry, &results_stack, &limit_label),
            mode.title(),
        );
        split_view.set_sidebar(Some(&sidebar_page));
        split_view.set_content(Some(&libadwaita::NavigationPage::new(
            &build_notes_preview_page(
                &dialog,
                &back_button,
                &preview_title,
                &preview_meta,
                &preview_stack,
                &open_button,
            ),
            "Preview",
        )));
        split_view.set_show_content(false);
        dialog.set_child(Some(&build_notes_browser_shell(&dialog, &split_view)));

        let state = Rc::new(NotesBrowserState {
            window: self.clone(),
            dialog,
            split_view,
            sidebar_page,
            search_entry,
            list_view,
            selection,
            rows,
            results_stack,
            empty_label,
            limit_label,
            preview_title,
            preview_meta,
            preview_stack,
            markdown_preview,
            raw_preview_buffer,
            open_button,
            back_button,
            source_generation: Cell::new(0),
            listed_count: Cell::new(NotesBrowserListedCount::default()),
            limit_announced_generation: Cell::new(None),
            limit_announcements: Cell::new(0),
            publishing: Cell::new(false),
            snapshot_capture: Rc::default(),
            bound_rows: RefCell::default(),
            row_growth: RefCell::default(),
            search_debounce: Debounce::default(),
            preview_loads: RefCell::default(),
            all_entries: RefCell::new(Arc::new(
                crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                    entries.into_boxed_slice(),
                ),
            )),
            query_runtime: RefCell::default(),
            source_refreshes: RefCell::default(),
            source_admission: RefCell::default(),
            source_capacity_wakeup:
                crate::ui::plain_disposal::ProgressDisposalCapacityWakeup::default(),
            source_truncation: RefCell::new(Vec::new()),
            source_ready: Cell::new(false),
            disposed: Cell::new(false),
            mode: Cell::new(mode),
        });

        state.configure_mode(mode);

        state.search_entry.connect_search_changed({
            let state = Rc::downgrade(&state);
            move |entry| {
                if let Some(state) = state.upgrade() {
                    schedule_notes_browser_search(&state, entry.text().to_string());
                }
            }
        });
        state
            .list_view
            .set_factory(Some(&build_notes_row_factory(&state)));
        state
            .list_view
            .set_header_factory(Some(&build_notes_header_factory(&state)));
        state.selection.connect_selected_notify({
            let state = Rc::downgrade(&state);
            move |selection| {
                // A publish selects programmatically, and teardown clears the
                // model: neither is the user's navigation.
                if let Some(state) = state.upgrade()
                    && !state.publishing.get()
                    && !state.disposed.get()
                {
                    NotesBrowserState::refresh_preview(
                        &state,
                        position_of_selection(selection.selected()),
                        true,
                    );
                }
            }
        });
        // Enter (or a double click) selects and previews; it never opens. The
        // Open button and a bookmark row's Open button open.
        state.list_view.connect_activate({
            let state = Rc::downgrade(&state);
            move |_, position| {
                if let Some(state) = state.upgrade() {
                    if state.selection.selected() == position {
                        NotesBrowserState::refresh_preview(&state, Some(position), true);
                    } else {
                        state.selection.set_selected(position);
                    }
                }
            }
        });
        // A key that moves the selection lands where it would on the full
        // list: a staged first publish still growing lists the rest first
        // (`lighten-notes-browser-rows` D5).
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let state = Rc::downgrade(&state);
            move |_, _, _, _| {
                if let Some(state) = state.upgrade() {
                    state.finish_row_growth();
                }
                glib::Propagation::Proceed
            }
        });
        state.list_view.add_controller(keys);
        state.open_button.connect_clicked({
            let state = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = state.upgrade() {
                    state.open_selected();
                }
            }
        });
        state.back_button.connect_clicked({
            let state = Rc::downgrade(&state);
            move |_| {
                if let Some(state) = state.upgrade() {
                    state.split_view.set_show_content(false);
                }
            }
        });
        // Collapsed adaptive navigation owns back-button visibility. The
        // binding seeds the initial dialog layout and stays live as breakpoints
        // change without storing a signal handler ID.
        state
            .split_view
            .bind_property("collapsed", &state.back_button, "visible")
            .sync_create()
            .build();

        let active_browser = ActiveNotesBrowser::new(&state);
        *self.imp().active_notes_browser.borrow_mut() = Some(active_browser.clone());
        self.set_notes_browser_actions_enabled(true);

        // The dialog owns this holder while it is visible, keeping browser
        // state alive without child-widget signal closures strongly owning the
        // whole dialog subtree. The `closed` signal drops the state and breaks
        // the temporary dialog -> holder -> state -> dialog cycle.
        let state_holder = Rc::new(RefCell::new(Some(Rc::clone(&state))));
        state.dialog.connect_closed({
            let state_holder = Rc::clone(&state_holder);
            let window_weak = self.downgrade();
            move |_| {
                let state = state_holder.borrow_mut().take();
                if let Some(state) = state {
                    state.dispose_runtime();
                }
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let should_disable = {
                    let mut slot = window.imp().active_notes_browser.borrow_mut();
                    if slot
                        .as_ref()
                        .is_some_and(|current| current.same_target(&active_browser))
                    {
                        slot.take();
                        true
                    } else {
                        false
                    }
                };
                if should_disable {
                    window.set_notes_browser_actions_enabled(false);
                }
            }
        });

        state.dialog.present(Some(self));
        focus_after_present(&state.search_entry);
        state
    }

    /// Enable or disable actions that require a visible unified notes browser.
    pub(in crate::ui::window) fn set_notes_browser_actions_enabled(&self, enabled: bool) {
        for name in [
            "set-notes-browser-query",
            "select-notes-browser-row",
            "open-notes-browser-selection",
        ] {
            if let Some(action) = self.lookup_action(name)
                && let Some(simple) = action.downcast_ref::<gio::SimpleAction>()
            {
                simple.set_enabled(enabled);
            }
        }
    }

    /// Set the visible notes-browser filter text through the dialog search entry.
    pub(in crate::ui::window) fn set_notes_browser_query(&self, query: &str) {
        let Some(browser) = self.current_notes_browser() else {
            self.publish_status_message(
                "Open Browse Notes before filtering notes",
                MessageKind::Warning,
            );
            return;
        };
        if !browser.set_query(query) {
            self.set_notes_browser_actions_enabled(false);
        }
    }

    /// Select one visible notes-browser row without relying on pointer coordinates.
    pub(in crate::ui::window) fn select_notes_browser_row(&self, index: u32) {
        let Some(browser) = self.current_notes_browser() else {
            self.publish_status_message(
                "Open Browse Notes before selecting a notes row",
                MessageKind::Warning,
            );
            return;
        };
        if !browser.select_visible_row(index) {
            self.publish_status_message("That notes row is not visible", MessageKind::Warning);
        }
    }

    /// Open the currently selected notes-browser row through the visible workflow.
    pub(in crate::ui::window) fn open_notes_browser_selection(&self) {
        let Some(browser) = self.current_notes_browser() else {
            self.publish_status_message(
                "Open Browse Notes before opening a note",
                MessageKind::Warning,
            );
            return;
        };
        if !browser.open_selected() {
            self.publish_status_message(
                "Select a notes row before opening it",
                MessageKind::Warning,
            );
        }
    }

    /// Activate one note-search target through the existing note workflows.
    pub(in crate::ui::window) fn activate_palette_note_target(&self, target: &PaletteNoteTarget) {
        match target {
            PaletteNoteTarget::Bookmark { path, line, .. } => {
                open_editor_at_line(self, path, line.saturating_add(1));
            }
            PaletteNoteTarget::FolderNote {
                workspace_name,
                folder,
            } => self.open_folder_note_for_folder(workspace_name, folder),
            PaletteNoteTarget::DocumentNote {
                path,
                workspace_folders,
            } => {
                self.open_document(path);
                self.open_document_note_for_path_with_folders(path, workspace_folders.clone());
            }
        }
    }

    /// Return the current browser handle, clearing stale state left by a closed dialog.
    fn current_notes_browser(&self) -> Option<ActiveNotesBrowser> {
        let browser = self.imp().active_notes_browser.borrow().clone();
        if browser.as_ref().is_some_and(ActiveNotesBrowser::is_alive) {
            return browser;
        }
        self.imp().active_notes_browser.borrow_mut().take();
        self.set_notes_browser_actions_enabled(false);
        None
    }
}
/// Build the populated notes-browser chrome around the adaptive split view.
fn build_notes_browser_shell(
    dialog: &libadwaita::Dialog,
    split_view: &libadwaita::NavigationSplitView,
) -> gtk4::Box {
    let shell = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    install_dialog_escape_close(dialog, &shell);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header.set_margin_start(18);
    header.set_margin_end(18);
    header.set_margin_top(18);
    let title = gtk4::Label::new(None);
    title.set_halign(gtk4::Align::Start);
    title.set_hexpand(true);
    title.set_xalign(0.0);
    title.add_css_class("title-4");
    dialog
        .bind_property("title", &title, "label")
        .sync_create()
        .build();
    header.append(&title);

    let close_button = build_dialog_close_button(dialog);
    install_dialog_escape_close(dialog, &close_button);
    header.append(&close_button);
    shell.append(&header);
    shell.append(split_view);

    shell
}
/// Build the browse rail used by the unified notes browser.
fn build_notes_sidebar(
    dialog: &libadwaita::Dialog,
    search_entry: &gtk4::SearchEntry,
    results_stack: &gtk4::Stack,
    limit_label: &gtk4::Label,
) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    install_dialog_escape_close(dialog, &content);

    content.append(search_entry);
    // The list's own scroller is the only one, so the list gets a real
    // viewport and stays virtualized (ledger A1).
    content.append(results_stack);
    content.append(limit_label);

    content
}

/// Build the preview page used by the unified notes browser.
fn build_notes_preview_page(
    dialog: &libadwaita::Dialog,
    back_button: &gtk4::Button,
    preview_title: &gtk4::Label,
    preview_meta: &gtk4::Label,
    preview_stack: &gtk4::Stack,
    open_button: &gtk4::Button,
) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    install_dialog_escape_close(dialog, &content);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header.append(back_button);

    let title_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    title_box.set_hexpand(true);
    title_box.append(preview_title);
    title_box.append(preview_meta);
    header.append(&title_box);
    content.append(&header);

    content.append(preview_stack);

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    actions.set_halign(gtk4::Align::End);
    actions.append(open_button);
    content.append(&actions);

    content
}

pub(super) trait NotesBrowserEntryExt {
    /// User-facing row title used in the browser list.
    fn row_title(&self) -> String;
    /// Secondary row text used for scope and location metadata.
    fn row_subtitle(&self) -> String;
    /// Optional row detail showing the first meaningful line of note text.
    fn row_detail(&self) -> Option<String>;
    /// Title shown in the preview header for the selected note.
    fn preview_title(&self) -> String;
    /// Secondary preview metadata shown under the selected note title.
    fn preview_meta(&self) -> String;
    /// Render context used by the shared markdown preview widget.
    fn render_context(&self) -> MarkdownPreviewRenderContext;
    /// Symbolic icon used by the browser row.
    fn sidebar_icon_name(&self) -> &'static str;
}

impl NotesBrowserEntryExt for NotesBrowserEntry {
    fn row_title(&self) -> String {
        self.title.clone()
    }

    fn row_subtitle(&self) -> String {
        self.subtitle.clone()
    }

    fn row_detail(&self) -> Option<String> {
        self.detail.clone()
    }

    fn preview_title(&self) -> String {
        self.row_title()
    }

    fn preview_meta(&self) -> String {
        self.row_subtitle()
    }

    fn render_context(&self) -> MarkdownPreviewRenderContext {
        match &self.target {
            PaletteNoteTarget::FolderNote { folder, .. } => {
                MarkdownPreviewRenderContext::new(None, vec![folder.clone()])
            }
            PaletteNoteTarget::Bookmark {
                path,
                workspace_folders,
                ..
            }
            | PaletteNoteTarget::DocumentNote {
                path,
                workspace_folders,
            } => MarkdownPreviewRenderContext::new(Some(path.clone()), workspace_folders.clone()),
        }
    }

    fn sidebar_icon_name(&self) -> &'static str {
        match self.target {
            PaletteNoteTarget::Bookmark { .. } => "bookmark-new-symbolic",
            PaletteNoteTarget::FolderNote { .. } => "folder-symbolic",
            PaletteNoteTarget::DocumentNote { .. } => "text-x-generic-symbolic",
        }
    }
}

impl NotesBrowserState {
    /// Follow a publish: grow a staged first publish one step per frame, or
    /// stop a growth a later publish ended.
    pub(super) fn track_row_growth(&self) {
        // A list with no frame clock yet has no frame to spare: list it all.
        if !self.list_view.is_realized() {
            self.rows.finish_growth();
        }
        let growth = (self.rows.growing_rows() > 0).then(|| {
            crate::ui::list_growth::RowGrowth::start(Box::new(self.rows.clone()), &self.list_view)
        });
        let previous = self.row_growth.replace(growth);
        drop(previous);
    }

    /// List every row a staged first publish has not listed yet.
    pub(super) fn finish_row_growth(&self) {
        if self.rows.growing_rows() == 0 {
            return;
        }
        let growth = self.row_growth.take();
        drop(growth);
        self.rows.finish_growth();
    }

    pub(super) fn configure_mode(&self, mode: palette_service::NotesBrowserMode) {
        self.mode.set(mode);
        self.dialog.set_title(mode.title());
        self.sidebar_page.set_title(mode.title());
        self.search_entry
            .set_placeholder_text(Some(mode.search_placeholder()));
        accessibility::set_labelled_description(
            &self.search_entry,
            mode.search_accessible_label(),
            mode.search_description(),
        );
        self.show_empty_results(mode.empty_source_label());
        accessibility::set_labelled_description(
            &self.list_view,
            mode.results_accessible_label(),
            mode.results_description(),
        );
        accessibility::set_labelled_description(
            &self.preview_stack,
            &format!("{} preview", mode.title()),
            mode.results_description(),
        );
        accessibility::set_labelled_description(
            &self.open_button,
            mode.open_action_label(),
            mode.results_description(),
        );
        self.back_button
            .set_tooltip_text(Some(&format!("Back to {}", mode.title())));
        self.show_unselected_preview();
    }
    /// Refresh the preview pane for the row at one list position.
    pub(super) fn refresh_preview(state: &Rc<Self>, index: Option<u32>, user_selected: bool) {
        // Any newly rendered selection supersedes older closed-file excerpt
        // work; a closed-file bookmark branch resubmits below.
        state.preview_loads.borrow_mut().invalidate();

        let Some(index) = index else {
            state.show_unselected_preview();
            return;
        };

        let Some(entry_index) = state.rows.source_index_at(index) else {
            state.show_unselected_preview();
            return;
        };
        let all_entries = state.all_entries.borrow();
        let Some(entry) = usize::try_from(entry_index)
            .ok()
            .and_then(|index| all_entries.get(index))
        else {
            state.show_unselected_preview();
            return;
        };

        state.preview_title.set_label(&entry.preview_title());
        state.preview_meta.set_label(&entry.preview_meta());
        if matches!(&entry.target, PaletteNoteTarget::Bookmark { .. }) {
            Self::refresh_bookmark_preview(state, entry);
        } else if entry.note_text().trim().is_empty() {
            state.show_markdown_placeholder("This note is empty.");
        } else {
            state.show_markdown_preview();
            state
                .markdown_preview
                .render_markdown_with_context(entry.note_text(), &entry.render_context());
        }
        state.open_button.set_sensitive(true);
        accessibility::set_disabled(&state.open_button, false);
        accessibility::set_value_text(
            &state.open_button,
            &format!("Open {}", entry.preview_title()),
        );
        accessibility::set_value_text(&state.preview_stack, &entry.preview_title());

        if user_selected {
            // `show-content` is only visible while collapsed, but setting it
            // before the adaptive layout settles preserves the user's
            // navigation request during resize and widget-test transitions.
            state.split_view.set_show_content(true);
        }
    }

    /// Reset the preview pane to the initial no-selection state.
    fn show_unselected_preview(&self) {
        let mode = self.mode.get();
        self.preview_title.set_label(mode.unselected_title());
        self.preview_meta.set_label(mode.unselected_meta());
        self.show_markdown_placeholder(mode.unselected_placeholder());
        self.open_button.set_sensitive(false);
        accessibility::set_disabled(&self.open_button, true);
        accessibility::set_value_text(&self.open_button, mode.unselected_value_text());
        accessibility::set_value_text(&self.preview_stack, mode.unselected_value_text());
    }

    /// Switch to the Markdown/status preview child and clear hidden raw state.
    pub(super) fn show_markdown_preview(&self) {
        self.raw_preview_buffer.set_text("");
        self.preview_stack
            .set_visible_child_name(NOTES_PREVIEW_MARKDOWN_CHILD);
    }

    /// Show a status-style placeholder in the Markdown child.
    fn show_markdown_placeholder(&self, description: &str) {
        self.show_markdown_preview();
        self.markdown_preview.show_placeholder(description);
    }

    /// Show plain text inside the Markdown child to preserve preview allocation.
    pub(super) fn show_markdown_content_placeholder(&self, description: &str) {
        self.show_markdown_preview();
        self.markdown_preview.show_content_placeholder(description);
    }
    /// Return the source index of the selected row.
    pub(super) fn selected_source_index(&self) -> Option<u32> {
        self.rows
            .source_index_at(position_of_selection(self.selection.selected())?)
    }

    /// Return the backing entry index for the selected row.
    pub(super) fn selected_entry_index(&self) -> Option<usize> {
        usize::try_from(self.selected_source_index()?).ok()
    }

    /// Show the list, or the empty state with `message`.
    pub(super) fn show_results(&self, empty_message: &str) {
        if self.rows.is_empty() {
            self.show_empty_results(empty_message);
        } else {
            self.results_stack
                .set_visible_child_name(NOTES_RESULTS_LIST_CHILD);
        }
    }

    /// Show the empty-state status label with `message`.
    pub(super) fn show_empty_results(&self, message: &str) {
        self.empty_label.set_label(message);
        accessibility::set_label(&self.empty_label, message);
        self.results_stack
            .set_visible_child_name(NOTES_RESULTS_EMPTY_CHILD);
    }
    /// Open the currently selected note through the same window workflows used elsewhere.
    pub(super) fn open_selected(&self) {
        let Some(entry_index) = self.selected_entry_index() else {
            return;
        };
        self.open_entry(entry_index);
    }

    /// Open one source entry without retaining its path graph in row callbacks.
    fn open_entry(&self, entry_index: usize) {
        let Some(target) = self
            .all_entries
            .borrow()
            .get(entry_index)
            .map(|entry| entry.target.clone())
        else {
            return;
        };

        self.dialog.close();
        self.window.activate_palette_note_target(&target);
    }
}

impl NotesBrowserState {
    /// Open the entry a listed row names, if the row still belongs to the
    /// published source (a row of a replaced source opens nothing).
    fn open_listed_row(&self, row: &NotesBrowserRow) {
        if row.generation() != self.source_generation.get() || self.disposed.get() {
            return;
        }
        if let Ok(index) = usize::try_from(row.source_index()) {
            self.open_entry(index);
        }
    }
}

/// `GtkSingleSelection` reports "nothing selected" as `GTK_INVALID_LIST_POSITION`.
fn position_of_selection(selected: u32) -> Option<u32> {
    (selected != gtk4::INVALID_LIST_POSITION).then_some(selected)
}

/// Row widgets the list has realized: its direct `row` children. Bounded by
/// GTK's realized-row cap (ledger A1), and reading it creates nothing.
fn realized_row_widgets(list_view: &gtk4::ListView) -> usize {
    let mut count = 0usize;
    let mut child = list_view.first_child();
    while let Some(widget) = child {
        if widget.css_name() == "row" {
            count = count.saturating_add(1);
        }
        child = widget.next_sibling();
    }
    count
}

/// The widgets of one browser row, built once per recycled row.
struct NotesRowWidgets {
    icon: gtk4::Image,
    title: LushtextRowText,
    subtitle: LushtextRowText,
    open_button: gtk4::Button,
}

/// Find a row's widgets from its child box (icon, title box, Open button).
fn notes_row_widgets(child: &gtk4::Widget) -> Option<NotesRowWidgets> {
    let icon = child.first_child()?.downcast::<gtk4::Image>().ok()?;
    let title_box = icon.next_sibling()?;
    let open_button = title_box.next_sibling()?.downcast::<gtk4::Button>().ok()?;
    let title = title_box
        .first_child()?
        .downcast::<LushtextRowText>()
        .ok()?;
    let subtitle = title.next_sibling()?.downcast::<LushtextRowText>().ok()?;
    Some(NotesRowWidgets {
        icon,
        title,
        subtitle,
        open_button,
    })
}

/// Build the row factory: the layout of `AdwSidebar`'s rows (an icon with the
/// presentation role, an ellipsized title, a dimmed subtitle and, for a
/// bookmark, an Open suffix button), created only for the realized rows.
fn build_notes_row_factory(state: &Rc<NotesBrowserState>) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup({
        let state = Rc::downgrade(state);
        move |_, object| {
            let Some(list_item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            // `AdwSidebar`'s row: the icon, then the two text lines, then a
            // suffix, spaced like its `.navigation-sidebar` row content.
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, NOTES_ROW_SPACING_SP);
            row.set_margin_top(NOTES_ROW_MARGIN_VERTICAL_SP);
            row.set_margin_bottom(NOTES_ROW_MARGIN_VERTICAL_SP);
            row.set_margin_start(NOTES_ROW_MARGIN_START_SP);
            row.add_css_class("notes-browser-row");
            let icon = gtk4::Image::new();
            icon.add_css_class("icon");
            accessibility::set_role(&icon, gtk4::AccessibleRole::Presentation);
            row.append(&icon);
            let title_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            title_box.set_hexpand(true);
            title_box.set_valign(gtk4::Align::Center);
            // Row text measured without shaping (`lighten-notes-browser-rows`
            // D2): a query that rebinds the realized rows redraws them
            // instead of laying every one out again (ledger A108).
            let title = LushtextRowText::new();
            title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            title.set_xalign(0.0);
            title.add_css_class("title");
            let subtitle = LushtextRowText::new();
            subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            subtitle.set_xalign(0.0);
            subtitle.add_css_class("subtitle");
            subtitle.add_css_class("caption");
            subtitle.add_css_class("dim-label");
            title_box.append(&title);
            title_box.append(&subtitle);
            row.append(&title_box);
            let open_button = gtk4::Button::builder()
                .icon_name("document-open-symbolic")
                .valign(gtk4::Align::Center)
                .visible(false)
                .build();
            open_button.add_css_class("flat");
            open_button.connect_clicked({
                let state = state.clone();
                let list_item = list_item.downgrade();
                move |_| {
                    let (Some(state), Some(list_item)) = (state.upgrade(), list_item.upgrade())
                    else {
                        return;
                    };
                    if let Some(row) = list_item
                        .item()
                        .and_then(|item| item.downcast::<NotesBrowserRow>().ok())
                    {
                        state.open_listed_row(&row);
                    }
                }
            });
            row.append(&open_button);
            list_item.set_child(Some(&row));
        }
    });
    factory.connect_bind({
        let state = Rc::downgrade(state);
        move |_, object| {
            let Some(list_item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let (Some(state), Some(widgets)) = (
                state.upgrade(),
                list_item.child().as_ref().and_then(notes_row_widgets),
            ) else {
                return;
            };
            let Some(row) = list_item
                .item()
                .and_then(|item| item.downcast::<NotesBrowserRow>().ok())
            else {
                return;
            };
            bind_notes_row(&state, list_item, &row, &widgets);
            // A republish retargets this row object in place (`row_model`):
            // project it again without a new widget.
            let handler = row.connect_content_changed({
                let state = Rc::downgrade(&state);
                let list_item = list_item.downgrade();
                move |row| {
                    let (Some(state), Some(list_item)) = (state.upgrade(), list_item.upgrade())
                    else {
                        return;
                    };
                    if let Some(widgets) = list_item.child().as_ref().and_then(notes_row_widgets) {
                        bind_notes_row(&state, &list_item, row, &widgets);
                    }
                }
            });
            state
                .bound_rows
                .borrow_mut()
                .insert(list_item.as_ptr() as usize, row, handler);
        }
    });
    factory.connect_unbind({
        let state = Rc::downgrade(state);
        move |_, object| {
            let Some(list_item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            if let Some(state) = state.upgrade() {
                state
                    .bound_rows
                    .borrow_mut()
                    .remove(list_item.as_ptr() as usize);
            }
            accessibility::clear_row_accessibility_for_list_item(list_item);
            if let Some(widgets) = list_item.child().as_ref().and_then(notes_row_widgets) {
                widgets.title.set_text("");
                widgets.subtitle.set_text("");
                widgets.open_button.set_visible(false);
                widgets.open_button.set_tooltip_text(None);
                accessibility::clear_row_accessibility(&widgets.open_button);
            }
        }
    });
    factory
}

/// Project one source entry onto a recycled row.
fn bind_notes_row(
    state: &NotesBrowserState,
    list_item: &gtk4::ListItem,
    row: &NotesBrowserRow,
    widgets: &NotesRowWidgets,
) {
    if row.generation() != state.source_generation.get() {
        return;
    }
    let all_entries = state.all_entries.borrow();
    let Some(entry) = usize::try_from(row.source_index())
        .ok()
        .and_then(|index| all_entries.get(index))
    else {
        return;
    };
    let bookmark_label = matches!(&entry.target, PaletteNoteTarget::Bookmark { .. }).then(|| {
        entry
            .title
            .strip_prefix("Bookmark · ")
            .unwrap_or(&entry.title)
    });
    let title = if state.mode.get() == palette_service::NotesBrowserMode::Bookmarks {
        bookmark_label.map_or_else(|| entry.row_title(), str::to_owned)
    } else {
        entry.row_title()
    };
    let subtitle = entry.row_detail().map_or_else(
        || entry.row_subtitle(),
        |detail| format!("{} · {detail}", entry.row_subtitle()),
    );
    widgets.icon.set_icon_name(Some(entry.sidebar_icon_name()));
    widgets.title.set_text(&title);
    widgets.subtitle.set_text(&subtitle);
    widgets.subtitle.set_visible(!subtitle.is_empty());
    if let Some(child) = list_item.child() {
        child.set_tooltip_text(Some(&subtitle));
    }
    // The section name is spoken with every row: a header is not a focus stop
    // (ledger A94), so the description is how Orca hears the section.
    let section = row.category().map_or("", |category| category.label());
    let description = format!("{section} · {subtitle}");
    accessibility::apply_list_item_row_accessibility(
        list_item,
        accessibility::RowAccessibility::new(&title).description(&description),
    );
    if let Some(bookmark_label) = bookmark_label {
        let action_label = format!("Open bookmark {bookmark_label}");
        widgets.open_button.set_tooltip_text(Some(&action_label));
        accessibility::set_labelled_description(&widgets.open_button, &action_label, &subtitle);
        widgets.open_button.set_visible(true);
    } else {
        widgets.open_button.set_visible(false);
    }
}

/// Build the section header factory: the section title, styled as
/// `AdwSidebar`'s section header (`.heading`).
fn build_notes_header_factory(state: &Rc<NotesBrowserState>) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
            return;
        };
        let title = LushtextRowText::new();
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        title.set_xalign(0.0);
        title.add_css_class("heading");
        title.set_margin_start(NOTES_HEADER_MARGIN_START_SP);
        title.add_css_class("notes-browser-section-header");
        accessibility::set_role(&title, gtk4::AccessibleRole::Heading);
        header.set_child(Some(&title));
    });
    factory.connect_bind({
        let state = Rc::downgrade(state);
        move |_, object| {
            let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
                return;
            };
            let Some(row) = header
                .item()
                .and_then(|item| item.downcast::<NotesBrowserRow>().ok())
            else {
                return;
            };
            project_section_header(header, &row);
            if let Some(state) = state.upgrade() {
                let handler = row.connect_content_changed({
                    let header = header.downgrade();
                    move |row| {
                        if let Some(header) = header.upgrade() {
                            project_section_header(&header, row);
                        }
                    }
                });
                state
                    .bound_rows
                    .borrow_mut()
                    .insert(header.as_ptr() as usize, row, handler);
            }
        }
    });
    factory.connect_unbind({
        let state = Rc::downgrade(state);
        move |_, object| {
            if let (Some(state), Some(header)) =
                (state.upgrade(), object.downcast_ref::<gtk4::ListHeader>())
            {
                state
                    .bound_rows
                    .borrow_mut()
                    .remove(header.as_ptr() as usize);
            }
        }
    });
    factory
}

/// Show the section title of the header's first row.
fn project_section_header(header: &gtk4::ListHeader, row: &NotesBrowserRow) {
    let label = row.category().map_or("", |category| category.label());
    if let Some(title) = header
        .child()
        .and_then(|child| child.downcast::<LushtextRowText>().ok())
    {
        title.set_text(label);
    }
}
