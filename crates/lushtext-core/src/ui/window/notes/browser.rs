// SPDX-License-Identifier: GPL-3.0-or-later

//! Called presentation surface: the unified notes-browser dialog.
//!
//! # Role
//!
//! This module carries **no role**. It projects `WFR-NOTES-BOOKMARKS` onto the
//! browser dialog's widgets — the adaptive shell, the grouped Adwaita sidebar and
//! its rows, the preview stack, and the per-session state those widgets need —
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
use libadwaita::prelude::{AdwDialogExt, NavigationPageExt, SidebarItemExt};

use crate::model::palette::{
    PaletteNoteCategory, PaletteNoteEntry, PaletteNoteTarget, PaletteOpenEditorNoteSnapshot,
};
use crate::services::palette as palette_service;
use crate::ui::accessibility;
use crate::ui::markdown_preview::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;
use super::bookmark_execution::{ensure_raw_preview_target_tag, open_editor_at_line};
use super::chrome::{
    build_dialog_close_button, empty_browser_label, focus_after_present,
    install_dialog_escape_close,
};
use super::policy::NotesBrowserModeExt as _;
use super::query_execution::schedule_notes_browser_search;

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

/// Bounded live-editor request material plus content-free omission evidence.
pub(super) struct OpenEditorNoteSnapshots {
    pub(super) entries: Vec<PaletteOpenEditorNoteSnapshot>,
    pub(super) retained_bytes: u64,
    pub(super) truncated: bool,
}

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
    /// Adwaita browse rail for bookmarks, folder notes, and document notes.
    pub(super) sidebar: libadwaita::Sidebar,
    /// Visible notice when the current result set is capped for responsiveness.
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
    /// Entry indexes currently shown in the sidebar's grouped visual order.
    pub(super) filtered_indices: RefCell<Vec<usize>>,
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
    /// Typed source omissions reported separately from query render truncation.
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
        let Ok(row) = usize::try_from(index) else {
            return false;
        };
        if row >= state.filtered_indices.borrow().len() {
            return false;
        }
        state.sidebar.set_selected(index);
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
        Some(NotesBrowserRuntimeSnapshot {
            source_entries,
            source_truncated,
            source_ready: state.source_ready.get(),
            mode: state.mode.get(),
            query,
            source,
            preview,
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
            mode.open_action_label(),
            mode.results_description(),
        );
        self.back_button
            .set_tooltip_text(Some(&format!("Back to {}", mode.title())));
        self.show_unselected_preview();
    }
    /// Refresh the preview pane for one selected sidebar item.
    pub(super) fn refresh_preview(state: &Rc<Self>, index: Option<usize>, user_selected: bool) {
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

/// Append note entries as semantic Adwaita sidebar sections and return the
/// exact flat order used for selection lookup.
///
/// One section per `PaletteNoteCategory`, in that type's own order and under its
/// own label, so the browser cannot disagree with the palette about which
/// categories exist or what they are called.
pub(super) fn append_notes_sidebar_sections(
    state: &Rc<NotesBrowserState>,
    all_entries: &[NotesBrowserEntry],
    matching_indices: &[usize],
) -> Vec<usize> {
    let sidebar = &state.sidebar;
    let mut ordered_indices = Vec::with_capacity(matching_indices.len());
    for category in PaletteNoteCategory::ALL {
        append_note_sidebar_section(
            state,
            sidebar,
            category.label(),
            matching_indices.iter().copied().filter(|index| {
                all_entries
                    .get(*index)
                    .is_some_and(|entry| entry.category == category)
            }),
            all_entries,
            &mut ordered_indices,
        );
    }
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
