// SPDX-License-Identifier: GPL-3.0-or-later

//! The staged window-composition gate (`first-frame-window-shell`, design D7).
//!
//! The window is built whole in stage C with its staged content invisible and
//! revealed over its first frames (`ui/window/composition/`). This module
//! proves, on the real `after-paint` path:
//!
//! - `open` and `activate` return without a window, hold the application, and
//!   a second request before stage C merges into one window;
//! - activation files open in bounded turns;
//! - the stage order, and that no reveal runs before the frame showing its
//!   predecessor painted;
//! - every forced path, issued before each stage, ends in the state a composed
//!   window reaches, and reads and projections force nothing;
//! - revealing staged content never moves the rail or the pane (design D3), and
//!   the protected chrome is constant in every frame of composition;
//! - the readiness blocker, teardown during each stage, initial focus, and the
//!   evidence surface's three driven proofs.
//!
//! These tests present with `window.present()`, not `present_window`, which
//! waits for composition to complete.

use crate::common::{
    ensure_gtk_init, fixture, flush_after_delay, flush_events, present_window, wait_until,
};
use gio::prelude::*;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::app::LushtextApplication;
use lushtext_core::config::keys;
use lushtext_core::model::session::{SessionData, SessionTab};
use lushtext_core::ui::automation::current_idle_blocker;
use lushtext_core::ui::window::{
    CompositionEvidence, CompositionStage, LushtextWindow, set_composition_collapsed_for_test,
    set_composition_hold_before_for_test, shell_geometry_evidence,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// Composition waits on painted frames, which a loaded compositor delays.
const COMPOSED: Duration = Duration::from_secs(10);
/// The session restore's page turn (`SESSION_RESTORE_PAGES_PER_TURN`), which
/// bounds an activation turn too.
const PAGES_PER_TURN: i32 = 4;

/// The reveal stages, in order, each with the point a test holds before it.
const HOLD_POINTS: [CompositionStage; 3] = [
    CompositionStage::Content,
    CompositionStage::Workspace,
    CompositionStage::Properties,
];

fn app() -> LushtextApplication {
    crate::common::test_lushtext_application()
}

fn evidence(window: &LushtextWindow) -> CompositionEvidence {
    window.composition_evidence()
}

fn lushtext_windows(app: &LushtextApplication) -> Vec<LushtextWindow> {
    app.windows()
        .into_iter()
        .filter_map(|window| window.downcast::<LushtextWindow>().ok())
        .collect()
}

/// Seed the restored layout and size the next window reads.
fn seed_layout(workspace: bool, properties: bool, width: i32, height: i32) {
    ensure_gtk_init();
    let settings = lushtext_core::ui::settings_writes::shared();
    settings
        .set_boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED, true)
        .expect("set split-view-layout-migrated");
    settings
        .set_boolean(keys::WORKSPACE_SIDEBAR_VISIBLE, workspace)
        .expect("set workspace-sidebar-visible");
    settings
        .set_boolean(keys::PROPERTIES_SIDEBAR_VISIBLE, properties)
        .expect("set properties-sidebar-visible");
    crate::common::seed_restored_window_size(width, height);
    // Restored with the window: a previous window's toggle must not leak into
    // the next one's starting state.
    settings
        .set_boolean(keys::SEARCH_PANEL_VISIBLE, false)
        .expect("set search-panel-visible");
}

fn seed_preset(fraction: f64) {
    lushtext_core::ui::settings_writes::shared()
        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, fraction)
        .expect("set workspace-sidebar-width-fraction");
}

fn wait_composed(window: &LushtextWindow) {
    wait_until(COMPOSED, || evidence(window).complete);
}

/// Wait until composition stands held before `stage`: the first frame
/// painted, every scheduled reveal before `stage` ran, nothing queued.
fn wait_held_before(window: &LushtextWindow, stage: CompositionStage) {
    wait_until(COMPOSED, || {
        let evidence = evidence(window);
        evidence.shell_first_frame_us.is_some()
            && !evidence.dispatch_queued
            && evidence
                .scheduled
                .iter()
                .filter(|scheduled| scheduled.is_reveal() && *scheduled < stage)
                .all(|before| evidence.done.contains(before))
    });
    flush_after_delay(Duration::from_millis(60));
    let evidence = evidence(window);
    assert!(
        !evidence.done.contains(stage) && !evidence.complete,
        "the hold keeps {stage:?} from running: {evidence:?}"
    );
}

fn activate(window: &LushtextWindow, name: &str, parameter: Option<&glib::Variant>) {
    gtk4::prelude::ActionGroupExt::activate_action(window, name, parameter);
}

/// The window's own `visible` property of a widget (not its ancestors').
fn own_visible(widget: &impl IsA<gtk4::Widget>) -> bool {
    widget.property::<bool>("visible")
}

// --- The request path (D1) ---------------------------------------------------

#[test]
fn test_open_returns_before_any_window_exists_and_one_window_composes() {
    let app = app();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let path = dir.path().join("first.txt");
    fixture::write_text(&path, "first\n");

    app.open(&[gio::File::for_path(&path)], "");
    assert!(
        app.windows().is_empty(),
        "`open` must return before any window work: the main loop runs first"
    );
    assert_eq!(
        app.pending_window_file_count(),
        Some(1),
        "the request is recorded, holding the application, until stage C"
    );

    flush_events();
    let windows = lushtext_windows(&app);
    assert_eq!(windows.len(), 1, "stage C constructed one window");
    assert_eq!(
        app.pending_window_file_count(),
        None,
        "stage C took the request and released its hold"
    );
    let window = &windows[0];
    wait_composed(window);
    wait_until(COMPOSED, || window.imp().tab_view.n_pages() == 1);
    let evidence = evidence(window);
    assert!(
        evidence.done.contains(CompositionStage::Warm),
        "{evidence:?}"
    );
    assert_eq!(evidence.stages_pending, 0);
}

#[test]
fn test_a_second_open_before_construction_merges_into_the_first_request() {
    let app = app();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    fixture::write_text(&first, "first\n");
    fixture::write_text(&second, "second\n");

    app.activate();
    app.open(&[gio::File::for_path(&first)], "");
    app.open(&[gio::File::for_path(&second)], "");
    assert!(app.windows().is_empty());
    assert_eq!(app.pending_window_file_count(), Some(2));

    flush_events();
    let windows = lushtext_windows(&app);
    assert_eq!(
        windows.len(),
        1,
        "three requests before stage C make one window"
    );
    wait_until(COMPOSED, || windows[0].imp().tab_view.n_pages() == 2);
}

/// A request that reaches a window between stage C and stage P does no
/// window work: it admits its files, and the window still realizes and
/// presents in its own stages and composes to the end.
#[test]
fn test_a_request_before_the_window_is_presented_does_no_window_work() {
    let app = app();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let second = dir.path().join("second.txt");
    fixture::write_text(&second, "second\n");
    app.activate();
    let context = glib::MainContext::default();
    while lushtext_windows(&app).is_empty() {
        context.iteration(true);
    }
    let window = lushtext_windows(&app).remove(0);
    assert!(!window.is_realized(), "precondition: stage R has not run");

    app.open(&[gio::File::for_path(&second)], "");
    assert!(
        !window.is_realized() && !window.is_mapped(),
        "a request before stage P must not realize or present the window"
    );
    assert!(
        lushtext_windows(&app).len() == 1,
        "the request went to the window"
    );
    wait_composed(&window);
    wait_until(COMPOSED, || window.imp().tab_view.n_pages() == 1);
    window.destroy();
    flush_events();
}

#[test]
fn test_a_shutdown_before_construction_drops_the_request() {
    let app = app();
    app.activate();
    assert_eq!(app.pending_window_file_count(), Some(0));
    // What `shutdown` runs first; emitting the signal by hand would tear a
    // GApplication down outside `g_application_run`.
    app.drop_pending_window_request();
    assert_eq!(
        app.pending_window_file_count(),
        None,
        "shutting down before stage C drops the request and its hold"
    );
    flush_events();
    assert!(
        app.windows().is_empty(),
        "no window is constructed after shutdown"
    );
}

#[test]
fn test_many_activation_files_open_in_bounded_turns() {
    let app = app();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let files: Vec<_> = (0..10)
        .map(|index| {
            let path = dir.path().join(format!("file-{index}.txt"));
            fixture::write_text(&path, &format!("{index}\n"));
            gio::File::for_path(path)
        })
        .collect();
    app.open(&files, "");

    // Drive the main loop one iteration at a time: one activation turn is one
    // dispatch, and no iteration may open more than one page turn.
    let mut largest_step = 0;
    let mut window = None;
    let deadline = std::time::Instant::now() + COMPOSED;
    while std::time::Instant::now() < deadline {
        let before = window
            .as_ref()
            .map_or(0, |window: &LushtextWindow| window.imp().tab_view.n_pages());
        glib::MainContext::default().iteration(false);
        if window.is_none() {
            window = lushtext_windows(&app).into_iter().next();
        }
        let Some(current) = window.as_ref() else {
            continue;
        };
        let after = current.imp().tab_view.n_pages();
        largest_step = largest_step.max(after - before);
        if after == 10 {
            break;
        }
        if before == after {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    let window = window.expect("a window was constructed");
    assert_eq!(window.imp().tab_view.n_pages(), 10, "every file opened");
    assert!(
        largest_step <= PAGES_PER_TURN,
        "one dispatch opened {largest_step} pages, more than one turn of {PAGES_PER_TURN}"
    );
}

// --- Stage order (D1, D7) ----------------------------------------------------

/// What one painted frame showed: the stages done when it painted.
type FrameLog = Rc<RefCell<Vec<CompositionEvidence>>>;

/// Record the composition evidence at every `after-paint`, after the
/// workflow's own handler (connected first, at realize) decided.
fn record_frames(window: &LushtextWindow) -> FrameLog {
    let log: FrameLog = Rc::new(RefCell::new(Vec::new()));
    let recorder = Rc::clone(&log);
    window.connect_realize(move |window| {
        let Some(clock) = window.frame_clock() else {
            return;
        };
        let recorder = Rc::clone(&recorder);
        let window_weak = window.downgrade();
        clock.connect_after_paint(move |_| {
            if let Some(window) = window_weak.upgrade()
                && !window.composition_evidence().complete
            {
                recorder.borrow_mut().push(window.composition_evidence());
            }
        });
    });
    log
}

#[test]
fn test_reveal_stages_follow_painted_frames_in_order() {
    seed_layout(true, true, 1600, 900);
    let app = app();
    let window = LushtextWindow::new(app.upcast_ref());
    let frames = record_frames(&window);
    let at_construction = evidence(&window);
    assert!(at_construction.begun && !at_construction.complete);
    assert!(!at_construction.content_visible && !at_construction.workspace_visible);
    assert!(!at_construction.properties_visible);
    assert_eq!(
        at_construction
            .scheduled
            .iter()
            .filter(|stage| stage.is_reveal())
            .count(),
        3,
        "B1, B2 and B3 are always scheduled"
    );

    window.present();
    wait_composed(&window);
    let frames = frames.borrow();
    assert!(
        !frames.is_empty(),
        "precondition: the harness delivers after-paint"
    );
    let reveals = |evidence: &CompositionEvidence| {
        evidence
            .done
            .iter()
            .filter(|stage| stage.is_reveal())
            .collect::<Vec<_>>()
    };
    assert!(
        reveals(&frames[0]).is_empty(),
        "the first frame shows the shell only: {:?}",
        frames[0]
    );
    let mut previous: Vec<CompositionStage> = Vec::new();
    for frame in frames.iter() {
        let current = reveals(frame);
        assert!(
            current.len() <= previous.len() + 1,
            "one frame carried two stages: {previous:?} -> {current:?}"
        );
        assert!(
            current.starts_with(&previous),
            "stages ran out of order: {current:?}"
        );
        previous = current;
    }
    let composed = evidence(&window);
    assert!(composed.content_visible && composed.workspace_visible && composed.properties_visible);
    assert_eq!(composed.forced_stages, 0, "nothing forced a stage");
    assert!(composed.shell_first_frame_us <= composed.composition_complete_us);
    window.destroy();
    flush_events();
}

/// The window's background is transparent (the editor's background opacity
/// shows through it) and the empty page is the opaque surface over the
/// content area. Until stage B1 reveals the content stack, the content box
/// itself must be opaque, or the first frame shows what is behind the window.
#[test]
fn test_the_content_area_is_opaque_until_the_content_stage_reveals_it() {
    seed_layout(false, false, 1600, 900);
    let app = app();
    set_composition_hold_before_for_test(Some(CompositionStage::Content));
    let window = LushtextWindow::new(app.upcast_ref());
    set_composition_hold_before_for_test(None);
    window.present();
    wait_held_before(&window, CompositionStage::Content);
    let content_box = window.imp().content_box.clone();
    assert!(
        content_box.has_css_class("shell-chrome-opaque"),
        "the content box paints the window background while the content is staged"
    );
    window.release_composition_hold_for_test();
    wait_composed(&window);
    assert!(
        !content_box.has_css_class("shell-chrome-opaque"),
        "once revealed, the content draws its own surfaces (the editor may be translucent)"
    );
    window.destroy();
    flush_events();
}

/// Every action that acts inside the workspace sidebar forces its stage, also
/// when the restored layout already shows the sidebar (where requesting it
/// again changes no state and so never reaches the toggle's forced reveal).
#[test]
fn test_sidebar_actions_before_the_workspace_stage_reveal_the_sidebar() {
    let app = app();
    let dir = tempfile::tempdir().expect("reveal tempdir");
    for (action, parameter) in [
        ("focus-workspace-tree", None),
        ("focus-workspace-header", None),
        ("show-workspace-tree-context-menu", None),
        ("show-workspace-header-context-menu", None),
        (
            "reveal-workspace-path",
            Some(
                dir.path()
                    .join("missing.txt")
                    .display()
                    .to_string()
                    .to_variant(),
            ),
        ),
    ] {
        seed_layout(true, true, 1600, 900);
        set_composition_hold_before_for_test(Some(CompositionStage::Workspace));
        let window = LushtextWindow::new(app.upcast_ref());
        set_composition_hold_before_for_test(None);
        window.present();
        wait_held_before(&window, CompositionStage::Workspace);
        activate(&window, action, parameter.as_ref());
        let evidence = evidence(&window);
        assert!(
            evidence.workspace_visible && evidence.forced_stages == 1,
            "{action} before B2 must reveal the sidebar: {evidence:?}"
        );
        window.release_composition_hold_for_test();
        wait_composed(&window);
        window.destroy();
        flush_events();
    }
}

#[test]
fn test_rails_the_layout_hides_are_revealed_hidden_and_stay_hidden() {
    seed_layout(false, false, 1600, 900);
    let app = app();
    let window = LushtextWindow::new(app.upcast_ref());
    window.present();
    wait_composed(&window);
    let composed = evidence(&window);
    assert!(
        composed.content_visible && composed.workspace_visible && composed.properties_visible,
        "composition reveals every staged surface, into hidden rails too: {composed:?}"
    );
    let imp = window.imp();
    assert!(
        !imp.workspace_split_view.shows_sidebar(),
        "revealing the sidebar's content does not show its rail"
    );
    // A hidden sidebar sits off-screen to the left, as an unstaged window's.
    let bounds = imp
        .sidebar
        .compute_bounds(&window)
        .expect("the revealed sidebar has bounds");
    assert!(
        bounds.width() > 0.0 && (bounds.x() + bounds.width()).abs() < 1.0,
        "the hidden sidebar is allocated off-screen: {bounds:?}"
    );
    activate(&window, "set-sidebar-visible", Some(&true.to_variant()));
    activate(&window, "toggle-properties", None);
    let toggled = evidence(&window);
    assert!(toggled.workspace_visible && toggled.properties_visible);
    assert_eq!(toggled.forced_stages, 0, "nothing was left to force");
    window.destroy();
    flush_events();
}

// --- Forced paths (D4) ---------------------------------------------------------

/// The shell state a forced path must leave as a composed window does.
#[derive(Debug, PartialEq)]
struct ShellState {
    workspace_requested: bool,
    workspace_rendered: bool,
    properties_requested: bool,
    properties_rendered: bool,
    content_visible: bool,
    workspace_visible: bool,
    properties_visible: bool,
    search_panel_revealed: bool,
    search_panel_visible: bool,
    palette_revealed: bool,
    palette_visible: bool,
    open_popover_visible: bool,
    preview_mode: bool,
    tabs: i32,
    editor_search_visible: bool,
}

fn shell_state(window: &LushtextWindow) -> ShellState {
    let imp = window.imp();
    let geometry = shell_geometry_evidence(window);
    let composition = evidence(window);
    ShellState {
        workspace_requested: geometry.workspace_requested_visible,
        workspace_rendered: geometry.workspace_rendered_visible,
        properties_requested: geometry.properties_requested_visible,
        properties_rendered: geometry.properties_rendered_visible,
        content_visible: composition.content_visible,
        workspace_visible: composition.workspace_visible,
        properties_visible: composition.properties_visible,
        search_panel_revealed: imp.search_panel_revealer.reveals_child(),
        search_panel_visible: own_visible(&*imp.search_panel),
        palette_revealed: imp.palette_revealer.reveals_child(),
        palette_visible: own_visible(&*imp.command_palette),
        open_popover_visible: imp.open_popover.is_visible(),
        preview_mode: imp.preview_mode.get(),
        tabs: imp.tab_view.n_pages(),
        editor_search_visible: imp
            .tab_view
            .selected_page()
            .and_then(|page| {
                page.child()
                    .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
                    .ok()
            })
            .is_some_and(|editor| editor.is_search_visible()),
    }
}

/// When a forced path is issued: before the window is presented (before R), or
/// with composition held before one reveal stage.
#[derive(Clone, Copy, Debug)]
enum IssuedAt {
    Unpresented,
    Before(CompositionStage),
}

/// Run `op` on a window at `at`, finish composition, settle, and return the
/// shell state. Every window starts from the same restored layout, which has
/// both rails requested, so a forced toggle changes what renders.
fn state_after(
    app: &LushtextApplication,
    at: Option<IssuedAt>,
    op: &dyn Fn(&LushtextWindow),
) -> ShellState {
    seed_layout(true, true, 1600, 900);
    set_composition_collapsed_for_test(at.is_none());
    set_composition_hold_before_for_test(match at {
        Some(IssuedAt::Before(stage)) => Some(stage),
        _ => None,
    });
    let window = LushtextWindow::new(app.upcast_ref());
    set_composition_collapsed_for_test(false);
    set_composition_hold_before_for_test(None);
    match at {
        Some(IssuedAt::Unpresented) => {
            wait_startup_restore_settled(&window);
            op(&window);
            window.present();
        }
        Some(IssuedAt::Before(stage)) => {
            window.present();
            wait_held_before(&window, stage);
            wait_startup_restore_settled(&window);
            op(&window);
            window.release_composition_hold_for_test();
        }
        None => {
            window.present();
            wait_composed(&window);
            wait_startup_restore_settled(&window);
            op(&window);
        }
    }
    wait_composed(&window);
    // Activation turns and a restore finish on their own after composition.
    wait_until(COMPOSED, || {
        !shell_geometry_evidence(&window).workspace_sidebar_transition_pending
            && evidence(&window).activation_opens_queued == 0
            && !window.session_restore_evidence().restoring
            && no_tab_is_loading(&window)
    });
    flush_after_delay(Duration::from_millis(300));
    let state = shell_state(&window);
    window.destroy();
    flush_events();
    state
}

/// Wait until the window's own startup session restore (its startup read
/// begins in stage C) has finished. An operation that restores a session must
/// come after it: a startup restore that lands later cancels the operation's
/// restore and installs the test profile's empty session instead.
fn wait_startup_restore_settled(window: &LushtextWindow) {
    crate::common::wait_for_startup_data_flow(window);
    wait_until(COMPOSED, || !window.session_restore_evidence().restoring);
}

/// Whether no editor tab of `window` is still loading.
fn no_tab_is_loading(window: &LushtextWindow) -> bool {
    let tab_view = &window.imp().tab_view;
    (0..tab_view.n_pages()).all(|index| {
        tab_view
            .nth_page(index)
            .child()
            .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
            .is_ok_and(|editor| {
                editor.load_state() != lushtext_core::ui::editor_page::EditorLoadState::Loading
            })
    })
}

/// Assert `op` reaches the reference state from every issue point.
fn assert_forced_path_matches_composed(
    app: &LushtextApplication,
    name: &str,
    op: &dyn Fn(&LushtextWindow),
) {
    let reference = state_after(app, None, op);
    let mut points = vec![IssuedAt::Unpresented];
    points.extend(HOLD_POINTS.map(IssuedAt::Before));
    for at in points {
        let state = state_after(app, Some(at), op);
        assert_eq!(
            state, reference,
            "{name} issued {at:?} must match a composed window"
        );
    }
}

#[test]
fn test_sidebar_toggles_issued_before_each_stage_match_a_composed_window() {
    let app = app();
    assert_forced_path_matches_composed(&app, "win.toggle-sidebar", &|window| {
        activate(window, "toggle-sidebar", None);
    });
    assert_forced_path_matches_composed(&app, "win.set-sidebar-visible", &|window| {
        activate(window, "set-sidebar-visible", Some(&false.to_variant()));
        activate(window, "set-sidebar-visible", Some(&true.to_variant()));
    });
}

#[test]
fn test_properties_toggles_issued_before_each_stage_match_a_composed_window() {
    let app = app();
    assert_forced_path_matches_composed(&app, "F9 / win.toggle-properties", &|window| {
        activate(window, "toggle-properties", None);
    });
    assert_forced_path_matches_composed(&app, "win.set-properties-visible", &|window| {
        activate(window, "set-properties-visible", Some(&false.to_variant()));
        activate(window, "set-properties-visible", Some(&true.to_variant()));
    });
}

#[test]
fn test_hidden_surfaces_issued_before_each_stage_match_a_composed_window() {
    let app = app();
    assert_forced_path_matches_composed(&app, "Ctrl+Shift+F", &|window| {
        activate(window, "toggle-search-panel", None);
    });
    assert_forced_path_matches_composed(&app, "Ctrl+Shift+P", &|window| {
        activate(window, "toggle-command-palette", None);
    });
    assert_forced_path_matches_composed(&app, "the Open popover button", &|window| {
        window.imp().open_menu_button.popup();
    });
}

#[test]
fn test_editor_paths_issued_before_each_stage_match_a_composed_window() {
    let app = app();
    assert_forced_path_matches_composed(&app, "win.begin-search", &|window| {
        activate(window, "new-tab", None);
        activate(window, "begin-search", None);
    });
    assert_forced_path_matches_composed(&app, "a preview action", &|window| {
        activate(window, "new-tab", None);
        activate(window, "set-preview-mode", Some(&true.to_variant()));
    });
}

#[test]
fn test_cli_opens_issued_before_each_stage_match_a_composed_window() {
    let app = app();
    let dir = tempfile::tempdir().expect("activation tempdir");
    let paths: Vec<PathBuf> = (0..6)
        .map(|index| {
            let path = dir.path().join(format!("cli-{index}.txt"));
            fixture::write_text(&path, &format!("{index}\n"));
            path
        })
        .collect();
    // Six files are more than one bounded turn admits.
    assert!(paths.len() > usize::try_from(PAGES_PER_TURN).unwrap_or(usize::MAX));
    let paths = Rc::new(paths);
    assert_forced_path_matches_composed(&app, "a CLI open", &move |window| {
        let app = window
            .application()
            .and_then(|app| app.downcast::<LushtextApplication>().ok())
            .expect("the window's application");
        // The active window receives the activation, as from a second launch.
        window.present();
        let files: Vec<_> = paths.iter().map(gio::File::for_path).collect();
        app.open(&files, "");
        flush_events();
    });
}

#[test]
fn test_session_restore_completion_issued_before_each_stage_matches_a_composed_window() {
    let app = app();
    let dir = tempfile::tempdir().expect("session tempdir");
    let path = dir.path().join("restored.txt");
    fixture::write_text(&path, "restored\n");
    let path = Rc::new(path);
    assert_forced_path_matches_composed(&app, "session-restore completion", &move |window| {
        window.restore_session_for_test(session_with(&path));
    });
}

#[test]
fn test_an_automation_action_issued_before_each_stage_matches_a_composed_window() {
    let app = app();
    // Automation mutates state only through the window's `org.gtk.Actions`,
    // the same group these calls activate.
    assert_forced_path_matches_composed(&app, "an automation target-state action", &|window| {
        activate(window, "set-focus-mode", Some(&true.to_variant()));
        activate(window, "set-focus-mode", Some(&false.to_variant()));
        activate(window, "set-properties-visible", Some(&true.to_variant()));
    });
}

fn session_with(path: &Path) -> SessionData {
    SessionData {
        tabs: vec![SessionTab {
            path: Some(path.to_path_buf()),
            draft_id: None,
            cursor_line: 0,
            cursor_col: 0,
            scroll_line: 0,
            pinned: false,
        }],
        active_tab_index: Some(0),
    }
}

#[test]
fn test_reads_and_projections_during_composition_force_nothing() {
    seed_layout(true, true, 1600, 900);
    let app = app();
    set_composition_hold_before_for_test(Some(CompositionStage::Content));
    let window = LushtextWindow::new(app.upcast_ref());
    set_composition_hold_before_for_test(None);
    window.present();
    wait_held_before(&window, CompositionStage::Content);

    // Allocation: `size_allocate` reads the search panel and the palette.
    window.set_default_size(1500, 880);
    window.queue_allocate();
    flush_after_delay(Duration::from_millis(100));
    // A status message renders the notification and the status bar.
    window.publish_status_message(
        "composition projection probe",
        lushtext_core::services::notifications::NotificationSeverity::Info,
    );
    // Session-restored pages run the tab projections (content stack, sidebar
    // row states, Open popover rows, status bar) on the invisible tab view.
    let dir = tempfile::tempdir().expect("session tempdir");
    let path = dir.path().join("projected.txt");
    fixture::write_text(&path, "projected\n");
    window.restore_session_for_test(session_with(&path));
    wait_until(COMPOSED, || !window.session_restore_evidence().restoring);
    flush_after_delay(Duration::from_millis(100));
    // Evidence and snapshot reads.
    let _ = shell_geometry_evidence(&window);
    let _ = lushtext_core::ui::automation::app_snapshot(&app);

    let held = evidence(&window);
    assert_eq!(
        held.forced_stages, 0,
        "a read or projection forced a stage: {held:?}"
    );
    assert!(!held.content_visible && !held.workspace_visible && !held.properties_visible);
    assert_eq!(
        window.imp().tab_view.n_pages(),
        1,
        "pages append to the invisible tab view"
    );
    window.release_composition_hold_for_test();
    wait_composed(&window);
    assert!(evidence(&window).content_visible);
    window.destroy();
    flush_events();
}

// --- Geometry (D3, D7) -------------------------------------------------------

/// A widget-bounds coordinate in whole pixels.
#[expect(
    clippy::cast_possible_truncation,
    reason = "widget bounds in a window of at most a few thousand pixels fit an i32"
)]
fn px(value: f32) -> i32 {
    value.round() as i32
}

/// A widget's bounds in `window`, as `(x, y, width, height)`.
type Rect = (i32, i32, i32, i32);
/// A widget's left and right edges in `window`, when it is mapped.
type Edges = Option<(i32, i32)>;

/// The rail (the workspace split view's sidebar host), the content area,
/// whose right edge is the pane edge, and the properties slot, in window
/// coordinates. The slot is unmapped until its panel is revealed, so the pane
/// edge is read from the content area, which the properties split view
/// allocates beside it from the first frame.
fn rail_and_pane(window: &LushtextWindow) -> (Edges, Edges, Edges) {
    let imp = window.imp();
    let edges = |widget: Option<gtk4::Widget>| {
        widget
            .filter(gtk4::prelude::WidgetExt::is_mapped)
            .and_then(|widget| widget.compute_bounds(window))
            .map(|bounds| (px(bounds.x()), px(bounds.x() + bounds.width())))
    };
    (
        edges(imp.sidebar.parent()),
        edges(Some(imp.content_box.clone().upcast())),
        edges(imp.properties_panel.parent()),
    )
}

#[test]
fn test_revealing_staged_rails_never_moves_the_rail_or_the_pane() {
    let app = app();
    let gtk_settings = gtk4::Settings::default().expect("default GTK settings");
    for (dpi, scale) in [(96, "1"), (120, "1.25"), (144, "1.5"), (192, "2")] {
        gtk_settings.set_gtk_xft_dpi(dpi * 1024);
        for (preset, fraction) in [("Small", 0.2), ("Comfy", 0.3), ("Large", 0.4)] {
            for (label, width) in [("wide", 1700), ("medium", 1150), ("compact", 760)] {
                seed_layout(true, true, width, 860);
                seed_preset(fraction);
                set_composition_hold_before_for_test(Some(CompositionStage::Workspace));
                let window = LushtextWindow::new(app.upcast_ref());
                set_composition_hold_before_for_test(None);
                window.present();
                wait_held_before(&window, CompositionStage::Workspace);
                let (rail, content, _) = rail_and_pane(&window);
                window.release_composition_hold_for_test();
                wait_composed(&window);
                flush_after_delay(Duration::from_millis(60));
                let (rail_after, content_after, _) = rail_and_pane(&window);
                assert_eq!(
                    (rail, content),
                    (rail_after, content_after),
                    "revealing the rails moved an edge at text scale {scale}, {preset}, {label} ({width} px)"
                );
                window.destroy();
                flush_events();
            }
        }
    }
    gtk_settings.set_gtk_xft_dpi(96 * 1024);
}

/// The protected chrome at one `after-paint`.
#[derive(Clone, Debug, PartialEq)]
struct ChromeFrame {
    header_bar: Option<Rect>,
    tab_bar: Option<Rect>,
    status_bar: Option<Rect>,
    open_button: Option<String>,
    rail: Edges,
    content: Edges,
}

fn chrome_frame(window: &LushtextWindow) -> ChromeFrame {
    let imp = window.imp();
    let rect = |widget: &gtk4::Widget| {
        widget.compute_bounds(window).map(|bounds| {
            (
                px(bounds.x()),
                px(bounds.y()),
                px(bounds.width()),
                px(bounds.height()),
            )
        })
    };
    let (rail, content, _) = rail_and_pane(window);
    ChromeFrame {
        header_bar: rect(imp.header_bar.upcast_ref()),
        tab_bar: rect(imp.tab_bar.upcast_ref()),
        status_bar: rect(imp.status_bar.upcast_ref()),
        open_button: imp.open_button_stack.visible_child_name().map(String::from),
        rail,
        content,
    }
}

#[test]
fn test_the_protected_chrome_is_constant_in_every_frame_of_composition() {
    let app = app();
    let gtk_settings = gtk4::Settings::default().expect("default GTK settings");
    // The last case is a width where the Open-button breakpoint applies: at a
    // text scale of 2 its `max-width: 400sp` holds at 700 px (ledger A18).
    for (dpi, width, label) in [
        (96, 1700, "wide"),
        (96, 1150, "medium"),
        (96, 760, "compact"),
        (192, 700, "Open-button breakpoint"),
    ] {
        gtk_settings.set_gtk_xft_dpi(dpi * 1024);
        seed_layout(true, true, width, 860);
        let window = LushtextWindow::new(app.upcast_ref());
        let trace = Rc::new(RefCell::new(Vec::<ChromeFrame>::new()));
        let recorder = Rc::clone(&trace);
        window.connect_realize(move |window| {
            let Some(clock) = window.frame_clock() else {
                return;
            };
            let recorder = Rc::clone(&recorder);
            let window_weak = window.downgrade();
            clock.connect_after_paint(move |_| {
                if let Some(window) = window_weak.upgrade() {
                    // Up to and including the frame that completes composition.
                    let complete_before = recorder.borrow().last().is_some()
                        && window.composition_evidence().complete;
                    if !complete_before || recorder.borrow().len() < 2 {
                        recorder.borrow_mut().push(chrome_frame(&window));
                    }
                }
            });
        });
        window.present();
        wait_composed(&window);
        flush_after_delay(Duration::from_millis(60));
        let trace = trace.borrow();
        assert!(
            trace.len() >= 2,
            "{label}: composition spans several frames"
        );
        let first = &trace[0];
        assert!(first.header_bar.is_some() && first.status_bar.is_some());
        for (index, frame) in trace.iter().enumerate() {
            assert_eq!(
                frame, first,
                "{label}: frame {index} moved the protected chrome"
            );
        }
        window.destroy();
        flush_events();
    }
    gtk_settings.set_gtk_xft_dpi(96 * 1024);
}

/// A CLI open admits its files in stage B1, after the first frame, but the
/// page it adds is certain from stage C: the tab strip is drawn from the
/// first frame and does not move when the page arrives.
#[test]
fn test_a_cli_open_draws_the_tab_strip_from_the_first_frame() {
    let app = app();
    seed_layout(false, false, 1150, 860);
    let dir = tempfile::tempdir().expect("activation tempdir");
    let path = dir.path().join("first.txt");
    fixture::write_text(&path, "first\n");
    let trace = Rc::new(RefCell::new(Vec::<(Option<Rect>, i32)>::new()));
    let recorder = Rc::clone(&trace);
    let added = app.connect_window_added(move |_, window| {
        let Some(window) = window.downcast_ref::<LushtextWindow>() else {
            return;
        };
        let recorder = Rc::clone(&recorder);
        window.connect_realize(move |window| {
            let Some(clock) = window.frame_clock() else {
                return;
            };
            let recorder = Rc::clone(&recorder);
            let window_weak = window.downgrade();
            clock.connect_after_paint(move |_| {
                // Stage R realizes the window in a dispatch of its own, so its
                // frame clock can paint once before `present` maps it; the
                // first presented frame is the first mapped one.
                if let Some(window) = window_weak.upgrade()
                    && window.is_mapped()
                    && !(recorder.borrow().len() >= 2 && window.composition_evidence().complete)
                {
                    let frame = chrome_frame(&window);
                    recorder
                        .borrow_mut()
                        .push((frame.tab_bar, window.imp().tab_view.n_pages()));
                }
            });
        });
    });

    app.open(&[gio::File::for_path(&path)], "");
    flush_events();
    let windows = lushtext_windows(&app);
    assert_eq!(windows.len(), 1);
    let window = &windows[0];
    wait_composed(window);
    wait_until(COMPOSED, || window.imp().tab_view.n_pages() == 1);
    flush_after_delay(Duration::from_millis(60));
    app.disconnect(added);
    let trace = trace.borrow();
    assert!(
        trace.len() >= 2,
        "composition spans several frames: {trace:?}"
    );
    let (first_strip, first_pages) = trace[0];
    assert_eq!(first_pages, 0, "the page arrives after the first frame");
    assert!(
        first_strip.is_some_and(|(_, _, width, height)| width > 0 && height > 0),
        "the tab strip is drawn in the first frame: {trace:?}"
    );
    assert!(
        trace.iter().all(|(strip, _)| *strip == first_strip),
        "the tab strip never moves during composition: {trace:?}"
    );
    window.destroy();
    flush_events();
}

// --- Readiness, teardown, focus (D5, D6) ------------------------------------

#[test]
fn test_the_readiness_blocker_covers_composition_and_ends_with_it() {
    seed_layout(true, false, 1600, 900);
    let app = app();
    set_composition_hold_before_for_test(Some(CompositionStage::Content));
    let window = LushtextWindow::new(app.upcast_ref());
    set_composition_hold_before_for_test(None);
    window.present();
    wait_held_before(&window, CompositionStage::Content);
    assert_eq!(
        current_idle_blocker(&app).as_deref(),
        Some("window-composition"),
        "idle waits for composition"
    );
    let actions =
        glib::MainContext::default()
            .block_on(lushtext_core::ui::automation::wait_for_ready_for_test(
            app.clone(),
            lushtext_core::model::automation::AutomationReadinessPredicate::WindowActionsExported,
            100,
        ));
    assert!(
        actions.ok,
        "window-actions-exported does not wait for composition"
    );
    window.release_composition_hold_for_test();
    wait_composed(&window);
    assert_ne!(
        current_idle_blocker(&app).as_deref(),
        Some("window-composition")
    );

    set_composition_hold_before_for_test(Some(CompositionStage::Content));
    let held = LushtextWindow::new(app.upcast_ref());
    set_composition_hold_before_for_test(None);
    held.present();
    wait_held_before(&held, CompositionStage::Content);
    held.destroy();
    flush_events();
    assert!(evidence(&held).torn_down);
    assert!(
        !evidence(&held).blocks_readiness,
        "the blocker cannot outlive its window"
    );
    window.destroy();
    flush_events();
}

#[test]
fn test_closing_during_each_stage_stops_composition() {
    let app = app();
    for stage in HOLD_POINTS {
        seed_layout(true, true, 1600, 900);
        set_composition_hold_before_for_test(Some(stage));
        let window = LushtextWindow::new(app.upcast_ref());
        set_composition_hold_before_for_test(None);
        window.present();
        wait_held_before(&window, stage);
        crate::common::wait_for_startup_data_flow(&window);
        window.close();
        wait_until(COMPOSED, || !app.windows().contains(window.upcast_ref()));
        let at_close = evidence(&window);
        assert!(
            at_close.torn_down,
            "leaving the application tears composition down"
        );
        window.release_composition_hold_for_test();
        flush_after_delay(Duration::from_millis(200));
        assert_eq!(
            evidence(&window).done,
            at_close.done,
            "a stage ran after the window closed during {stage:?}"
        );
    }
}

#[test]
fn test_closing_before_restore_reaches_the_pages_keeps_unrestored_descriptors() {
    // First: GTK initialization points app data at the test's own directory,
    // so the seeded session can never reach the user's.
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("session tempdir");
    let path = dir.path().join("kept.txt");
    fixture::write_text(&path, "kept\n");
    lushtext_core::services::session_service::save(
        &lushtext_core::services::json_store::data_dir(),
        &session_with(&path),
    )
    .expect("seed the session");
    seed_layout(true, false, 1600, 900);
    let app = app();
    set_composition_hold_before_for_test(Some(CompositionStage::Content));
    let window = LushtextWindow::new(app.upcast_ref());
    set_composition_hold_before_for_test(None);
    window.present();
    wait_held_before(&window, CompositionStage::Content);
    crate::common::wait_for_startup_data_flow(&window);
    window.close();
    wait_until(COMPOSED, || !app.windows().contains(window.upcast_ref()));
    let session = lushtext_core::services::session_service::load(
        &lushtext_core::services::json_store::data_dir(),
    )
    .expect("load the session");
    assert!(
        session
            .tabs
            .iter()
            .any(|tab| tab.path.as_deref() == Some(path.as_path())),
        "closing during composition keeps the session's descriptors: {:?}",
        session.tabs
    );
}

/// A CLI open keeps its priority over the restored session, as before staged
/// composition: its tab comes first and stays selected, though activation
/// opens are admitted only in stage B1 and the session loads in stage C.
#[test]
fn test_a_cli_open_keeps_priority_over_the_restored_session() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("session tempdir");
    let restored = dir.path().join("restored.txt");
    let cli = dir.path().join("cli.txt");
    fixture::write_text(&restored, "restored\n");
    fixture::write_text(&cli, "cli\n");
    lushtext_core::services::session_service::save(
        &lushtext_core::services::json_store::data_dir(),
        &session_with(&restored),
    )
    .expect("seed the session");
    seed_layout(false, false, 1600, 900);
    let app = app();
    app.open(&[gio::File::for_path(&cli)], "");
    flush_events();
    let window = lushtext_windows(&app).remove(0);
    wait_composed(&window);
    wait_until(COMPOSED, || {
        window.imp().tab_view.n_pages() == 2 && !window.session_restore_evidence().restoring
    });
    let path_at = |index: i32| {
        window
            .imp()
            .tab_view
            .nth_page(index)
            .child()
            .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
            .ok()
            .and_then(|editor| editor.file_path())
    };
    assert_eq!(path_at(0), Some(cli.clone()), "the CLI tab comes first");
    let selected = window
        .imp()
        .tab_view
        .selected_page()
        .and_then(|page| {
            page.child()
                .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
                .ok()
        })
        .and_then(|editor| editor.file_path());
    assert_eq!(selected, Some(cli), "the CLI tab stays selected");
    window.destroy();
    flush_events();
}

/// A window that left its application opens nothing more, so files that
/// still reach it must not count as tabs to come.
#[test]
fn test_activation_files_after_teardown_are_dropped() {
    seed_layout(false, false, 1600, 900);
    let app = app();
    let window = LushtextWindow::new(app.upcast_ref());
    window.present();
    wait_composed(&window);
    window.close();
    wait_until(COMPOSED, || !app.windows().contains(window.upcast_ref()));
    let dir = tempfile::tempdir().expect("activation tempdir");
    let late = dir.path().join("late.txt");
    fixture::write_text(&late, "late\n");
    window.admit_activation_files(vec![gio::File::for_path(&late)]);
    assert!(
        !window.imp().tab_bar.property::<bool>("visible"),
        "a torn-down window's tab strip does not wait for pages that never open"
    );
}

/// Bookmark navigation focuses the editor, so issued before stage B1 (with a
/// restored tab in the still invisible tab view) it reveals the content first.
#[test]
fn test_bookmark_navigation_before_the_content_stage_focuses_the_editor() {
    seed_layout(false, false, 1600, 900);
    let app = app();
    let dir = tempfile::tempdir().expect("bookmark tempdir");
    let path = dir.path().join("marked.txt");
    fixture::write_text(&path, "one\ntwo\nthree\n");
    set_composition_hold_before_for_test(Some(CompositionStage::Content));
    let window = LushtextWindow::new(app.upcast_ref());
    set_composition_hold_before_for_test(None);
    window.present();
    wait_held_before(&window, CompositionStage::Content);
    window.restore_session_for_test(session_with(&path));
    wait_until(COMPOSED, || {
        window.imp().tab_view.n_pages() == 1 && no_tab_is_loading(&window)
    });
    activate(&window, "toggle-bookmark", None);
    activate(&window, "next-bookmark", None);
    let editor = window
        .imp()
        .tab_view
        .nth_page(0)
        .child()
        .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
        .expect("the restored editor");
    assert!(
        evidence(&window).content_visible,
        "navigating to a bookmark reveals the content it focuses"
    );
    assert!(
        gtk4::prelude::GtkWindowExt::focus(&window)
            .is_some_and(|focus| &focus == editor.source_view().upcast_ref::<gtk4::Widget>()),
        "the bookmark's editor has keyboard focus"
    );
    window.release_composition_hold_for_test();
    wait_composed(&window);
    window.destroy();
    flush_events();
}

/// Task 2.4's preconditions for forced reveals (design D4, D5), checked on
/// the toolkit and on the window: a widget made visible and focused in one
/// dispatch takes focus, and an editor page appended to the still invisible
/// tab view has the window as its root, so its load priority and focus work
/// as before composition.
#[test]
fn test_staged_content_is_rooted_and_focusable_in_the_dispatch_that_reveals_it() {
    ensure_gtk_init();
    let entry = gtk4::Entry::new();
    entry.set_visible(false);
    let probe = gtk4::Window::builder().child(&entry).build();
    probe.present();
    flush_after_delay(Duration::from_millis(60));
    entry.set_visible(true);
    assert!(
        entry.grab_focus(),
        "a widget made visible in this dispatch takes focus"
    );
    assert!(
        gtk4::prelude::GtkWindowExt::focus(&probe).is_some_and(|focus| {
            focus == *entry.upcast_ref::<gtk4::Widget>() || focus.is_ancestor(&entry)
        }),
        "the focus is the entry (its text delegate)"
    );
    probe.destroy();

    seed_layout(false, false, 1600, 900);
    let app = app();
    let dir = tempfile::tempdir().expect("root tempdir");
    let path = dir.path().join("rooted.txt");
    fixture::write_text(&path, "rooted\n");
    set_composition_hold_before_for_test(Some(CompositionStage::Content));
    let window = LushtextWindow::new(app.upcast_ref());
    set_composition_hold_before_for_test(None);
    window.present();
    wait_held_before(&window, CompositionStage::Content);
    window.restore_session_for_test(session_with(&path));
    wait_until(COMPOSED, || window.imp().tab_view.n_pages() == 1);
    let editor = window
        .imp()
        .tab_view
        .nth_page(0)
        .child()
        .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
        .expect("the restored editor");
    assert!(
        !evidence(&window).content_visible,
        "precondition: B1 has not run"
    );
    assert_eq!(
        editor
            .root()
            .as_ref()
            .map(glib::object::Cast::upcast_ref::<gtk4::Widget>),
        Some(window.upcast_ref::<gtk4::Widget>()),
        "a page in the invisible tab view is rooted in the window"
    );
    window.release_composition_hold_for_test();
    wait_composed(&window);
    window.destroy();
    flush_events();
}

/// A CLI open into an empty session is persisted: the tab opens in stage B1
/// while the startup restore still runs (its session saves are suppressed), so
/// the restore's end must save what it held back.
#[test]
fn test_a_cli_open_into_an_empty_session_is_saved() {
    ensure_gtk_init();
    let data_dir = lushtext_core::services::json_store::data_dir();
    lushtext_core::services::session_service::save(&data_dir, &SessionData::default())
        .expect("seed an empty session");
    let dir = tempfile::tempdir().expect("activation tempdir");
    let cli = dir.path().join("saved.txt");
    fixture::write_text(&cli, "saved\n");
    seed_layout(false, false, 1600, 900);
    let app = app();
    app.open(&[gio::File::for_path(&cli)], "");
    flush_events();
    let window = lushtext_windows(&app).remove(0);
    wait_composed(&window);
    wait_until(COMPOSED, || {
        window.imp().tab_view.n_pages() == 1 && !window.session_restore_evidence().restoring
    });
    let saved = || {
        lushtext_core::services::session_service::load(&data_dir).is_ok_and(|session| {
            session
                .tabs
                .iter()
                .any(|tab| tab.path.as_deref() == Some(cli.as_path()))
        })
    };
    assert!(
        crate::common::wait_until_or_false(Duration::from_secs(5), saved),
        "the session file records the CLI tab"
    );
    window.destroy();
    flush_events();
}

/// Where keyboard focus sits, by the shell region that owns it.
fn focus_region(window: &LushtextWindow) -> String {
    let imp = window.imp();
    let Some(focus) = gtk4::prelude::GtkWindowExt::focus(window) else {
        return "none".to_owned();
    };
    let regions: [(&str, gtk4::Widget); 5] = [
        ("header", imp.header_bar.clone().upcast()),
        ("tabs", imp.tab_bar.clone().upcast()),
        ("content", imp.content_stack.clone().upcast()),
        ("sidebar", imp.sidebar.clone().upcast()),
        ("status", imp.status_bar.clone().upcast()),
    ];
    for (name, region) in regions {
        if focus == region || focus.is_ancestor(&region) {
            return format!("{name}:{}", focus.type_().name());
        }
    }
    format!("other:{}", focus.type_().name())
}

#[test]
fn test_initial_focus_matches_a_composed_window() {
    let dir = tempfile::tempdir().expect("activation tempdir");
    let path = dir.path().join("focus.txt");
    fixture::write_text(&path, "focus\n");
    for files in [vec![], vec![gio::File::for_path(&path)]] {
        let mut regions = Vec::new();
        for collapsed in [true, false] {
            seed_layout(true, false, 1600, 900);
            let app = app();
            set_composition_collapsed_for_test(collapsed);
            if files.is_empty() {
                app.activate();
            } else {
                app.open(&files, "");
            }
            flush_events();
            set_composition_collapsed_for_test(false);
            let window = lushtext_windows(&app).pop().expect("a window");
            wait_composed(&window);
            wait_until(COMPOSED, || {
                usize::try_from(window.imp().tab_view.n_pages()).unwrap_or(0) == files.len()
            });
            flush_after_delay(Duration::from_millis(200));
            regions.push(focus_region(&window));
            window.destroy();
            flush_events();
        }
        assert_eq!(
            regions[0],
            regions[1],
            "initial focus with {} activation file(s)",
            files.len()
        );
    }
}

// --- The evidence surface's driven proofs ----------------------------------

/// Proof 1 of 3 — reading must not mutate, across every operation that takes
/// the runtime's mutable borrow: a forced reveal, a frame-gated reveal, an
/// activation admission, and teardown.
#[test]
fn test_composition_evidence_reads_stay_side_effect_free_across_composition() {
    seed_layout(false, false, 1600, 900);
    let app = app();
    set_composition_hold_before_for_test(Some(CompositionStage::Content));
    let window = LushtextWindow::new(app.upcast_ref());
    set_composition_hold_before_for_test(None);
    let constructed = evidence(&window);
    assert_eq!(constructed, evidence(&window));

    window.present();
    wait_held_before(&window, CompositionStage::Content);
    let held = evidence(&window);
    assert_eq!(held, evidence(&window));
    assert_ne!(held.done, constructed.done, "realize and present ran");

    activate(&window, "set-sidebar-visible", Some(&true.to_variant()));
    let forced = evidence(&window);
    assert_eq!(forced, evidence(&window));
    assert_eq!(forced.forced_stages, 2, "the sidebar forced B1 and B2");

    let dir = tempfile::tempdir().expect("activation tempdir");
    let path = dir.path().join("admitted.txt");
    fixture::write_text(&path, "admitted\n");
    window.admit_activation_files(vec![gio::File::for_path(&path)]);
    let admitted = evidence(&window);
    assert_eq!(admitted, evidence(&window));

    window.release_composition_hold_for_test();
    wait_composed(&window);
    let composed = evidence(&window);
    assert_eq!(composed, evidence(&window));
    window.destroy();
    flush_events();
    let torn_down = evidence(&window);
    assert_eq!(torn_down, evidence(&window));
    assert!(torn_down.torn_down);
}

/// Proof 2 of 3 — a disposed window answers honestly instead of panicking.
#[test]
fn test_composition_evidence_answers_honestly_after_the_window_is_disposed() {
    seed_layout(true, true, 1600, 900);
    let app = app();
    let window = LushtextWindow::new(app.upcast_ref());
    present_window(&window);
    window.close();
    flush_events();
    // SAFETY: this test window is disposed exactly once, and everything after
    // it only reads the evidence surface, which uses `try_get()`.
    unsafe { window.run_dispose() };
    let disposed = evidence(&window);
    assert!(disposed.torn_down);
    assert!(
        !disposed.content_visible && !disposed.workspace_visible && !disposed.properties_visible,
        "disposed template children read as not visible: {disposed:?}"
    );
    assert!(!disposed.blocks_readiness);
}

/// Proof 3 of 3 — reading materializes nothing. No lazily populated toolkit
/// collection is in reach, so the proof is that reads leave the workspace
/// tree's materialized state and the realized sidebar rows unchanged.
#[test]
fn test_composition_evidence_reads_materialize_nothing() {
    seed_layout(true, false, 1600, 900);
    let app = app();
    let window = LushtextWindow::new(app.upcast_ref());
    present_window(&window);
    let before = window.imp().sidebar.workspace_tree_evidence();
    for _ in 0..8 {
        let _ = evidence(&window);
    }
    flush_events();
    assert_eq!(window.imp().sidebar.workspace_tree_evidence(), before);
    window.destroy();
    flush_events();
}
