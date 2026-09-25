// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget coverage for `WFR-SHELL-GEOMETRY`'s evidence surface, its state
//! extremes, and the persistence discipline the role move is most likely to
//! break.

use crate::common::{ensure_gtk_init, flush_events, present_window, test_window, wait_until};
use gio::prelude::SettingsExt;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::ui::window::{LushtextWindow, ShellGeometryEvidence, shell_geometry_evidence};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

fn evidence(window: &LushtextWindow) -> ShellGeometryEvidence {
    shell_geometry_evidence(window)
}

fn set_sidebar_visible(window: &LushtextWindow, visible: bool) {
    gtk4::prelude::ActionGroupExt::activate_action(
        window,
        "set-sidebar-visible",
        Some(&visible.to_variant()),
    );
    flush_events();
}

fn set_properties_visible(window: &LushtextWindow, visible: bool) {
    gtk4::prelude::ActionGroupExt::activate_action(
        window,
        "set-properties-visible",
        Some(&visible.to_variant()),
    );
    flush_events();
}

/// Proof 1 of 3 — **reentrancy**.
///
/// Driven through each operation that takes a mutable borrow of the state the
/// accessor reads: the two visibility toggles (which write the requested-state
/// cells and the compact-surface slot) and a breakpoint re-tune (which borrows
/// `properties_breakpoint` mutably). The surface is read *after* each one.
#[test]
fn test_shell_geometry_evidence_reads_stay_side_effect_free_across_layout_mutation() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    let baseline = evidence(&window);
    assert_eq!(
        baseline,
        evidence(&window),
        "an unchanged shell reads stably"
    );
    assert!(
        baseline.workspace_requested_visible,
        "precondition: a fresh window restores the sidebar as requested-visible"
    );

    // Drive against the default rather than into it, so the state being re-read
    // genuinely changes.
    set_sidebar_visible(&window, false);
    wait_until(Duration::from_secs(3), || {
        !evidence(&window).workspace_requested_visible
    });
    let hidden_sidebar = evidence(&window);
    assert_eq!(hidden_sidebar, evidence(&window));
    assert_ne!(
        hidden_sidebar.workspace_requested_visible, baseline.workspace_requested_visible,
        "the drive must actually have changed the state being re-read"
    );

    set_sidebar_visible(&window, true);
    wait_until(Duration::from_secs(3), || {
        evidence(&window).workspace_requested_visible
    });
    let shown = evidence(&window);
    assert_eq!(shown, evidence(&window));

    set_properties_visible(&window, true);
    wait_until(Duration::from_secs(3), || {
        evidence(&window).properties_requested_visible
    });
    let both = evidence(&window);
    assert_eq!(both, evidence(&window));

    set_properties_visible(&window, false);
    set_sidebar_visible(&window, false);
    wait_until(Duration::from_secs(3), || {
        !evidence(&window).workspace_requested_visible
            && !evidence(&window).properties_requested_visible
    });
    let hidden = evidence(&window);
    assert_eq!(hidden, evidence(&window));
    assert!(
        !hidden.split_width_syncing,
        "no width sync may be left re-entrant after the toggles settle"
    );
}

/// Proof 2 of 3 — **disposal honesty**.
///
/// Both split views and the layout view are `TemplateChild`ren, which GTK4
/// clears in `dispose()` before Rust's `Drop`.
#[test]
fn test_shell_geometry_evidence_answers_honestly_after_the_window_is_disposed() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    set_sidebar_visible(&window, true);
    wait_until(Duration::from_secs(3), || {
        evidence(&window).workspace_requested_visible
    });

    // Closing is not disposing: GTK defers template-child teardown.
    window.close();
    flush_events();

    // SAFETY: this test window is disposed exactly once, and everything after
    // this point only reads the evidence surface.
    unsafe { window.run_dispose() };

    let disposed = evidence(&window);
    assert_eq!(
        disposed.workspace_fraction, 0.0,
        "a cleared split view reports the neutral fraction rather than panicking"
    );
    assert_eq!(disposed.properties_fraction, 0.0);
    assert!(!disposed.workspace_rendered_visible);
    assert!(!disposed.properties_rendered_visible);
    assert_eq!(
        evidence(&window),
        disposed,
        "repeated reads of a disposed window must stay identical"
    );
}

/// Proof 3 of 3 — **non-materialization**.
///
/// Nothing here walks a lazily created toolkit collection, so no read can bring
/// state into being. Proved in both extremes, including the two derivations a
/// read could plausibly be tempted to perform: the cached breakpoint threshold
/// and the last-synced allocation width.
#[test]
fn test_shell_geometry_evidence_reads_materialize_no_toolkit_state() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);

    let collapsed = evidence(&window);
    for _ in 0..5 {
        assert_eq!(evidence(&window), collapsed);
    }

    set_sidebar_visible(&window, true);
    set_properties_visible(&window, true);
    wait_until(Duration::from_secs(3), || {
        evidence(&window).workspace_requested_visible
            && evidence(&window).properties_requested_visible
    });
    let expanded = evidence(&window);
    for _ in 0..5 {
        assert_eq!(evidence(&window), expanded);
    }
    assert_eq!(
        evidence(&window).properties_breakpoint_max_width,
        expanded.properties_breakpoint_max_width,
        "reading must not recompute or reinstall the breakpoint condition"
    );
    assert_eq!(
        evidence(&window).split_width_synced_for_width,
        expanded.split_width_synced_for_width,
        "reading must not run a split-view width sync"
    );
    assert!(
        !evidence(&window).split_width_syncing,
        "reading must not enter the width-sync guard"
    );
}

/// Data safety (task 7.3): **an allocation tick clamps, it does not persist.**
///
/// `.agents/rules/ui.md`: allocation-time split-view sync is for live geometry
/// only; it must not write `workspace-sidebar-width-fraction` or
/// `properties-sidebar-width-fraction` to GSettings, and must not reparse an
/// `AdwBreakpoint` condition on every animation frame. Persistence stays tied
/// to explicit user intent, restore, or animation completion.
///
/// This is the rule a role move is most likely to break silently, because
/// breaking it does not fail any behavioural test — it shows up as sidebar and
/// properties animations running below the monitor refresh rate in the
/// installed Flatpak, which is how it was found the first time. The surface's
/// separate `persisted_*` fields exist to make it assertable at all.
#[test]
fn test_allocation_ticks_clamp_geometry_without_persisting_or_reparsing() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    set_sidebar_visible(&window, true);
    set_properties_visible(&window, true);
    wait_until(Duration::from_secs(3), || {
        evidence(&window).workspace_requested_visible
            && evidence(&window).properties_requested_visible
    });
    flush_events();

    let before = evidence(&window);

    // Drive a run of allocations. Each one reaches the same
    // `sync_split_view_widths_for_allocation` path an animation frame does.
    for width in [1100, 980, 1240, 900, 1180] {
        window.set_default_size(width, 800);
        window.queue_allocate();
        flush_events();
    }
    wait_until(Duration::from_secs(3), || {
        !evidence(&window).split_width_syncing
    });
    flush_events();

    let after = evidence(&window);
    assert_eq!(
        after.persisted_workspace_fraction, before.persisted_workspace_fraction,
        "an allocation tick must not write the workspace width fraction to GSettings"
    );
    assert_eq!(
        after.persisted_properties_fraction, before.persisted_properties_fraction,
        "an allocation tick must not write the properties width fraction to GSettings"
    );
    assert!(
        !after.split_width_syncing,
        "the width-sync guard must be released after the allocations settle"
    );
    assert!(
        after.properties_breakpoint_installed,
        "the properties breakpoint must still be installed after the resizes"
    );
}

/// State extreme: a constrained width keeps both surfaces reachable.
#[test]
fn test_shell_geometry_state_extreme_constrained_width() {
    ensure_gtk_init();
    // `set_default_size` does not shrink an already-presented window, so a
    // narrow case gets its own window sized before presentation.
    let narrow = test_window();
    narrow.set_default_size(560, 640);
    present_window(&narrow);

    set_properties_visible(&narrow, true);
    wait_until(Duration::from_secs(3), || {
        evidence(&narrow).properties_requested_visible
    });
    let compact = evidence(&narrow);
    assert!(
        compact.properties_requested_visible,
        "the properties action stays reachable at a constrained width"
    );
    assert!(
        compact.window_width > 0,
        "the surface reports a real allocation to derive against"
    );
    assert!(
        compact.properties_breakpoint_max_width > 0,
        "the derived breakpoint threshold is cached rather than recomputed per frame"
    );
}

/// State extreme: both surfaces open together at a wide width, and closing one
/// leaves the other's requested state alone.
#[test]
fn test_shell_geometry_state_extreme_both_surfaces_at_a_wide_width() {
    ensure_gtk_init();
    let wide = test_window();
    wide.set_default_size(1600, 900);
    present_window(&wide);

    set_sidebar_visible(&wide, false);
    set_properties_visible(&wide, false);
    wait_until(Duration::from_secs(3), || {
        !evidence(&wide).workspace_requested_visible
            && !evidence(&wide).properties_requested_visible
    });
    let collapsed = evidence(&wide);
    assert!(!collapsed.workspace_rendered_visible);
    assert!(!collapsed.properties_rendered_visible);

    set_sidebar_visible(&wide, true);
    wait_until(Duration::from_secs(3), || {
        evidence(&wide).workspace_requested_visible
    });
    set_properties_visible(&wide, true);
    wait_until(Duration::from_secs(3), || {
        evidence(&wide).properties_requested_visible
    });
    let both = evidence(&wide);
    assert!(both.workspace_requested_visible && both.properties_requested_visible);
    assert!(
        both.workspace_fraction > 0.0,
        "a shown workspace pane must have a non-zero width fraction"
    );

    set_sidebar_visible(&wide, false);
    wait_until(Duration::from_secs(3), || {
        !evidence(&wide).workspace_requested_visible
    });
    assert!(
        evidence(&wide).properties_requested_visible,
        "hiding the workspace pane must not retract the properties request"
    );
}

/// Large Text scales the breakpoint conditions (`sp` follows the text scale
/// factor), so at 1.25 the `max-width: 1350sp` properties breakpoint applies at
/// every width up to 1687 px while the policy, which compares the allocated
/// width with 1350, keeps the pane. With a `layout-name` setter on that
/// breakpoint the two writers fought inside one allocation: the setter wrote
/// `sheet`, and the reconciliation run from `notify::layout-name` wrote `pane`
/// straight back, reparenting the content twice and handing focus to the
/// editor. The breakpoint-loop model found it (`shell_loop_layout_setter_flaps`);
/// the breakpoint now only triggers the reconciliation, which is the single
/// writer, so `layout-name` never leaves the pane where the policy wants it.
#[test]
fn test_large_text_breakpoint_never_flips_the_pane_to_the_sheet() {
    ensure_gtk_init();
    let gtk_settings = gtk4::Settings::default().expect("default GTK settings");
    // 1.25 x 96 dpi, in the 1/1024 units `gtk-xft-dpi` takes.
    gtk_settings.set_gtk_xft_dpi(120 * 1024);
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    // Construct at the width it will be allocated at, so the restore-time
    // reconciliation already renders the pane.
    settings
        .set_int(lushtext_core::config::keys::WINDOW_WIDTH, 1500)
        .expect("set window width");
    settings
        .set_int(lushtext_core::config::keys::WINDOW_HEIGHT, 900)
        .expect("set window height");
    let window = test_window();
    let layout_view = window.imp().properties_layout_view.clone();
    assert_eq!(
        layout_view.layout_name().as_deref(),
        Some("pane"),
        "1500 px is above the Comfy properties breakpoint (1350), so the policy wants the pane"
    );

    let seen = Rc::new(RefCell::new(Vec::<String>::new()));
    let recorder = Rc::clone(&seen);
    layout_view.connect_notify_local(Some("layout-name"), move |view, _| {
        recorder
            .borrow_mut()
            .push(view.layout_name().map(String::from).unwrap_or_default());
    });

    present_window(&window);
    wait_until(Duration::from_secs(5), || {
        evidence(&window).split_width_synced_for_width == 1500
    });
    flush_events();

    assert_eq!(evidence(&window).window_width, 1500);
    assert_eq!(layout_view.layout_name().as_deref(), Some("pane"));
    assert!(
        seen.borrow().is_empty(),
        "layout-name must not change where the policy keeps the pane, saw {:?}",
        seen.borrow()
    );
    window.destroy();
    flush_events();
}

/// At a text scale of 2 the `max-width: 400sp` Open-button breakpoint holds at
/// the 640 px minimum window width, and only the last added matching breakpoint
/// applies (ledger A18), so it displaced the workspace breakpoint and its
/// `collapsed = true` setter: the workspace pane stopped overlaying at the
/// narrowest windows. The Open-button breakpoint now carries the same
/// `collapsed = true` setter, since every width it matches also matches the
/// workspace breakpoint's `max-width: 860sp`.
#[test]
fn test_doubled_text_scale_keeps_the_workspace_collapsed_at_the_minimum_width() {
    ensure_gtk_init();
    let gtk_settings = gtk4::Settings::default().expect("default GTK settings");
    gtk_settings.set_gtk_xft_dpi(192 * 1024);
    let settings = gio::Settings::new(lushtext_core::config::APP_ID);
    settings
        .set_int(lushtext_core::config::keys::WINDOW_WIDTH, 700)
        .expect("set window width");
    settings
        .set_int(lushtext_core::config::keys::WINDOW_HEIGHT, 700)
        .expect("set window height");
    let window = test_window();
    present_window(&window);
    wait_until(Duration::from_secs(5), || {
        evidence(&window).split_width_synced_for_width == 700
    });
    flush_events();
    assert_eq!(evidence(&window).window_width, 700);
    assert!(
        window.imp().workspace_split_view.is_collapsed(),
        "700 px is under 860sp at every text scale, so the workspace pane must be collapsed"
    );
    // Switch between the two breakpoints at a fixed width by changing only the
    // text scale (ledger A14: re-evaluated at the next allocation): 1.5 makes
    // the workspace breakpoint current, 2 the Open-button one again.
    // The Open-button stack shows whether that breakpoint is current.
    let open_button_narrow = |window: &LushtextWindow| {
        window
            .imp()
            .open_button_stack
            .visible_child_name()
            .as_deref()
            == Some("narrow")
    };
    assert!(open_button_narrow(&window));
    for (dpi, narrow, current) in [
        (144 * 1024, false, "workspace"),
        (192 * 1024, true, "Open-button"),
    ] {
        gtk_settings.set_gtk_xft_dpi(dpi);
        window.queue_allocate();
        wait_until(Duration::from_secs(5), || {
            open_button_narrow(&window) == narrow
        });
        flush_events();
        assert_eq!(
            open_button_narrow(&window),
            narrow,
            "the {current} breakpoint became current"
        );
        assert!(
            window.imp().workspace_split_view.is_collapsed(),
            "switching to the {current} breakpoint must keep the workspace collapsed"
        );
    }
    window.destroy();
    flush_events();
}
