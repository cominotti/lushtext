// SPDX-License-Identifier: MIT OR Apache-2.0

//! A27: applying or removing a tag does not dirty a `GtkTextBuffer`. It emits
//! no `changed`, leaves `modified` unset, and records no undo history
//! (`can-undo` stays false), with undo enabled.
//!
//! The protected view (C2) folds and unfolds by tagging, and relies on that
//! leaving a document clean, unsaved and un-undoable.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, invisible_tag, paragraph};
use crate::{AxiomId, Observation};

/// Characters in the probe paragraph.
pub const CHARS: usize = 10_000;

/// Probe A27.
#[must_use]
pub fn probe_a27() -> Observation {
    Recorder::run(AxiomId::new(27), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let buffer = gtk4::TextBuffer::new(None);
        buffer.set_enable_undo(true);
        buffer.set_text(&paragraph(ParagraphKind::Ascii, CHARS, 27));
        buffer.set_modified(false);
        let tag = invisible_tag(&buffer);
        let changes = Rc::new(Cell::new(0u32));
        let counter = changes.clone();
        buffer.connect_changed(move |_| counter.set(counter.get() + 1));
        let modified_notifies = Rc::new(Cell::new(0u32));
        let notifier = modified_notifies.clone();
        buffer.connect_modified_changed(move |_| notifier.set(notifier.get() + 1));
        recorder.measure("enable_undo", buffer.enables_undo());
        recorder.measure("can_undo_before", buffer.can_undo());
        recorder.control(
            buffer.enables_undo() && !buffer.can_undo() && !buffer.is_modified(),
            "control: undo is enabled and the buffer starts clean with no history",
        )?;

        let start = buffer.iter_at_offset(100);
        let end = buffer.end_iter();
        buffer.apply_tag(&tag, &start, &end);
        let after_apply = (changes.get(), buffer.is_modified(), buffer.can_undo());
        buffer.remove_tag(&tag, &start, &end);
        let after_remove = (changes.get(), buffer.is_modified(), buffer.can_undo());
        recorder.measure("changed_after_apply", after_apply.0);
        recorder.measure("modified_after_apply", after_apply.1);
        recorder.measure("can_undo_after_apply", after_apply.2);
        recorder.measure("changed_after_remove", after_remove.0);
        recorder.measure("modified_after_remove", after_remove.1);
        recorder.measure("can_undo_after_remove", after_remove.2);
        recorder.measure("modified_changed_emissions", modified_notifies.get());

        let mut at = buffer.end_iter();
        buffer.insert(&mut at, "x");
        recorder.measure("changed_after_control_insert", changes.get());
        recorder.measure("can_undo_after_control_insert", buffer.can_undo());
        recorder.control(
            changes.get() == 1 && buffer.can_undo() && buffer.is_modified(),
            "control: a character insert does emit changed, dirty the buffer and record undo",
        )?;
        recorder.axiom(
            after_apply.0 == 0 && after_remove.0 == 0,
            "tag apply and remove emit no changed",
        )?;
        recorder.axiom(
            !after_apply.1 && !after_remove.1,
            "tag apply and remove leave modified unset",
        )?;
        recorder.axiom(
            !after_apply.2 && !after_remove.2,
            "tag apply and remove record no undo history",
        )
    })
}
