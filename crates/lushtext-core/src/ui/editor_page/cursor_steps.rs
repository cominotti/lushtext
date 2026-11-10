// SPDX-License-Identifier: GPL-3.0-or-later

//! Arrow-key steps that lay nothing out (`adopt-gnome-51`, lane regression
//! L2).
//!
//! Cross-cutting with **no role**: it owns no decision but the one in
//! `model::cursor_step`, and serves every workflow that moves the caret.
//!
//! Left and Right emit `move-cursor` with `GTK_MOVEMENT_VISUAL_POSITIONS`,
//! whose cost is quadratic in the display line's length (ledger A107): on a
//! 10,000-character unwrapped paragraph each key press blocked the GTK thread
//! for 1.2 s, and the keys typed after it waited behind it. A connected
//! handler runs before `GtkTextView`'s class handler, so when the cursor's
//! paragraph is long and holds no right-to-left text the editor re-emits the
//! step as `GTK_MOVEMENT_LOGICAL_POSITIONS` (which lands on the same iter,
//! A107) and stops the visual emission. Everything else of the step stays
//! GTK's: extending the selection, the virtual cursor, the blink, scrolling
//! the cursor on screen.
//!
//! The rewrite is limited to the cases where both steps provably land in the
//! same paragraph: one position, an insert not at the paragraph's edge in
//! the direction of travel (the neighbouring paragraph could be
//! right-to-left), a visible cursor (a caret-free view scrolls instead), and
//! not the collapse of a selection without Shift, which GTK answers without
//! a layout pass anyway.

use gtk4::prelude::*;

use super::LushtextEditorPage;
use crate::model::cursor_step::logical_step_suffices;

/// Whether the visual step `count` from the insert of `view` should be
/// answered logically.
fn answers_logically(view: &gtk4::TextView, count: i32, extend_selection: bool) -> bool {
    if !matches!(count, 1 | -1) || !view.is_cursor_visible() {
        return false;
    }
    let buffer = view.buffer();
    if buffer.has_selection() && !extend_selection {
        return false;
    }
    let insert = buffer.iter_at_mark(&buffer.get_insert());
    if (count > 0 && insert.ends_line()) || (count < 0 && insert.starts_line()) {
        return false;
    }
    let mut start = insert;
    start.set_line_offset(0);
    let mut end = insert;
    if !end.ends_line() {
        end.forward_to_line_end();
    }
    logical_step_suffices(&buffer.slice(&start, &end, true))
}

impl LushtextEditorPage {
    /// Answer long left-to-right paragraphs' visual arrow-key steps with
    /// logical ones (from `constructed`).
    pub(super) fn setup_logical_cursor_steps(&self) {
        let view: gtk4::TextView = self.source_view().clone().upcast();
        view.connect_move_cursor(|view, step, count, extend_selection| {
            if step == gtk4::MovementStep::VisualPositions
                && answers_logically(view, count, extend_selection)
            {
                view.stop_signal_emission_by_name("move-cursor");
                view.emit_move_cursor(
                    gtk4::MovementStep::LogicalPositions,
                    count,
                    extend_selection,
                );
            }
        });
    }
}
