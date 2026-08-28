// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — execution. Runs one print request to a `PrintOutcome`.
//!
//! This module owns the GtkSourceView `PrintCompositor` and the
//! `gtk4::PrintOperation` lifecycle, and it is where GTK's result vocabulary is
//! mapped onto the workflow's own once. Printed output follows the editor view,
//! so the compositor is built `from_view` and inherits the editor's font, tab
//! width, syntax highlighting, and wrap mode rather than re-deriving them.

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::ui::editor_page::LushtextEditorPage;

use super::super::LushtextWindow;
use super::policy::PrintOutcome;

/// Run the native print operation for one editor page.
pub(super) fn run_native_print_operation(
    window: &LushtextWindow,
    editor: &LushtextEditorPage,
) -> PrintOutcome {
    let compositor = sourceview5::PrintCompositor::from_view(editor.source_view());
    let op = gtk4::PrintOperation::new();

    // GTK calls `paginate` repeatedly until it returns true; only then is the
    // total page count known, so it is set from inside the callback rather than
    // before the run.
    let comp_paginate = compositor.clone();
    op.connect_paginate(move |op, context| {
        let done = comp_paginate.paginate(context);
        if done {
            op.set_n_pages(comp_paginate.n_pages());
        }
        done
    });

    op.connect_draw_page(move |_, context, page_nr| {
        compositor.draw_page(context, page_nr);
    });

    classify_operation_result(op.run(gtk4::PrintOperationAction::PrintDialog, Some(window)))
}

/// Map GTK's print result onto the workflow's outcome vocabulary.
fn classify_operation_result(
    result: Result<gtk4::PrintOperationResult, glib::Error>,
) -> PrintOutcome {
    match result {
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
