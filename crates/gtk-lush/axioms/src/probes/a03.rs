// SPDX-License-Identifier: MIT OR Apache-2.0

//! A3: a `GtkViewport` allocates a non-scrollable child its **minimum** in
//! the scroll direction (its default `vscroll-policy` is `minimum`). This is
//! why `ViewportSliceBin::measure` reports the full content as its minimum
//! while an outer scroller exists: a smaller minimum truncates the outer
//! range.

use gtk4::prelude::*;

use super::LAYOUT_SETTLE;
use crate::fixtures::FixedHost;
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The vertical minimum the host reports.
pub(crate) const REPORTED_MINIMUM: i32 = 2_000;
/// The vertical natural size the host reports, well above the minimum.
pub(crate) const REPORTED_NATURAL: i32 = 5_000;

/// The A3 fixture: a [`FixedHost`] reporting a minimum and a larger natural
/// height, inside a scroller that wraps it in a `GtkViewport`.
#[derive(Clone, Debug)]
pub struct ViewportedHost {
    /// The scroller.
    pub scroller: gtk4::ScrolledWindow,
    /// The viewport the scroller created around the host.
    pub viewport: Option<gtk4::Viewport>,
    /// The non-scrollable host.
    pub host: FixedHost,
}

impl ViewportedHost {
    /// Build the fixture.
    #[must_use]
    pub fn new() -> Self {
        let label = gtk4::Label::new(Some("minimum 2000 px, natural 5000 px"));
        let host = FixedHost::new(&label, REPORTED_MINIMUM);
        host.report_vertical_size(REPORTED_MINIMUM, REPORTED_NATURAL);
        let scroller = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .child(&host)
            .build();
        let viewport = scroller.child().and_downcast::<gtk4::Viewport>();
        Self {
            scroller,
            viewport,
            host,
        }
    }
}

impl Default for ViewportedHost {
    fn default() -> Self {
        Self::new()
    }
}

/// Probe A3. See the module documentation.
#[must_use]
pub fn probe_a03() -> Observation {
    Recorder::run(AxiomId::new(3), |recorder| {
        let fixture = ViewportedHost::new();
        let shown = Presented::new(&fixture.scroller);
        recorder.control(shown.realized(), "control: the fixture window realizes")?;
        let Some(viewport) = fixture.viewport.clone() else {
            return recorder.control(false, "control: the scroller wraps the host in a viewport");
        };
        let default_policy = viewport.vscroll_policy();
        let default_height = fixture.host.height();
        let default_upper = fixture.scroller.vadjustment().upper();
        recorder.measure("default_policy", format!("{default_policy:?}"));
        recorder.measure("default_policy_height", default_height);
        recorder.measure("default_policy_upper", default_upper);

        // Control: the fixture distinguishes the two sizes — under the
        // natural policy the same host is allocated its natural height.
        viewport.set_vscroll_policy(gtk4::ScrollablePolicy::Natural);
        settle(LAYOUT_SETTLE);
        recorder.measure("natural_policy_height", fixture.host.height());
        recorder.control(
            fixture.host.height() == REPORTED_NATURAL,
            "control: under the natural policy the host gets its natural height",
        )?;
        viewport.set_vscroll_policy(default_policy);
        settle(LAYOUT_SETTLE);

        recorder.axiom(
            default_policy == gtk4::ScrollablePolicy::Minimum,
            "A3: a viewport's default vertical scroll policy is minimum",
        )?;
        recorder.axiom(
            default_height == REPORTED_MINIMUM,
            "A3: under the default policy the viewport allocates the child its minimum",
        )?;
        recorder.axiom(
            (default_upper - f64::from(REPORTED_MINIMUM)).abs() < f64::EPSILON,
            "A3: the scroller's range is the child's minimum, not its natural height",
        )
    })
}
