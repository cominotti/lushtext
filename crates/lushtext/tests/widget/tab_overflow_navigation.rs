// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget coverage for `WFR-TAB-OVERFLOW-NAVIGATION` (`tab-strip-overflow-navigation`):
//! the scroll controls' visibility, enablement, steps, hold-to-repeat and its
//! cancellation by the strip's own input, focus hand-off, Focus Mode, session
//! restore, right to left, the fail-closed lookup, and the evidence surface's
//! three driven proofs.
//!
//! Steps land at once through the workflow's test policy (presented widget
//! tests do not reliably advance `AdwTimedAnimation` frame clocks). A hold is
//! started and ended by emitting its gesture's `pressed` and `released` by
//! name, and the bar's observers are found through `observe_controllers()` and
//! emitted the same way; a claim is a no-op without an event sequence, so the
//! real pointer press is proved by the accessibility smoke and the live check.

use crate::common::{
    activate_action, ensure_gtk_init, flush_events, present_window, test_window_with_restored_size,
    wait_for_painted_frame, wait_until_or_false,
};
use glib::object::ObjectExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::services::filesystem::fixture;
use lushtext_core::ui::window::{
    LushtextWindow, TabOverflowEvidence, TabOverflowTestTiming, find_tab_scroller,
    set_tab_overflow_test_timing, tab_overflow_evidence,
};
use std::time::Duration;

const SETTLE: Duration = Duration::from_secs(5);
/// A hold delay no test waits out, so a hold stays pending while a test drives
/// its cancellation.
const LONG_REPEAT_DELAY_MS: u32 = 60_000;

fn evidence(window: &LushtextWindow) -> TabOverflowEvidence {
    tab_overflow_evidence(window)
}

fn timing(repeat_delay_ms: u32, repeat_interval_ms: u32) {
    set_tab_overflow_test_timing(TabOverflowTestTiming {
        immediate_steps: true,
        repeat_delay_ms,
        repeat_interval_ms,
    });
}

fn window_at(width: i32) -> LushtextWindow {
    ensure_gtk_init();
    timing(0, 0);
    // The tab box's own appear, close and scroll animations do not reliably
    // advance in presented widget tests either; with animations off
    // Libadwaita completes each at once, so every state below is terminal.
    gtk4::Settings::default()
        .expect("default GTK settings")
        .set_gtk_enable_animations(false);
    let window = test_window_with_restored_size(width, 700);
    present_window(&window);
    window
}

fn settled(window: &LushtextWindow) {
    // A tab-box scroll that completed at once stays `FINISHED` until the
    // box's next allocation, and a write before it would be reverted (ledger
    // A132): let one frame lay the strip out first.
    window.imp().tab_bar.queue_allocate();
    wait_for_painted_frame(window);
    assert!(
        wait_until_or_false(SETTLE, || {
            let now = evidence(window);
            !now.reconcile_pending && !now.animation_active
        }),
        "the tab-overflow reconcile never settled"
    );
    flush_events();
}

fn open_tabs(window: &LushtextWindow, count: usize) {
    for _ in 0..count {
        activate_action(window, "new-tab");
    }
    flush_events();
    settled(window);
}

fn unpinned(window: &LushtextWindow) -> gtk4::ScrolledWindow {
    find_tab_scroller(window.imp().tab_bar.upcast_ref(), false).expect("the unpinned tab scroller")
}

fn scroll_to(window: &LushtextWindow, value: f64) {
    unpinned(window).hadjustment().set_value(value);
    settled(window);
}

fn overflowing_window() -> LushtextWindow {
    let window = window_at(1000);
    open_tabs(&window, 40);
    assert!(
        evidence(&window).controls_visible,
        "40 tabs overflow a 1000 px strip"
    );
    // Each page attach scrolled the strip to its new tab (ledger A132).
    scroll_to(&window, 0.0);
    window
}

fn buttons(window: &LushtextWindow) -> (gtk4::Button, gtk4::Button) {
    evidence(window)
        .buttons
        .expect("the scroll buttons are built")
}

fn hold_gesture(button: &gtk4::Button) -> gtk4::GestureClick {
    let controllers = button.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| controllers.item(index).and_downcast::<gtk4::GestureClick>())
        .find(|gesture| gesture.propagation_phase() == gtk4::PropagationPhase::Capture)
        .expect("the scroll button's capture-phase hold gesture")
}

fn bar_controllers(window: &LushtextWindow) -> Vec<gtk4::EventController> {
    let controllers = window.imp().tab_bar.observe_controllers();
    (0..controllers.n_items())
        .filter_map(|index| {
            controllers
                .item(index)
                .and_downcast::<gtk4::EventController>()
        })
        .filter(|controller| controller.propagation_phase() == gtk4::PropagationPhase::Capture)
        .collect()
}

fn bar_scroll_observer(window: &LushtextWindow) -> gtk4::EventControllerScroll {
    bar_controllers(window)
        .into_iter()
        .find_map(|controller| controller.downcast::<gtk4::EventControllerScroll>().ok())
        .expect("the bar's capture-phase scroll observer")
}

fn bar_press_observer(window: &LushtextWindow) -> gtk4::GestureClick {
    bar_controllers(window)
        .into_iter()
        .find_map(|controller| controller.downcast::<gtk4::GestureClick>().ok())
        .expect("the bar's capture-phase press observer")
}

fn press(gesture: &gtk4::GestureClick) {
    gesture.emit_by_name::<()>("pressed", &[&1i32, &4.0f64, &4.0f64]);
}

fn release(gesture: &gtk4::GestureClick) {
    gesture.emit_by_name::<()>("released", &[&1i32, &4.0f64, &4.0f64]);
}

fn scroll_value(window: &LushtextWindow) -> i32 {
    evidence(window)
        .extent
        .expect("an unpinned extent")
        .value_px
}

fn max_value(window: &LushtextWindow) -> i32 {
    evidence(window)
        .extent
        .expect("an unpinned extent")
        .max_value()
}

#[test]
fn test_no_scroll_buttons_while_the_tabs_fit() {
    let window = window_at(1000);
    open_tabs(&window, 3);
    let now = evidence(&window);
    assert!(now.scroller_found);
    assert!(!now.controls_visible);
    assert!(
        !now.group_attached,
        "nothing to show leaves the group detached"
    );
    assert_eq!(now.scroll_actions_enabled, [false, false]);
}

#[test]
fn test_scroll_buttons_appear_on_overflow_without_changing_the_strip_height() {
    let window = window_at(1000);
    open_tabs(&window, 3);
    assert!(wait_until_or_false(SETTLE, || evidence(&window).bar_height > 0));
    let height_before = evidence(&window).bar_height;
    open_tabs(&window, 37);
    let now = evidence(&window);
    assert!(now.controls_visible && now.group_attached);
    assert_eq!(now.controls_shown_count, 1);
    assert_eq!(
        now.bar_height, height_before,
        "the strip's height must not move"
    );
    let (back, forward) = buttons(&window);
    assert_eq!(
        back.parent().as_ref(),
        now.group.as_ref().map(glib::object::Cast::upcast_ref)
    );
    assert!(forward.property::<bool>("visible"));
}

#[test]
fn test_closing_tabs_until_they_fit_hides_the_buttons_and_the_band_keeps_the_state() {
    let window = window_at(1000);
    open_tabs(&window, 40);
    assert!(evidence(&window).controls_visible);
    let tab_view = window.imp().tab_view.get();
    while evidence(&window).controls_visible {
        assert!(
            tab_view.n_pages() > 1,
            "the buttons must hide before one tab remains"
        );
        let page = tab_view.nth_page(tab_view.n_pages() - 1);
        tab_view.close_page(&page);
        flush_events();
        settled(&window);
    }
    let hidden = evidence(&window);
    assert!(!hidden.group_attached);
    // A fixed width and tab set: repeated reconciles never flip the state.
    for _ in 0..5 {
        unpinned(&window)
            .hadjustment()
            .emit_by_name::<()>("changed", &[]);
        settled(&window);
        assert_eq!(evidence(&window).controls_visible, hidden.controls_visible);
        assert_eq!(
            evidence(&window).controls_shown_count,
            hidden.controls_shown_count
        );
    }
}

#[test]
fn test_pinned_only_overflow_shows_no_scroll_buttons() {
    let window = window_at(700);
    open_tabs(&window, 60);
    let tab_view = window.imp().tab_view.get();
    for index in 0..tab_view.n_pages() {
        tab_view.set_page_pinned(&tab_view.nth_page(index), true);
    }
    flush_events();
    settled(&window);
    let now = evidence(&window);
    assert!(
        now.bar_overflowing,
        "60 pinned tabs overflow a 700 px strip"
    );
    assert!(
        !now.controls_visible,
        "pinned-only overflow shows no scroll button"
    );
}

#[test]
fn test_forward_scrolls_without_changing_selection_text_or_focus() {
    let window = overflowing_window();
    let tab_view = window.imp().tab_view.get();
    let selected = tab_view.selected_page();
    let focus = gtk4::prelude::GtkWindowExt::focus(&window);
    let before = scroll_value(&window);
    activate_action(&window, "scroll-tabs-forward");
    settled(&window);
    let after = scroll_value(&window);
    let increment = unpinned(&window).hadjustment().page_increment();
    assert!(after > before, "Forward moved the strip");
    assert!(
        (f64::from(after - before) - increment).abs() <= 1.0,
        "one step is one page increment"
    );
    assert_eq!(tab_view.selected_page(), selected);
    assert_eq!(gtk4::prelude::GtkWindowExt::focus(&window), focus);
    assert_eq!(tab_view.n_pages(), 40);
}

#[test]
fn test_edge_enablement_on_both_actions() {
    let window = overflowing_window();
    assert_eq!(evidence(&window).scroll_actions_enabled, [false, true]);
    scroll_to(&window, f64::from(max_value(&window)));
    assert_eq!(evidence(&window).scroll_actions_enabled, [true, false]);
    scroll_to(&window, f64::from(max_value(&window) / 2));
    assert_eq!(evidence(&window).scroll_actions_enabled, [true, true]);
}

#[test]
fn test_wheel_like_movement_updates_enablement_only() {
    let window = overflowing_window();
    let shown = evidence(&window).controls_shown_count;
    scroll_to(&window, 300.0);
    let now = evidence(&window);
    assert_eq!(now.scroll_actions_enabled, [true, true]);
    assert_eq!(now.controls_shown_count, shown);
    assert_eq!(now.guard_cancellations, 0);
    assert_eq!(scroll_value(&window), 300);
}

#[test]
fn test_a_focused_button_that_disables_hands_focus_on() {
    let window = overflowing_window();
    let (back, forward) = buttons(&window);
    scroll_to(&window, f64::from(max_value(&window) - 10));
    // Let the new tabs' own editor-focus retries run out first.
    crate::common::flush_after_delay(Duration::from_millis(800));
    assert!(forward.grab_focus(), "Forward takes keyboard focus");
    flush_events();
    let focus_now = gtk4::prelude::GtkWindowExt::focus(&window);
    assert!(
        focus_now.as_ref() == Some(forward.upcast_ref()),
        "Forward holds keyboard focus, not {:?}",
        focus_now.map(|widget| widget.type_().name())
    );
    activate_action(&window, "scroll-tabs-forward");
    settled(&window);
    assert_eq!(evidence(&window).scroll_actions_enabled, [true, false]);
    assert_eq!(
        gtk4::prelude::GtkWindowExt::focus(&window).as_ref(),
        Some(back.upcast_ref()),
        "focus moves to Back when Forward disables at the end"
    );

    // With both disabled (the tabs fit), focus goes to the selected editor.
    // The first tab stays selected, so no close moves the selection: a
    // selection change with focus inside the bar focuses the new tab instead
    // (`select_page`, `adw-tab-box.c`).
    let tab_view = window.imp().tab_view.get();
    tab_view.set_selected_page(&tab_view.nth_page(0));
    crate::common::flush_after_delay(Duration::from_millis(800));
    settled(&window);
    assert!(
        forward.grab_focus(),
        "at the start Forward is the enabled one"
    );
    while tab_view.n_pages() > 2 {
        tab_view.close_page(&tab_view.nth_page(tab_view.n_pages() - 1));
        flush_events();
    }
    settled(&window);
    let editor = crate::common::active_editor(&window);
    assert!(
        wait_until_or_false(SETTLE, || {
            gtk4::prelude::GtkWindowExt::focus(&window).is_some_and(|focus| {
                focus == *editor.source_view().upcast_ref::<gtk4::Widget>()
                    || focus.is_ancestor(editor.source_view())
            })
        }),
        "focus moves to the editor when no control can hold it"
    );
}

#[test]
fn test_hold_repeats_to_the_edge_and_a_release_adds_no_step() {
    let window = overflowing_window();
    timing(10, 10);
    let (_, forward) = buttons(&window);
    let gesture = hold_gesture(&forward);
    press(&gesture);
    assert!(scroll_value(&window) > 0, "a press steps at once");
    assert!(
        wait_until_or_false(SETTLE, || !evidence(&window).repeat_active),
        "the hold stops by itself at the edge"
    );
    assert_eq!(scroll_value(&window), max_value(&window));
    release(&gesture);
    settled(&window);
    assert_eq!(
        scroll_value(&window),
        max_value(&window),
        "a release adds no step"
    );
    assert_eq!(evidence(&window).guard_cancellations, 0);

    // Released mid-hold: the repeat source is gone and nothing else moves.
    scroll_to(&window, 0.0);
    timing(LONG_REPEAT_DELAY_MS, 10);
    press(&gesture);
    let stepped = scroll_value(&window);
    assert!(evidence(&window).repeat_active);
    release(&gesture);
    assert!(!evidence(&window).repeat_active);
    settled(&window);
    assert_eq!(scroll_value(&window), stepped);
}

#[test]
fn test_hiding_the_buttons_mid_hold_drops_the_repeat() {
    let window = overflowing_window();
    timing(LONG_REPEAT_DELAY_MS, 10);
    let (_, forward) = buttons(&window);
    press(&hold_gesture(&forward));
    assert!(evidence(&window).repeat_active);
    activate_action(&window, "toggle-focus-mode");
    flush_events();
    settled(&window);
    let now = evidence(&window);
    assert!(
        !now.controls_visible && !now.group_attached,
        "Focus Mode hides the group"
    );
    assert!(
        !now.repeat_active,
        "the repeat source is gone when the button hides"
    );
    activate_action(&window, "toggle-focus-mode");
    flush_events();
    settled(&window);
    assert!(
        evidence(&window).controls_visible,
        "leaving Focus Mode shows them again"
    );
}

fn assert_cancels_hold(name: &str, drive: impl FnOnce(&LushtextWindow)) {
    let window = overflowing_window();
    timing(LONG_REPEAT_DELAY_MS, 10);
    let (_, forward) = buttons(&window);
    press(&hold_gesture(&forward));
    let before = evidence(&window);
    assert!(before.repeat_active, "{name}: the hold is pending");
    let value = scroll_value(&window);
    drive(&window);
    let now = evidence(&window);
    assert!(
        !now.repeat_active && !now.animation_active,
        "{name}: cancelled"
    );
    assert_eq!(
        now.guard_cancellations,
        before.guard_cancellations + 1,
        "{name}: counted once"
    );
    if name != "selecting a tab" && name != "a page attach" {
        assert_eq!(scroll_value(&window), value, "{name}: pause, never reset");
    }
}

#[test]
fn test_the_strips_own_input_cancels_a_hold() {
    assert_cancels_hold("selecting a tab", |window| {
        let tab_view = window.imp().tab_view.get();
        tab_view.set_selected_page(&tab_view.nth_page(20));
    });
    assert_cancels_hold("a page attach", |window| activate_action(window, "new-tab"));
    assert_cancels_hold("a reorder", |window| {
        let tab_view = window.imp().tab_view.get();
        assert!(tab_view.reorder_page(&tab_view.nth_page(5), 6));
    });
    assert_cancels_hold("a transfer", |window| {
        window.imp().tab_view.notify("is-transferring-page");
    });
    assert_cancels_hold("the bar's scroll controller", |window| {
        let scroll = bar_scroll_observer(window);
        let _ = scroll.emit_by_name::<bool>("scroll", &[&0.0f64, &1.0f64]);
    });
    assert_cancels_hold("the bar's press gesture", |window| {
        let press_gesture = bar_press_observer(window);
        press(&press_gesture);
    });
}

#[test]
fn test_observers_cancel_nothing_while_no_step_is_active_or_inside_the_group() {
    let window = overflowing_window();
    let scroll = bar_scroll_observer(&window);
    let press_gesture = bar_press_observer(&window);
    let _ = scroll.emit_by_name::<bool>("scroll", &[&0.0f64, &1.0f64]);
    press(&press_gesture);
    assert_eq!(evidence(&window).guard_cancellations, 0);

    // A press resolved inside the end group, during a hold.
    timing(LONG_REPEAT_DELAY_MS, 10);
    let (_, forward) = buttons(&window);
    press(&hold_gesture(&forward));
    let group = evidence(&window).group.expect("the end group");
    let bounds = group
        .compute_bounds(&*window.imp().tab_bar)
        .expect("the group has bounds inside the bar");
    let (x, y) = (
        f64::from(bounds.x() + bounds.width() / 2.0),
        f64::from(bounds.y() + bounds.height() / 2.0),
    );
    press_gesture.emit_by_name::<()>("pressed", &[&1i32, &x, &y]);
    let now = evidence(&window);
    assert!(
        now.repeat_active,
        "a press inside the group cancels nothing"
    );
    assert_eq!(now.guard_cancellations, 0);
}

#[test]
fn test_scroll_buttons_never_take_focus_on_click_and_hold_in_the_capture_phase() {
    let window = overflowing_window();
    let (back, forward) = buttons(&window);
    for button in [&back, &forward] {
        assert!(!button.gets_focus_on_click());
        assert_eq!(hold_gesture(button).button(), gtk4::gdk::BUTTON_PRIMARY);
    }
    assert!(crate::common::accessible_label_is(
        &back,
        "Scroll Tabs Back"
    ));
    assert!(crate::common::accessible_label_is(
        &forward,
        "Scroll Tabs Forward"
    ));
    assert_eq!(back.tooltip_text().as_deref(), Some("Scroll Tabs Back"));
}

#[test]
fn test_pinned_tabs_stay_put_while_the_strip_scrolls() {
    let window = overflowing_window();
    let tab_view = window.imp().tab_view.get();
    tab_view.set_page_pinned(&tab_view.nth_page(0), true);
    flush_events();
    settled(&window);
    scroll_to(&window, 0.0);
    let pinned = find_tab_scroller(window.imp().tab_bar.upcast_ref(), true).expect("pinned");
    let pinned_value = pinned.hadjustment().value();
    let pinned_x = pinned
        .compute_bounds(&*window.imp().tab_bar)
        .map(|bounds| bounds.x());
    activate_action(&window, "scroll-tabs-forward");
    settled(&window);
    assert!(scroll_value(&window) > 0);
    assert_eq!(pinned.hadjustment().value(), pinned_value);
    assert_eq!(
        pinned
            .compute_bounds(&*window.imp().tab_bar)
            .map(|bounds| bounds.x()),
        pinned_x
    );
}

#[test]
fn test_no_tabs_hides_the_group() {
    let window = overflowing_window();
    let tab_view = window.imp().tab_view.get();
    while tab_view.n_pages() > 0 {
        tab_view.close_page(&tab_view.nth_page(0));
        flush_events();
    }
    settled(&window);
    let now = evidence(&window);
    assert!(!now.controls_visible && !now.group_attached);
    assert_eq!(now.scroll_actions_enabled, [false, false]);
}

#[test]
fn test_a_restored_session_publishes_the_controls_once_at_its_terminal() {
    let window = window_at(1000);
    let dir = tempfile::tempdir().expect("session tempdir");
    let tabs = (0..40)
        .map(|index| {
            let path = dir.path().join(format!("restored-{index:02}.txt"));
            fixture::write_text(&path, "restored\n");
            SessionTab {
                path: Some(path),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }
        })
        .collect();
    window.restore_session_for_test(SessionData {
        tabs,
        active_tab_index: Some(0),
    });
    assert!(wait_until_or_false(Duration::from_secs(20), || {
        let now = evidence(&window);
        window.imp().tab_view.n_pages() == 40 && now.controls_visible && !now.reconcile_pending
    }));
    settled(&window);
    let now = evidence(&window);
    assert!(now.controls_visible);
    assert_eq!(
        now.controls_shown_count, 1,
        "shown once, after the restored tabs are in place"
    );
}

#[test]
fn test_right_to_left_maps_back_to_the_upper_end() {
    ensure_gtk_init();
    let default_direction = gtk4::Widget::default_direction();
    gtk4::Widget::set_default_direction(gtk4::TextDirection::Rtl);
    let window = window_at(1000);
    open_tabs(&window, 40);
    // The first tab is at the upper end in right to left (ledger A132).
    scroll_to(&window, f64::from(max_value(&window)));
    let now = evidence(&window);
    assert!(now.rtl);
    assert_eq!(
        now.scroll_actions_enabled,
        [false, true],
        "at the first tab Back is off"
    );
    activate_action(&window, "scroll-tabs-forward");
    settled(&window);
    assert!(
        scroll_value(&window) < max_value(&window),
        "Forward moves toward the lower end"
    );
    gtk4::Widget::set_default_direction(default_direction);
}

#[test]
fn test_the_lookup_fails_closed_on_a_bar_shaped_tree_without_an_unpinned_scroller() {
    ensure_gtk_init();
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let revealer = gtk4::Revealer::new();
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let pinned = gtk4::ScrolledWindow::new();
    pinned.add_css_class("pinned");
    row.append(&gtk4::Label::new(Some("start")));
    row.append(&pinned);
    row.append(&gtk4::Label::new(Some("end")));
    revealer.set_child(Some(&row));
    root.append(&revealer);
    assert!(find_tab_scroller(root.upcast_ref(), false).is_none());
    assert_eq!(
        find_tab_scroller(root.upcast_ref(), true).as_ref(),
        Some(&pinned)
    );
}

#[test]
fn test_the_minimum_window_keeps_the_tab_strip_usable_with_the_controls_shown() {
    let window = window_at(640);
    open_tabs(&window, 40);
    let now = evidence(&window);
    assert!(now.controls_visible);
    let strip = unpinned(&window);
    assert!(
        strip.width() >= 100,
        "the tabs keep a usable width beside the controls"
    );
    let group = now.group.expect("group");
    let bar = window.imp().tab_bar.get();
    let group_bounds = group.compute_bounds(&bar).expect("group bounds");
    assert!(
        f64::from(group_bounds.x() + group_bounds.width()) <= f64::from(bar.width()) + 0.5,
        "the controls stay inside the bar at the minimum width"
    );
}

/// Proof 1 of 3 — **reentrancy**: the surface reads stably after each
/// operation that mutates the state it reads (a reconcile, a step, a hold and
/// its guard cancellation), and reading changes nothing.
#[test]
fn test_tab_overflow_evidence_reads_stay_side_effect_free_across_step_hold_and_cancellation() {
    let window = overflowing_window();
    let first = evidence(&window);
    assert_eq!(first, evidence(&window));
    activate_action(&window, "scroll-tabs-forward");
    settled(&window);
    let stepped = evidence(&window);
    assert_ne!(stepped.extent, first.extent, "the drive changed the state");
    assert_eq!(stepped, evidence(&window));
    timing(LONG_REPEAT_DELAY_MS, 10);
    let (_, forward) = buttons(&window);
    press(&hold_gesture(&forward));
    let holding = evidence(&window);
    assert!(holding.repeat_active);
    assert_eq!(holding, evidence(&window));
    let tab_view = window.imp().tab_view.get();
    tab_view.set_selected_page(&tab_view.nth_page(3));
    let cancelled = evidence(&window);
    assert_eq!(cancelled.guard_cancellations, 1);
    assert_eq!(cancelled, evidence(&window));
}

/// Proof 2 of 3 — **disposal honesty**: the bar and the tab view are template
/// children, which dispose clears before `Drop`.
#[test]
fn test_tab_overflow_evidence_answers_honestly_after_the_window_is_disposed() {
    let window = overflowing_window();
    window.close();
    flush_events();
    // SAFETY: this test window is disposed exactly once, and everything after
    // this point only reads the evidence surface.
    unsafe { window.run_dispose() };
    let disposed = evidence(&window);
    assert!(!disposed.repeat_active && !disposed.animation_active && !disposed.reconcile_pending);
    assert_eq!(disposed.bar_height, 0);
    assert_eq!(evidence(&window), disposed);
}

/// Proof 3 of 3 — **no materialization**: the tab box holds every tab eagerly;
/// repeated reads create no tab widget, schedule nothing and move nothing.
#[test]
fn test_tab_overflow_evidence_reads_materialize_nothing() {
    let window = overflowing_window();
    let strip = unpinned(&window);
    let count_tabs = || {
        let mut count = 0;
        let mut child = strip.child().and_then(|tab_box| tab_box.first_child());
        while let Some(current) = child {
            count += 1;
            child = current.next_sibling();
        }
        count
    };
    let tabs = count_tabs();
    let before = evidence(&window);
    for _ in 0..5 {
        assert_eq!(evidence(&window), before);
    }
    assert_eq!(count_tabs(), tabs);
    assert!(
        !evidence(&window).reconcile_pending,
        "a read schedules no reconcile"
    );
}
