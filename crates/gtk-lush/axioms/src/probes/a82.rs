// SPDX-License-Identifier: MIT OR Apache-2.0

//! A82: an `AdwTabView` page that has **never been selected** is neither
//! mapped nor realized (its bin stays child-invisible, `adw-tab-view.c`
//! `attach_page`), **but a `GtkTextView` inside a `GtkScrolledWindow` has a
//! layout anyway**: the scrolled window hands the view its adjustments when it
//! adopts it, and `gtk_text_view_set_vadjustment` →
//! `gtk_text_view_set_vadjustment_values` calls
//! `gtk_text_view_ensure_layout` (`gtktextview.c` 4.22.5). So text loaded into
//! the buffer of a background tab that was **never shown** is validated in full
//! while hidden, from the same incremental idle as A80: a
//! `G_PRIORITY_DEFAULT_IDLE` source armed after the load waits for it.
//!
//! The layout does not even need the scrolled window: a `GtkSourceView` in no
//! widget tree, never realized (kept out of the tab view, whose measure would
//! propagate its validated height to the window), validates a load into its
//! buffer the same way. The control is a buffer no view shows: a load into it
//! is not laid out and the lower source dispatches at once.
//!
//! A page selected once and hidden again is unmapped but stays realized.
//! The first page appended to an **empty** tab view is selected by the tab
//! view itself (`insert_page`: `if (!self->selected_page)`).
//!
//! Selecting a different page or leaving a page unselected therefore cannot
//! keep a background editor from laying out its document: only taking the
//! layout's buffer away (A83) or keeping the buffer empty does.
//!
//! Recorded magnitudes, never verdicts: iterations and microseconds before the
//! lower source dispatched.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::{Recorder, Stop};
use crate::probes::post_open::{MAX_ITERATIONS, dense_text};
use crate::session::{Presented, flush_events, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Lines loaded into a page's buffer.
pub const LINES: usize = 1 << 14;
/// Characters per line.
pub const LINE_CHARS: usize = 40;
/// Seed of the probe document.
pub const SEED: u64 = 82;
/// Iterations within which a lower source counts as "at once": the two
/// one-shot validation idles an invalidation of an empty or absent layout
/// queues (`first_validate` and one incremental batch) fit in it.
pub const AT_ONCE_ITERATIONS: usize = 4;

const BUDGET: Duration = Duration::from_secs(60);
const QUIET: Duration = Duration::from_millis(100);

/// One editor-like view: line-numbered, monospace, not wrapping.
pub struct TabPageView {
    /// The page's widget: the scrolled window, or the bare view.
    pub page_child: gtk4::Widget,
    /// The view.
    pub view: sourceview5::View,
    /// The view's buffer.
    pub buffer: sourceview5::Buffer,
}

impl TabPageView {
    /// A view, in a scrolled window when `scrolled`.
    #[must_use]
    pub fn new(scrolled: bool) -> Self {
        let buffer = sourceview5::Buffer::new(None);
        buffer.set_highlight_syntax(false);
        let view = sourceview5::View::with_buffer(&buffer);
        view.set_monospace(true);
        view.set_show_line_numbers(true);
        view.set_wrap_mode(gtk4::WrapMode::None);
        let page_child = if scrolled {
            gtk4::ScrolledWindow::builder()
                .child(&view)
                .hexpand(true)
                .vexpand(true)
                .build()
                .upcast()
        } else {
            view.clone().upcast()
        };
        Self {
            page_child,
            view,
            buffer,
        }
    }
}

/// When the lower source dispatched after a load.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LowerDelivery {
    /// Main-context iterations from arming to its dispatch.
    pub iteration: usize,
    /// Microseconds from arming to its dispatch.
    pub micros: u128,
}

/// A presented `AdwTabView` holding a label page (selected) and three view
/// pages, none selected.
pub struct TabViewFixture {
    /// The tab view.
    pub tab_view: libadwaita::TabView,
    /// The page that was appended first.
    pub front: libadwaita::TabPage,
    /// A scrolled view page that is selected once, then hidden.
    pub shown_once: (libadwaita::TabPage, TabPageView),
    /// A scrolled view page that is never selected.
    pub never_shown: (libadwaita::TabPage, TabPageView),
    /// A view in no scrolled window and in no widget tree (a page would
    /// propagate its validated height to the window), never realized.
    pub bare: TabPageView,
    _shown: Presented,
}

impl TabViewFixture {
    fn new(recorder: &mut Recorder) -> Result<Self, Stop> {
        let tab_view = libadwaita::TabView::new();
        let front = tab_view.append(&gtk4::Label::new(Some("front page")));
        let add = |scrolled: bool| {
            let page_view = TabPageView::new(scrolled);
            let page = tab_view.append(&page_view.page_child);
            (page, page_view)
        };
        let shown_once = add(true);
        let never_shown = add(true);
        let bare = TabPageView::new(false);
        let shown = Presented::checked(
            recorder,
            &tab_view,
            "control: the tab view's window was realized",
        )?;
        settle(LAYOUT_SETTLE);
        Ok(Self {
            tab_view,
            front,
            shown_once,
            never_shown,
            bare,
            _shown: shown,
        })
    }

    /// The fixture for a sample; `None` when the window never realized.
    #[must_use]
    pub fn standalone() -> Option<Self> {
        Self::new(&mut Recorder::detached()).ok()
    }
}

/// Append the probe document to `buffer`, arm one `G_PRIORITY_DEFAULT_IDLE`
/// source, and iterate the default context until it dispatched and nothing
/// was ready for a while.
#[must_use]
pub fn load_and_trace(buffer: &impl IsA<gtk4::TextBuffer>) -> Option<LowerDelivery> {
    let buffer = buffer.as_ref();
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, &dense_text(LINES, LINE_CHARS, SEED));
    trace_lower_delivery()
}

/// Arm one `G_PRIORITY_DEFAULT_IDLE` source and iterate the default context
/// until it dispatched and nothing was ready for a while.
#[must_use]
pub fn trace_lower_delivery() -> Option<LowerDelivery> {
    let started = Instant::now();
    let iteration = Rc::new(Cell::new(0usize));
    let delivered = Rc::new(Cell::new(None::<LowerDelivery>));
    {
        let iteration = Rc::clone(&iteration);
        let delivered = Rc::clone(&delivered);
        glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            delivered.set(Some(LowerDelivery {
                iteration: iteration.get(),
                micros: started.elapsed().as_micros(),
            }));
            glib::ControlFlow::Break
        });
    }
    let context = glib::MainContext::default();
    let mut quiet_since = Instant::now();
    while started.elapsed() < BUDGET && iteration.get() < MAX_ITERATIONS {
        iteration.set(iteration.get() + 1);
        if context.iteration(false) {
            quiet_since = Instant::now();
        } else {
            iteration.set(iteration.get() - 1);
            if delivered.get().is_some() && quiet_since.elapsed() >= QUIET {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    delivered.get()
}

/// A one-line description of a delivery.
#[must_use]
pub fn describe(delivery: Option<LowerDelivery>) -> String {
    delivery.map_or_else(
        || "never".to_owned(),
        |d| format!("iteration {} after {} us", d.iteration, d.micros),
    )
}

fn waited(delivery: Option<LowerDelivery>) -> bool {
    delivery.is_some_and(|d| d.iteration > AT_ONCE_ITERATIONS)
}

fn at_once(delivery: Option<LowerDelivery>) -> bool {
    delivery.is_some_and(|d| d.iteration <= AT_ONCE_ITERATIONS)
}

/// Probe A82.
#[must_use]
pub fn probe_a82() -> Observation {
    Recorder::run(AxiomId::new(82), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = TabViewFixture::new(recorder)?;
        recorder.axiom(
            fixture.tab_view.selected_page().as_ref() == Some(&fixture.front),
            "the first page appended to an empty tab view is selected by the tab view",
        )?;

        let (_, never) = &fixture.never_shown;
        recorder.axiom(
            !never.view.is_mapped() && !never.view.is_realized(),
            "a page never selected is neither mapped nor realized",
        )?;
        let never_delivery = load_and_trace(&never.buffer);
        recorder.measure("never_shown_scrolled_load", describe(never_delivery));
        recorder.axiom(
            waited(never_delivery) && !never.view.is_realized(),
            "a view in a scrolled window on a page never selected validates text loaded into its \
             buffer in full while hidden: the G_PRIORITY_DEFAULT_IDLE source waits for it",
        )?;

        let unviewed = sourceview5::Buffer::new(None);
        unviewed.set_highlight_syntax(false);
        let unviewed_delivery = load_and_trace(&unviewed);
        recorder.measure("buffer_without_view_load", describe(unviewed_delivery));
        recorder.control(
            at_once(unviewed_delivery),
            "control: a load into a buffer no view shows is not laid out",
        )?;
        let bare = &fixture.bare;
        let bare_delivery = load_and_trace(&bare.buffer);
        recorder.measure("bare_view_load", describe(bare_delivery));
        recorder.axiom(
            waited(bare_delivery) && !bare.view.is_realized(),
            "a source view in no scrolled window and no widget tree, never realized, also \
             validates text loaded into its buffer in full",
        )?;

        let (shown_page, shown) = &fixture.shown_once;
        fixture.tab_view.set_selected_page(shown_page);
        settle(LAYOUT_SETTLE);
        recorder.control(
            shown.view.is_mapped(),
            "control: a selected page's view is mapped",
        )?;
        fixture.tab_view.set_selected_page(&fixture.front);
        flush_events();
        recorder.axiom(
            !shown.view.is_mapped() && shown.view.is_realized(),
            "selecting another page unmaps the page but leaves it realized",
        )?;
        let shown_delivery = load_and_trace(&shown.buffer);
        recorder.measure("shown_once_load", describe(shown_delivery));
        recorder.axiom(
            waited(shown_delivery),
            "a page selected once and hidden again validates text loaded into its buffer while \
             hidden",
        )?;
        drop(fixture);
        flush_events();
        Ok(())
    })
}
