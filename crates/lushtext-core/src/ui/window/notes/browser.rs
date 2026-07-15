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
use libadwaita::prelude::{AdwDialogExt, SidebarItemExt};

use crate::model::palette::{PaletteNoteCategory, PaletteNoteTarget};
use crate::services::palette::{
    NoteSourceRefreshRequest, NoteSourceRefreshStart, PaletteNoteSourceOutcome,
};
use crate::services::{json_store, palette as palette_service};
use crate::ui::accessibility;
use crate::ui::markdown_preview::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};
use crate::ui::status_bar::MessageKind;

use super::{
    ActiveNotesBrowser, LushtextWindow, NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT,
    NOTES_BROWSER_RENDER_LIMIT, NOTES_PREVIEW_MARKDOWN_CHILD, NOTES_PREVIEW_RAW_CHILD,
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
/// Compact empty-browser width.
const EMPTY_NOTES_BROWSER_WIDTH_SP: i32 = 640;
/// Compact empty-browser height.
const EMPTY_NOTES_BROWSER_HEIGHT_SP: i32 = 480;

impl LushtextWindow {
    /// Browse notes across the current workspace scope.
    pub(in crate::ui::window) fn show_notes_dialog(&self) {
        let workspaces_file = self.imp().sidebar.workspaces_file();
        let scope_snapshot = workspaces_file.current_scope_snapshot();
        let all_workspaces = workspaces_file.workspaces;
        let open_editor_snapshots = self.open_editor_note_snapshots_bounded(
            scope_snapshot.folder_paths(),
            &all_workspaces,
            NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT,
        );
        let state = self.present_notes_browser(Vec::new());
        state.limit_label.set_label("Loading notes…");
        state.limit_label.set_visible(true);
        let request = NoteSourceRefreshRequest {
            data_dir: json_store::data_dir(),
            scope_snapshot,
            open_editor_snapshots: Arc::from(open_editor_snapshots),
            limits: notes_browser_source_limits(),
        };
        let start = state.source_refreshes.borrow_mut().submit(request);
        if let Some(start) = start {
            start_notes_browser_source_load(&state, start);
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
        );
        let request = NoteSourceRefreshRequest {
            data_dir: json_store::data_dir(),
            scope_snapshot,
            open_editor_snapshots: Arc::from(open_editor_snapshots),
            limits: palette_service::PALETTE_NOTE_SOURCE_LIMITS,
        };
        let start = self
            .imp()
            .command_palette_note_refreshes
            .borrow_mut()
            .submit(request);
        if let Some(start) = start {
            self.start_command_palette_note_refresh(start);
        }
    }

    fn start_command_palette_note_refresh(&self, start: NoteSourceRefreshStart) {
        let NoteSourceRefreshStart {
            generation,
            request,
            cancellation,
        } = start;
        let window_weak = self.downgrade();
        spawn_blocking_then(
            (),
            move || {
                palette_service::load_note_entries_bounded_for_scope(
                    &request.data_dir,
                    &request.scope_snapshot,
                    &request.open_editor_snapshots,
                    request.limits,
                    &cancellation,
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

    fn finish_command_palette_note_refresh(
        &self,
        generation: u64,
        result: anyhow::Result<PaletteNoteSourceOutcome>,
    ) {
        let (accepted, next) = {
            let mut refreshes = self.imp().command_palette_note_refreshes.borrow_mut();
            let accepted = refreshes.is_current(generation);
            let next = refreshes.finish(generation);
            (accepted, next)
        };

        if accepted {
            match result {
                Ok(PaletteNoteSourceOutcome::Complete { load, .. }) => {
                    Self::trace_browse_recovery_diagnostics(&load.diagnostics);
                    let has_diagnostics = !load.diagnostics.is_empty();
                    let was_truncated = !load.truncation_reasons.is_empty();
                    self.imp().command_palette.set_note_entries(load.entries);
                    if was_truncated && self.imp().palette_revealer.reveals_child() {
                        self.publish_status_message(
                            "Command palette note source was limited to stay responsive",
                            MessageKind::Warning,
                        );
                    } else if has_diagnostics && self.imp().palette_revealer.reveals_child() {
                        self.publish_status_message(
                            "Some note data could not be loaded for the palette",
                            MessageKind::Warning,
                        );
                    }
                }
                Ok(PaletteNoteSourceOutcome::Cancelled { .. }) => {}
                Err(error) => {
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
        self.imp().command_palette.set_note_entries(Vec::new());
    }
    /// Present the unified notes browser for the current workspace scope.
    fn present_notes_browser(&self, entries: Vec<NotesBrowserEntry>) -> Rc<NotesBrowserState> {
        let dialog = libadwaita::Dialog::builder()
            .title("Notes")
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
        split_view.set_sidebar(Some(&libadwaita::NavigationPage::new(
            &build_notes_sidebar(&dialog, &search_entry, &sidebar, &limit_label),
            "Notes",
        )));
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
            preview_generation: Cell::new(0),
            all_entries: RefCell::new(Arc::from(entries)),
            query_runtime: RefCell::default(),
            source_refreshes: RefCell::default(),
            source_truncation: RefCell::new(Vec::new()),
            source_ready: Cell::new(false),
            disposed: Cell::new(false),
        });

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
        );
        let bookmarks = snapshots
            .iter()
            .map(|snapshot| snapshot.bookmarks.len())
            .sum();
        (snapshots.len(), bookmarks)
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

fn retire_note_source_result(result: anyhow::Result<PaletteNoteSourceOutcome>) {
    let Ok(PaletteNoteSourceOutcome::Complete { load, .. }) = result else {
        return;
    };
    spawn_blocking_then((), move || drop(load), |(), ()| {});
}

fn start_notes_browser_source_load(state: &Rc<NotesBrowserState>, start: NoteSourceRefreshStart) {
    let NoteSourceRefreshStart {
        generation,
        request,
        cancellation,
    } = start;
    let state_weak = Rc::downgrade(state);
    spawn_blocking_then(
        (),
        move || {
            palette_service::load_note_entries_bounded_for_scope(
                &request.data_dir,
                &request.scope_snapshot,
                &request.open_editor_snapshots,
                request.limits,
                &cancellation,
            )
        },
        move |(), result| {
            let Some(state) = state_weak.upgrade() else {
                retire_note_source_result(result);
                return;
            };
            finish_notes_browser_source_load(&state, generation, result);
        },
    );
}

fn finish_notes_browser_source_load(
    state: &Rc<NotesBrowserState>,
    generation: u64,
    result: anyhow::Result<PaletteNoteSourceOutcome>,
) {
    let (accepted, next) = {
        let mut refreshes = state.source_refreshes.borrow_mut();
        let accepted = refreshes.is_current(generation) && !state.disposed.get();
        let next = refreshes.finish(generation);
        (accepted, next)
    };
    if accepted {
        match result {
            Ok(PaletteNoteSourceOutcome::Complete { load, .. }) => {
                LushtextWindow::trace_browse_recovery_diagnostics(&load.diagnostics);
                let has_diagnostics = !load.diagnostics.is_empty();
                let source_truncation = load.truncation_reasons;
                let source = Arc::<[NotesBrowserEntry]>::from(load.entries);
                if source.is_empty() {
                    state.dispose_runtime();
                    state.dialog.close();
                    build_empty_notes_dialog().present(Some(&state.window));
                    if has_diagnostics {
                        state.window.publish_status_message(
                            "Some note data could not be loaded",
                            MessageKind::Warning,
                        );
                    }
                    return;
                }

                let previous = state.all_entries.replace(source);
                retire_notes_browser_source(previous);
                *state.source_truncation.borrow_mut() = source_truncation;
                state.source_ready.set(true);
                if !state.source_truncation.borrow().is_empty() {
                    state.window.publish_status_message(
                        "The Notes source was limited to stay responsive",
                        MessageKind::Warning,
                    );
                } else if has_diagnostics {
                    state.window.publish_status_message(
                        "Some note data could not be loaded",
                        MessageKind::Warning,
                    );
                }
                submit_notes_browser_query(state, state.search_entry.text().to_string());
            }
            Ok(PaletteNoteSourceOutcome::Cancelled { .. }) => {}
            Err(error) => {
                tracing::error!("Failed to list notes: {error}");
                state.limit_label.set_label("Notes could not be listed");
                state.limit_label.set_visible(true);
                state
                    .window
                    .publish_status_message("Notes could not be listed", MessageKind::Error);
            }
        }
    } else {
        retire_note_source_result(result);
    }
    if let Some(next) = next {
        start_notes_browser_source_load(state, next);
    }
}

fn retire_notes_browser_source(source: Arc<[NotesBrowserEntry]>) {
    if source.is_empty() {
        return;
    }
    spawn_blocking_then((), move || drop(source), |(), ()| {});
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
    let title = gtk4::Label::new(Some("Notes"));
    title.set_halign(gtk4::Align::Start);
    title.set_hexpand(true);
    title.set_xalign(0.0);
    title.add_css_class("title-4");
    header.append(&title);

    let close_button = build_dialog_close_button(dialog);
    install_dialog_escape_close(dialog, &close_button);
    header.append(&close_button);
    shell.append(&header);
    shell.append(split_view);

    shell
}
/// Build an explicit empty state when the current scope has no notes yet.
fn build_empty_notes_dialog() -> libadwaita::Dialog {
    let dialog = libadwaita::Dialog::builder()
        .title("Notes")
        .content_width(EMPTY_NOTES_BROWSER_WIDTH_SP)
        .content_height(EMPTY_NOTES_BROWSER_HEIGHT_SP)
        // `AdwStatusPage` has a narrow natural request; following content size
        // recreates the cramped empty-state column instead of this readable target.
        .follows_content_size(false)
        .build();

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    install_dialog_escape_close(&dialog, &content);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let title = gtk4::Label::new(Some("Notes"));
    title.set_halign(gtk4::Align::Start);
    title.set_hexpand(true);
    title.add_css_class("title-4");
    header.append(&title);
    let close_button = build_dialog_close_button(&dialog);
    install_dialog_escape_close(&dialog, &close_button);
    header.append(&close_button);
    content.append(&header);

    let status = libadwaita::StatusPage::builder()
        .icon_name("text-x-generic-symbolic")
        .title("No notes yet")
        .description(
            "Bookmarks, document notes, and folder notes will appear here once you save one.",
        )
        .build();
    accessibility::set_role(&status, gtk4::AccessibleRole::Status);
    accessibility::set_labelled_description(
        &status,
        "No notes yet",
        "Bookmarks, document notes, and folder notes will appear here once you save one.",
    );
    status.set_hexpand(true);
    status.set_vexpand(true);
    content.append(&status);
    dialog.set_child(Some(&content));
    focus_after_present(&close_button);
    dialog
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
    /// Cancel source/query publication and release retained browser payloads.
    fn dispose_runtime(&self) {
        if self.disposed.replace(true) {
            return;
        }
        let _ = self.search_debounce.invalidate();
        self.source_refreshes.borrow_mut().invalidate();
        self.query_runtime.borrow_mut().invalidate();
        self.source_ready.set(false);
        self.filtered_indices.borrow_mut().clear();
        let source = self.all_entries.replace(Arc::from([]));
        retire_notes_browser_source(source);
    }

    /// Refresh the preview pane for one selected sidebar item.
    fn refresh_preview(state: &Rc<Self>, index: Option<usize>, user_selected: bool) {
        let generation = state.advance_preview_generation();
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
            Self::refresh_bookmark_preview(state, entry, generation);
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

    /// Advance the preview token that async bookmark loads must match.
    fn advance_preview_generation(&self) -> u32 {
        let generation = self.preview_generation.get().wrapping_add(1);
        self.preview_generation.set(generation);
        generation
    }

    /// Reset the preview pane to the initial no-selection state.
    fn show_unselected_preview(&self) {
        self.preview_title.set_label("Select a note");
        self.preview_meta
            .set_label("Choose a bookmark, folder note, or document note to preview it here.");
        self.show_markdown_placeholder("Select a note to preview its details.");
        self.open_button.set_sensitive(false);
        accessibility::set_disabled(&self.open_button, true);
        accessibility::set_value_text(&self.open_button, "No note selected");
        accessibility::set_value_text(&self.preview_stack, "No note selected");
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
    let request = palette_service::NotesBrowserQueryRequest { query };
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
            finish_notes_browser_query(&state, generation, outcome);
        },
    );
}

fn finish_notes_browser_query(
    state: &Rc<NotesBrowserState>,
    generation: u64,
    outcome: palette_service::PaletteSearchOutcome<palette_service::NotesBrowserQueryResult>,
) {
    let (accepted, next) = {
        let mut runtime = state.query_runtime.borrow_mut();
        let accepted = runtime.is_current(generation) && !state.disposed.get();
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
    spawn_blocking_then((), move || drop(value), |(), ()| {});
}

/// Publish one current background match while preserving grouped selection.
fn publish_notes_browser_query(
    state: &Rc<NotesBrowserState>,
    result: &palette_service::NotesBrowserQueryResult,
) {
    let previously_selected = state.selected_entry_index();
    state.sidebar.remove_all();
    let source = state.all_entries.borrow();
    let grouped_indices =
        append_notes_sidebar_sections(&state.sidebar, &source, &result.matching_indices);
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
        messages.push(
            "Some later notes were omitted because the source reached its safety limits."
                .to_string(),
        );
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
    sidebar: &libadwaita::Sidebar,
    all_entries: &[NotesBrowserEntry],
    matching_indices: &[usize],
) -> Vec<usize> {
    let mut ordered_indices = Vec::with_capacity(matching_indices.len());
    append_note_sidebar_section(
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
        section.append(build_notes_sidebar_item(entry));
        ordered_indices.push(index);
    }

    if ordered_indices.len() > start_len {
        sidebar.append(section);
    }
}

/// Build one Adwaita sidebar item while preserving the old row's searchable
/// metadata and preview line in the visible subtitle/tooltip.
fn build_notes_sidebar_item(entry: &NotesBrowserEntry) -> libadwaita::SidebarItem {
    let subtitle = entry.row_detail().map_or_else(
        || entry.row_subtitle(),
        |detail| format!("{} · {detail}", entry.row_subtitle()),
    );
    libadwaita::SidebarItem::builder()
        .title(entry.row_title())
        .subtitle(subtitle.clone())
        .tooltip(subtitle)
        .icon_name(entry.sidebar_icon_name())
        .build()
}

/// Resolve an Adwaita sidebar item back to the flat backing vector index.
fn sidebar_item_index(item: Option<libadwaita::SidebarItem>) -> Option<usize> {
    item.and_then(|item| usize::try_from(item.index()).ok())
}
