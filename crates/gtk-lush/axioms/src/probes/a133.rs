// SPDX-License-Identifier: MIT OR Apache-2.0

//! A133: `AdwTabBar:is-overflowing` notifies when the tabs cross the overflow
//! threshold, both ways (`update_is_overflowing`, `adw-tab-bar.c`, run on
//! each scroller adjustment's `changed`, which the tab box emits from its own
//! allocation). The unpinned adjustment still overflows after a three-button
//! end action widget is attached at the width where it first overflowed, and
//! attaching or removing that widget leaves the bar's height unchanged.
//! Selecting a page whose tab is out of view scrolls the strip until the tab
//! is inside the page (`select_page` → `scroll_to_tab`, `adw-tab-box.c`).
//!
//! LushText's overflow controls (`tab-strip-overflow-navigation` D1, D2)
//! sit in the end action widget, appear once the unpinned adjustment
//! overflows, and must not change the strip's height; the All Tabs popover
//! selects a page and relies on the strip bringing it into view.
//!
//! **Recorded, not a pass condition:** whether each notify ran inside the
//! bar's allocation (`notify_inside_allocation`). LushText defers its
//! reconcile either way.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

use super::tab_strip::{
    STRIP_WIDTH, TabStripFixture, WINDOW_SIZE, describe_adjustment, overflows, run_for, wait_frames,
};
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// Tabs the probe starts with: they fit the strip.
pub const START_TABS: usize = 2;
/// The most tabs the probe adds while looking for the overflow point.
pub const MAX_TABS: usize = 64;
/// Tabs the probe holds when it selects an out-of-view page.
pub const SELECTION_TABS: usize = 30;
/// A width at which every tab the probe holds fits.
pub const WIDE_STRIP: i32 = 4_000;
/// How long the probe lets a tab's appear or scroll animation run out.
const ANIMATION_SETTLE: Duration = Duration::from_millis(400);

/// One `notify::is-overflowing` the probe saw.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverflowNotify {
    /// The value it notified.
    pub overflowing: bool,
    /// Whether it ran inside the host's allocation of the bar.
    pub inside_allocation: bool,
    /// The tabs open at the time.
    pub tabs: u32,
}

/// The A133 fixture: `START_TABS` tabs in a `STRIP_WIDTH`-wide bar.
#[must_use]
pub fn fixture() -> TabStripFixture {
    TabStripFixture::new(START_TABS)
}

/// A three-button end action widget like LushText's: back, forward and a
/// count.
#[must_use]
pub fn end_group() -> gtk4::Box {
    let group = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    for icon in ["go-previous-symbolic", "go-next-symbolic"] {
        let button = gtk4::Button::from_icon_name(icon);
        button.add_css_class("flat");
        group.append(&button);
    }
    let count = gtk4::Button::with_label("128");
    count.add_css_class("flat");
    group.append(&count);
    group
}

/// Record every `notify::is-overflowing` of `fixture`'s bar.
#[must_use]
pub fn record_notifies(fixture: &TabStripFixture) -> Rc<RefCell<Vec<OverflowNotify>>> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&log);
    let host = fixture.host.downgrade();
    let view = fixture.tab_view.downgrade();
    fixture.bar.connect_is_overflowing_notify(move |bar| {
        sink.borrow_mut().push(OverflowNotify {
            overflowing: bar.is_overflowing(),
            inside_allocation: host
                .upgrade()
                .is_some_and(|host| host.is_allocating_child()),
            tabs: view
                .upgrade()
                .map_or(0, |view| u32::try_from(view.n_pages()).unwrap_or(u32::MAX)),
        });
    });
    log
}

fn settle_frames(fixture: &TabStripFixture) {
    run_for(ANIMATION_SETTLE);
    let _ = wait_frames(&fixture.bar, 2);
}

fn describe_notifies(notifies: &[OverflowNotify]) -> String {
    super::join_entries(notifies.iter().map(|notify| {
        format!(
            "{} at {} tabs, inside allocation {}",
            notify.overflowing, notify.tabs, notify.inside_allocation
        )
    }))
}

/// Probe A133.
#[must_use]
pub fn probe_a133() -> Observation {
    Recorder::run(AxiomId::new(133), |recorder| {
        let fixture = fixture();
        let _shown = Presented::checked_sized(
            recorder,
            &fixture.root,
            WINDOW_SIZE,
            "control: the tab strip's window was realized",
        )?;
        settle(super::LAYOUT_SETTLE);
        recorder.measure("bar_height", fixture.bar.height());
        let unpinned = recorder.require(
            fixture.unpinned(),
            "control: the bar has an unpinned tab scroller",
        )?;
        let adjustment = unpinned.hadjustment();
        recorder.measure("strip_width", STRIP_WIDTH);
        recorder.control(
            !fixture.bar.is_overflowing() && !overflows(&adjustment),
            "control: the starting tabs fit the strip",
        )?;
        let notifies = record_notifies(&fixture);

        // Add tabs until the bar overflows.
        while !fixture.bar.is_overflowing() && fixture.pages().len() < MAX_TABS {
            let _ = fixture.append_tab();
            settle_frames(&fixture);
        }
        recorder.measure("overflow_tabs", fixture.pages().len());
        recorder.measure("overflow_adjustment", describe_adjustment(&adjustment));
        recorder.control(
            fixture.bar.is_overflowing() && overflows(&adjustment),
            "control: adding tabs made the unpinned tabs overflow",
        )?;
        let up = notifies.borrow().clone();
        recorder.measure("notifies_up", describe_notifies(&up));
        recorder.axiom(
            up.last().is_some_and(|notify| notify.overflowing),
            "is-overflowing notifies true when the tabs cross the overflow threshold",
        )?;

        // The end widget: overflow held, height unchanged.
        let without = fixture.measured_height();
        let allocated_without = fixture.bar.height();
        let group = end_group();
        fixture.bar.set_end_action_widget(Some(&group));
        settle_frames(&fixture);
        let with = fixture.measured_height();
        let allocated_with = fixture.bar.height();
        recorder.measure("end_widget_width", group.width());
        recorder.measure("with_end_widget", describe_adjustment(&adjustment));
        recorder.measure(
            "height_min_nat_without_with",
            format!("{} {} {} {}", without.0, without.1, with.0, with.1),
        );
        recorder.measure(
            "allocated_height_without_with",
            format!("{allocated_without} {allocated_with}"),
        );
        recorder.axiom(
            overflows(&adjustment) && fixture.bar.is_overflowing(),
            "the unpinned tabs still overflow with a three-button end widget attached at the \
             width where they first overflowed",
        )?;
        recorder.axiom(
            with == without && allocated_with == allocated_without,
            "attaching the end widget leaves the bar's height unchanged",
        )?;
        fixture.bar.set_end_action_widget(None::<&gtk4::Widget>);
        settle_frames(&fixture);
        let removed = fixture.measured_height();
        let allocated_removed = fixture.bar.height();
        recorder.measure(
            "height_min_nat_allocated_removed",
            format!("{} {} {allocated_removed}", removed.0, removed.1),
        );
        recorder.axiom(
            removed == without && allocated_removed == allocated_without,
            "removing the end widget leaves the bar's height unchanged",
        )?;

        // Widen until the tabs fit again.
        let before_down = notifies.borrow().len();
        fixture.host.set_child_width(WIDE_STRIP);
        settle_frames(&fixture);
        let down: Vec<OverflowNotify> = notifies.borrow()[before_down..].to_vec();
        recorder.measure("notifies_down", describe_notifies(&down));
        recorder.axiom(
            !fixture.bar.is_overflowing() && down.last().is_some_and(|notify| !notify.overflowing),
            "is-overflowing notifies false when the tabs fit again",
        )?;
        let all = notifies.borrow().clone();
        recorder.measure(
            "notify_inside_allocation",
            super::join_entries(all.iter().map(|notify| notify.inside_allocation)),
        );

        // Selecting an out-of-view page brings its tab inside the page.
        fixture.host.set_child_width(STRIP_WIDTH);
        while fixture.pages().len() < SELECTION_TABS {
            let _ = fixture.append_tab();
        }
        settle_frames(&fixture);
        let pages = fixture.pages();
        let first = recorder.require(pages.first().cloned(), "control: the first page exists")?;
        let last = recorder.require(pages.last().cloned(), "control: the last page exists")?;
        fixture.tab_view.set_selected_page(&first);
        settle_frames(&fixture);
        adjustment.set_value(0.0);
        settle_frames(&fixture);
        recorder.measure("before_selection", describe_adjustment(&adjustment));
        recorder.control(
            overflows(&adjustment) && !TabStripFixture::tab_overlaps(&unpinned, &last),
            "control: the last tab is out of view",
        )?;
        fixture.tab_view.set_selected_page(&last);
        settle_frames(&fixture);
        recorder.measure("after_selection", describe_adjustment(&adjustment));
        recorder.measure(
            "last_tab_span",
            format!("{:?}", TabStripFixture::tab_span(&unpinned, &last)),
        );
        recorder.axiom(
            TabStripFixture::tab_inside(&unpinned, &last),
            "selecting an out-of-view page scrolls the strip until its tab is inside the page",
        )
    })
}
