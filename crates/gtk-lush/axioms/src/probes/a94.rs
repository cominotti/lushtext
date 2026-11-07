// SPDX-License-Identifier: MIT OR Apache-2.0

//! A94: section headers made by a `GtkListView`'s `header-factory` are not
//! keyboard focus stops: the header widget is not focusable, and moving focus
//! down from the last row of a section lands on the first row of the next
//! one. GTK gives the header widget the accessible role `row-header`
//! (`GtkListHeaderWidget`); the probe records it.
//!
//! LushText's notes browser (`bound-notes-browser-by-budget` D9) therefore
//! speaks a row's section in that row's accessible description rather than
//! relying on a screen reader announcing the header as focus passes it.

use gtk4::prelude::*;

use super::lazy_sections::{CountingSectionModel, children_named, sectioned_list};
use crate::observation::Recorder;
use crate::session::{Presented, flush_events};
use crate::{AxiomId, Observation};

/// Rows in the fixture.
pub const ROWS: u32 = 12;
/// Rows per section: three sections.
pub const SECTION_LEN: u32 = 4;

/// What the probe saw: headers, focusable headers, the header's accessible
/// role, and the label focused after moving down from a section's last row.
#[must_use]
pub fn observe_headers() -> (usize, usize, String, Option<String>) {
    let model = CountingSectionModel::new(ROWS, SECTION_LEN);
    let (scroller, list, _selection) = sectioned_list(&model);
    let shown = Presented::detached(&scroller);
    flush_events();
    let headers = children_named(&list, "header");
    let focusable = headers
        .iter()
        .filter(|header| header.is_focusable())
        .count();
    let role = headers
        .first()
        .map(|header| format!("{:?}", header.accessible_role()))
        .unwrap_or_default();
    list.scroll_to(SECTION_LEN - 1, gtk4::ListScrollFlags::FOCUS, None);
    flush_events();
    let _ = list.child_focus(gtk4::DirectionType::Down);
    flush_events();
    let focused = gtk4::prelude::RootExt::focus(shown.window())
        .and_then(|focus| {
            let mut current = Some(focus);
            while let Some(widget) = current {
                if widget.css_name() == "row" || widget.css_name() == "header" {
                    return Some(widget);
                }
                current = widget.parent();
            }
            None
        })
        .map(|widget| {
            let label = widget
                .first_child()
                .and_downcast::<gtk4::Label>()
                .map(|label| label.text().to_string())
                .unwrap_or_default();
            format!("{}:{label}", widget.css_name())
        });
    (headers.len(), focusable, role, focused)
}

/// Probe A94. See the module documentation.
#[must_use]
pub fn probe_a94() -> Observation {
    Recorder::run(AxiomId::new(94), |recorder| {
        let (headers, focusable, role, focused) = observe_headers();
        recorder.measure("headers", headers);
        recorder.measure("focusable_headers", focusable);
        recorder.measure("header_role", &role);
        recorder.measure("focused_after_down", focused.clone().unwrap_or_default());
        recorder.control(headers > 0, "control: the header factory made headers")?;
        recorder.axiom(focusable == 0, "A94: a section header is not focusable")?;
        recorder.axiom(
            focused.as_deref() == Some(&format!("row:row {SECTION_LEN}")),
            "A94: moving down from a section's last row focuses the next section's first row",
        )
    })
}
