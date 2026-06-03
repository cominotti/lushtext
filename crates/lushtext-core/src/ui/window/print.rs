// SPDX-License-Identifier: GPL-3.0-or-later

//! Print action and its narrow test seam.
//!
//! Production printing still uses GtkSourceView's `PrintCompositor` so printed
//! output follows the editor view. The plain snapshot exists so widget tests can
//! prove the active document chosen for printing without opening a native
//! printer dialog.

#[cfg(feature = "test-utils")]
use std::cell::RefCell;
#[cfg(feature = "test-utils")]
use std::path::PathBuf;

use gtk4::gio;
use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::status_bar::MessageKind;

use super::LushtextWindow;

/// Plain document facts captured at the moment the user invokes Print.
#[cfg(feature = "test-utils")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintDocumentSnapshot {
    /// Visible tab title used as print metadata.
    pub title: String,
    /// Backing path, if this document has already been saved or opened.
    pub path: Option<PathBuf>,
    /// Buffer text that should be represented by the print operation.
    pub content: String,
    /// Whether the document had unsaved changes before printing began.
    pub modified: bool,
    /// Draft identity active for this tab before printing began.
    pub draft_id: Option<String>,
}

/// Result category returned by the production print operation or test runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintOutcome {
    /// GTK accepted or completed the print request.
    Completed,
    /// GTK handed the print request off asynchronously.
    InProgress,
    /// The user canceled the print dialog.
    Cancelled,
    /// Printing failed before the request could complete.
    Failed(String),
}

#[cfg(feature = "test-utils")]
type TestPrintRunner = Box<dyn Fn(&PrintDocumentSnapshot) -> PrintOutcome>;

#[cfg(feature = "test-utils")]
thread_local! {
    static TEST_PRINT_RUNNER: RefCell<Option<TestPrintRunner>> = RefCell::new(None);
}

/// Temporarily replace the native print operation with a test runner.
#[cfg(feature = "test-utils")]
pub fn with_print_runner_for_test<R>(
    runner: impl Fn(&PrintDocumentSnapshot) -> PrintOutcome + 'static,
    f: impl FnOnce() -> R,
) -> R {
    TEST_PRINT_RUNNER.with(|cell| {
        let previous = cell.replace(Some(Box::new(runner)));
        let result = f();
        cell.replace(previous);
        result
    })
}

/// Register the `win.print` action on the window.
///
/// The action opens a native print dialog for the active editor page,
/// using `sourceview5::PrintCompositor::from_view()` to preserve the
/// editor's font, tab width, syntax highlighting, and wrap mode.
pub(super) fn setup_print_action(window: &LushtextWindow) {
    let action = gio::SimpleAction::new("print", None);
    action.set_enabled(false);

    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let Some(editor) = window.active_editor() else {
            return;
        };

        handle_print_outcome(&window, run_print_operation(&window, &editor));
    });

    window.add_action(&action);
}

#[cfg(feature = "test-utils")]
fn snapshot_document_for_print(editor: &LushtextEditorPage) -> PrintDocumentSnapshot {
    let buffer = editor.buffer();
    PrintDocumentSnapshot {
        title: editor.title(),
        path: editor.file_path(),
        content: buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string(),
        modified: editor.is_modified(),
        draft_id: editor.draft_id(),
    }
}

fn run_print_operation(window: &LushtextWindow, editor: &LushtextEditorPage) -> PrintOutcome {
    #[cfg(feature = "test-utils")]
    {
        let snapshot = snapshot_document_for_print(editor);
        if let Some(outcome) =
            TEST_PRINT_RUNNER.with(|cell| cell.borrow().as_ref().map(|runner| runner(&snapshot)))
        {
            return outcome;
        }
    }

    run_native_print_operation(window, editor)
}

fn run_native_print_operation(
    window: &LushtextWindow,
    editor: &LushtextEditorPage,
) -> PrintOutcome {
    let view = editor.source_view();
    let compositor = sourceview5::PrintCompositor::from_view(view);

    let op = gtk4::PrintOperation::new();

    // Paginate the compositor iteratively. GTK calls this repeatedly until it
    // returns true. Once done, set the total page count.
    let comp_paginate = compositor.clone();
    op.connect_paginate(move |op, context| {
        let done = comp_paginate.paginate(context);
        if done {
            op.set_n_pages(comp_paginate.n_pages());
        }
        done
    });

    // Render each page via the compositor.
    op.connect_draw_page(move |_, context, page_nr| {
        compositor.draw_page(context, page_nr);
    });

    match op.run(gtk4::PrintOperationAction::PrintDialog, Some(window)) {
        Ok(gtk4::PrintOperationResult::Apply) => PrintOutcome::Completed,
        Ok(gtk4::PrintOperationResult::InProgress) => PrintOutcome::InProgress,
        Ok(gtk4::PrintOperationResult::Cancel) => PrintOutcome::Cancelled,
        Ok(gtk4::PrintOperationResult::Error) => {
            PrintOutcome::Failed("print operation reported an error".to_string())
        }
        Ok(_) => PrintOutcome::Failed("print operation returned an unexpected result".to_string()),
        Err(error) => PrintOutcome::Failed(error.to_string()),
    }
}

fn handle_print_outcome(window: &LushtextWindow, outcome: PrintOutcome) {
    match outcome {
        PrintOutcome::Completed | PrintOutcome::InProgress | PrintOutcome::Cancelled => {}
        PrintOutcome::Failed(message) => {
            tracing::error!("Print failed: {message}");
            window.publish_status_message(&format!("Print failed: {message}"), MessageKind::Error);
        }
    }
}
