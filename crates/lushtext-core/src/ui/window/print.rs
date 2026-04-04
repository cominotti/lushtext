// SPDX-License-Identifier: GPL-3.0-or-later

//! Print action: uses GtkSourceView's PrintCompositor to print the active
//! editor's content via the native GTK print dialog.

use gtk4::gio;
use gtk4::prelude::*;
use sourceview5::prelude::*;

/// Register the `win.print` action on the window.
///
/// The action opens a native print dialog for the active editor page,
/// using `sourceview5::PrintCompositor::from_view()` to preserve the
/// editor's font, tab width, syntax highlighting, and wrap mode.
pub(super) fn setup_print_action(window: &super::LushtextWindow) {
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

        let view = editor.source_view();
        let compositor = sourceview5::PrintCompositor::from_view(view);

        let op = gtk4::PrintOperation::new();

        // Paginate the compositor iteratively. GTK calls this repeatedly
        // until it returns true. Once done, set the total page count.
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

        if let Err(err) = op.run(gtk4::PrintOperationAction::PrintDialog, Some(&window)) {
            tracing::error!("Print failed: {err}");
        }
    });

    window.add_action(&action);
}
