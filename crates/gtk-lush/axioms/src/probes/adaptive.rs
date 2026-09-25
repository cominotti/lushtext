// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the breakpoint and split-view probes (A14–A18) share: a text-scale
//! override on the default `GtkSettings`, and an `AdwBreakpointBin` allocated
//! at an exact width.
//!
//! `AdwWindow` and `AdwApplicationWindow` evaluate their breakpoints through
//! an internal `AdwBreakpointBin` wrapped around their content, so a bin
//! allocated at an exact width shows the same evaluation a window of that
//! width performs, without asking a compositor for a window size at every
//! step. The A14 probe checks that equivalence at the boundary with real
//! windows.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use libadwaita::prelude::*;

use super::{frame_of, join_entries};
use crate::fixtures::FixedHost;
use crate::session::wait_until;

/// `gtk-xft-dpi` at text scale 1.0: 96 dpi in 1/1024 units.
pub const BASE_XFT_DPI: i32 = 96 * 1024;
/// The label the setter target shows before any breakpoint applies.
pub const ORIGINAL_LABEL: &str = "original";
/// The value the A14–A16 probes' breakpoint setter writes.
pub const APPLIED_LABEL: &str = "applied";
/// The minimum width the probe bin requests. `AdwBreakpointBin` needs an
/// explicit minimum size, and every probe allocates it at least this wide.
pub const BIN_MIN_WIDTH: i32 = 100;
/// The minimum height the probe bin requests.
pub(crate) const BIN_MIN_HEIGHT: i32 = 80;
/// The height the host allocates the bin.
pub(crate) const BIN_HEIGHT: i32 = 200;
/// The height the bin's content allocates its label.
pub(crate) const LABEL_HEIGHT: i32 = 40;
/// How long a probe waits for one allocation after it changed a width.
pub(crate) const ALLOCATION_BUDGET: Duration = Duration::from_secs(2);

/// The `gtk-xft-dpi` value for a text scale given in permille
/// (`1250` is 1.25).
#[must_use]
pub const fn xft_dpi_for_scale(scale_permille: i32) -> i32 {
    BASE_XFT_DPI * scale_permille / 1000
}

/// Set the default `GtkSettings` text scale (`gtk-xft-dpi`), in permille.
/// Does nothing when there is no default settings object.
pub fn set_text_scale(scale_permille: i32) {
    if let Some(settings) = gtk4::Settings::default() {
        write_text_scale(&settings, scale_permille);
    }
}

/// Write `settings`' text scale (`gtk-xft-dpi`), in permille: the one writer
/// behind [`set_text_scale`] and [`SettingsOverride::set_text_scale`].
fn write_text_scale(settings: &gtk4::Settings, scale_permille: i32) {
    settings.set_gtk_xft_dpi(xft_dpi_for_scale(scale_permille));
}

/// Overrides of the default `GtkSettings` a probe needs, reset to the
/// session's own values when dropped, so a probe run in a longer-lived
/// process leaves no trace.
#[derive(Debug)]
pub(crate) struct SettingsOverride {
    settings: Option<gtk4::Settings>,
    text_scale: Cell<bool>,
    animations: Cell<bool>,
}

impl SettingsOverride {
    /// Start overriding nothing yet.
    pub(crate) fn new() -> Self {
        Self {
            settings: gtk4::Settings::default(),
            text_scale: Cell::new(false),
            animations: Cell::new(false),
        }
    }

    /// Whether a default settings object exists to override.
    pub(crate) fn available(&self) -> bool {
        self.settings.is_some()
    }

    /// The current `gtk-xft-dpi`.
    pub(crate) fn xft_dpi(&self) -> Option<i32> {
        self.settings.as_ref().map(gtk4::Settings::gtk_xft_dpi)
    }

    /// Set the text scale, in permille.
    pub(crate) fn set_text_scale(&self, scale_permille: i32) {
        if let Some(settings) = self.settings.as_ref() {
            write_text_scale(settings, scale_permille);
            self.text_scale.set(true);
        }
    }

    /// Turn toolkit animations off, so an animated property reaches its end
    /// state in the frame it changes.
    pub(crate) fn disable_animations(&self) {
        if let Some(settings) = self.settings.as_ref() {
            settings.set_gtk_enable_animations(false);
            self.animations.set(true);
        }
    }

    /// `value` sp in pixels, as Libadwaita converts it under these settings.
    pub(crate) fn sp_to_px(&self, value: f64) -> f64 {
        libadwaita::LengthUnit::Sp.to_px(value, self.settings.as_ref())
    }
}

impl Drop for SettingsOverride {
    fn drop(&mut self) {
        if let Some(settings) = self.settings.as_ref() {
            if self.text_scale.get() {
                settings.reset_property("gtk-xft-dpi");
            }
            if self.animations.get() {
                settings.reset_property("gtk-enable-animations");
            }
        }
    }
}

/// The condition string `max-width: <sp>sp`.
#[must_use]
pub fn max_width_sp(sp: i32) -> String {
    format!("max-width: {sp}sp")
}

/// Parse `max-width: <sp>sp`. The string is well formed by construction.
pub(crate) fn max_width_condition(sp: i32) -> Option<libadwaita::BreakpointCondition> {
    libadwaita::BreakpointCondition::parse(&max_width_sp(sp)).ok()
}

/// What a breakpoint's `apply` or `unapply` handler saw when it ran.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SignalSite {
    /// How many times the signal has fired.
    pub count: u32,
    /// The content's allocation count when it last fired.
    pub content_allocations: u32,
    /// Whether it last fired while the host was allocating the bin.
    pub inside_bin_allocation: bool,
    /// Whether the setter target already showed the applied value then.
    pub label_was_applied_value: bool,
    /// The bin's frame-clock frame counter when it last fired (`-1` without
    /// a frame clock).
    pub frame: i64,
}

/// What the content saw as one of its allocations began.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AllocationSight {
    /// The label's text.
    pub label: String,
    /// The frame-clock frame counter (`-1` without a frame clock).
    pub frame: i64,
}

/// The sights as `label@frame` entries, for an observation.
pub(crate) fn describe_sights(sights: &[AllocationSight]) -> String {
    join_entries(
        sights
            .iter()
            .map(|sight| format!("{}@{}", sight.label, sight.frame)),
    )
}

/// The A14–A16 and A18 fixture: an `AdwBreakpointBin` that a [`FixedHost`]
/// allocates at an exact width, whose child is a second [`FixedHost`]
/// (counting its own allocations) around a label that the breakpoints'
/// setters write.
#[derive(Clone, Debug)]
pub struct BreakpointFixture {
    /// Allocates the bin at the width a probe chooses.
    pub host: FixedHost,
    /// The bin under observation.
    pub bin: libadwaita::BreakpointBin,
    /// The bin's child; counts its allocations.
    pub content: FixedHost,
    /// The property every setter writes is this label's `label`.
    pub label: gtk4::Label,
    /// What the content saw as each of its allocations began.
    seen_at_allocation: Rc<RefCell<Vec<AllocationSight>>>,
}

impl BreakpointFixture {
    /// Build the fixture, the bin allocated `width` pixels wide.
    #[must_use]
    pub fn new(width: i32) -> Self {
        let label = gtk4::Label::new(Some(ORIGINAL_LABEL));
        let content = FixedHost::new(&label, LABEL_HEIGHT);
        let bin = libadwaita::BreakpointBin::builder()
            .width_request(BIN_MIN_WIDTH)
            .height_request(BIN_MIN_HEIGHT)
            .child(&content)
            .build();
        let host = FixedHost::new(&bin, BIN_HEIGHT);
        host.set_child_width(width);
        let seen_at_allocation = Rc::new(RefCell::new(Vec::new()));
        let seen = seen_at_allocation.clone();
        let weak_label = label.downgrade();
        content.set_during_allocation(move || {
            if let Some(label) = weak_label.upgrade() {
                seen.borrow_mut().push(AllocationSight {
                    label: label.text().to_string(),
                    frame: frame_of(&label),
                });
            }
        });
        Self {
            host,
            bin,
            content,
            label,
            seen_at_allocation,
        }
    }

    /// Add a `max-width: <sp>sp` breakpoint whose one setter writes `value`
    /// into the label. Returns `None` if the condition did not parse.
    #[must_use]
    pub fn add_max_width_breakpoint(&self, sp: i32, value: &str) -> Option<libadwaita::Breakpoint> {
        let breakpoint = self.max_width_breakpoint(sp, value)?;
        self.bin.add_breakpoint(breakpoint.clone());
        Some(breakpoint)
    }

    /// A `max-width: <sp>sp` breakpoint whose one setter writes `value` into
    /// the label, not yet installed on the bin.
    #[must_use]
    pub fn max_width_breakpoint(&self, sp: i32, value: &str) -> Option<libadwaita::Breakpoint> {
        let breakpoint = libadwaita::Breakpoint::new(max_width_condition(sp)?);
        breakpoint.add_setter(&self.label, "label", Some(&value.to_value()));
        Some(breakpoint)
    }

    /// Replace `breakpoint`'s condition with `max-width: <sp>sp`.
    pub fn set_max_width(breakpoint: &libadwaita::Breakpoint, sp: i32) {
        breakpoint.set_condition(max_width_condition(sp).as_ref());
    }

    /// Allocate the bin `width` pixels wide and wait for that allocation.
    /// Returns whether the host was allocated again within the budget.
    #[must_use]
    pub fn allocate_at(&self, width: i32) -> bool {
        let before = self.host.allocations();
        self.host.set_child_width(width);
        wait_until(ALLOCATION_BUDGET, || self.host.allocations() > before)
    }

    /// The bin's frame-clock frame counter now (`-1` before it has one).
    #[must_use]
    pub fn frame(&self) -> i64 {
        frame_of(&self.bin)
    }

    /// Whether `breakpoint` is the bin's current breakpoint.
    #[must_use]
    pub fn is_current(&self, breakpoint: &libadwaita::Breakpoint) -> bool {
        self.bin.current_breakpoint().as_ref() == Some(breakpoint)
    }

    /// What the content saw as its allocations began, oldest first,
    /// clearing the record.
    pub(crate) fn take_seen_at_allocation(&self) -> Vec<AllocationSight> {
        self.seen_at_allocation.take()
    }

    /// Record where `breakpoint`'s `apply` (or, with `unapply`, its
    /// `unapply`) handler runs, and what the label shows then compared with
    /// `applied_value`.
    pub(crate) fn watch_signal(
        &self,
        breakpoint: &libadwaita::Breakpoint,
        unapply: bool,
        applied_value: &'static str,
    ) -> Rc<Cell<SignalSite>> {
        let site = Rc::new(Cell::new(SignalSite::default()));
        let record = site.clone();
        let host = self.host.downgrade();
        let content = self.content.downgrade();
        let label = self.label.downgrade();
        let handler = move |_: &libadwaita::Breakpoint| {
            let previous = record.get();
            record.set(SignalSite {
                count: previous.count + 1,
                content_allocations: content.upgrade().map_or(0, |content| content.allocations()),
                inside_bin_allocation: host
                    .upgrade()
                    .is_some_and(|host| host.is_allocating_child()),
                label_was_applied_value: label
                    .upgrade()
                    .is_some_and(|label| label.text() == applied_value),
                frame: host.upgrade().map_or(-1, |host| frame_of(&host)),
            });
        };
        if unapply {
            breakpoint.connect_unapply(handler);
        } else {
            breakpoint.connect_apply(handler);
        }
        site
    }
}
