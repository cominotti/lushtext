// SPDX-License-Identifier: GPL-3.0-or-later

//! Headless adoption checks for GTK Lush crates consumed outside LushText.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use crate::common::{ensure_gtk_init, flush_events, present_window, test_application, wait_until};
use gtk_lush_tasks::{FreshnessToken, spawn_blocking_then};
use gtk_lush_viewport::{ViewportAxis, ViewportObserver};
use gtk_lush_widgets::{ClipBin, RenderHoldCapture, RenderHoldOverlay};
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

    spawn_blocking_then(
        requested,
        || String::from("adoption payload"),
        {
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
        },
    );

    wait_until(Duration::from_secs(2), || completed.get());
    assert!(accepted.get());
}

fn long_viewport_text() -> String {
    (0..80)
        .map(|line| format!("viewport row {line} with a long adoption surface sample"))
        .collect::<Vec<_>>()
        .join("\n")
}
