// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — the print workflow's single observable state.
//!
//! One accessor reads the whole surface. Two constraints follow from that and
//! are the reason this module is small and defensive:
//!
//! * **No field may be read from inside a mutable borrow** of the state it
//!   reads. This surface takes no `RefCell` borrow at all, so the constraint
//!   holds by construction rather than by discipline.
//! * **A disposed widget is a stage.** GTK4 clears template children in
//!   `dispose()`, before Rust's `Drop`, so the tab view is reached through
//!   `try_get()` and the surface answers `document: None` when the child is
//!   gone. The panicking accessor is the trap here: `LushtextWindow::active_editor`
//!   derefs `imp().tab_view` directly and reads as an ordinary window operation
//!   at the call site, which is exactly how slot 5a turned a teardown
//!   observation into a crash. This module must not call it.
//!
//! Reading is side-effect free: `selected_page()` inspects an already-realized
//! `AdwTabView` and materializes nothing.

use std::path::PathBuf;

use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::ui::buffer_snapshot;
use crate::ui::editor_page::LushtextEditorPage;

use super::super::LushtextWindow;

/// Facts about the document the shell would print.
///
/// Folded in from the former standalone `PrintDocumentSnapshot`, which was a
/// second typed observation path over the same state. It is a component of the
/// surface now, not a parallel one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintDocumentFacts {
    /// Visible tab title used as print metadata.
    pub title: String,
    /// Backing path, if this document has already been saved or opened.
    pub path: Option<PathBuf>,
    /// Buffer text that should be represented by the print operation.
    ///
    /// `None` when the buffer is large enough that
    /// `char_count_requires_chunked_snapshot` says a direct capture does not
    /// belong on the GTK thread. An evidence field must be bounded, and a
    /// document-sized copy taken on every read is not — so the surface reports
    /// the size and declines the body rather than stalling a frame to observe it.
    pub content: Option<String>,
    /// Character count of the buffer, available even when `content` is not.
    pub char_count: i32,
    /// Whether the document had unsaved changes before printing began.
    pub modified: bool,
    /// Draft identity active for this tab before printing began.
    pub draft_id: Option<String>,
}

/// Everything a test or probe may observe about the print workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintEvidence {
    /// Whether `win.print` is currently enabled.
    pub action_enabled: bool,
    /// The document the shell would print, or `None` when no tab is active or
    /// the window has been disposed.
    pub document: Option<PrintDocumentFacts>,
}

/// Read the whole print surface.
#[must_use]
pub fn print_evidence(window: &LushtextWindow) -> PrintEvidence {
    PrintEvidence {
        action_enabled: window
            .lookup_action("print")
            .and_then(|action| action.downcast::<gtk4::gio::SimpleAction>().ok())
            .is_some_and(|action| action.is_enabled()),
        document: active_print_target(window)
            .as_ref()
            .and_then(document_facts),
    }
}

/// The selected editor page, reached without the panicking template accessor.
fn active_print_target(window: &LushtextWindow) -> Option<LushtextEditorPage> {
    window
        .imp()
        .tab_view
        .try_get()
        .and_then(|tab_view| tab_view.selected_page())
        .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
}

/// Capture the plain document facts for one editor page.
///
/// `None` when the editor's own `source_view` template child is gone. Both
/// convenient accessors — `editor.buffer()` and `editor.source_view()` — deref
/// that child and panic once GTK has cleared it, which is the same transitive
/// trap `active_print_target` avoids one level up. Reaching a disposed editor
/// through a still-live window is exactly what happens while a tab is torn down.
fn document_facts(editor: &LushtextEditorPage) -> Option<PrintDocumentFacts> {
    let buffer = editor.imp().source_view.try_get()?.buffer();
    let char_count = buffer.char_count();
    // Bounded: decline a document-sized copy rather than take one on every read.
    // The threshold is the shared limit `ui::buffer_snapshot` owns — called, not
    // duplicated.
    let content = (!buffer_snapshot::char_count_requires_chunked_snapshot(char_count)).then(|| {
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string()
    });

    Some(PrintDocumentFacts {
        title: editor.title(),
        path: editor.file_path(),
        content,
        char_count,
        modified: editor.is_modified(),
        draft_id: editor.draft_id(),
    })
}
