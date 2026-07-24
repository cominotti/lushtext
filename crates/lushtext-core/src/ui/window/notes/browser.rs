// SPDX-License-Identifier: GPL-3.0-or-later

//! Unified notes-browser loading, search, projection, and activation workflows.

use super::bookmarks::{ensure_raw_preview_target_tag, open_editor_at_line};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_settle::Debounce;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::gio;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, NavigationPageExt, SidebarItemExt};

use crate::model::palette::{PaletteNoteCategory, PaletteNoteEntry, PaletteNoteTarget};
use crate::services::palette::{
    NoteSourceRefreshRequest, NoteSourceRefreshStart, PaletteNoteSourceOutcome,
};
use crate::services::{json_store, palette as palette_service};
use crate::ui::accessibility;
use crate::ui::markdown_preview::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};
use crate::ui::status_bar::MessageKind;

use super::{
    ActiveNotesBrowser, LushtextWindow, NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT,
    NOTES_BROWSER_RENDER_LIMIT, NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT,
    NOTES_PREVIEW_MARKDOWN_CHILD, NOTES_PREVIEW_RAW_CHILD,
    NOTES_RAW_PREVIEW_TEXT_MARGIN_HORIZONTAL_SP, NOTES_RAW_PREVIEW_TEXT_MARGIN_VERTICAL_SP,
    NotesBrowserEntry, NotesBrowserState, build_dialog_close_button, empty_browser_label,
    focus_after_present, install_dialog_escape_close, notes_browser_source_limits,
};

/// Coalesce command-palette note-source reloads after live note/bookmark bursts.
const COMMAND_PALETTE_NOTES_REFRESH_DEBOUNCE_MS: u64 = 150;
/// Fixed notes browser width.
const NOTES_BROWSER_WIDTH_SP: i32 = 980;
/// Fixed notes browser height.
const NOTES_BROWSER_HEIGHT_SP: i32 = 700;

trait NotesBrowserModeExt {
    fn title(self) -> &'static str;
    fn loading_label(self) -> &'static str;
    fn deferred_label(self) -> &'static str;
    fn search_placeholder(self) -> &'static str;
    fn search_accessible_label(self) -> &'static str;
    fn search_description(self) -> &'static str;
    fn results_accessible_label(self) -> &'static str;
    fn results_description(self) -> &'static str;
    fn empty_source_label(self) -> &'static str;
    fn no_matches_label(self) -> &'static str;
    fn unselected_title(self) -> &'static str;
    fn unselected_meta(self) -> &'static str;
    fn unselected_placeholder(self) -> &'static str;
    fn source_limit_message(self) -> &'static str;
}

impl NotesBrowserModeExt for palette_service::NotesBrowserMode {
    fn title(self) -> &'static str {
        match self {
            Self::AllNotes => "Notes",
            Self::Bookmarks => "Bookmarks",
        }
    }

    fn loading_label(self) -> &'static str {
        match self {
            Self::AllNotes => "Loading notes…",
            Self::Bookmarks => "Loading bookmarks…",
        }
    }

    fn deferred_label(self) -> &'static str {
        match self {
            Self::AllNotes => "Notes deferred by memory pressure",
            Self::Bookmarks => "Bookmarks deferred by memory pressure",
        }
    }

    fn search_placeholder(self) -> &'static str {
        match self {
            Self::AllNotes => "Search Notes…",
            Self::Bookmarks => "Search Bookmarks…",
        }
    }

    fn search_accessible_label(self) -> &'static str {
        match self {
            Self::AllNotes => "Search notes",
            Self::Bookmarks => "Search bookmarks",
        }
    }

    fn search_description(self) -> &'static str {
        match self {
            Self::AllNotes => "Filter bookmarks, document notes, and folder notes",
            Self::Bookmarks => "Filter bookmarks in the current workspace",
        }
    }

    fn results_accessible_label(self) -> &'static str {
        match self {
            Self::AllNotes => "Notes results",
            Self::Bookmarks => "Bookmark results",
        }
    }

    fn results_description(self) -> &'static str {
        match self {
            Self::AllNotes => "Choose a bookmark, document note, or folder note",
            Self::Bookmarks => "Choose a bookmark to preview or open",
        }
    }

    fn empty_source_label(self) -> &'static str {
        match self {
            Self::AllNotes => "No notes yet",
            Self::Bookmarks => "No bookmarks exist in the current workspace",
        }
    }

    fn no_matches_label(self) -> &'static str {
        match self {
            Self::AllNotes => "No notes match that search",
            Self::Bookmarks => "No bookmarks match that search",
        }
    }

    fn unselected_title(self) -> &'static str {
        match self {
            Self::AllNotes => "Select a note",
            Self::Bookmarks => "Select a bookmark",
        }
    }

    fn unselected_meta(self) -> &'static str {
        match self {
            Self::AllNotes => {
                "Choose a bookmark, folder note, or document note to preview it here."
            }
            Self::Bookmarks => "Choose a bookmark to preview its source context here.",
        }
    }

    fn unselected_placeholder(self) -> &'static str {
        match self {
            Self::AllNotes => "Select a note to preview its details.",
            Self::Bookmarks => "Select a bookmark to preview its source context.",
        }
    }

    fn source_limit_message(self) -> &'static str {
        match self {
            Self::AllNotes => {
                "Some later notes were omitted because the source reached its safety limits."
            }
            Self::Bookmarks => {
                "Some later bookmarks were omitted because the source reached its safety limits."
            }
        }
    }
}

enum GuardedNoteSourceOutcome {
    Complete {
        load: palette_service::PaletteNoteSourceLoad,
        entries: crate::ui::plain_disposal::DisposalOwned<Box<[PaletteNoteEntry]>>,
        had_recovery_diagnostics: bool,
    },
    Cancelled,
    Failed(anyhow::Error),
}

fn guard_note_source_on_worker(
    result: anyhow::Result<PaletteNoteSourceOutcome>,
    reservation: crate::ui::plain_disposal::DisposalReservation,
) -> GuardedNoteSourceOutcome {
    match result {
        Ok(PaletteNoteSourceOutcome::Complete { mut load, metrics }) => {
            let diagnostics = std::mem::take(&mut load.diagnostics);
            let had_recovery_diagnostics = !diagnostics.is_empty();
            LushtextWindow::trace_browse_recovery_diagnostics(&diagnostics);
            drop(diagnostics);
            let entries = std::mem::take(&mut load.entries).into_boxed_slice();
            debug_assert_eq!(
                metrics.retained_bytes,
                crate::model::palette::palette_note_entries_retained_byte_weight(&entries)
            );
            debug_assert!(
                metrics.retained_bytes <= palette_service::MAX_PALETTE_NOTE_RETAINED_BYTES
            );
            GuardedNoteSourceOutcome::Complete {
                load,
                entries: reservation.shrink_to_and_own(metrics.retained_bytes, entries),
                had_recovery_diagnostics,
            }
        }
        Ok(PaletteNoteSourceOutcome::Cancelled { .. }) => GuardedNoteSourceOutcome::Cancelled,
        Err(error) => GuardedNoteSourceOutcome::Failed(error),
    }
}

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

    fn submit_notes_browser_source(
        &self,
        state: &Rc<NotesBrowserState>,
        mode: palette_service::NotesBrowserMode,
    ) {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope_snapshot = workspaces_file.current_scope_snapshot();
        let all_workspaces = workspaces_file.workspaces;
        let open_editor_snapshots = self.open_editor_note_snapshots_bounded(
            scope_snapshot.folder_paths(),
            &all_workspaces,
            NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT,
            NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT,
        );
        debug_assert!(
            open_editor_snapshots.retained_bytes <= NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT
        );
        state.limit_label.set_label(mode.loading_label());
        state.limit_label.set_visible(true);
        let request = NoteSourceRefreshRequest {
            data_dir: json_store::data_dir(),
            scope_snapshot,
            open_editor_snapshots: Arc::from(open_editor_snapshots.entries),
            open_editor_snapshots_truncated: open_editor_snapshots.truncated,
            mode,
            limits: notes_browser_source_limits(),
        };
        let start = state.source_refreshes.borrow_mut().submit(request);
        if let Some(start) = start {
            start_notes_browser_source_load(state, start);
        }
    }
    /// Coalesce cached note-row refreshes after bursty note and bookmark edits.
    pub(in crate::ui::window) fn refresh_command_palette_note_source_debounced(&self) {
        if !self.imp().palette_revealer.reveals_child() {
            self.invalidate_command_palette_note_source();
            return;
        }

        self.imp().command_palette_notes_refresh_debounce.schedule(
            self,
            Duration::from_millis(COMMAND_PALETTE_NOTES_REFRESH_DEBOUNCE_MS),
            |window, _| {
                window.refresh_command_palette_note_source();
            },
        );
    }

    /// Refresh cached command-palette note rows from the current workspace scope.
    ///
    /// The GTK thread only snapshots open-editor bookmark metadata here. Sidecar
    /// listing and document identity work stay in the background task, and the
    /// generation guard prevents stale completions from replacing newer rows.
    pub(in crate::ui::window) fn refresh_command_palette_note_source(&self) {
        if !self.imp().palette_revealer.reveals_child() {
            self.invalidate_command_palette_note_source();
            return;
        }

        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope_snapshot = workspaces_file.current_scope_snapshot();
        let all_workspaces = workspaces_file.workspaces;
        let open_editor_snapshots = self.open_editor_note_snapshots_bounded(
            scope_snapshot.folder_paths(),
            &all_workspaces,
            palette_service::MAX_PALETTE_NOTE_ENTRIES,
            NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT,
        );
        debug_assert!(
            open_editor_snapshots.retained_bytes <= NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT
        );
        let request = NoteSourceRefreshRequest {
            data_dir: json_store::data_dir(),
            scope_snapshot,
            open_editor_snapshots: Arc::from(open_editor_snapshots.entries),
            open_editor_snapshots_truncated: open_editor_snapshots.truncated,
            mode: palette_service::NotesBrowserMode::AllNotes,
            limits: palette_service::PALETTE_NOTE_SOURCE_LIMITS,
        };
        let start = self
            .imp()
            .command_palette_note_refreshes
            .borrow_mut()
            .submit(request);
        if let Some(start) = start {
            self.start_command_palette_note_refresh(start);
        } else {
            self.finish_cancelled_command_palette_note_admission();
        }
    }

    fn start_command_palette_note_refresh(&self, start: NoteSourceRefreshStart) {
        if start.cancellation.is_cancelled() {
            self.finish_command_palette_note_refresh(
                start.generation,
                GuardedNoteSourceOutcome::Cancelled,
            );
            return;
        }
        let observed_epoch = crate::ui::plain_disposal::disposal_capacity_epoch();
        let weight = palette_service::MAX_PALETTE_NOTE_RETAINED_BYTES;
        let reservation = self
            .imp()
            .command_palette
            .note_source_reservation_weight()
            .map_or_else(
                || crate::ui::plain_disposal::try_reserve_for_gtk(weight),
                |current_weight| {
                    crate::ui::plain_disposal::try_reserve_replacement_for_gtk(
                        weight,
                        current_weight,
                    )
                },
            );
        let Some(reservation) = reservation else {
            debug_assert!(self.imp().command_palette_note_admission.borrow().is_none());
            self.imp()
                .command_palette_note_admission
                .replace(Some(start));
            let window_weak = self.downgrade();
            self.imp()
                .command_palette_note_capacity_wakeup
                .arm(observed_epoch, move || {
                    if let Some(window) = window_weak.upgrade() {
                        window.retry_command_palette_note_admission();
                    }
                });
            if self.imp().palette_revealer.reveals_child() {
                self.publish_status_message(
                    "Command palette note update deferred by memory pressure",
                    MessageKind::Warning,
                );
            }
            return;
        };

        let NoteSourceRefreshStart {
            generation,
            request,
            cancellation,
        } = start;
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                guard_note_source_on_worker(
                    palette_service::load_note_entries_bounded_for_scope(
                        &request.data_dir,
                        &request.scope_snapshot,
                        &request.open_editor_snapshots,
                        request.open_editor_snapshots_truncated,
                        request.mode,
                        request.limits,
                        &cancellation,
                    ),
                    reservation,
                )
            },
            move |(), result| {
                let Some(window) = window_weak.upgrade() else {
                    retire_note_source_result(result);
                    return;
                };
                window.finish_command_palette_note_refresh(generation, result);
            },
        );
    }

    fn retry_command_palette_note_admission(&self) {
        let Some(start) = self
            .imp()
            .command_palette_note_admission
            .borrow_mut()
            .take()
        else {
            return;
        };
        self.start_command_palette_note_refresh(start);
    }

    fn finish_cancelled_command_palette_note_admission(&self) {
        let cancelled = self
            .imp()
            .command_palette_note_admission
            .borrow()
            .as_ref()
            .is_some_and(|start| start.cancellation.is_cancelled());
        if !cancelled {
            return;
        }
        self.imp().command_palette_note_capacity_wakeup.cancel();
        let start = self
            .imp()
            .command_palette_note_admission
            .borrow_mut()
            .take();
        if let Some(start) = start {
            self.finish_command_palette_note_refresh(
                start.generation,
                GuardedNoteSourceOutcome::Cancelled,
            );
        }
    }

    fn finish_command_palette_note_refresh(
        &self,
        generation: u64,
        result: GuardedNoteSourceOutcome,
    ) {
        let (accepted, next) = {
            let mut refreshes = self.imp().command_palette_note_refreshes.borrow_mut();
            let accepted = refreshes.is_current(generation);
            let next = refreshes.finish(generation);
            (accepted, next)
        };

        if accepted {
            match result {
                GuardedNoteSourceOutcome::Complete {
                    load,
                    entries,
                    had_recovery_diagnostics,
                } => {
                    let was_truncated = !load.truncation_reasons.is_empty();
                    self.imp().command_palette.set_guarded_note_entries(entries);
                    if was_truncated && self.imp().palette_revealer.reveals_child() {
                        self.publish_status_message(
                            "Command palette note source was limited to stay responsive",
                            MessageKind::Warning,
                        );
                    } else if had_recovery_diagnostics
                        && self.imp().palette_revealer.reveals_child()
                    {
                        self.publish_status_message(
                            "Some note data could not be loaded for the palette",
                            MessageKind::Warning,
                        );
                    }
                }
                GuardedNoteSourceOutcome::Cancelled => {}
                GuardedNoteSourceOutcome::Failed(error) => {
                    tracing::warn!("Failed to refresh command-palette notes: {error}");
                    if self.imp().palette_revealer.reveals_child() {
                        self.publish_status_message(
                            "Notes could not be loaded for the palette",
                            MessageKind::Warning,
                        );
                    }
                }
            }
        } else {
            retire_note_source_result(result);
        }

        if let Some(next) = next {
            self.start_command_palette_note_refresh(next);
        }
    }

    fn invalidate_command_palette_note_source(&self) {
        self.imp()
            .command_palette_note_refreshes
            .borrow_mut()
            .invalidate();
        self.imp()
            .command_palette_note_admission
            .borrow_mut()
            .take();
        self.imp().command_palette_note_capacity_wakeup.cancel();
        self.imp().command_palette.clear_note_entries();
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
        install_dialog_escape_close(&dialog, &search_entry);
        search_entry.set_placeholder_text(Some("Search Notes..."));
        accessibility::set_labelled_description(
            &search_entry,
            "Search notes",
            "Filter bookmarks, document notes, and folder notes",
        );

        let sidebar = libadwaita::Sidebar::new();
        accessibility::set_role(&sidebar, gtk4::AccessibleRole::List);
        sidebar.set_mode(libadwaita::SidebarMode::Sidebar);
        sidebar.set_vexpand(true);
        sidebar.set_placeholder(Some(&empty_browser_label("No notes match that search")));
        accessibility::set_labelled_description(
            &sidebar,
            "Notes results",
            "Choose a bookmark, document note, or folder note",
        );
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
            "Shown when the notes browser limits a large result set",
        );

        let preview_title = gtk4::Label::new(Some("Select a note"));
        preview_title.set_halign(gtk4::Align::Start);
        preview_title.set_xalign(0.0);
        preview_title.add_css_class("title-4");

        let preview_meta = gtk4::Label::new(Some(
            "Choose a bookmark, folder note, or document note to preview it here.",
        ));
        preview_meta.set_halign(gtk4::Align::Start);
        preview_meta.set_xalign(0.0);
        preview_meta.set_wrap(true);
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
        raw_preview_view.set_monospace(true);
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
            &build_notes_sidebar(&dialog, &search_entry, &sidebar, &limit_label),
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
            sidebar,
            limit_label,
            preview_title,
            preview_meta,
            preview_stack,
            markdown_preview,
            raw_preview_buffer,
            open_button,
            back_button,
            filtered_indices: RefCell::new(Vec::new()),
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
        state.sidebar.connect_selected_item_notify({
            let state = Rc::downgrade(&state);
            move |sidebar| {
                if let Some(state) = state.upgrade() {
                    NotesBrowserState::refresh_preview(
                        &state,
                        sidebar_item_index(sidebar.selected_item()),
                        true,
                    );
                }
            }
        });
        state.sidebar.connect_activated({
            let state = Rc::downgrade(&state);
            move |sidebar, index| {
                if let Some(state) = state.upgrade() {
                    sidebar.set_selected(index);
                    NotesBrowserState::refresh_preview(&state, usize::try_from(index).ok(), true);
                }
            }
        });
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
                if let Some(state) = state_holder.borrow_mut().take() {
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

    /// Return scalar source/query evidence for the visible Notes browser.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn notes_browser_runtime_snapshot_for_test(
        &self,
    ) -> Option<super::NotesBrowserRuntimeSnapshot> {
        self.imp()
            .active_notes_browser
            .borrow()
            .as_ref()
            .and_then(ActiveNotesBrowser::runtime_snapshot)
    }

    /// Snapshot open-editor admission counts with a focused test limit.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn open_editor_note_snapshot_counts_for_test(&self, limit: usize) -> (usize, usize) {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope_snapshot = workspaces_file.current_scope_snapshot();
        let snapshots = self.open_editor_note_snapshots_bounded(
            scope_snapshot.folder_paths(),
            &workspaces_file.workspaces,
            limit,
            NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT,
        );
        let bookmarks = snapshots
            .entries
            .iter()
            .map(|snapshot| snapshot.bookmarks.len())
            .sum();
        (snapshots.entries.len(), bookmarks)
    }

    /// Return retained-byte and truncation evidence for a focused snapshot budget.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn open_editor_note_snapshot_retained_evidence_for_test(
        &self,
        item_limit: usize,
        retained_byte_limit: u64,
    ) -> (usize, usize, u64, bool) {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope_snapshot = workspaces_file.current_scope_snapshot();
        let snapshots = self.open_editor_note_snapshots_bounded(
            scope_snapshot.folder_paths(),
            &workspaces_file.workspaces,
            item_limit,
            retained_byte_limit,
        );
        let bookmarks = snapshots
            .entries
            .iter()
            .map(|snapshot| snapshot.bookmarks.len())
            .sum();
        (
            snapshots.entries.len(),
            bookmarks,
            snapshots.retained_bytes,
            snapshots.truncated,
        )
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

fn retire_note_source_result(result: GuardedNoteSourceOutcome) {
    drop(result);
}

fn start_notes_browser_source_load(state: &Rc<NotesBrowserState>, start: NoteSourceRefreshStart) {
    if start.cancellation.is_cancelled() {
        finish_notes_browser_source_load(
            state,
            start.generation,
            start.request.mode,
            GuardedNoteSourceOutcome::Cancelled,
        );
        return;
    }
    let observed_epoch = crate::ui::plain_disposal::progress_disposal_capacity_epoch();
    let weight = palette_service::MAX_PALETTE_NOTE_RETAINED_BYTES;
    let reservation = state.all_entries.borrow().reservation_weight().map_or_else(
        || crate::ui::plain_disposal::try_reserve_progress_for_gtk(weight),
        |current_weight| {
            crate::ui::plain_disposal::try_reserve_progress_replacement_for_gtk(
                weight,
                current_weight,
            )
        },
    );
    let Some(reservation) = reservation else {
        let mode = start.request.mode;
        debug_assert!(state.source_admission.borrow().is_none());
        state.source_admission.replace(Some(start));
        let state_weak = Rc::downgrade(state);
        state.source_capacity_wakeup.arm(observed_epoch, move || {
            if let Some(state) = state_weak.upgrade() {
                retry_notes_browser_source_admission(&state);
            }
        });
        state.limit_label.set_label(mode.deferred_label());
        state.limit_label.set_visible(true);
        state
            .window
            .publish_status_message(mode.deferred_label(), MessageKind::Warning);
        return;
    };

    let NoteSourceRefreshStart {
        generation,
        request,
        cancellation,
    } = start;
    let mode = request.mode;
    let state_weak = Rc::downgrade(state);
    spawn_blocking_then(
        (),
        move || {
            guard_note_source_on_worker(
                palette_service::load_note_entries_bounded_for_scope(
                    &request.data_dir,
                    &request.scope_snapshot,
                    &request.open_editor_snapshots,
                    request.open_editor_snapshots_truncated,
                    request.mode,
                    request.limits,
                    &cancellation,
                ),
                reservation,
            )
        },
        move |(), result| {
            let Some(state) = state_weak.upgrade() else {
                retire_note_source_result(result);
                return;
            };
            finish_notes_browser_source_load(&state, generation, mode, result);
        },
    );
}

fn retry_notes_browser_source_admission(state: &Rc<NotesBrowserState>) {
    let Some(start) = state.source_admission.borrow_mut().take() else {
        return;
    };
    start_notes_browser_source_load(state, start);
}

fn finish_notes_browser_source_load(
    state: &Rc<NotesBrowserState>,
    generation: u64,
    mode: palette_service::NotesBrowserMode,
    result: GuardedNoteSourceOutcome,
) {
    let (accepted, next) = {
        let mut refreshes = state.source_refreshes.borrow_mut();
        let accepted =
            refreshes.is_current(generation) && state.mode.get() == mode && !state.disposed.get();
        let next = refreshes.finish(generation);
        (accepted, next)
    };
    if accepted {
        match result {
            GuardedNoteSourceOutcome::Complete {
                load,
                entries,
                had_recovery_diagnostics,
            } => {
                let source_truncation = load.truncation_reasons;
                let previous = state
                    .all_entries
                    .replace(Arc::new(entries.into_retained_current()));
                drop(previous);
                *state.source_truncation.borrow_mut() = source_truncation;
                state.source_ready.set(true);
                if !state.source_truncation.borrow().is_empty() {
                    state.window.publish_status_message(
                        match mode {
                            palette_service::NotesBrowserMode::AllNotes => {
                                "The Notes source was limited to stay responsive"
                            }
                            palette_service::NotesBrowserMode::Bookmarks => {
                                "The bookmark source was limited to stay responsive"
                            }
                        },
                        MessageKind::Warning,
                    );
                } else if had_recovery_diagnostics {
                    state.window.publish_status_message(
                        match mode {
                            palette_service::NotesBrowserMode::AllNotes => {
                                "Some note data could not be loaded"
                            }
                            palette_service::NotesBrowserMode::Bookmarks => {
                                "Some bookmark data could not be loaded"
                            }
                        },
                        MessageKind::Warning,
                    );
                }
                submit_notes_browser_query(state, state.search_entry.text().to_string());
            }
            GuardedNoteSourceOutcome::Cancelled => {}
            GuardedNoteSourceOutcome::Failed(error) => {
                tracing::error!("Failed to list {}: {error}", mode.title().to_lowercase());
                state.limit_label.set_label(match mode {
                    palette_service::NotesBrowserMode::AllNotes => "Notes could not be listed",
                    palette_service::NotesBrowserMode::Bookmarks => "Bookmarks could not be listed",
                });
                state.limit_label.set_visible(true);
                state.window.publish_status_message(
                    match mode {
                        palette_service::NotesBrowserMode::AllNotes => "Notes could not be listed",
                        palette_service::NotesBrowserMode::Bookmarks => {
                            "Bookmarks could not be listed"
                        }
                    },
                    MessageKind::Error,
                );
            }
        }
    } else {
        retire_note_source_result(result);
    }
    if let Some(next) = next {
        start_notes_browser_source_load(state, next);
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
    sidebar: &libadwaita::Sidebar,
    limit_label: &gtk4::Label,
) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    install_dialog_escape_close(dialog, &content);

    content.append(search_entry);

    let scroll = gtk4::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(sidebar)
        .build();
    content.append(&scroll);
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
    /// Symbolic icon used by the grouped Adwaita sidebar item.
    fn sidebar_icon_name(&self) -> &'static str;
    /// Return whether this row belongs in the supplemental open-tab section.
    fn is_open_tab(&self) -> bool;
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

    fn is_open_tab(&self) -> bool {
        self.category == PaletteNoteCategory::OpenTabs
    }
}
impl NotesBrowserState {
    fn configure_mode(&self, mode: palette_service::NotesBrowserMode) {
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
        self.sidebar
            .set_placeholder(Some(&empty_browser_label(mode.empty_source_label())));
        accessibility::set_labelled_description(
            &self.sidebar,
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
            match mode {
                palette_service::NotesBrowserMode::AllNotes => "Open selected note",
                palette_service::NotesBrowserMode::Bookmarks => "Open selected bookmark",
            },
            mode.results_description(),
        );
        self.back_button
            .set_tooltip_text(Some(&format!("Back to {}", mode.title())));
        self.show_unselected_preview();
    }

    fn begin_mode(&self, mode: palette_service::NotesBrowserMode) {
        let _ = self.search_debounce.invalidate();
        self.query_runtime.borrow_mut().invalidate();
        self.source_ready.set(false);
        self.source_truncation.borrow_mut().clear();
        self.sidebar.remove_all();
        self.filtered_indices.borrow_mut().clear();
        self.split_view.set_show_content(false);
        self.preview_loads.borrow_mut().invalidate();
        self.configure_mode(mode);
    }

    /// Cancel source/query publication and release retained browser payloads.
    fn dispose_runtime(&self) {
        if self.disposed.replace(true) {
            return;
        }
        let _ = self.search_debounce.invalidate();
        self.source_refreshes.borrow_mut().invalidate();
        self.source_admission.borrow_mut().take();
        self.source_capacity_wakeup.cancel();
        self.query_runtime.borrow_mut().invalidate();
        self.preview_loads.borrow_mut().invalidate();
        self.source_ready.set(false);
        self.filtered_indices.borrow_mut().clear();
        let source = self.all_entries.replace(Arc::new(
            crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                Vec::<NotesBrowserEntry>::new().into_boxed_slice(),
            ),
        ));
        drop(source);
    }

    /// Refresh the preview pane for one selected sidebar item.
    fn refresh_preview(state: &Rc<Self>, index: Option<usize>, user_selected: bool) {
        // Any newly rendered selection supersedes older closed-file excerpt
        // work; a closed-file bookmark branch resubmits below.
        state.preview_loads.borrow_mut().invalidate();

        let Some(index) = index else {
            state.show_unselected_preview();
            return;
        };

        let Some(entry_index) = state.filtered_indices.borrow().get(index).copied() else {
            state.show_unselected_preview();
            return;
        };
        let all_entries = state.all_entries.borrow();
        let Some(entry) = all_entries.get(entry_index) else {
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
        accessibility::set_value_text(
            &self.open_button,
            match mode {
                palette_service::NotesBrowserMode::AllNotes => "No note selected",
                palette_service::NotesBrowserMode::Bookmarks => "No bookmark selected",
            },
        );
        accessibility::set_value_text(
            &self.preview_stack,
            match mode {
                palette_service::NotesBrowserMode::AllNotes => "No note selected",
                palette_service::NotesBrowserMode::Bookmarks => "No bookmark selected",
            },
        );
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
    /// Return the backing entry index for the currently selected sidebar item.
    pub(super) fn selected_entry_index(&self) -> Option<usize> {
        let selected = sidebar_item_index(self.sidebar.selected_item())?;
        self.filtered_indices.borrow().get(selected).copied()
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

/// Debounce browser search so large note sets do not rebuild on every keystroke.
fn schedule_notes_browser_search(state: &Rc<NotesBrowserState>, query: String) {
    if !state.source_ready.get() || state.disposed.get() {
        return;
    }
    if query.is_empty() {
        let _ = state.search_debounce.invalidate();
        submit_notes_browser_query(state, query);
        return;
    }
    let state_weak = Rc::downgrade(state);
    state.search_debounce.schedule(
        &state.search_entry,
        Duration::from_millis(150),
        move |_, _| {
            let Some(state) = state_weak.upgrade() else {
                return;
            };
            submit_notes_browser_query(&state, query);
        },
    );
}

fn submit_notes_browser_query(state: &Rc<NotesBrowserState>, query: String) {
    let request = palette_service::NotesBrowserQueryRequest {
        query,
        mode: state.mode.get(),
    };
    let start = state.query_runtime.borrow_mut().submit(request);
    if let Some(start) = start {
        start_notes_browser_query(state, start);
    }
}

fn start_notes_browser_query(
    state: &Rc<NotesBrowserState>,
    start: palette_service::PaletteSearchStart<palette_service::NotesBrowserQueryRequest>,
) {
    let palette_service::PaletteSearchStart {
        generation,
        request,
        cancellation,
    } = start;
    let mode = request.mode;
    let source = Arc::clone(&state.all_entries.borrow());
    let state_weak = Rc::downgrade(state);
    spawn_blocking_then(
        (),
        move || {
            palette_service::query_notes_browser_source(
                &source,
                &request,
                NOTES_BROWSER_RENDER_LIMIT,
                &cancellation,
            )
        },
        move |(), outcome| {
            let Some(state) = state_weak.upgrade() else {
                retire_notes_browser_query_result(outcome);
                return;
            };
            finish_notes_browser_query(&state, generation, mode, outcome);
        },
    );
}

fn finish_notes_browser_query(
    state: &Rc<NotesBrowserState>,
    generation: u64,
    mode: palette_service::NotesBrowserMode,
    outcome: palette_service::PaletteSearchOutcome<palette_service::NotesBrowserQueryResult>,
) {
    let (accepted, next) = {
        let mut runtime = state.query_runtime.borrow_mut();
        let accepted =
            runtime.is_current(generation) && state.mode.get() == mode && !state.disposed.get();
        let next = runtime.finish(generation);
        (accepted, next)
    };
    if accepted {
        if let palette_service::PaletteSearchOutcome::Complete { value, .. } = outcome {
            publish_notes_browser_query(state, &value);
        }
    } else {
        retire_notes_browser_query_result(outcome);
    }
    if let Some(next) = next {
        start_notes_browser_query(state, next);
    }
}

fn retire_notes_browser_query_result(
    outcome: palette_service::PaletteSearchOutcome<palette_service::NotesBrowserQueryResult>,
) {
    let palette_service::PaletteSearchOutcome::Complete { value, .. } = outcome else {
        return;
    };
    // Query ownership is capped at 500 scalar indexes; the document-sized
    // immutable source remains guarded separately.
    drop(value);
}

/// Publish one current background match while preserving grouped selection.
fn publish_notes_browser_query(
    state: &Rc<NotesBrowserState>,
    result: &palette_service::NotesBrowserQueryResult,
) {
    let previously_selected = state.selected_entry_index();
    state.sidebar.remove_all();
    let source = state.all_entries.borrow();
    let empty_message = if source.is_empty() && state.search_entry.text().is_empty() {
        state.mode.get().empty_source_label()
    } else {
        state.mode.get().no_matches_label()
    };
    state
        .sidebar
        .set_placeholder(Some(&empty_browser_label(empty_message)));
    let grouped_indices = append_notes_sidebar_sections(state, &source, &result.matching_indices);
    update_notes_browser_limit_label(state, result.truncated);

    if grouped_indices.is_empty() {
        *state.filtered_indices.borrow_mut() = Vec::new();
        NotesBrowserState::refresh_preview(state, None, false);
        return;
    }
    let selected = previously_selected
        .and_then(|previous| grouped_indices.iter().position(|index| *index == previous))
        .unwrap_or(0);
    *state.filtered_indices.borrow_mut() = grouped_indices;
    state
        .sidebar
        .set_selected(u32::try_from(selected).unwrap_or(0));
    NotesBrowserState::refresh_preview(state, Some(selected), false);
}

fn update_notes_browser_limit_label(state: &NotesBrowserState, render_truncated: bool) {
    let mut messages = Vec::new();
    if !state.source_truncation.borrow().is_empty() {
        messages.push(state.mode.get().source_limit_message().to_string());
    }
    if render_truncated {
        messages.push(format!(
            "Showing first {NOTES_BROWSER_RENDER_LIMIT} matches. Refine search to narrow results."
        ));
    }
    let message = messages.join(" ");
    state.limit_label.set_label(&message);
    accessibility::set_label(&state.limit_label, &message);
    state.limit_label.set_visible(!messages.is_empty());
}

/// Append note entries as semantic Adwaita sidebar sections and return the
/// exact flat order used for selection lookup.
fn append_notes_sidebar_sections(
    state: &Rc<NotesBrowserState>,
    all_entries: &[NotesBrowserEntry],
    matching_indices: &[usize],
) -> Vec<usize> {
    let sidebar = &state.sidebar;
    let mut ordered_indices = Vec::with_capacity(matching_indices.len());
    append_note_sidebar_section(
        state,
        sidebar,
        "Bookmarks",
        matching_indices.iter().copied().filter(|index| {
            all_entries.get(*index).is_some_and(|entry| {
                entry.category == PaletteNoteCategory::Bookmarks && !entry.is_open_tab()
            })
        }),
        all_entries,
        &mut ordered_indices,
    );
    append_note_sidebar_section(
        state,
        sidebar,
        "Folder Notes",
        matching_indices.iter().copied().filter(|index| {
            all_entries
                .get(*index)
                .is_some_and(|entry| entry.category == PaletteNoteCategory::FolderNotes)
        }),
        all_entries,
        &mut ordered_indices,
    );
    append_note_sidebar_section(
        state,
        sidebar,
        "Document Notes",
        matching_indices.iter().copied().filter(|index| {
            all_entries.get(*index).is_some_and(|entry| {
                entry.category == PaletteNoteCategory::DocumentNotes && !entry.is_open_tab()
            })
        }),
        all_entries,
        &mut ordered_indices,
    );
    append_note_sidebar_section(
        state,
        sidebar,
        "Open Tabs",
        matching_indices.iter().copied().filter(|index| {
            all_entries
                .get(*index)
                .is_some_and(NotesBrowserEntry::is_open_tab)
        }),
        all_entries,
        &mut ordered_indices,
    );
    ordered_indices
}

/// Add one non-empty Notes browser section to the sidebar.
fn append_note_sidebar_section(
    state: &Rc<NotesBrowserState>,
    sidebar: &libadwaita::Sidebar,
    title: &str,
    indices: impl Iterator<Item = usize>,
    all_entries: &[NotesBrowserEntry],
    ordered_indices: &mut Vec<usize>,
) {
    let section = libadwaita::SidebarSection::new();
    section.set_title(Some(title));

    let start_len = ordered_indices.len();
    for index in indices {
        let Some(entry) = all_entries.get(index) else {
            continue;
        };
        section.append(build_notes_sidebar_item(state, index, entry));
        ordered_indices.push(index);
    }

    if ordered_indices.len() > start_len {
        sidebar.append(section);
    }
}

/// Build one Adwaita sidebar item while preserving the old row's searchable
/// metadata and preview line in the visible subtitle/tooltip.
fn build_notes_sidebar_item(
    state: &Rc<NotesBrowserState>,
    entry_index: usize,
    entry: &NotesBrowserEntry,
) -> libadwaita::SidebarItem {
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
    let mut builder = libadwaita::SidebarItem::builder()
        .title(title)
        .subtitle(subtitle.clone())
        .tooltip(subtitle.clone())
        .icon_name(entry.sidebar_icon_name());

    if let Some(bookmark_label) = bookmark_label {
        let action_label = format!("Open bookmark {bookmark_label}");
        let open_button = gtk4::Button::builder()
            .icon_name("document-open-symbolic")
            .tooltip_text(&action_label)
            .valign(gtk4::Align::Center)
            .build();
        open_button.add_css_class("flat");
        accessibility::set_labelled_description(&open_button, &action_label, &subtitle);
        open_button.connect_clicked({
            let state = Rc::downgrade(state);
            move |_| {
                if let Some(state) = state.upgrade() {
                    state.open_entry(entry_index);
                }
            }
        });
        builder = builder.suffix(&open_button);
    }

    builder.build()
}

/// Resolve an Adwaita sidebar item back to the flat backing vector index.
fn sidebar_item_index(item: Option<libadwaita::SidebarItem>) -> Option<usize> {
    item.and_then(|item| usize::try_from(item.index()).ok())
}
