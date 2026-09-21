// SPDX-License-Identifier: GPL-3.0-or-later

//! Headless adoption checks for GTK Lush crates consumed outside LushText.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use crate::common::{
    RowPlacement, RowStillnessProbe, assert_reveal_then_rest, assert_rows_still_across_selection,
    ensure_gtk_init, first_label, flush_after_delay, flush_events, force_layout, mapped_list_rows,
    mapped_row_with_label, placement_relative_to, present_window, row_straddling_bottom,
    sample_placements, test_application, wait_until,
};
use gtk_lush_tasks::{FreshnessToken, spawn_blocking_then};
use gtk_lush_viewport::{ViewportAxis, ViewportObserver};
use gtk_lush_widgets::{ClipBin, RenderHoldCapture, RenderHoldOverlay, ViewportSliceBin};
use gtk4::prelude::*;

#[test]
fn test_adoption_render_hold_captures_mapped_overlay() {
    ensure_gtk_init();
    let app = test_application();
    let overlay = gtk4::Overlay::new();
    overlay.set_size_request(320, 160);

    let live_child = gtk4::Label::new(Some("render hold mapped adoption surface"));
    live_child.set_hexpand(true);
    live_child.set_vexpand(true);
    live_child.add_css_class("title-2");
    overlay.set_child(Some(&live_child));

    let hold = RenderHoldOverlay::new(&overlay, &live_child);
    let window = libadwaita::ApplicationWindow::builder()
        .application(&app)
        .default_width(420)
        .default_height(240)
        .content(&overlay)
        .build();

    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        overlay.is_mapped()
            && live_child.is_mapped()
            && overlay.width() > 0
            && overlay.height() > 0
            && live_child.width() > 0
            && live_child.height() > 0
    });

    assert_eq!(hold.capture(), RenderHoldCapture::Captured);
    assert!(hold.is_active());
    assert!(hold.cover_is_visible());
    assert!(!hold.cover_can_target());
    assert_eq!(live_child.opacity(), 0.0);

    hold.warm_live_child();
    assert!(hold.is_warmed());
    assert_eq!(live_child.opacity(), 1.0);

    hold.clear();
    assert!(!hold.is_active());
    assert!(!hold.cover_is_visible());
}

#[test]
fn test_adoption_clipbin_constrains_wide_child_without_root_horizontal_scrollbar() {
    ensure_gtk_init();
    let app = test_application();

    let label = gtk4::Label::new(Some(
        "a deliberately wide adoption label that must yield to constrained chrome",
    ));
    label.set_hexpand(true);
    label.set_xalign(0.0);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);

    let clip_bin = ClipBin::with_child(&label);
    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Never)
        .propagate_natural_width(false)
        .min_content_width(180)
        .child(&clip_bin)
        .build();
    let window = libadwaita::ApplicationWindow::builder()
        .application(&app)
        .default_width(220)
        .default_height(120)
        .content(&scroller)
        .build();

    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        clip_bin.is_mapped() && clip_bin.width() > 0 && label.is_mapped()
    });

    assert_eq!(scroller.hscrollbar_policy(), gtk4::PolicyType::Never);
    assert!(clip_bin.width() <= scroller.width());
    assert!(clip_bin.child().is_some());
}

#[test]
fn test_adoption_viewport_observer_watches_real_scrollable_adjustments() {
    ensure_gtk_init();
    let app = test_application();

    let text_view = gtk4::TextView::new();
    text_view.set_monospace(true);
    text_view.set_wrap_mode(gtk4::WrapMode::None);
    text_view.buffer().set_text(&long_viewport_text());

    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .min_content_width(260)
        .min_content_height(160)
        .child(&text_view)
        .build();
    let window = libadwaita::ApplicationWindow::builder()
        .application(&app)
        .default_width(360)
        .default_height(240)
        .content(&scroller)
        .build();

    present_window(&window);
    wait_until(Duration::from_secs(2), || {
        text_view.is_mapped() && text_view.width() > 0 && text_view.height() > 0
    });

    let bounds_events = Rc::new(Cell::new(0u32));
    let value_events = Rc::new(Cell::new(0u32));
    let Some(observer) = ViewportObserver::for_scrollable(
        &text_view,
        {
            let bounds_events = Rc::clone(&bounds_events);
            move |change| {
                if change.axis() == ViewportAxis::Vertical {
                    bounds_events.set(bounds_events.get().saturating_add(1));
                }
            }
        },
        {
            let value_events = Rc::clone(&value_events);
            move |change| {
                if change.axis() == ViewportAxis::Vertical && !change.rests_at_lower() {
                    value_events.set(value_events.get().saturating_add(1));
                }
            }
        },
    ) else {
        panic!("text view should expose scrollable adjustments after mapping");
    };
    assert_eq!(observer.len(), 4);

    let Some(vadjustment) = text_view.vadjustment() else {
        panic!("text view should expose a vertical adjustment");
    };
    let page_size = (vadjustment.page_size() + 24.0).max(48.0);
    vadjustment.configure(12.0, 0.0, 1_000.0, 1.0, 80.0, page_size);
    vadjustment.set_value(36.0);
    flush_events();

    assert!(bounds_events.get() >= 1);
    assert!(value_events.get() >= 1);
}

#[test]
fn test_adoption_task_completion_returns_to_main_loop_with_freshness() {
    ensure_gtk_init();

    let completed = Rc::new(Cell::new(false));
    let accepted = Rc::new(Cell::new(false));
    let requested = FreshnessToken::new(3);

    spawn_blocking_then(requested, || String::from("adoption payload"), {
        let completed = Rc::clone(&completed);
        let accepted = Rc::clone(&accepted);
        move |token, payload| {
            accepted.set(
                token
                    .accept(FreshnessToken::new(3), payload)
                    .is_ok_and(|fresh| fresh.into_inner() == "adoption payload"),
            );
            completed.set(true);
        }
    });

    wait_until(Duration::from_secs(2), || completed.get());
    assert!(accepted.get());
}

fn long_viewport_text() -> String {
    (0..80)
        .map(|line| format!("viewport row {line} with a long adoption surface sample"))
        .collect::<Vec<_>>()
        .join("\n")
}

// --- ViewportSliceBin: the outer scroller belongs to the user -----------------
//
// A slice bin advertises its child's full height to an outer scroller and
// allocates only the band the viewport shows. It may move the outer scroller,
// but *only* to honour a request its child made (`scroll_to`, keyboard focus).
// A bin that also moves it while merely resting fights the user: with content
// above it, it scrolls that content out of view and pins itself to the top;
// with two bins in one scroller, each pulls the other's correction back and the
// view oscillates forever. These checks are here rather than in the LushText
// sidebar suite because the contract is the widget's, not the sidebar's — any
// consumer that puts chrome above a slice bin depends on it.

/// Rows per bin: comfortably past `GTK_LIST_VIEW_MAX_LIST_ITEMS` so the bin is
/// taller than any test viewport and the outer scroller has real range.
const SLICE_ADOPTION_ROWS: u32 = 400;
/// Height of the chrome placed above a slice bin inside the outer scroller.
const SLICE_ADOPTION_HEADER_HEIGHT: i32 = 54;
/// Scroll positions within this many logical pixels count as the same place.
const SCROLL_TOLERANCE: f64 = 1.0;
/// CSS class giving the padded fixture list the same asymmetric vertical
/// padding Libadwaita's `navigation-sidebar` rule gives LushText's tree
/// (`padding-top: 6px; padding-bottom: 4px`). A `GtkListView` works in its CSS
/// content box, so this is what makes it disagree with a host that hands it
/// border-box geometry; an unpadded list cannot see that defect.
const SLICE_ADOPTION_PADDED_CLASS: &str = "slice-adoption-padded";

fn install_padded_list_css() {
    // One provider per process: the selector is inert on every other widget,
    // and the harness runs the whole suite in one display.
    thread_local! {
        static INSTALLED: Cell<bool> = const { Cell::new(false) };
    }
    if INSTALLED.replace(true) {
        return;
    }
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&format!(
        "listview.{SLICE_ADOPTION_PADDED_CLASS} {{ padding-top: 6px; padding-bottom: 4px; }}"
    ));
    let display = gtk4::gdk::Display::default().expect("a display for the padded list css");
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn adoption_list(rows: u32) -> gtk4::ListView {
    let strings: Vec<String> = (0..rows).map(|index| format!("row {index:04}")).collect();
    let model = gtk4::StringList::new(&strings.iter().map(String::as_str).collect::<Vec<_>>());
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<gtk4::ListItem>().expect("list item");
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk4::ListItem>().expect("list item");
        let text = item
            .item()
            .and_downcast::<gtk4::StringObject>()
            .map(|object| object.string().to_string())
            .unwrap_or_default();
        if let Some(label) = item.child().and_downcast::<gtk4::Label>() {
            label.set_text(&text);
        }
    });
    gtk4::ListView::new(Some(gtk4::SingleSelection::new(Some(model))), Some(factory))
}

/// A scroller whose content is `sections` copies of "header then slice bin".
struct SliceAdoptionFixture {
    window: libadwaita::ApplicationWindow,
    scroller: gtk4::ScrolledWindow,
    headers: Vec<gtk4::Label>,
    lists: Vec<gtk4::ListView>,
    bins: Vec<ViewportSliceBin>,
}

impl SliceAdoptionFixture {
    fn present(sections: usize) -> Self {
        Self::present_with(sections, false)
    }

    /// `padded` gives every list the `navigation-sidebar`-like vertical padding.
    fn present_with(sections: usize, padded: bool) -> Self {
        ensure_gtk_init();
        if padded {
            install_padded_list_css();
        }
        let app = test_application();
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let mut headers = Vec::new();
        let mut lists = Vec::new();
        let mut bins = Vec::new();
        for index in 0..sections {
            let header = gtk4::Label::new(Some(&format!("section {index}")));
            header.set_height_request(SLICE_ADOPTION_HEADER_HEIGHT);
            header.set_xalign(0.0);
            content.append(&header);
            let list = adoption_list(SLICE_ADOPTION_ROWS);
            if padded {
                list.add_css_class(SLICE_ADOPTION_PADDED_CLASS);
            }
            let bin = ViewportSliceBin::with_child(&list);
            content.append(&bin);
            headers.push(header);
            lists.push(list);
            bins.push(bin);
        }
        let scroller = gtk4::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .child(&content)
            .build();
        let window = libadwaita::ApplicationWindow::builder()
            .application(&app)
            .default_width(420)
            .default_height(600)
            .content(&scroller)
            .build();
        present_window(&window);
        let fixture = Self {
            window,
            scroller,
            headers,
            lists,
            bins,
        };
        wait_until(Duration::from_secs(10), || {
            fixture.adjustment().upper() > f64::from(fixture.scroller.height())
        });
        flush_after_delay(Duration::from_millis(400));
        fixture
    }

    fn adjustment(&self) -> gtk4::Adjustment {
        self.scroller.vadjustment()
    }

    /// Sample the outer scroll position repeatedly, so a value that merely
    /// looks right at one instant cannot pass for a stable one.
    fn settled_values(&self, samples: usize) -> Vec<f64> {
        (0..samples)
            .map(|_| {
                flush_after_delay(Duration::from_millis(120));
                self.adjustment().value()
            })
            .collect()
    }

    fn top_of(&self, widget: &impl IsA<gtk4::Widget>) -> Option<f64> {
        widget
            .as_ref()
            .compute_bounds(&self.scroller)
            .map(|bounds| f64::from(bounds.y()))
    }

    fn fully_visible(&self, widget: &impl IsA<gtk4::Widget>) -> bool {
        widget
            .as_ref()
            .compute_bounds(&self.scroller)
            .is_some_and(|bounds| {
                f64::from(bounds.y()) >= -0.5
                    && f64::from(bounds.y() + bounds.height())
                        <= f64::from(self.scroller.height()) + 0.5
            })
    }
}

#[test]
fn test_adoption_slice_bin_leaves_the_outer_scroller_at_rest() {
    let fixture = SliceAdoptionFixture::present(1);
    let values = fixture.settled_values(12);
    assert!(
        values.iter().all(|value| *value < SCROLL_TOLERANCE),
        "a resting slice bin must not scroll the outer window away from the top; saw {values:?}"
    );
    assert!(
        fixture.fully_visible(&fixture.headers[0]),
        "chrome above the slice bin must stay visible; header top {:?}, viewport height {}",
        fixture.top_of(&fixture.headers[0]),
        fixture.scroller.height()
    );
    drop(fixture.window);
}

#[test]
fn test_adoption_slice_bin_lets_the_outer_scroller_return_to_the_top() {
    let fixture = SliceAdoptionFixture::present(1);
    let adjustment = fixture.adjustment();
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    flush_after_delay(Duration::from_millis(400));
    assert!(
        adjustment.value() > 0.0,
        "the scroller must reach the bottom"
    );

    adjustment.set_value(0.0);
    let values = fixture.settled_values(12);
    assert!(
        values.iter().all(|value| *value < SCROLL_TOLERANCE),
        "scrolling back to the top must stick instead of snapping to the list's first row; saw {values:?}"
    );
    assert!(fixture.fully_visible(&fixture.headers[0]));
    drop(fixture.window);
}

#[test]
fn test_adoption_two_slice_bins_in_one_scroller_do_not_oscillate() {
    let fixture = SliceAdoptionFixture::present(2);
    let adjustment = fixture.adjustment();
    let bottom = adjustment.upper() - adjustment.page_size();
    adjustment.set_value(bottom);
    let values = fixture.settled_values(15);
    assert!(
        values
            .iter()
            .all(|value| (value - bottom).abs() < SCROLL_TOLERANCE),
        "two slice bins must not pull the outer scroller back and forth, and the end of the \
         content must stay reachable; wanted {bottom}, saw {values:?}"
    );
    drop(fixture.window);
}

#[test]
fn test_adoption_slice_bin_still_forwards_a_child_scroll_request() {
    // The positive control for the two checks above: suppressing the bin's
    // unrequested scrolling must not suppress the requested kind.
    let fixture = SliceAdoptionFixture::present(1);
    let adjustment = fixture.adjustment();
    assert!(adjustment.value() < SCROLL_TOLERANCE);

    let target = SLICE_ADOPTION_ROWS - 1;
    fixture.lists[0].scroll_to(target, gtk4::ListScrollFlags::NONE, None);
    wait_until(Duration::from_secs(5), || adjustment.value() > 0.0);
    flush_after_delay(Duration::from_millis(400));
    assert!(
        adjustment.value() > 0.0,
        "a child scroll_to must still move the outer scroller"
    );

    let values = fixture.settled_values(10);
    let first = values[0];
    assert!(
        values
            .iter()
            .all(|value| (value - first).abs() < SCROLL_TOLERANCE),
        "an honoured request must settle instead of re-asking; saw {values:?}"
    );
    drop(fixture.window);
}

impl SliceAdoptionFixture {
    fn selection(&self, section: usize) -> gtk4::SingleSelection {
        self.lists[section]
            .model()
            .and_downcast::<gtk4::SingleSelection>()
            .expect("the adoption list uses a SingleSelection")
    }

    fn row_index(label: &str) -> u32 {
        label
            .strip_prefix("row ")
            .and_then(|digits| digits.parse().ok())
            .expect("adoption rows are labelled `row NNNN`")
    }

    /// Where the row labelled `label` is drawn relative to its section header.
    fn placement(&self, section: usize, label: &str) -> Option<RowPlacement> {
        let row = mapped_row_with_label(&self.lists[section], label)?;
        placement_relative_to(&row, &self.headers[section])
    }

    fn force_layout(&self) {
        force_layout(self.bins.iter().map(Cast::upcast_ref::<gtk4::Widget>));
    }

    fn sampled_placements(&self, section: usize, label: &str, samples: usize) -> Vec<RowPlacement> {
        sample_placements(
            samples,
            || self.force_layout(),
            || self.placement(section, label),
        )
    }

    /// Labels of the rows currently drawn wholly inside the outer viewport.
    fn fully_visible_labels(&self, section: usize) -> Vec<String> {
        mapped_list_rows(&self.lists[section])
            .filter(|row| self.fully_visible(row))
            .filter_map(|row| first_label(&row).map(|label| label.text().to_string()))
            .collect()
    }

    fn assert_rows_still_across_selection(&self, when: &str) {
        let outer = self.adjustment();
        let visible_labels = || self.fully_visible_labels(0);
        let select = |label: &str| self.selection(0).set_selected(Self::row_index(label));
        let focus = |label: &str| {
            if let Some(row) = mapped_row_with_label(&self.lists[0], label) {
                row.grab_focus();
            }
        };
        let sample = |label: &str, samples: usize| self.sampled_placements(0, label, samples);
        assert_rows_still_across_selection(
            &RowStillnessProbe {
                outer: &outer,
                visible_labels: &visible_labels,
                select: &select,
                focus: &focus,
                sample: &sample,
            },
            when,
        );
    }
}

#[test]
fn test_adoption_padded_slice_bin_keeps_rows_still_across_selection_at_the_top() {
    // The list's CSS padding makes it work in a content box 10px shorter than
    // the border box the bin measures; the bin must publish geometry the list
    // does not correct, or the list re-derives its value a few pixels off on
    // every anchor change and draws its rows there.
    let fixture = SliceAdoptionFixture::present_with(1, true);
    assert!(fixture.adjustment().value() < SCROLL_TOLERANCE);
    fixture.assert_rows_still_across_selection("at the top");
    drop(fixture.window);
}

#[test]
fn test_adoption_padded_slice_bin_keeps_rows_still_across_selection_mid_content() {
    let fixture = SliceAdoptionFixture::present_with(1, true);
    let outer = fixture.adjustment();
    outer.set_value((outer.upper() - outer.page_size()) / 2.0);
    flush_after_delay(Duration::from_millis(400));
    assert!(
        outer.value() > SCROLL_TOLERANCE,
        "the fixture must have scrolled"
    );
    fixture.assert_rows_still_across_selection("mid-content");
    drop(fixture.window);
}

#[test]
fn test_adoption_padded_slice_bin_still_reveals_a_clipped_row_and_then_rests() {
    // Positive control for the stillness checks; the travel is the list's
    // decision (see `gtk_lush_widgets::outer_scroll_request`).
    let fixture = SliceAdoptionFixture::present_with(1, true);
    let outer = fixture.adjustment();
    // Nudge so no row boundary coincides with the viewport bottom.
    outer.set_value(7.0);
    flush_after_delay(Duration::from_millis(300));
    let resting = outer.value();
    let (row, overflow) = row_straddling_bottom(&fixture.lists[0], &fixture.scroller)
        .expect("a row straddling the viewport bottom");
    let label = first_label(&row)
        .expect("a labelled row")
        .text()
        .to_string();
    fixture.lists[0].scroll_to(
        SliceAdoptionFixture::row_index(&label),
        gtk4::ListScrollFlags::FOCUS,
        None,
    );
    wait_until(Duration::from_secs(5), || {
        (outer.value() - resting).abs() > 0.5
    });
    assert_reveal_then_rest(resting, overflow, &fixture.settled_values(6), &label);
    let row =
        mapped_row_with_label(&fixture.lists[0], &label).expect("the revealed row stays rendered");
    assert!(
        fixture.fully_visible(&row),
        "{label} must be fully inside the viewport after the request"
    );
    drop(fixture.window);
}

#[test]
fn test_adoption_padded_slice_bin_stops_allocating_and_correcting_at_rest() {
    // The bin learns the child's content-box inset from its first allocation
    // and republishes geometry in the child's frame. From then on an ordinary
    // allocation leaves the child nothing to correct, so at rest the bin must
    // neither keep allocating nor keep writing its offset back: a count that
    // grows across idle passes is a layout loop, and a correction that keeps
    // firing means the child still disagrees with what it is handed.
    let fixture = SliceAdoptionFixture::present_with(1, true);
    let outer = fixture.adjustment();
    outer.set_value((outer.upper() - outer.page_size()) / 2.0);
    flush_after_delay(Duration::from_millis(400));
    let bin = &fixture.bins[0];
    let corrections_after_settling = bin.correction_count();
    let allocations: Vec<u64> = (0..6)
        .map(|_| {
            flush_after_delay(Duration::from_millis(120));
            bin.allocation_count()
        })
        .collect();
    assert!(
        allocations.iter().all(|count| *count == allocations[0]),
        "a resting bin must not keep allocating; saw {allocations:?}"
    );
    assert_eq!(
        bin.correction_count(),
        corrections_after_settling,
        "a resting bin must not keep correcting the child's value"
    );
    // A selection change on an already visible row is an ordinary allocation
    // too: it may allocate, but it must not need a correction.
    let visible = fixture.fully_visible_labels(0);
    fixture
        .selection(0)
        .set_selected(SliceAdoptionFixture::row_index(&visible[2]));
    fixture.force_layout();
    assert_eq!(
        bin.correction_count(),
        corrections_after_settling,
        "an ordinary allocation must not need a correction once the inset is known"
    );
    drop(fixture.window);
}
