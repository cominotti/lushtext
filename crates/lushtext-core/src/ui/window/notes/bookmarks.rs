// SPDX-License-Identifier: GPL-3.0-or-later

//! Bookmark actions, persistence, browsing, editing, and preview rendering.

use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::translate::IntoGlib;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::pango;
use gtk4::prelude::*;
use libadwaita::prelude::{AdwDialogExt, PreferencesGroupExt};

use crate::model::bookmark::BookmarkRecord;
use crate::model::palette::PaletteNoteTarget;
use crate::services::{bookmark_excerpt, bookmark_service, json_store};
use crate::ui::accessibility;
use crate::ui::editor_page::{
    BookmarkEditError, BookmarkNavigationDirection, BookmarkToggleState, LushtextEditorPage,
};
use crate::ui::status_bar::MessageKind;

use super::browser::NotesBrowserEntryExt;
use super::{
    LushtextWindow, NOTES_PREVIEW_RAW_CHILD, NotesBrowserEntry, NotesBrowserState,
    build_dialog_close_button,
};

#[cfg(feature = "test-utils")]
use std::sync::atomic::{AtomicU64, Ordering};

/// Debounce interval for bookmark sidecar saves.
const NOTES_SAVE_DEBOUNCE_MS: u64 = 200;
/// Text tag applied to the bookmarked row inside the raw preview surface.
const NOTES_RAW_BOOKMARK_TARGET_TAG: &str = "bookmark-target-line";
#[cfg(feature = "test-utils")]
static BOOKMARK_EXCERPT_PREVIEW_DELAY_MS: AtomicU64 = AtomicU64::new(0);

/// Configure an artificial closed-file bookmark preview delay for widget tests.
#[cfg(feature = "test-utils")]
pub fn set_bookmark_excerpt_preview_delay_for_test(delay_ms: u64) {
    BOOKMARK_EXCERPT_PREVIEW_DELAY_MS.store(delay_ms, Ordering::Release);
}

impl LushtextWindow {
    /// Toggle the bookmark on the current cursor line.
    pub(in crate::ui::window) fn toggle_bookmark(&self) {
        let Some(editor) = self.require_saved_editor("Bookmarks require a saved file") else {
            return;
        };

        match editor.toggle_bookmark_at_cursor() {
            BookmarkToggleState::Added(line) => self.publish_status_message(
                &format!("Bookmark added at line {}", line.saturating_add(1)),
                MessageKind::Info,
            ),
            BookmarkToggleState::Removed(line) => self.publish_status_message(
                &format!("Bookmark removed from line {}", line.saturating_add(1)),
                MessageKind::Info,
            ),
        }
    }

    /// Edit the bookmark on the current cursor line.
    pub(in crate::ui::window) fn edit_bookmark(&self) {
        let Some(editor) = self.require_saved_editor("Bookmarks require a saved file") else {
            return;
        };
        let Some(bookmark) = editor.current_bookmark() else {
            self.publish_status_message(
                "Move the cursor to a bookmarked line first",
                MessageKind::Warning,
            );
            return;
        };

        self.present_bookmark_edit_dialog(&editor, &bookmark);
    }

    /// Build and present the modal editor for one existing bookmark.
    ///
    /// The window layer owns modal UI and status feedback, while accepted edits
    /// are delegated back to `LushtextEditorPage` so live mark movement, minimap
    /// refresh, and debounced sidecar persistence stay on the existing path.
    pub(super) fn present_bookmark_edit_dialog(
        &self,
        editor: &LushtextEditorPage,
        bookmark: &BookmarkRecord,
    ) {
        // A custom `AdwDialog` gives this form two fields, custom actions, and
        // inline validation feedback without closing on invalid input.
        let dialog = libadwaita::Dialog::builder()
            .title("Edit Bookmark")
            .content_width(420)
            .build();

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 16);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.set_margin_top(18);
        content.set_margin_bottom(18);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let title_label = gtk4::Label::new(Some("Edit Bookmark"));
        title_label.set_halign(gtk4::Align::Start);
        title_label.set_hexpand(true);
        title_label.add_css_class("title-4");
        header.append(&title_label);

        header.append(&build_dialog_close_button(&dialog));
        content.append(&header);

        let group = libadwaita::PreferencesGroup::new();
        accessibility::set_role(&group, gtk4::AccessibleRole::Group);
        accessibility::set_labelled_description(
            &group,
            "Bookmark fields",
            "Edit the bookmark label and one-based line number",
        );

        // Adwaita preference rows provide standard GNOME labeled form controls
        // here; the explicit accessible group keeps the modal form meaningful
        // outside a preferences window.
        let label_row = libadwaita::EntryRow::builder().title("Label").build();
        accessibility::set_labelled_description(
            &label_row,
            "Bookmark label",
            "Optional bookmark name shown in lists, gutter tooltips, and note browsers",
        );
        if let Some(label) = bookmark.label.as_deref() {
            label_row.set_text(label);
        }
        group.add(&label_row);

        let line_row = libadwaita::EntryRow::builder()
            .title("Line")
            .text(bookmark.line.saturating_add(1).to_string())
            .build();
        accessibility::set_labelled_description(
            &line_row,
            "Bookmark line",
            "One-based document line number for this bookmark",
        );
        group.add(&line_row);
        content.append(&group);

        let error_label = gtk4::Label::new(None);
        error_label.set_halign(gtk4::Align::Start);
        error_label.set_xalign(0.0);
        error_label.set_wrap(true);
        error_label.add_css_class("error");
        error_label.set_visible(false);
        accessibility::set_role(&error_label, gtk4::AccessibleRole::Status);
        accessibility::set_label(&error_label, "Bookmark edit feedback");
        accessibility::set_hidden(&error_label, true);
        content.append(&error_label);

        let button_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        button_box.set_halign(gtk4::Align::End);
        let cancel_button = gtk4::Button::with_label("Cancel");
        accessibility::set_labelled_description(
            &cancel_button,
            "Cancel",
            "Close bookmark editor without saving changes",
        );
        let dialog_weak = dialog.downgrade();
        cancel_button.connect_clicked(move |_| {
            if let Some(dialog) = dialog_weak.upgrade() {
                dialog.close();
            }
        });
        button_box.append(&cancel_button);

        let save_button = gtk4::Button::with_label("Save");
        save_button.add_css_class("suggested-action");
        accessibility::set_labelled_description(
            &save_button,
            "Save bookmark",
            "Save the bookmark label and line number",
        );
        button_box.append(&save_button);
        content.append(&button_box);

        let error_weak = error_label.downgrade();
        let line_row_weak = line_row.downgrade();
        line_row.connect_changed(move |_| {
            if let Some(error_label) = error_weak.upgrade() {
                clear_bookmark_edit_error(&error_label);
            }
            if let Some(line_row) = line_row_weak.upgrade() {
                accessibility::set_invalid(&line_row, false);
            }
        });

        let error_weak = error_label.downgrade();
        let line_row_weak = line_row.downgrade();
        label_row.connect_changed(move |_| {
            if let Some(error_label) = error_weak.upgrade() {
                clear_bookmark_edit_error(&error_label);
            }
            if let Some(line_row) = line_row_weak.upgrade() {
                accessibility::set_invalid(&line_row, false);
            }
        });

        let bookmark_id = bookmark.id.clone();
        let editor_weak = editor.downgrade();
        let window_weak = self.downgrade();
        let dialog_weak = dialog.downgrade();
        save_button.connect_clicked(move |_| {
            let label = (!label_row.text().trim().is_empty()).then(|| label_row.text().to_string());
            let target_line = match parse_bookmark_target_line(&line_row.text()) {
                Ok(line) => line,
                Err(message) => {
                    show_bookmark_edit_error(&error_label, Some(&line_row), &message);
                    return;
                }
            };

            let Some(editor) = editor_weak.upgrade() else {
                return;
            };
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            // The window only parses dialog input. The editor validates line
            // range and collisions because it owns live marks and buffer state.
            match editor.update_bookmark(&bookmark_id, label, target_line) {
                Ok(outcome) => {
                    window.publish_status_message(
                        &format!("Bookmark saved at line {}", outcome.line.saturating_add(1)),
                        MessageKind::Info,
                    );
                    if let Some(dialog) = dialog_weak.upgrade() {
                        dialog.close();
                    }
                }
                Err(error) => {
                    let message = bookmark_edit_error_message(&error);
                    let line_row = if matches!(error, BookmarkEditError::NotFound) {
                        None
                    } else {
                        Some(&line_row)
                    };
                    show_bookmark_edit_error(&error_label, line_row, &message);
                }
            }
        });

        dialog.set_child(Some(&content));
        dialog.present(Some(self));
    }

    /// Jump to the next or previous bookmark in the active file.
    pub(in crate::ui::window) fn navigate_bookmark_action(
        &self,
        direction: BookmarkNavigationDirection,
    ) {
        let Some(editor) = self.require_saved_editor("Bookmarks require a saved file") else {
            return;
        };

        let Some(bookmark) = editor.navigate_bookmark(direction) else {
            self.publish_status_message(
                "No bookmarks exist in the active file",
                MessageKind::Warning,
            );
            return;
        };

        self.publish_status_message(
            &format!("Jumped to {}", bookmark.display_label()),
            MessageKind::Info,
        );
    }

    /// Browse workspace bookmarks in a searchable dialog.
    pub(in crate::ui::window) fn show_bookmarks_dialog(&self) {
        let workspace_folders = self.workspace_folder_paths_for_notes();
        if workspace_folders.is_empty() {
            self.publish_status_message(
                "Add a workspace before browsing bookmarks",
                MessageKind::Warning,
            );
            return;
        }
        self.show_notes_browser_mode(crate::services::palette::NotesBrowserMode::Bookmarks);
    }
    /// Debounce bookmark persistence so one burst of edits produces one sidecar write.
    pub(super) fn save_bookmarks_debounced(&self, editor: &LushtextEditorPage) {
        let window_weak = self.downgrade();
        editor.imp().bookmarks.persistence.save_debounce.schedule(
            editor,
            Duration::from_millis(NOTES_SAVE_DEBOUNCE_MS),
            move |editor, _| {
                if editor.imp().bookmarks.persistence.save_inflight.get() {
                    editor.imp().bookmarks.persistence.save_dirty.set(true);
                    return;
                }

                if let Some(window) = window_weak.upgrade() {
                    window.persist_bookmarks_now(&editor);
                }
            },
        );
    }

    /// Write the current bookmark snapshot to disk.
    fn persist_bookmarks_now(&self, editor: &LushtextEditorPage) {
        let Some(path) = editor.file_path() else {
            return;
        };
        let bookmarks = editor.bookmark_records();
        let data_dir = json_store::data_dir();
        editor.imp().bookmarks.persistence.save_inflight.set(true);
        editor.imp().bookmarks.persistence.save_dirty.set(false);

        let window_weak = self.downgrade();
        spawn_blocking_then(
            editor.clone(),
            move || bookmark_service::save_for_path(&data_dir, &path, &bookmarks).map(|_| ()),
            move |editor, result| {
                editor.imp().bookmarks.persistence.save_inflight.set(false);
                if let Err(error) = result {
                    tracing::error!("Failed to save bookmarks: {error}");
                    if let Some(window) = window_weak.upgrade() {
                        window.publish_status_message("Bookmark save failed", MessageKind::Warning);
                    }
                }
                if editor.imp().bookmarks.persistence.save_dirty.replace(false)
                    && let Some(window) = window_weak.upgrade()
                {
                    window.persist_bookmarks_now(&editor);
                }
            },
        );
    }
}

impl NotesBrowserState {
    /// Resolve and render a bookmark preview for the selected row.
    pub(super) fn refresh_bookmark_preview(state: &Rc<Self>, entry: &NotesBrowserEntry) {
        let PaletteNoteTarget::Bookmark { path, line, .. } = &entry.target else {
            return;
        };

        let presentation = bookmark_excerpt::presentation_for_path(path);
        if let Some(editor) = state.window.open_editor_for_path(path) {
            // Live-editor previews bypass closed-file workers entirely; the
            // caller already invalidated older closed-file work.
            state.render_bookmark_excerpt_state(
                entry,
                live_bookmark_excerpt_for_editor(&editor, *line, presentation),
            );
            return;
        }

        state.render_bookmark_excerpt_state(
            entry,
            bookmark_excerpt::BookmarkExcerptState::Loading { presentation },
        );

        let start = state.preview_loads.borrow_mut().submit(
            bookmark_excerpt::BookmarkExcerptPreviewRequest {
                path: path.clone(),
                line: *line,
            },
        );
        if let Some(start) = start {
            Self::start_bookmark_preview_load(state, start);
        }
    }

    /// Launch the sole active closed-file excerpt worker for one admitted request.
    fn start_bookmark_preview_load(
        state: &Rc<Self>,
        start: bookmark_excerpt::BookmarkExcerptPreviewStart,
    ) {
        if start.cancellation.is_cancelled() {
            Self::finish_bookmark_preview_load(state, start.generation, None);
            return;
        }

        let bookmark_excerpt::BookmarkExcerptPreviewStart {
            generation,
            request,
            cancellation,
        } = start;
        let path = request.path.clone();
        let line = request.line;
        let state_weak = Rc::downgrade(state);
        spawn_blocking_then(
            (),
            move || {
                delay_bookmark_excerpt_preview_for_test();
                bookmark_excerpt::load_from_path_cancellable(
                    &request.path,
                    request.line,
                    &cancellation,
                )
            },
            move |(), outcome| {
                let Some(state) = state_weak.upgrade() else {
                    return;
                };
                let completion = match outcome {
                    bookmark_excerpt::BookmarkExcerptLoadOutcome::Completed(result) => {
                        Some((path, line, result))
                    }
                    bookmark_excerpt::BookmarkExcerptLoadOutcome::Cancelled => None,
                };
                Self::finish_bookmark_preview_load(&state, generation, completion);
            },
        );
    }

    /// Retire one active excerpt terminal, publish if current, then start the latest request.
    ///
    /// Every terminal (success, unavailable, cancelled, and the pre-cancelled
    /// short circuit) passes through this single transition so active ownership
    /// clears exactly once and a retained pending request cannot stall.
    fn finish_bookmark_preview_load(
        state: &Rc<Self>,
        generation: u64,
        completion: Option<(
            std::path::PathBuf,
            u32,
            bookmark_excerpt::BookmarkExcerptState,
        )>,
    ) {
        let (accepted, next) = {
            let mut loads = state.preview_loads.borrow_mut();
            let accepted = loads.is_current(generation) && !state.disposed.get();
            let next = loads.finish(generation);
            (accepted, next)
        };
        if accepted && let Some((path, line, result)) = completion {
            state.apply_bookmark_preview_completion(&path, line, result);
        }
        if let Some(next) = next {
            Self::start_bookmark_preview_load(state, next);
        }
    }

    /// Apply a closed-file preview only if it still belongs to the selected row.
    fn apply_bookmark_preview_completion(
        &self,
        path: &Path,
        line: u32,
        result: bookmark_excerpt::BookmarkExcerptState,
    ) {
        if !self.selected_bookmark_matches(path, line) {
            return;
        }

        let Some(entry_index) = self.selected_entry_index() else {
            return;
        };
        let all_entries = self.all_entries.borrow();
        let Some(entry) = all_entries.get(entry_index) else {
            return;
        };
        self.render_bookmark_excerpt_state(entry, result);
    }

    /// Render one resolved bookmark preview state into the active preview child.
    fn render_bookmark_excerpt_state(
        &self,
        entry: &NotesBrowserEntry,
        state: bookmark_excerpt::BookmarkExcerptState,
    ) {
        match state {
            bookmark_excerpt::BookmarkExcerptState::Loading { .. } => {
                self.show_markdown_content_placeholder("Loading bookmark preview...");
            }
            bookmark_excerpt::BookmarkExcerptState::Unavailable(unavailable) => {
                self.show_markdown_content_placeholder(bookmark_unavailable_description(
                    unavailable.reason,
                ));
            }
            bookmark_excerpt::BookmarkExcerptState::Ready(excerpt) => match excerpt.presentation {
                bookmark_excerpt::BookmarkExcerptPresentation::Markdown => {
                    self.show_markdown_preview();
                    self.markdown_preview.render_markdown_with_context(
                        &excerpt.body_text_with_markers(),
                        &entry.render_context(),
                    );
                }
                bookmark_excerpt::BookmarkExcerptPresentation::PlainText => {
                    self.render_raw_bookmark_excerpt(&excerpt);
                }
            },
        }
    }

    /// Render a plain-text bookmark excerpt into the raw preview surface.
    fn render_raw_bookmark_excerpt(&self, excerpt: &bookmark_excerpt::BookmarkExcerpt) {
        self.markdown_preview.clear();
        let formatted = format_raw_bookmark_excerpt(excerpt);
        self.raw_preview_buffer.set_text(&formatted.text);
        let tag = ensure_raw_preview_target_tag(&self.raw_preview_buffer);
        let start = self
            .raw_preview_buffer
            .iter_at_offset(formatted.target_start);
        let end = self.raw_preview_buffer.iter_at_offset(formatted.target_end);
        self.raw_preview_buffer.apply_tag(&tag, &start, &end);
        self.preview_stack
            .set_visible_child_name(NOTES_PREVIEW_RAW_CHILD);
    }
    /// Check that an async bookmark completion still belongs to the selected row.
    fn selected_bookmark_matches(&self, path: &Path, line: u32) -> bool {
        let Some(entry_index) = self.selected_entry_index() else {
            return false;
        };
        matches!(
            self.all_entries
                .borrow()
                .get(entry_index)
                .map(|entry| &entry.target),
            Some(PaletteNoteTarget::Bookmark {
                path: selected_path,
                line: selected_line,
                ..
            }) if selected_path == path && *selected_line == line
        )
    }
}

/// Extract live source context from an open editor without snapshotting the full buffer.
fn live_bookmark_excerpt_for_editor(
    editor: &LushtextEditorPage,
    target_line: u32,
    presentation: bookmark_excerpt::BookmarkExcerptPresentation,
) -> bookmark_excerpt::BookmarkExcerptState {
    let buffer = editor.buffer();
    let line_count = u32::try_from(buffer.line_count().max(1)).unwrap_or(u32::MAX);
    if target_line >= line_count {
        return bookmark_excerpt::BookmarkExcerptState::Unavailable(
            bookmark_excerpt::BookmarkExcerptUnavailable {
                presentation,
                reason: bookmark_excerpt::BookmarkExcerptUnavailableReason::LineOutOfRange,
            },
        );
    }

    let before =
        u32::try_from(bookmark_excerpt::BOOKMARK_EXCERPT_CONTEXT_BEFORE_LINES).unwrap_or(u32::MAX);
    let after =
        u32::try_from(bookmark_excerpt::BOOKMARK_EXCERPT_CONTEXT_AFTER_LINES).unwrap_or(u32::MAX);
    let first_line = target_line.saturating_sub(before);
    let last_line = target_line
        .saturating_add(after)
        .min(line_count.saturating_sub(1));

    let mut lines = Vec::new();
    for line in first_line..=last_line {
        lines.push(buffer_line_text(&buffer, line, line_count));
    }

    bookmark_excerpt::extract_from_context_lines(
        presentation,
        first_line,
        target_line,
        lines,
        first_line > 0,
        last_line.saturating_add(1) < line_count,
    )
}

/// Copy one bounded line from a `GtkTextBuffer`.
fn buffer_line_text(buffer: &sourceview5::Buffer, line: u32, line_count: u32) -> String {
    let start = buffer
        .iter_at_line(i32::try_from(line).unwrap_or(i32::MAX))
        .unwrap_or_else(|| buffer.end_iter());
    let line_end = if line.saturating_add(1) < line_count {
        buffer
            .iter_at_line(i32::try_from(line.saturating_add(1)).unwrap_or(i32::MAX))
            .unwrap_or_else(|| buffer.end_iter())
    } else {
        buffer.end_iter()
    };
    let mut capped_end = start;
    capped_end.forward_chars(
        i32::try_from(bookmark_excerpt::BOOKMARK_EXCERPT_LINE_CHAR_LIMIT.saturating_add(1))
            .unwrap_or(i32::MAX),
    );
    let end = if capped_end.offset() < line_end.offset() {
        capped_end
    } else {
        line_end
    };
    buffer
        .text(&start, &end, true)
        .trim_end_matches('\n')
        .trim_end_matches('\r')
        .to_string()
}

/// Ensure the raw bookmark target-line tag exists in the given buffer.
pub(super) fn ensure_raw_preview_target_tag(buffer: &gtk4::TextBuffer) -> gtk4::TextTag {
    let table = buffer.tag_table();
    if let Some(tag) = table.lookup(NOTES_RAW_BOOKMARK_TARGET_TAG) {
        return tag;
    }

    let tag = gtk4::TextTag::new(Some(NOTES_RAW_BOOKMARK_TARGET_TAG));
    tag.set_weight(pango::Weight::Bold.into_glib());
    table.add(&tag);
    tag
}

/// Formatted raw bookmark body plus text-buffer offsets for target emphasis.
struct RawBookmarkExcerptText {
    /// Text inserted into the raw preview buffer.
    text: String,
    /// Character offset where the target line starts.
    target_start: i32,
    /// Character offset immediately after the target line.
    target_end: i32,
}

/// Render raw source context with line numbers and a target-line marker.
fn format_raw_bookmark_excerpt(
    excerpt: &bookmark_excerpt::BookmarkExcerpt,
) -> RawBookmarkExcerptText {
    let line_number_width = excerpt
        .lines
        .last()
        .map_or(1, |line| line.number.saturating_add(1).to_string().len())
        .max(2);
    let mut text = String::new();
    let mut target_start = 0;
    let mut target_end = 0;

    if excerpt.window.truncation.before {
        push_raw_preview_line(&mut text, "... earlier bookmark context omitted ...");
    }

    for (index, line) in excerpt.lines.iter().enumerate() {
        if index == excerpt.window.target_line_index {
            target_start = raw_preview_offset(&text);
        }

        let marker = if index == excerpt.window.target_line_index {
            ">"
        } else {
            " "
        };
        let line_number = line.number.saturating_add(1);
        push_raw_preview_line(
            &mut text,
            &format!("{marker} {line_number:>line_number_width$} | {}", line.text),
        );

        if index == excerpt.window.target_line_index {
            target_end = raw_preview_offset(&text).saturating_sub(1);
        }
    }

    if excerpt.window.truncation.after {
        push_raw_preview_line(&mut text, "... later bookmark context omitted ...");
    }

    RawBookmarkExcerptText {
        text,
        target_start,
        target_end,
    }
}

fn push_raw_preview_line(text: &mut String, line: &str) {
    text.push_str(line);
    text.push('\n');
}

fn raw_preview_offset(text: &str) -> i32 {
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX)
}

fn bookmark_unavailable_description(
    reason: bookmark_excerpt::BookmarkExcerptUnavailableReason,
) -> &'static str {
    match reason {
        bookmark_excerpt::BookmarkExcerptUnavailableReason::MissingOrUnreadable => {
            "Bookmark preview unavailable: the file is missing or cannot be read."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::BinaryOrUnsupported => {
            "Bookmark preview unavailable: this file is not UTF-8 text."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::TooLargeToPreview => {
            "Bookmark preview unavailable: this file is too large to preview."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::LineBeyondPreviewBudget => {
            "Bookmark preview unavailable: the bookmarked line is beyond the preview budget."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::LineOutOfRange => {
            "Bookmark preview unavailable: the bookmarked line is no longer in this file."
        }
    }
}

/// Sleep only under `test-utils` so widget tests can exercise stale completions.
fn delay_bookmark_excerpt_preview_for_test() {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = BOOKMARK_EXCERPT_PREVIEW_DELAY_MS.load(Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
}
/// Parse only the syntax of a user-facing 1-based bookmark line.
///
/// Range and collision checks stay in the editor layer so failed edits leave the
/// live bookmark projection unchanged.
fn parse_bookmark_target_line(text: &str) -> Result<u32, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("Enter a line number.".to_string());
    }

    trimmed
        .parse::<u32>()
        .map_err(|_| "Line must be a whole number.".to_string())
}

/// Convert editor validation failures into dialog feedback.
fn bookmark_edit_error_message(error: &BookmarkEditError) -> String {
    match error {
        BookmarkEditError::NotFound => "That bookmark is no longer available.".to_string(),
        BookmarkEditError::LineOutOfRange {
            requested_line,
            max_line,
        } => format!("Line {requested_line} is outside this document. Use 1 through {max_line}."),
        BookmarkEditError::LineOccupied { line } => {
            format!("Line {line} already has another bookmark.")
        }
    }
}

/// Show bookmark-edit validation feedback and expose the failed field to assistive tech.
fn show_bookmark_edit_error(
    error_label: &gtk4::Label,
    invalid_line_row: Option<&libadwaita::EntryRow>,
    message: &str,
) {
    error_label.set_label(message);
    error_label.set_visible(true);
    accessibility::set_label(error_label, message);
    accessibility::set_hidden(error_label, false);
    accessibility::set_invalid(error_label, true);
    accessibility::announce_with_lane(error_label, message, accessibility::AnnouncementLane::Alert);
    if let Some(line_row) = invalid_line_row {
        accessibility::set_invalid(line_row, true);
    }
}

/// Hide bookmark-edit validation feedback and clear stale accessible error state.
fn clear_bookmark_edit_error(error_label: &gtk4::Label) {
    error_label.set_visible(false);
    error_label.set_label("");
    accessibility::set_label(error_label, "Bookmark edit feedback");
    accessibility::set_hidden(error_label, true);
    accessibility::set_invalid(error_label, false);
}
/// Open a file at a specific 1-based line number and focus the editor.
pub(super) fn open_editor_at_line(window: &LushtextWindow, path: &Path, line: u32) {
    window.open_document(path);

    let Some(editor) = window.active_editor() else {
        return;
    };

    let line_zero_based = line.saturating_sub(1);
    if editor.is_evicted() {
        editor.set_restore_position(line_zero_based, 0, line_zero_based.saturating_sub(3));
        window.reload_if_evicted();
    } else if editor.buffer().char_count() > 0 {
        let buffer = editor.buffer();
        let iter = buffer
            .iter_at_line(i32::try_from(line_zero_based).unwrap_or(i32::MAX))
            .unwrap_or_else(|| buffer.end_iter());
        buffer.place_cursor(&iter);
        let mark = buffer.create_mark(None, &iter, true);
        editor
            .source_view()
            .scroll_to_mark(&mark, 0.2, true, 0.0, 0.0);
        buffer.delete_mark(&mark);
    } else {
        editor.set_restore_position(line_zero_based, 0, line_zero_based.saturating_sub(3));
    }
    editor.source_view().grab_focus();
}
