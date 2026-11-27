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
use glib::translate::IntoGlib;
use gtk4::gio;
use gtk4::prelude::*;
use lushtext_model::session::{SessionData, SessionTab};
use lushtext_services::filesystem::fixture;
use lushtext_ui::ui::window::{
    LushtextAllTabsPopover, LushtextWindow, TabOverflowEvidence, TabOverflowTestTiming,
    find_tab_scroller, set_tab_overflow_test_timing, tab_overflow_evidence,
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
            !now.reconcile_pending && !now.animation_active && !now.selection_reveal_pending
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

fn restored_session(dir: &std::path::Path, count: usize, active: usize) -> SessionData {
    let tabs = (0..count)
        .map(|index| {
            let path = dir.join(format!("restored-{index:02}.txt"));
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
    SessionData {
        tabs,
        active_tab_index: Some(active),
    }
}

/// Every `AdwTab` of the unpinned tab box with the page it shows (the same
/// walk as the A132/A133 probes' `tab_widgets`).
fn tab_widgets(window: &LushtextWindow) -> Vec<(libadwaita::TabPage, gtk4::Widget)> {
    let mut tabs = Vec::new();
    let Some(tab_box) = unpinned(window).child() else {
        return tabs;
    };
    let mut container = tab_box.first_child();
    while let Some(current) = container {
        let mut child = current.first_child();
        while let Some(candidate) = child {
            if candidate.type_().name() == "AdwTab"
                && candidate.find_property("page").is_some()
                && let Some(page) = candidate.property::<Option<libadwaita::TabPage>>("page")
            {
                tabs.push((page, candidate.clone()));
            }
            child = candidate.next_sibling();
        }
        container = current.next_sibling();
    }
    tabs
}

/// Whether `page`'s whole tab lies inside the unpinned scroller, to a pixel
/// (the probes' `tab_inside`).
fn tab_inside(window: &LushtextWindow, page: &libadwaita::TabPage) -> bool {
    let scroller = unpinned(window);
    tab_widgets(window)
        .into_iter()
        .find(|(shown, _)| shown == page)
        .and_then(|(_, tab)| tab.compute_bounds(&scroller))
        .is_some_and(|bounds| {
            f64::from(bounds.x()) >= -1.0
                && f64::from(bounds.x() + bounds.width()) <= f64::from(scroller.width()) + 1.0
        })
}

fn restore_and_settle(window: &LushtextWindow, session: SessionData) {
    let count = i32::try_from(session.tabs.len()).expect("tab count");
    window.restore_session_for_test(session);
    assert!(
        wait_until_or_false(Duration::from_secs(20), || {
            window.imp().tab_view.n_pages() == count && !window.session_restore_evidence().restoring
        }),
        "the restore reached its terminal"
    );
    settled(window);
}

fn assert_selected_tab_in_view(window: &LushtextWindow, shape: &str) {
    let tab_view = window.imp().tab_view.get();
    let selected = tab_view.selected_page().expect("a selected tab");
    assert!(
        tab_inside(window, &selected),
        "{shape}: the selected tab (position {}) is out of view at strip value {:?}",
        tab_view.page_position(&selected),
        evidence(window).extent
    );
    // The evidence (and the automation snapshot it feeds) reads the same.
    assert_eq!(
        evidence(window).selected_tab_in_view,
        Some(true),
        "{shape}: the evidence reports the selected tab in view"
    );
}

/// The number of new tabs at which the strip first overflows at `width`.
fn first_overflowing_count(width: i32) -> usize {
    let probe = window_at(width);
    let mut count = 0;
    while !evidence(&probe).controls_visible {
        assert!(count < 64, "the strip overflows before 64 tabs");
        open_tabs(&probe, 1);
        count += 1;
    }
    probe.close();
    flush_events();
    count
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
        now.group_attached && now.all_tabs_button_visible,
        "three tabs that fit attach the group for the All Tabs button alone"
    );
    assert_eq!(now.all_tabs_count, 3);
    assert_eq!(now.scroll_actions_enabled, [false, false]);
    let (back, forward) = buttons(&window);
    assert!(!back.property::<bool>("visible") && !forward.property::<bool>("visible"));
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
    assert!(
        hidden.group_attached && hidden.all_tabs_button_visible,
        "the All Tabs button stays"
    );
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

    // With both hidden (the tabs fit), focus goes to the All Tabs button, and
    // from there to the selected editor once it hides with one tab left.
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
    let (all_tabs, _) = evidence(&window).all_tabs.expect("the All Tabs button");
    assert!(
        gtk4::prelude::GtkWindowExt::focus(&window)
            .is_some_and(|focus| focus == *all_tabs.upcast_ref::<gtk4::Widget>()
                || focus.is_ancestor(&all_tabs)),
        "focus hands off to All Tabs when both scroll buttons are gone"
    );
    tab_view.close_page(&tab_view.nth_page(1));
    flush_events();
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
        // GtkButton's own click gesture is off, so no pointer release can
        // emit `clicked` and add a step.
        let controllers = button.observe_controllers();
        let own_clicks: Vec<_> = (0..controllers.n_items())
            .filter_map(|index| controllers.item(index).and_downcast::<gtk4::GestureClick>())
            .filter(|gesture| gesture.propagation_phase() != gtk4::PropagationPhase::Capture)
            .collect();
        assert!(
            !own_clicks.is_empty(),
            "GtkButton's own click gesture exists"
        );
        assert!(
            own_clicks
                .iter()
                .all(|gesture| gesture.propagation_phase() == gtk4::PropagationPhase::None),
            "GtkButton's own click gesture is disabled"
        );
    }
    // Keyboard activation (Space or Enter) still runs the action: one step.
    let before = scroll_value(&window);
    assert!(forward.activate(), "Forward activates");
    // GtkButton emits `clicked` once its activate timeout ends.
    crate::common::flush_after_delay(Duration::from_millis(400));
    settled(&window);
    assert!(
        scroll_value(&window) > before,
        "keyboard activation steps once"
    );
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
fn test_a_restore_leaves_the_selected_tab_in_view() {
    // Shape 1: the first of forty tabs, which every later attach scrolls away
    // from (ledger A132).
    let window = window_at(1000);
    let dir = tempfile::tempdir().expect("session tempdir");
    restore_and_settle(&window, restored_session(dir.path(), 40, 0));
    assert_selected_tab_in_view(&window, "first of forty");
    // Scrolled to its end, the strip no longer shows the selected first tab.
    scroll_to(&window, f64::from(max_value(&window)));
    assert_eq!(
        evidence(&window).selected_tab_in_view,
        Some(false),
        "the evidence reports the selected tab out of view at the strip's end"
    );

    // Shape 2: a tab in the middle of the strip.
    let window = window_at(1000);
    let dir = tempfile::tempdir().expect("session tempdir");
    restore_and_settle(&window, restored_session(dir.path(), 40, 20));
    assert_selected_tab_in_view(&window, "middle of forty");

    // Shape 3: only the last tab overflows, so the selected first tab was
    // still on screen when the restore's last attach scrolled away from it.
    let count = first_overflowing_count(1000);
    let window = window_at(1000);
    let dir = tempfile::tempdir().expect("session tempdir");
    restore_and_settle(&window, restored_session(dir.path(), count, 0));
    assert!(evidence(&window).controls_visible, "{count} tabs overflow");
    assert_selected_tab_in_view(&window, "first of the first overflowing count");
}

#[test]
fn test_a_restore_cancelled_without_publishing_still_reconciles_the_controls() {
    let window = window_at(1000);
    let dir = tempfile::tempdir().expect("session tempdir");
    window.restore_session_for_test(restored_session(dir.path(), 40, 0));
    // A strip event while the restore's projection batch is open: the
    // reconcile defers to the batch's terminal.
    unpinned(&window)
        .hadjustment()
        .emit_by_name::<()>("changed", &[]);
    // One main-loop iteration at a time: the zero-delay reconcile runs before
    // the restore's idle turns can reach the terminal.
    let context = glib::MainContext::default();
    let mut waits = false;
    for _ in 0..1_000 {
        context.iteration(false);
        if evidence(&window).reconcile_waits_for_terminal {
            waits = true;
            break;
        }
    }
    assert!(waits, "the reconcile waits for the restore's terminal");
    assert!(window.session_restore_evidence().restoring);
    // A superseding empty session cancels the batch without publishing.
    window.restore_session_for_test(SessionData {
        tabs: Vec::new(),
        active_tab_index: None,
    });
    assert!(
        wait_until_or_false(SETTLE, || !evidence(&window).reconcile_pending),
        "the cancelled batch leaves the tab-overflow reconcile pending"
    );
    let now = evidence(&window);
    assert_eq!(
        now.controls_visible,
        window.imp().tab_view.n_pages() > 20,
        "the controls follow the strip once the batch is released"
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

// --- All Tabs (tasks 3.1-3.4) -------------------------------------------

fn all_tabs(window: &LushtextWindow) -> (gtk4::MenuButton, LushtextAllTabsPopover) {
    evidence(window)
        .all_tabs
        .expect("the All Tabs button and popover are built")
}

fn popover_settled(window: &LushtextWindow) {
    assert!(
        wait_until_or_false(SETTLE, || {
            let now = evidence(window);
            !now.row_growth_pending && !now.reconcile_pending
        }),
        "the All Tabs popover settled"
    );
    if let Some((_, popover)) = evidence(window).all_tabs
        && popover.is_visible()
    {
        // The rows a publish listed are realized in the popover's next frame.
        crate::common::wait_for_painted_frame(&popover);
    }
    flush_events();
}

fn open_all_tabs(window: &LushtextWindow) {
    activate_action(window, "toggle-all-tabs");
    assert!(
        wait_until_or_false(SETTLE, || evidence(window).popover_visible),
        "Ctrl+Shift+O's action opens the popover"
    );
    popover_settled(window);
}

/// One realized row: (title, subtitle, modified, pinned, current).
type PopoverRow = (String, String, bool, bool, bool);

/// The realized rows, top to bottom.
fn popover_rows(window: &LushtextWindow) -> Vec<PopoverRow> {
    let (_, popover) = all_tabs(window);
    let mut rows: Vec<(f32, PopoverRow)> = crate::common::realized_list_rows(&popover.list_view())
        .into_iter()
        .filter_map(|row| {
            let content = row.first_child()?;
            let marker = content.first_child()?;
            let text = marker.next_sibling()?;
            let pinned = text.next_sibling()?;
            let current = pinned.next_sibling()?;
            let labels = crate::common::labels_in(&text);
            let y = row
                .compute_bounds(&popover.list_view())
                .map_or(0.0, |bounds| bounds.y());
            Some((
                y,
                (
                    labels.first().cloned().unwrap_or_default(),
                    labels.get(1).cloned().unwrap_or_default(),
                    marker.opacity() > 0.5,
                    pinned.property::<bool>("visible"),
                    current.property::<bool>("visible"),
                ),
            ))
        })
        .collect();
    rows.sort_by(|left, right| left.0.total_cmp(&right.0));
    rows.into_iter().map(|(_, row)| row).collect()
}

fn titles(window: &LushtextWindow) -> Vec<String> {
    popover_rows(window).into_iter().map(|row| row.0).collect()
}

fn set_query(window: &LushtextWindow, query: &str) {
    let parameter = query.to_variant();
    gio::prelude::ActionGroupExt::activate_action(window, "set-all-tabs-query", Some(&parameter));
    flush_events();
    popover_settled(window);
}

fn open_files(window: &LushtextWindow, dir: &std::path::Path, names: &[&str]) {
    for name in names {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fixture::create_dir_all(parent);
        }
        fixture::write_text(&path, "contents\n");
        window.open_document(&path);
    }
    let count = i32::try_from(names.len()).expect("count");
    assert!(wait_until_or_false(SETTLE, || {
        let tab_view = window.imp().tab_view.get();
        tab_view.n_pages() >= count
            && (0..tab_view.n_pages()).all(|index| {
                tab_view
                    .nth_page(index)
                    .child()
                    .downcast::<lushtext_ui::ui::editor_page::LushtextEditorPage>()
                    .is_ok_and(|editor| {
                        editor.load_state()
                            != lushtext_ui::ui::editor_page::EditorLoadState::Loading
                    })
            })
    }));
    settled(window);
}

fn focus_is(window: &LushtextWindow, widget: &impl IsA<gtk4::Widget>) -> bool {
    gtk4::prelude::GtkWindowExt::focus(window).is_some_and(|focus| {
        focus == *widget.upcast_ref::<gtk4::Widget>() || focus.is_ancestor(widget)
    })
}

#[test]
fn test_all_tabs_button_hides_with_one_tab_and_shows_the_count_from_two() {
    let window = window_at(1000);
    open_tabs(&window, 1);
    let one = evidence(&window);
    assert!(!one.all_tabs_button_visible);
    assert!(
        !one.group_attached,
        "one tab leaves no end action widget, so no empty space"
    );
    assert_eq!(one.all_tabs_actions_enabled, [false, false, false]);
    open_tabs(&window, 1);
    let two = evidence(&window);
    assert!(two.all_tabs_button_visible && two.group_attached);
    assert_eq!(two.all_tabs_count, 2);
    assert_eq!(two.all_tabs_actions_enabled, [true, true, false]);
    let (button, _) = all_tabs(&window);
    assert!(button.property::<bool>("visible"));
    assert!(crate::common::accessible_label_is(
        &button,
        "All Tabs, 2 open"
    ));
    assert_eq!(button.tooltip_text().as_deref(), Some("All Tabs, 2 open"));
    assert!(!button.gets_focus_on_click());
}

#[test]
fn test_all_tabs_count_follows_open_and_close_and_a_restore_publishes_it_once() {
    let window = window_at(1000);
    open_tabs(&window, 3);
    assert_eq!(evidence(&window).all_tabs_count, 3);
    let tab_view = window.imp().tab_view.get();
    tab_view.close_page(&tab_view.nth_page(0));
    flush_events();
    settled(&window);
    assert_eq!(evidence(&window).all_tabs_count, 2);

    let restored = window_at(1000);
    let dir = tempfile::tempdir().expect("session tempdir");
    let before = evidence(&restored).all_tabs_count_publications;
    restore_and_settle(&restored, restored_session(dir.path(), 40, 0));
    let now = evidence(&restored);
    assert_eq!(now.all_tabs_count, 40);
    assert_eq!(
        now.all_tabs_count_publications,
        before + 1,
        "the count updates once, at the restore's terminal"
    );
}

#[test]
fn test_all_tabs_count_is_never_clipped_and_the_strip_keeps_a_usable_width() {
    for (tabs, digits) in [(2usize, 1usize), (12, 2), (128, 3)] {
        let window = window_at(640);
        open_tabs(&window, tabs);
        // A window can start with the previous window's saved tabs.
        let tab_view = window.imp().tab_view.get();
        while usize::try_from(tab_view.n_pages()).expect("count") > tabs {
            tab_view.close_page(&tab_view.nth_page(0));
            flush_events();
        }
        settled(&window);
        let (button, _) = all_tabs(&window);
        let label = button
            .child()
            .and_downcast::<gtk4::Label>()
            .expect("the count label");
        assert_eq!(label.text().chars().count(), digits);
        assert_eq!(label.text().as_str(), tabs.to_string());
        let (_, natural, _, _) = label.measure(gtk4::Orientation::Horizontal, -1);
        assert!(
            label.width() >= natural,
            "{tabs} tabs: the count is allocated its whole width"
        );
        assert!(
            unpinned(&window).width() >= 100,
            "{tabs} tabs: the strip keeps a usable width at the 640 px minimum"
        );
        window.close();
        flush_events();
    }
    // Large text: the desktop's text scaling enlarges the count, which still
    // fits.
    let settings = gtk4::Settings::default().expect("default GTK settings");
    let dpi = settings.gtk_xft_dpi();
    settings.set_gtk_xft_dpi(dpi.max(96 * 1024) * 3 / 2);
    let window = window_at(640);
    open_tabs(&window, 128);
    let (button, _) = all_tabs(&window);
    let label = button
        .child()
        .and_downcast::<gtk4::Label>()
        .expect("the count label");
    let (_, natural, _, _) = label.measure(gtk4::Orientation::Horizontal, -1);
    assert!(label.width() >= natural, "large text: 128 shows in full");
    assert!(unpinned(&window).width() >= 100);
    settings.set_gtk_xft_dpi(dpi);
}

#[test]
fn test_all_tabs_popover_lists_pinned_modified_and_untitled_rows_in_strip_order() {
    let window = window_at(1000);
    let dir = tempfile::tempdir().expect("tempdir");
    open_files(&window, dir.path(), &["pinned.txt", "modified.txt"]);
    activate_action(&window, "new-tab");
    settled(&window);
    let tab_view = window.imp().tab_view.get();
    let modified = tab_view
        .nth_page(1)
        .child()
        .downcast::<lushtext_ui::ui::editor_page::LushtextEditorPage>()
        .expect("an editor");
    let buffer = modified.buffer();
    buffer.insert(&mut buffer.end_iter(), "changed");
    tab_view.set_page_pinned(&tab_view.nth_page(0), true);
    tab_view.set_selected_page(&tab_view.nth_page(1));
    settled(&window);
    open_all_tabs(&window);
    let rows = popover_rows(&window);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].0, "pinned.txt");
    assert!(rows[0].3, "the pinned row shows its pin");
    assert_eq!(rows[1].0, "modified.txt");
    assert!(rows[1].2, "the modified row shows its unsaved-changes dot");
    assert!(rows[1].4, "the selected tab's row is marked current");
    assert_eq!(rows[2].1, "Not saved to a file");
    assert!(!rows[0].2 && !rows[2].2);
    assert_eq!(
        evidence(&window).popover_highlighted_position,
        Some(1),
        "the current row is the highlighted one"
    );
}

#[test]
fn test_all_tabs_row_text_measures_like_a_label_in_situ() {
    let window = window_at(1000);
    let dir = tempfile::tempdir().expect("tempdir");
    open_files(&window, dir.path(), &["measured.txt", "other.txt"]);
    open_all_tabs(&window);
    let (_, popover) = all_tabs(&window);
    let row = crate::common::realized_list_rows(&popover.list_view())
        .into_iter()
        .next()
        .expect("a realized row");
    let text_box = row
        .first_child()
        .and_then(|content| content.first_child())
        .and_then(|marker| marker.next_sibling())
        .and_downcast::<gtk4::Box>()
        .expect("the row's text box");
    let mut child = text_box.first_child();
    let mut compared = 0;
    while let Some(widget) = child {
        let row_text = widget
            .clone()
            .downcast::<lushtext_ui::ui::row_text::LushtextRowText>()
            .expect("row text");
        let label = gtk4::Label::new(Some(&row_text.text()));
        for class in row_text.css_classes() {
            label.add_css_class(&class);
        }
        label.set_ellipsize(row_text.ellipsize());
        text_box.append(&label);
        flush_events();
        let (_, text_height, _, _) = row_text.measure(gtk4::Orientation::Vertical, -1);
        let (_, label_height, _, _) = label.measure(gtk4::Orientation::Vertical, -1);
        assert_eq!(
            text_height,
            label_height,
            "{:?}: row text measures like a label inside the popover row",
            row_text.css_classes()
        );
        text_box.remove(&label);
        compared += 1;
        child = widget.next_sibling();
    }
    assert_eq!(compared, 2, "the title and the subtitle were compared");
}

#[test]
fn test_all_tabs_filters_by_name_and_folder_and_restores_strip_order() {
    let window = window_at(1000);
    let dir = tempfile::tempdir().expect("tempdir");
    open_files(
        &window,
        dir.path(),
        &[
            "notes.md",
            "main.rs",
            "README.md",
            "search/mod.rs",
            "palette/mod.rs",
        ],
    );
    let tab_view = window.imp().tab_view.get();
    tab_view.set_selected_page(&tab_view.nth_page(2));
    settled(&window);
    open_all_tabs(&window);
    set_query(&window, "md");
    let listed = titles(&window);
    assert_eq!(&listed[..2], ["notes.md", "README.md"]);
    let folder_has_md = dir
        .path()
        .display()
        .to_string()
        .to_lowercase()
        .contains("md");
    if !folder_has_md {
        assert!(
            !listed.contains(&"main.rs".to_owned()),
            "main.rs does not match md"
        );
    }
    assert_eq!(evidence(&window).popover_highlighted_position, Some(0));

    set_query(&window, "palette");
    let rows = popover_rows(&window);
    assert_eq!(rows[0].0, "mod.rs");
    assert!(
        rows[0].1.ends_with("palette"),
        "the palette folder's mod.rs first"
    );
    assert_eq!(evidence(&window).popover_highlighted_position, Some(0));

    set_query(&window, "zzzz-no-such-tab");
    let none = evidence(&window);
    assert!(none.popover_empty_state_visible, "the no-match state shows");
    assert_eq!(none.popover_rows, 0);
    assert_eq!(none.popover_highlighted_position, None);

    set_query(&window, "");
    let cleared = evidence(&window);
    assert!(!cleared.popover_empty_state_visible);
    assert_eq!(cleared.popover_rows, 5);
    assert_eq!(
        titles(&window),
        ["notes.md", "main.rs", "README.md", "mod.rs", "mod.rs"],
        "a cleared query lists strip order"
    );
    assert_eq!(cleared.popover_highlighted_position, Some(2));
}

#[test]
fn test_enter_selects_an_out_of_view_tab_with_its_editor_focused() {
    let window = overflowing_window();
    let tab_view = window.imp().tab_view.get();
    // The last tab is the selected one, scrolled out of view.
    let target = tab_view.nth_page(39);
    assert_eq!(tab_view.selected_page().as_ref(), Some(&target));
    assert!(
        !tab_inside(&window, &target),
        "the last tab starts out of view"
    );
    open_all_tabs(&window);
    let (_, popover) = all_tabs(&window);
    for _ in 0..39 {
        popover_key(&popover, gtk4::gdk::Key::Down);
    }
    popover_settled(&window);
    assert_eq!(evidence(&window).popover_highlighted_position, Some(39));
    popover.search_entry().emit_by_name::<()>("activate", &[]);
    flush_events();
    settled(&window);
    assert!(
        !evidence(&window).popover_visible,
        "activation closes the popover"
    );
    assert_eq!(tab_view.selected_page().as_ref(), Some(&target));
    assert!(
        tab_inside(&window, &target),
        "the selected tab is visible in the strip: extent {:?}",
        evidence(&window).extent
    );
    let editor = crate::common::active_editor(&window);
    assert!(
        wait_until_or_false(SETTLE, || focus_is(&window, editor.source_view())),
        "typing goes into the selected document"
    );

    // A tab that is neither selected nor in view.
    scroll_to(&window, 0.0);
    let other = tab_view.nth_page(30);
    assert!(!tab_inside(&window, &other));
    open_all_tabs(&window);
    let (_, popover) = all_tabs(&window);
    for _ in 0..9 {
        popover_key(&popover, gtk4::gdk::Key::Up);
    }
    assert_eq!(evidence(&window).popover_highlighted_position, Some(30));
    popover.search_entry().emit_by_name::<()>("activate", &[]);
    flush_events();
    settled(&window);
    assert_eq!(tab_view.selected_page().as_ref(), Some(&other));
    assert!(
        tab_inside(&window, &other),
        "the picked tab is visible in the strip"
    );
}

/// Emit `key` on the search entry's highlight-key controller, as a press
/// in the entry would.
fn popover_key(popover: &LushtextAllTabsPopover, key: gtk4::gdk::Key) {
    let controllers = popover.search_entry().observe_controllers();
    let keys = (0..controllers.n_items())
        .filter_map(|index| {
            controllers
                .item(index)
                .and_downcast::<gtk4::EventControllerKey>()
        })
        .find(|controller| controller.name().as_deref() == Some("all-tabs-highlight-keys"))
        .expect("the search entry's highlight-key controller");
    let _ = keys.emit_by_name::<bool>(
        "key-pressed",
        &[&key.into_glib(), &0u32, &gtk4::gdk::ModifierType::empty()],
    );
    flush_events();
}

#[test]
fn test_escape_and_the_shortcut_again_keep_the_selection_and_restore_focus() {
    let window = window_at(1000);
    open_tabs(&window, 3);
    let tab_view = window.imp().tab_view.get();
    let selected = tab_view.selected_page();
    let editor = crate::common::active_editor(&window);
    for close in ["escape", "shortcut", "click"] {
        crate::common::flush_after_delay(Duration::from_millis(300));
        assert!(editor.source_view().grab_focus());
        flush_events();
        if close == "click" {
            let (button, _) = all_tabs(&window);
            button.popup();
            flush_events();
            popover_settled(&window);
        } else {
            open_all_tabs(&window);
        }
        let (_, popover) = all_tabs(&window);
        assert!(evidence(&window).popover_visible, "{close}: open");
        match close {
            "escape" => popover
                .search_entry()
                .emit_by_name::<()>("stop-search", &[]),
            _ => activate_action(&window, "toggle-all-tabs"),
        }
        flush_events();
        assert!(
            wait_until_or_false(SETTLE, || !evidence(&window).popover_visible),
            "{close}: closed"
        );
        assert_eq!(
            tab_view.selected_page(),
            selected,
            "{close}: selection kept"
        );
        assert!(
            wait_until_or_false(SETTLE, || focus_is(&window, editor.source_view())),
            "{close}: the focus saved before opening comes back"
        );
    }
}

#[test]
fn test_escape_in_the_search_entry_closes_the_popover_and_nothing_under_it() {
    let window = window_at(1000);
    open_tabs(&window, 3);
    activate_action(&window, "toggle-search-panel");
    assert!(
        wait_until_or_false(SETTLE, || window
            .imp()
            .search_panel_revealer
            .reveals_child()),
        "precondition: the search panel shows"
    );
    open_all_tabs(&window);
    assert!(evidence(&window).popover_visible, "precondition: open");
    let (_, popover) = all_tabs(&window);
    // Escape in the entry: `stop-search` runs first, then the window's
    // Bubble-phase controller sees the same key press (the transient
    // dismissal ladder's child-handled latch).
    popover
        .search_entry()
        .emit_by_name::<()>("stop-search", &[]);
    assert!(window.handle_transient_escape_for_test());
    flush_events();
    assert!(
        wait_until_or_false(SETTLE, || !evidence(&window).popover_visible),
        "the popover closed"
    );
    assert!(
        window.imp().search_panel_revealer.reveals_child(),
        "the same Escape leaves the search panel open"
    );
}

#[test]
fn test_a_tab_closed_while_open_leaves_the_list_and_a_stale_activation_only_closes() {
    let window = window_at(1000);
    open_tabs(&window, 4);
    open_all_tabs(&window);
    let tab_view = window.imp().tab_view.get();
    let closing = tab_view.nth_page(3);
    let (_, popover) = all_tabs(&window);
    // Highlight the row of the tab about to close.
    for _ in 0..3 {
        popover_key(&popover, gtk4::gdk::Key::Down);
    }
    tab_view.close_page(&closing);
    flush_events();
    popover_settled(&window);
    assert_eq!(
        evidence(&window).popover_rows,
        3,
        "the closed tab's row is gone"
    );
    let selected = tab_view.selected_page();
    assert!(evidence(&window).popover_visible);
    // A row activated after its tab closed, before the re-snapshot ran:
    // only the popover closes. The activation runs as the tab detaches, so
    // the popover still lists the closed tab's row.
    popover_key(&popover, gtk4::gdk::Key::Up);
    let highlighted = evidence(&window)
        .popover_highlighted_position
        .expect("a highlighted row");
    let stale = tab_view.nth_page(i32::try_from(highlighted).expect("position"));
    assert_ne!(Some(&stale), selected.as_ref());
    let entry = popover.search_entry();
    let handler = tab_view.connect_page_detached({
        let stale = stale.clone();
        move |_, page, _| {
            if *page == stale {
                entry.emit_by_name::<()>("activate", &[]);
            }
        }
    });
    tab_view.close_page(&stale);
    flush_events();
    tab_view.disconnect(handler);
    assert!(wait_until_or_false(SETTLE, || !evidence(&window).popover_visible));
    assert_eq!(tab_view.selected_page(), selected, "no selection changed");
}

#[test]
fn test_256_tabs_at_the_minimum_size_scroll_only_the_list() {
    let window = window_at(640);
    open_tabs(&window, 256);
    open_all_tabs(&window);
    let now = evidence(&window);
    assert_eq!(now.popover_rows, 256);
    assert_eq!(now.popover_listed_rows, 256, "the staged publish finished");
    let (_, popover) = all_tabs(&window);
    let entry = popover.search_entry();
    assert!(
        entry.is_mapped() && entry.height() > 0,
        "the search entry stays visible"
    );
    let list = popover.list_scroller().vadjustment();
    assert!(list.upper() > list.page_size(), "only the row list scrolls");
    assert!(
        popover.height() <= window.height(),
        "the popover fits the window's height"
    );
    // Every tab is reachable by searching.
    set_query(&window, "untitled");
    assert_eq!(evidence(&window).popover_rows, 256);
}

#[test]
fn test_ctrl_shift_o_with_one_tab_changes_no_text_and_opens_nothing() {
    let window = window_at(1000);
    open_tabs(&window, 1);
    let editor = crate::common::active_editor(&window);
    assert!(editor.source_view().grab_focus());
    let text = crate::common::editor_text(&editor);
    assert!(!evidence(&window).all_tabs_actions_enabled[0]);
    activate_action(&window, "toggle-all-tabs");
    assert!(!evidence(&window).popover_visible);
    assert_eq!(crate::common::editor_text(&editor), text);
    // No default text-view or source-view binding claims Ctrl+Shift+O.
    let controllers = editor.source_view().observe_controllers();
    for index in 0..controllers.n_items() {
        let Some(shortcuts) = controllers
            .item(index)
            .and_downcast::<gtk4::ShortcutController>()
        else {
            continue;
        };
        for position in 0..shortcuts.n_items() {
            let Some(shortcut) = shortcuts.item(position).and_downcast::<gtk4::Shortcut>() else {
                continue;
            };
            let trigger = shortcut
                .trigger()
                .map(|trigger| trigger.to_str().to_string());
            assert!(
                !trigger.as_deref().is_some_and(|text| {
                    let text = text.to_lowercase();
                    text.contains("<shift><control>o") || text.contains("<control><shift>o")
                }),
                "{trigger:?} claims Ctrl+Shift+O in the editor"
            );
        }
    }
}

#[test]
fn test_one_announcement_per_settled_query() {
    let window = window_at(1000);
    open_tabs(&window, 6);
    open_all_tabs(&window);
    let before = evidence(&window).popover_announcements;
    let (_, popover) = all_tabs(&window);
    for query in ["u", "un", "unt", "unti"] {
        popover.search_entry().set_text(query);
        flush_events();
    }
    assert!(
        wait_until_or_false(SETTLE, || !evidence(&window).popover_announcement_pending),
        "the announcement settles"
    );
    assert_eq!(evidence(&window).popover_announcements, before + 1);
}

#[test]
fn test_a_close_before_the_query_settles_announces_no_match_count() {
    let window = window_at(1000);
    open_tabs(&window, 6);
    open_all_tabs(&window);
    let before = evidence(&window).popover_announcements;
    let (_, popover) = all_tabs(&window);
    popover.search_entry().set_text("unti");
    flush_events();
    assert!(
        evidence(&window).popover_announcement_pending,
        "precondition: the match count waits for the query to settle"
    );
    activate_action(&window, "toggle-all-tabs");
    flush_events();
    assert!(
        wait_until_or_false(SETTLE, || !evidence(&window).popover_visible),
        "the popover closed"
    );
    assert!(
        !evidence(&window).popover_announcement_pending,
        "the close drops the pending announcement"
    );
    // Past the announcement delay (500 ms): a count for a list that is gone
    // would be announced over the restored focus.
    crate::common::flush_after_delay(Duration::from_millis(800));
    assert_eq!(evidence(&window).popover_announcements, before);
}

#[test]
fn test_the_group_detaches_with_the_popover_closed_first() {
    // A tab closed down to one while the popover is open.
    let window = window_at(1000);
    open_tabs(&window, 2);
    open_all_tabs(&window);
    let tab_view = window.imp().tab_view.get();
    tab_view.close_page(&tab_view.nth_page(1));
    flush_events();
    settled(&window);
    let now = evidence(&window);
    assert!(!now.popover_visible && !now.group_attached);
    // Focus Mode entered while it is open.
    let window = window_at(1000);
    open_tabs(&window, 3);
    open_all_tabs(&window);
    activate_action(&window, "toggle-focus-mode");
    flush_events();
    settled(&window);
    let now = evidence(&window);
    assert!(!now.popover_visible && !now.group_attached && !now.all_tabs_button_visible);
    activate_action(&window, "toggle-focus-mode");
    flush_events();
    settled(&window);
}

#[test]
fn test_the_popover_retires_its_rows_after_it_closes() {
    let window = window_at(1000);
    open_tabs(&window, 64);
    open_all_tabs(&window);
    assert!(evidence(&window).popover_listed_rows > 0);
    let hide = false.to_variant();
    gio::prelude::ActionGroupExt::activate_action(&window, "set-all-tabs-visible", Some(&hide));
    flush_events();
    assert!(!evidence(&window).popover_visible);
    assert!(
        wait_until_or_false(SETTLE, || {
            let now = evidence(&window);
            !now.row_retirement_pending && now.popover_listed_rows == 0
        }),
        "the closed popover's rows are retired"
    );
    // Opening again publishes a fresh, staged list.
    open_all_tabs(&window);
    assert_eq!(evidence(&window).popover_listed_rows, 64);
}

#[test]
fn test_set_all_tabs_visible_and_query_drive_the_popover() {
    let window = window_at(1000);
    open_tabs(&window, 3);
    assert!(
        !evidence(&window).all_tabs_actions_enabled[2],
        "no query without the popover"
    );
    let show = true.to_variant();
    gio::prelude::ActionGroupExt::activate_action(&window, "set-all-tabs-visible", Some(&show));
    flush_events();
    popover_settled(&window);
    let open = evidence(&window);
    assert!(open.popover_visible);
    assert_eq!(open.all_tabs_actions_enabled, [true, true, true]);
    set_query(&window, "untitled");
    assert_eq!(evidence(&window).popover_query_len, 8);
    let hide = false.to_variant();
    gio::prelude::ActionGroupExt::activate_action(&window, "set-all-tabs-visible", Some(&hide));
    flush_events();
    let closed = evidence(&window);
    assert!(!closed.popover_visible);
    assert!(!closed.all_tabs_actions_enabled[2]);
}

/// Proof 1 of 3 for the popover half — **reentrancy**: the surface reads
/// stably across popup, filter, growth, close and retirement.
#[test]
fn test_all_tabs_evidence_reads_stay_side_effect_free_across_popup_filter_and_close() {
    let window = window_at(1000);
    open_tabs(&window, 64);
    let closed = evidence(&window);
    assert_eq!(closed, evidence(&window));
    activate_action(&window, "toggle-all-tabs");
    let growing = evidence(&window);
    assert!(growing.popover_visible);
    assert_eq!(growing, evidence(&window));
    popover_settled(&window);
    set_query(&window, "zzzz-no-such-tab");
    let filtered = evidence(&window);
    assert!(filtered.popover_rows < 64, "the filter changed the state");
    assert_eq!(filtered, evidence(&window));
    activate_action(&window, "toggle-all-tabs");
    let retiring = evidence(&window);
    assert!(!retiring.popover_visible);
    assert_eq!(retiring, evidence(&window));
}

/// Proof 3 of 3 for the popover half — **no materialization**: reading the
/// surface never asks the lazy model for a row object.
#[test]
fn test_all_tabs_evidence_reads_create_no_row_object() {
    let window = window_at(1000);
    open_tabs(&window, 256);
    open_all_tabs(&window);
    let before = evidence(&window);
    assert!(before.popover_row_objects > 0, "GTK realized some rows");
    assert!(
        before.popover_row_objects < 256,
        "only the realized band has row objects"
    );
    for _ in 0..5 {
        assert_eq!(
            evidence(&window).popover_row_objects,
            before.popover_row_objects
        );
    }
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
    // Disposed with the popover open: it is closed first, never disposed
    // while still parented.
    open_all_tabs(&window);
    window.close();
    flush_events();
    // SAFETY: this test window is disposed exactly once, and everything after
    // this point only reads the evidence surface.
    unsafe { window.run_dispose() };
    let disposed = evidence(&window);
    assert!(!disposed.repeat_active && !disposed.animation_active && !disposed.reconcile_pending);
    assert_eq!(disposed.bar_height, 0);
    assert!(!disposed.popover_visible && !disposed.row_growth_pending);
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

// --- Review fixes (code-review high, PR #119) -----------------------------

/// The user's own wheel or press on the strip ends a running selection
/// reveal, so its re-announced selection cannot undo the user's scroll. The
/// reveal is started synchronously, by an All Tabs pick of the tab that is
/// already selected, and the input follows in the same dispatch.
#[test]
fn test_a_wheel_or_press_on_the_strip_ends_the_selection_reveal() {
    for input in ["wheel", "press"] {
        let window = overflowing_window();
        open_all_tabs(&window);
        let (_, popover) = all_tabs(&window);
        let selected = window.imp().tab_view.selected_page();
        // The empty query highlights the selected tab; Enter picks it.
        popover.search_entry().emit_by_name::<()>("activate", &[]);
        assert_eq!(
            window.imp().tab_view.selected_page(),
            selected,
            "{input}: the pick keeps the selection"
        );
        assert!(
            evidence(&window).selection_reveal_pending,
            "{input}: picking the selected tab starts the reveal"
        );
        if input == "wheel" {
            let _ =
                bar_scroll_observer(&window).emit_by_name::<bool>("scroll", &[&0.0f64, &1.0f64]);
        } else {
            // On the tabs, away from the trailing control group.
            let bounds = unpinned(&window)
                .compute_bounds(&*window.imp().tab_bar)
                .expect("the unpinned scroller has bounds inside the bar");
            let (x, y) = (
                f64::from(bounds.x() + bounds.width() / 2.0),
                f64::from(bounds.y() + bounds.height() / 2.0),
            );
            bar_press_observer(&window).emit_by_name::<()>("pressed", &[&1i32, &x, &y]);
        }
        assert!(
            !evidence(&window).selection_reveal_pending,
            "{input}: the user's own input on the strip ends the reveal"
        );
        settled(&window);
    }
}

/// A restore whose strip Focus Mode hid shows the selected tab once the strip
/// returns.
#[test]
fn test_a_restore_behind_focus_mode_shows_the_selected_tab_when_the_strip_returns() {
    for active in [0usize, 20, 39] {
        let window = window_at(1000);
        activate_action(&window, "toggle-focus-mode");
        flush_events();
        let dir = tempfile::tempdir().expect("session tempdir");
        restore_and_settle(&window, restored_session(dir.path(), 40, active));
        assert!(
            !window.imp().tab_bar.is_mapped(),
            "Focus Mode hides the strip during the restore"
        );
        assert_eq!(
            evidence(&window).selected_tab_in_view,
            None,
            "a hidden strip shows no tab, in view or not"
        );
        activate_action(&window, "toggle-focus-mode");
        flush_events();
        settled(&window);
        assert_selected_tab_in_view(
            &window,
            &format!("tab {active} of forty, restored behind Focus Mode"),
        );
    }
}

/// With the focus in the popover's search entry, a close down to one tab
/// hides the All Tabs button and closes the popover; the focus saved before
/// opening comes back, with no second hand-off to the editor competing.
#[test]
fn test_a_close_to_one_tab_with_focus_in_the_popover_gives_the_saved_focus_back() {
    let window = window_at(1000);
    open_tabs(&window, 2);
    // Let the new tabs' own editor-focus retries run out first.
    crate::common::flush_after_delay(Duration::from_millis(300));
    let saved = window.imp().new_tab_button.get();
    assert!(
        saved.grab_focus(),
        "the header's New Tab button takes keyboard focus"
    );
    flush_events();
    open_all_tabs(&window);
    let (_, popover) = all_tabs(&window);
    assert!(
        focus_is(&window, &popover.search_entry()),
        "the popover's search entry holds keyboard focus"
    );
    let tab_view = window.imp().tab_view.get();
    let selected = tab_view.selected_page();
    let other = (0..tab_view.n_pages())
        .map(|index| tab_view.nth_page(index))
        .find(|page| Some(page) != selected.as_ref())
        .expect("a tab that is not selected");
    tab_view.close_page(&other);
    assert!(
        wait_until_or_false(SETTLE, || !evidence(&window).popover_visible),
        "the popover closes before its button hides"
    );
    // Past every retry a competing editor hand-off would make.
    crate::common::flush_after_delay(Duration::from_millis(400));
    assert!(
        focus_is(&window, &saved),
        "the focus saved before opening comes back, not {:?}",
        gtk4::prelude::GtkWindowExt::focus(&window).map(|widget| widget.type_().name())
    );
}

/// Up and Down from the search entry name the highlighted tab in the list's
/// value text, so an assistive technology can tell which tab Enter selects.
#[test]
fn test_moving_the_highlight_names_its_tab_in_the_lists_value_text() {
    let window = window_at(1000);
    let dir = tempfile::tempdir().expect("tempdir");
    open_files(&window, dir.path(), &["alpha.txt", "beta.txt", "gamma.txt"]);
    open_all_tabs(&window);
    let (_, popover) = all_tabs(&window);
    let list = popover.list_view();
    assert_eq!(
        evidence(&window).popover_highlighted_position,
        Some(2),
        "the empty query highlights the selected tab"
    );
    assert!(
        crate::common::accessible_value_text_is(
            &list,
            "Selected gamma.txt, current tab. 3 open tabs"
        ),
        "the list names the highlighted tab when the popover opens"
    );
    popover_key(&popover, gtk4::gdk::Key::Up);
    assert_eq!(evidence(&window).popover_highlighted_position, Some(1));
    assert!(
        crate::common::accessible_value_text_is(&list, "Selected beta.txt. 3 open tabs"),
        "Up names the newly highlighted tab"
    );
    set_query(&window, "zzz-no-such-tab");
    assert!(
        crate::common::accessible_value_text_is(&list, "No open tab matches"),
        "with nothing highlighted only the match count remains"
    );
}

/// The overflow actions exist only after the install stage's reconcile, so
/// they are published again once registered: none is enabled while its
/// control is hidden, even before the window's first dispatch.
#[test]
fn test_the_overflow_actions_start_disabled_before_the_first_dispatch() {
    // No main-loop iteration between construction and the reads.
    let window = crate::common::test_window();
    for name in [
        "scroll-tabs-back",
        "scroll-tabs-forward",
        "toggle-all-tabs",
        "set-all-tabs-visible",
        "set-all-tabs-query",
    ] {
        let action = window
            .lookup_action(name)
            .unwrap_or_else(|| panic!("{name} is registered"));
        assert!(
            !action.is_enabled(),
            "{name} is disabled while its control is hidden"
        );
    }
}

/// The end group's motion controller, when it has one.
fn group_motion(group: &gtk4::Box) -> Option<gtk4::EventControllerMotion> {
    let controllers = group.observe_controllers();
    (0..controllers.n_items()).find_map(|index| {
        controllers
            .item(index)
            .and_downcast::<gtk4::EventControllerMotion>()
    })
}

/// A wheel over our own trailing group moves no strip (the tab box's wheel
/// handler is on the box), so it cancels no hold and ends no reveal.
#[test]
fn test_a_wheel_over_the_end_group_cancels_nothing_and_keeps_the_reveal() {
    let window = overflowing_window();
    let group = evidence(&window).group.expect("the end group");
    let motion = group_motion(&group);
    let enter = || {
        if let Some(motion) = &motion {
            motion.emit_by_name::<()>("enter", &[&4.0f64, &4.0f64]);
        }
    };
    let leave = || {
        if let Some(motion) = &motion {
            motion.emit_by_name::<()>("leave", &[]);
        }
    };
    let wheel = || {
        let _ = bar_scroll_observer(&window).emit_by_name::<bool>("scroll", &[&0.0f64, &1.0f64]);
    };

    timing(LONG_REPEAT_DELAY_MS, 10);
    let (_, forward) = buttons(&window);
    press(&hold_gesture(&forward));
    enter();
    wheel();
    let now = evidence(&window);
    assert!(now.repeat_active, "a wheel over the group cancels no hold");
    assert_eq!(now.guard_cancellations, 0);
    leave();
    wheel();
    let now = evidence(&window);
    assert!(!now.repeat_active, "a wheel on the strip cancels the hold");
    assert_eq!(now.guard_cancellations, 1);
    release(&hold_gesture(&forward));
    timing(0, 0);
    settled(&window);

    open_all_tabs(&window);
    let (_, popover) = all_tabs(&window);
    // The empty query highlights the selected tab; Enter picks it.
    popover.search_entry().emit_by_name::<()>("activate", &[]);
    assert!(evidence(&window).selection_reveal_pending);
    enter();
    wheel();
    assert!(
        evidence(&window).selection_reveal_pending,
        "a wheel over the group keeps the reveal"
    );
    leave();
    settled(&window);
}

/// A close and a reopen in one dispatch, before the close's focus restore has
/// run: the reopened popover keeps the focus, and its own close gives back
/// the focus from before the first open.
#[test]
fn test_a_reopen_before_the_focus_restore_keeps_the_focus_in_the_popover() {
    let window = window_at(1000);
    open_tabs(&window, 3);
    let editor = crate::common::active_editor(&window);
    crate::common::flush_after_delay(Duration::from_millis(300));
    assert!(editor.source_view().grab_focus());
    flush_events();
    open_all_tabs(&window);
    let (_, popover) = all_tabs(&window);
    for visible in [false, true] {
        gio::prelude::ActionGroupExt::activate_action(
            &window,
            "set-all-tabs-visible",
            Some(&visible.to_variant()),
        );
    }
    flush_events();
    popover_settled(&window);
    assert!(evidence(&window).popover_visible, "reopened");
    assert!(
        focus_is(&window, &popover.search_entry()),
        "the reopened popover keeps the focus"
    );
    popover
        .search_entry()
        .emit_by_name::<()>("stop-search", &[]);
    assert!(
        wait_until_or_false(SETTLE, || !evidence(&window).popover_visible),
        "closed"
    );
    assert!(
        wait_until_or_false(SETTLE, || focus_is(&window, editor.source_view())),
        "the focus from before the first open comes back"
    );
}

/// A reveal on a strip whose unpinned tabs fit ends without re-announcing
/// the selection: the strip cannot hide the selected tab.
#[test]
fn test_a_reveal_on_a_strip_that_fits_announces_nothing() {
    let window = window_at(1000);
    open_tabs(&window, 3);
    assert!(!evidence(&window).controls_visible, "three tabs fit");
    open_all_tabs(&window);
    let (_, popover) = all_tabs(&window);
    let tab_view = window.imp().tab_view.get();
    let notifications = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let handler = tab_view.connect_selected_page_notify({
        let notifications = std::rc::Rc::clone(&notifications);
        move |_| notifications.set(notifications.get() + 1)
    });
    // The empty query highlights the selected tab; Enter picks it.
    popover.search_entry().emit_by_name::<()>("activate", &[]);
    assert!(
        evidence(&window).selection_reveal_pending,
        "picking the selected tab starts the reveal"
    );
    settled(&window);
    tab_view.disconnect(handler);
    assert_eq!(
        notifications.get(),
        0,
        "nothing overflows, so the reveal re-announces nothing"
    );
}
