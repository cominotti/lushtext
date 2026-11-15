// SPDX-License-Identifier: MIT OR Apache-2.0

//! A114: an application CSS rule on a class of `GtkTreeExpander`
//! ([`COLLAPSING_CSS`]) makes the arrow of its `expander:checked` icon draw
//! exactly as the collapsed arrow, in both text directions, under the shipped
//! Adwaita style, while the row stays expanded and the icon keeps its
//! `:checked` state.
//!
//! The probe renders the icon of an expanded row with the class, the icon of
//! the same row collapsed, and (as the control that the comparison can fail)
//! the expanded icon without the class, pixel for pixel, in LTR and RTL.
//!
//! LushText's sidebar (`slice-large-directory-collapse` D6) shows a sliced
//! collapse in the input's first frame this way: the row stays
//! GTK-expanded until its last step, and only the arrow says it is
//! collapsing.

use std::time::Duration;

use gtk4::prelude::*;

use super::a112::tree_fixture;
use crate::observation::Recorder;
use crate::session::{flush_events, settle};
use crate::{AxiomId, Observation};

/// The class LushText adds to a collapsing row's expander.
pub const COLLAPSING_CLASS: &str = "lushtext-collapsing";

/// The rule LushText ships (`resources/style/style.css`), at application
/// priority: the checked arrow of a collapsing expander draws as the
/// collapsed one. Adwaita does not turn the arrow: its `expander:checked`
/// swaps the icon source to `pan-down-symbolic` (the collapsed arrow is
/// `pan-end-symbolic`, `pan-end-symbolic-rtl` in RTL), so the rule sets the
/// collapsed source back, per direction.
pub const COLLAPSING_CSS: &str = "treeexpander.lushtext-collapsing > expander:checked { \
                                  -gtk-icon-source: -gtk-icontheme(\"pan-end-symbolic\"); } \
                                  treeexpander.lushtext-collapsing > expander:checked:dir(rtl) { \
                                  -gtk-icon-source: -gtk-icontheme(\"pan-end-symbolic-rtl\"); }";

/// How long the probe waits for an arrow transition to end.
pub const TRANSITION_SETTLE: Duration = Duration::from_millis(600);

/// The pixels of `widget` drawn by its window's renderer, and their size.
#[must_use]
pub fn rendered_pixels(widget: &impl IsA<gtk4::Widget>) -> Option<(Vec<u8>, i32, i32)> {
    let renderer = widget.native()?.renderer()?;
    let (width, height) = (widget.width(), widget.height());
    if width <= 0 || height <= 0 {
        return None;
    }
    let paintable = gtk4::WidgetPaintable::new(Some(widget));
    let snapshot = gtk4::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
    let node = snapshot.to_node()?;
    let texture = renderer.render_texture(&node, None);
    let stride = usize::try_from(texture.width()).ok()? * 4;
    let mut bytes = vec![0; stride * usize::try_from(texture.height()).ok()?];
    texture.download(&mut bytes, stride);
    Some((bytes, texture.width(), texture.height()))
}

/// Probe A114. See the module documentation.
#[must_use]
pub fn probe_a114() -> Observation {
    Recorder::run(AxiomId::new(114), |recorder| {
        let display = recorder.require(gtk4::gdk::Display::default(), "control: display")?;
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(COLLAPSING_CSS);
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let fixture = tree_fixture(4, 0);
        let shown = fixture.present();
        recorder.control(shown.realized(), "control: the window received a size")?;
        let row = recorder.require(
            fixture.tree.item(0).and_downcast::<gtk4::TreeListRow>(),
            "control: the directory row",
        )?;
        let expander = recorder.require(
            fixture
                .list
                .first_child()
                .and_then(|row| row.first_child())
                .and_downcast::<gtk4::TreeExpander>(),
            "control: the directory row is bound",
        )?;
        let mut held = true;
        for direction in [gtk4::TextDirection::Ltr, gtk4::TextDirection::Rtl] {
            expander.set_direction(direction);
            let icon_now = || super::a113::expander_icon(&expander);
            let draw = |expanded: bool, class: bool| {
                row.set_expanded(expanded);
                if class {
                    expander.add_css_class(COLLAPSING_CLASS);
                } else {
                    expander.remove_css_class(COLLAPSING_CLASS);
                }
                flush_events();
                if let Some(icon) = icon_now() {
                    icon.set_direction(direction);
                }
                settle(TRANSITION_SETTLE);
                icon_now().and_then(|icon| rendered_pixels(&icon))
            };
            let collapsed = draw(false, false);
            let checked = draw(true, false);
            let collapsing = draw(true, true);
            let still_expanded = row.is_expanded();
            let still_checked = icon_now()
                .is_some_and(|icon| icon.state_flags().contains(gtk4::StateFlags::CHECKED));
            let name = format!("{direction:?}");
            recorder.measure(
                "direction(collapsed_is_checked,collapsing_is_collapsed,expanded,checked)",
                format!(
                    "{name}: {},{},{still_expanded},{still_checked}",
                    collapsed.is_some() && collapsed == checked,
                    collapsed.is_some() && collapsed == collapsing
                ),
            );
            recorder.control(
                collapsed.is_some() && checked.is_some() && collapsed != checked,
                "control: the checked arrow draws differently from the collapsed one",
            )?;
            held &= collapsed == collapsing && still_expanded && still_checked;
            expander.remove_css_class(COLLAPSING_CLASS);
        }
        gtk4::style_context_remove_provider_for_display(&display, &provider);
        drop(shown);
        flush_events();
        recorder.axiom(
            held,
            "A114: a class rule draws the checked arrow as the collapsed one, in both \
             directions, without changing the row's expanded or the icon's checked state",
        )
    })
}
