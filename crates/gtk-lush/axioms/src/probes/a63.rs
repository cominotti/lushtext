// SPDX-License-Identifier: MIT OR Apache-2.0

//! A63: what drag and drop a `GtkTextView` takes part in, and what turns it
//! off. The view installs exactly one `GtkDropTarget` (string, copy or move)
//! and one `GtkDropControllerMotion` (drag-hover autoscroll), and **no**
//! `GtkDragSource`: a drag out of the view starts from its own
//! `GtkGestureDrag` (`gtk_text_view_start_selection_dnd`, reached only from
//! a press inside the selection), which A59's capture claim keeps from
//! running. The drop handler inserts with `gtk_text_buffer_insert_interactive`
//! and the view's `editable` as the default, so a non-editable view refuses a
//! drop (the handler returns false, the buffer is unchanged) **unless a tag
//! marks the drop point editable**, and its motion handler offers no action.
//! Removing the drop target with `gtk_widget_remove_controller` leaves the
//! view with none, so no drop reaches its handler at all.
//!
//! The probe drives the drop target's own `drop` and `motion` signals, which
//! is what GTK's drop dispatch emits; a real cross-surface drag is not
//! driven.
//!
//! Protected View (C2) removes the drop target on entry and relies on the
//! capture claim for drag-out (design D3, step 4); it never applies an
//! editable tag.

use gtk4::glib;
use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::TextViewFixture;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// The text the probe drops.
pub const DROPPED: &str = "DROPPED";
/// The probe document.
pub const DOCUMENT: &str = "first line of the document\nsecond line\n";

/// The drag-and-drop controllers a widget carries.
#[derive(Clone, Debug, Default)]
pub struct DndCensus {
    /// `GtkDropTarget`s.
    pub drop_targets: Vec<gtk4::DropTarget>,
    /// `GtkDropControllerMotion`s.
    pub drop_motion: u32,
    /// `GtkDragSource`s.
    pub drag_sources: u32,
    /// `GtkDropTargetAsync`s.
    pub async_targets: u32,
}

/// Count `widget`'s drag-and-drop controllers.
#[must_use]
pub fn dnd_census(widget: &impl IsA<gtk4::Widget>) -> DndCensus {
    let controllers = widget.observe_controllers();
    let mut census = DndCensus::default();
    for controller in (0..controllers.n_items()).filter_map(|index| controllers.item(index)) {
        if let Some(target) = controller.downcast_ref::<gtk4::DropTarget>() {
            census.drop_targets.push(target.clone());
        } else if controller.is::<gtk4::DropControllerMotion>() {
            census.drop_motion += 1;
        } else if controller.is::<gtk4::DragSource>() {
            census.drag_sources += 1;
        } else if controller.is::<gtk4::DropTargetAsync>() {
            census.async_targets += 1;
        }
    }
    census
}

/// A view over [`DOCUMENT`].
#[must_use]
pub fn dnd_view() -> TextViewFixture {
    let fixture = TextViewFixture::new(gtk4::WrapMode::WordChar);
    fixture.buffer.set_text(DOCUMENT);
    fixture
}

/// Emit `target`'s `drop` with [`DROPPED`] at (`x`, `y`), as GTK's drop
/// dispatch does. Returns what the handler answered.
#[must_use]
pub fn drop_text(target: &gtk4::DropTarget, x: f64, y: f64) -> bool {
    target.emit_by_name::<bool>("drop", &[&glib::BoxedValue(DROPPED.to_value()), &x, &y])
}

/// Emit `target`'s `motion` at (`x`, `y`). Returns the action offered.
pub fn hover(target: &gtk4::DropTarget, x: f64, y: f64) -> gtk4::gdk::DragAction {
    target.emit_by_name::<gtk4::gdk::DragAction>("motion", &[&x, &y])
}

fn text(fixture: &TextViewFixture) -> String {
    let (start, end) = fixture.buffer.bounds();
    fixture.buffer.text(&start, &end, true).to_string()
}

/// Probe A63.
#[must_use]
pub fn probe_a63() -> Observation {
    Recorder::run(AxiomId::new(63), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = dnd_view();
        let _shown = Presented::checked(
            recorder,
            &fixture.scrolled,
            "control: the text view was presented",
        )?;
        let census = dnd_census(&fixture.view);
        recorder.measure(
            "census",
            format!(
                "drop targets {}, drop-motion {}, drag sources {}, async targets {}",
                census.drop_targets.len(),
                census.drop_motion,
                census.drag_sources,
                census.async_targets
            ),
        );
        let target = recorder.require(
            census.drop_targets.first().cloned(),
            "control: the view has a drop target",
        )?;
        recorder.measure(
            "drop_target",
            format!(
                "actions {:?}, formats {}",
                target.actions(),
                target
                    .formats()
                    .map_or_else(String::new, |formats| formats.to_str().to_string())
            ),
        );
        recorder.axiom(
            census.drop_targets.len() == 1
                && census.drop_motion == 1
                && census.drag_sources == 0
                && census.async_targets == 0,
            "the view carries one drop target, one drop-motion controller and no drag source",
        )?;

        let (x, y) = (5.0, 5.0);
        let editable_hover = hover(&target, x, y);
        let editable_drop = drop_text(&target, x, y);
        settle(LAYOUT_SETTLE);
        let editable_text = text(&fixture);
        recorder.measure("editable_hover", format!("{editable_hover:?}"));
        recorder.measure("editable_drop", editable_drop);
        recorder.control(
            editable_drop && editable_text.contains(DROPPED),
            "control: an editable view inserts a dropped string",
        )?;

        fixture.buffer.set_text(DOCUMENT);
        fixture.view.set_editable(false);
        settle(LAYOUT_SETTLE);
        let protected_hover = hover(&target, x, y);
        let protected_drop = drop_text(&target, x, y);
        settle(LAYOUT_SETTLE);
        let protected_text = text(&fixture);
        recorder.measure("non_editable_hover", format!("{protected_hover:?}"));
        recorder.measure("non_editable_drop", protected_drop);

        let editable_tag = gtk4::TextTag::builder().editable(true).build();
        fixture.buffer.tag_table().add(&editable_tag);
        let (start, end) = fixture.buffer.bounds();
        fixture.buffer.apply_tag(&editable_tag, &start, &end);
        let tagged_drop = drop_text(&target, x, y);
        settle(LAYOUT_SETTLE);
        let tagged_text = text(&fixture);
        recorder.measure("non_editable_drop_on_editable_tag", tagged_drop);
        let (start, end) = fixture.buffer.bounds();
        fixture.buffer.remove_tag(&editable_tag, &start, &end);
        fixture.buffer.set_text(DOCUMENT);

        fixture.view.remove_controller(&target);
        settle(LAYOUT_SETTLE);
        let removed = dnd_census(&fixture.view);
        recorder.measure(
            "census_after_removal",
            format!(
                "drop targets {}, drop-motion {}",
                removed.drop_targets.len(),
                removed.drop_motion
            ),
        );
        recorder.axiom(
            !protected_drop && protected_text == DOCUMENT && protected_hover.is_empty(),
            "a non-editable view refuses a drop, leaves the buffer unchanged and offers no action",
        )?;
        recorder.axiom(
            tagged_drop && tagged_text.contains(DROPPED),
            "a tag marking the drop point editable overrides the view's editable=false",
        )?;
        recorder.axiom(
            removed.drop_targets.is_empty(),
            "removing the drop target leaves the view with none",
        )
    })
}
