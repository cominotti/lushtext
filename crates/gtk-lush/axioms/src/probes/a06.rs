// SPDX-License-Identifier: MIT OR Apache-2.0

//! A6: a `GtkScrollable` works in its CSS content box, so the page it
//! publishes is its allocation minus its padding and border. This is why
//! `ViewportSliceBin` publishes content-box `upper` and `page` to its child
//! and learns the inset from the page the child reports.

use gtk4::prelude::*;

use super::LAYOUT_SETTLE;
use crate::fixtures::{HostedList, LIST_HEIGHT};
use crate::observation::Recorder;
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

/// The CSS class that pads the probe list.
pub const INSET_CLASS: &str = "gtk-lush-axioms-inset";
/// Top padding the class applies.
pub(crate) const INSET_TOP: i32 = 7;
/// Bottom padding the class applies.
pub(crate) const INSET_BOTTOM: i32 = 5;

/// A style provider that pads [`INSET_CLASS`] lists, installed on the
/// default display until dropped.
#[derive(Debug)]
pub struct InsetStyle {
    provider: gtk4::CssProvider,
    display: Option<gtk4::gdk::Display>,
}

impl InsetStyle {
    /// Install the provider on the default display.
    #[must_use]
    pub fn install() -> Self {
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(&format!(
            "listview.{INSET_CLASS} {{ padding-top: {INSET_TOP}px; padding-bottom: \
             {INSET_BOTTOM}px; }}"
        ));
        let display = gtk4::gdk::Display::default();
        if let Some(display) = display.as_ref() {
            gtk4::style_context_add_provider_for_display(
                display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_USER,
            );
        }
        Self { provider, display }
    }
}

impl Drop for InsetStyle {
    fn drop(&mut self) {
        if let Some(display) = self.display.as_ref() {
            gtk4::style_context_remove_provider_for_display(display, &self.provider);
        }
    }
}

/// Probe A6. See the module documentation.
#[must_use]
pub fn probe_a06() -> Observation {
    Recorder::run(AxiomId::new(6), |recorder| {
        let style = InsetStyle::install();
        recorder.control(style.display.is_some(), "control: a default display exists")?;
        let hosted = HostedList::new();
        let shown = Presented::new(&hosted.host);
        recorder.control(shown.realized(), "control: the fixture window realizes")?;
        recorder.measure("allocation", LIST_HEIGHT);
        recorder.measure("unpadded_page", hosted.adjustment.page_size());
        recorder.control(
            (hosted.adjustment.page_size() - f64::from(LIST_HEIGHT)).abs() < f64::EPSILON,
            "control: an unpadded list publishes its whole allocation as its page",
        )?;

        hosted.list.add_css_class(INSET_CLASS);
        settle(LAYOUT_SETTLE);
        let inset = INSET_TOP + INSET_BOTTOM;
        recorder.measure("inset", inset);
        recorder.measure("padded_page", hosted.adjustment.page_size());
        recorder.measure("padded_content_height", hosted.list.height());
        recorder.axiom(
            (hosted.adjustment.page_size() - f64::from(LIST_HEIGHT - inset)).abs() < f64::EPSILON,
            "A6: a padded list publishes allocation minus inset as its page",
        )?;
        recorder.axiom(
            (hosted.adjustment.page_size() - f64::from(hosted.list.height())).abs() < f64::EPSILON,
            "A6: the published page is the list's content-box height",
        )
    })
}
