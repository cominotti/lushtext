// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — execution. Runs one print request to a `PrintOutcome`.
//!
//! This module owns the GtkSourceView `PrintCompositor` and the
//! `gtk4::PrintOperation` lifecycle, and it is where GTK's result vocabulary is
//! mapped onto the workflow's own once. Printed output follows the editor view:
//! the compositor takes the view's font, tab width, line numbers and wrap mode
//! and the document's syntax highlighting, as `PrintCompositor::from_view`
//! does, but over the **document** buffer: a tab never shown (preview-only
//! mode) has a view on an empty placeholder (`view_attachment`), and
//! `from_view` printed that blank page (review M4).

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
    let compositor = print_compositor(editor);
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

    // The print dialog is app-owned, modal, and followed by I/O, so it owes the
    // same suppression a file chooser does. `run` is synchronous, so the guard's
    // scope is the call itself.
    let _modal = crate::ui::window::ModalSurfaceGuard::acquire();
    classify_operation_result(op.run(gtk4::PrintOperationAction::PrintDialog, Some(window)))
}

/// The compositor for one editor page: `gtk_source_print_compositor_new_from_view`'s
/// settings, read from the view, over the document buffer.
pub(super) fn print_compositor(editor: &LushtextEditorPage) -> sourceview5::PrintCompositor {
    let view = editor.source_view();
    let buffer = editor.buffer();
    let compositor = sourceview5::PrintCompositor::builder()
        .buffer(&buffer)
        .tab_width(view.tab_width())
        .highlight_syntax(buffer.is_highlight_syntax())
        .wrap_mode(view.wrap_mode())
        .print_line_numbers(u32::from(view.shows_line_numbers()))
        .build();
    if let Some(font) = view.pango_context().font_description() {
        let font = font.to_string();
        compositor.set_body_font_name(&font);
        compositor.set_line_numbers_font_name(Some(&font));
        compositor.set_header_font_name(Some(&font));
        compositor.set_footer_font_name(Some(&font));
    }
    compositor
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
