// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared test infrastructure for widget tests.
//!
//! GTK4 must be initialized before constructing any widget, and GResources
//! must be registered before constructing widgets that use composite templates.
//! Both operations are one-time setup via `std::sync::Once`.

use gio::prelude::{ApplicationExt, Cast, ListModelExt, ObjectExt};
use glib::object::CastNone;
use glib::prelude::IsA;
use glib::prelude::ToValue;
pub use gtk_lush_proof_harness::{flush_after_delay, flush_events, wait_until};
use gtk4::prelude::{AdjustmentExt, GtkWindowExt, WidgetExt};
use lushtext_core::config::APP_ID;
pub use lushtext_core::services::filesystem::{
    fixture, metadata as fs_metadata, mutate as fs_mutate, read as fs_read,
};
use std::ffi::OsString;
use std::path::Path;
use std::sync::Once;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static GTK_INIT: Once = Once::new();

/// Initialize GTK4, register GResources, and set up GSettings for testing.
/// Safe to call multiple times; the actual work only runs once.
///
/// Uses the in-memory GSettings backend so tests don't pollute user's dconf.
/// Sets `LUSHTEXT_DATA_DIR` to a temp directory so session/draft I/O doesn't
/// touch the user's real data.
/// Requires the private headless compositor owned by the widget harness.
pub fn ensure_gtk_init() {
    GTK_INIT.call_once(|| {
        // Widget tests run in one isolated process before GTK startup, so they
        // can safely pin a memory-only GSettings backend for deterministic runs.
        // SAFETY: widget tests set these process environment variables before
        // GTK startup and before any background worker threads are spawned.
        unsafe { std::env::set_var("GSETTINGS_BACKEND", "memory") };
        // Isolate session/draft I/O from the user's real data directory.
        // PID-based naming prevents nextest's parallel test processes from
        // interfering with each other via shared session files.
        let test_data_dir =
            std::env::temp_dir().join(format!("lushtext-test-{}", std::process::id()));
        let _ = fs_mutate::remove_dir_all_if_exists(&test_data_dir);
        let _ = fs_mutate::create_dir_all(&test_data_dir);
        // SAFETY: widget tests set these process environment variables before
        // GTK startup and before any background worker threads are spawned.
        unsafe { std::env::set_var("LUSHTEXT_DATA_DIR", &test_data_dir) };
        lushtext_core::init_schema_dir();
        gtk4::init()
            .expect("GTK4 init failed — is a display server available? Try mutter --headless.");
        // Initialize libadwaita so widget templates can instantiate Adw widgets
        // (e.g. AdwWrapBox in the inline alert) in tests that construct widgets
        // directly without going through AdwApplication startup. adw_init is
        // idempotent, so later AdwApplication startups remain safe.
        libadwaita::init().expect("libadwaita init failed");
        sourceview5::init();
        lushtext_core::register_resources();
    });
}

pub fn test_application() -> libadwaita::Application {
    ensure_gtk_init();
    static APP_COUNTER: AtomicUsize = AtomicUsize::new(0);
    let app_id = format!(
        "{APP_ID}.widget-test-{}-{}",
        std::process::id(),
        APP_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let app: libadwaita::Application =
        lushtext_core::app::LushtextApplication::new_with_application_id(&app_id).upcast();
    app.register(gio::Cancellable::NONE)
        .expect("test application registration");
    app.emit_by_name::<()>("startup", &[]);
    while glib::MainContext::default().iteration(false) {}
    app
}

pub fn test_window() -> lushtext_core::ui::window::LushtextWindow {
    let app = test_application();
    lushtext_core::ui::window::LushtextWindow::new(&app)
}

/// Present a test window and wait for the headless compositor to realize it.
///
/// This is the single shared presentation helper for the widget-test tree.
/// Window realization is a precondition, not the behavior under test, so it
/// gives the compositor a generous async-scale budget for the surface
/// `configure` that yields a non-zero allocation; `wait_until` returns the
/// instant the size is real, so the larger ceiling only costs time on a slow,
/// loaded compositor. A short post-realization settle drains main-loop work
/// scheduled during allocation before the caller interacts with the window.
pub fn present_window(window: &(impl IsA<gtk4::Window> + IsA<gtk4::Widget>)) {
    window.present();
    wait_until(Duration::from_secs(5), || {
        window.width() > 0 && window.height() > 0
    });
    flush_after_delay(Duration::from_millis(20));
}

/// Temporarily point app-data I/O at a fresh directory for one widget test.
///
/// `json_store::data_dir()` reads `LUSHTEXT_DATA_DIR` dynamically, so tests
/// that need deterministic startup or Preferences data scans can isolate only
/// their own metadata without depending on earlier widget-test residue.
pub struct IsolatedDataDir {
    tempdir: tempfile::TempDir,
    previous: Option<OsString>,
}

impl IsolatedDataDir {
    #[must_use]
    pub fn path(&self) -> &Path {
        self.tempdir.path()
    }
}

impl Drop for IsolatedDataDir {
    fn drop(&mut self) {
        if let Some(previous) = self.previous.take() {
            // SAFETY: widget tests are driven on the single GTK harness thread;
            // this guard restores the process env after windows from the test
            // have been dropped.
            unsafe { std::env::set_var("LUSHTEXT_DATA_DIR", previous) };
        } else {
            // SAFETY: see the restoration case above.
            unsafe { std::env::remove_var("LUSHTEXT_DATA_DIR") };
        }
    }
}

pub fn isolated_data_dir() -> IsolatedDataDir {
    ensure_gtk_init();
    let tempdir = tempfile::tempdir().expect("isolated app data tempdir");
    let previous = std::env::var_os("LUSHTEXT_DATA_DIR");
    // SAFETY: widget tests are serialized by the GTK harness before the test
    // constructs windows or starts background app-data tasks.
    unsafe { std::env::set_var("LUSHTEXT_DATA_DIR", tempdir.path()) };
    IsolatedDataDir { tempdir, previous }
}

fn try_emit_key_pressed(widget: &gtk4::Widget, key: gtk4::gdk::Key) -> Option<glib::Propagation> {
    let controllers = widget.observe_controllers();
    for index in 0..controllers.n_items() {
        if let Some(controller) = controllers
            .item(index)
            .and_then(|object| object.downcast::<gtk4::EventControllerKey>().ok())
        {
            let args: [&dyn ToValue; 3] = [&key, &0u32, &gtk4::gdk::ModifierType::empty()];
            let stopped: bool =
                glib::object::ObjectExt::emit_by_name(&controller, "key-pressed", &args);
            return Some(if stopped {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            });
        }
    }
    None
}

/// Emit a synthetic key press on the window's currently focused widget.
pub fn emit_key_pressed_on_focus(
    window: &impl IsA<gtk4::Window>,
    key: gtk4::gdk::Key,
) -> glib::Propagation {
    let focus = window
        .as_ref()
        .focus()
        .expect("window should have a focused widget");
    let mut current = Some(focus);
    while let Some(widget) = current {
        if let Some(result) = try_emit_key_pressed(&widget, key) {
            return result;
        }
        current = widget.parent();
    }
    panic!("focused widget ancestry had no EventControllerKey");
}

/// Realized row widgets of a `GtkListView`: the visible, non-zero-height
/// children GTK currently keeps for the model. This is the rendered surface,
/// not the model, which is the distinction virtualization bugs hide behind.
pub fn realized_list_rows(list: &gtk4::ListView) -> Vec<gtk4::Widget> {
    let mut rows = Vec::new();
    let mut child = list.first_child();
    while let Some(widget) = child {
        if widget.is_visible() && widget.height() > 0 {
            rows.push(widget.clone());
        }
        child = widget.next_sibling();
    }
    rows
}

/// The first widget in `root`'s subtree (including `root`) matching `pred`,
/// walking first-child/next-sibling depth first.
/// Wait until the window's async command-palette index rebuild reaches a size.
/// Poll until `predicate` holds, returning whether it did within `budget`.
///
/// `wait_until` panics on timeout, which makes it an assertion rather than a
/// question; a test asserting that something is **not** reachable needs the
/// question. This wraps the shared helper rather than hand-rolling a poll loop,
/// because `wait_until`'s mechanism is load-bearing and is the opposite of the
/// obvious one: it sleeps briefly and *then* drains every ready main-loop
/// source, which is what dispatches `spawn_blocking_then`'s low-priority
/// `idle_add_once` completion. A loop that drains first and sleeps after
/// starves exactly the terminal it is waiting for.
pub fn wait_until_or_false(budget: Duration, predicate: impl FnMut() -> bool) -> bool {
    let mut predicate = predicate;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        wait_until(budget, &mut predicate);
    }))
    .is_ok()
}

/// The editor page of the currently selected tab.
///
/// Promoted here after a fifth copy appeared; `window.rs`, `app.rs`,
/// `command_palette.rs`, and `open_popover.rs` each grew their own.
pub fn active_editor(
    window: &lushtext_core::ui::window::LushtextWindow,
) -> lushtext_core::ui::editor_page::LushtextEditorPage {
    use glib::subclass::prelude::ObjectSubclassIsExt;
    use gtk4::prelude::Cast;
    window
        .imp()
        .tab_view
        .selected_page()
        .expect("selected tab page")
        .child()
        .downcast::<lushtext_core::ui::editor_page::LushtextEditorPage>()
        .expect("editor page child")
}

pub fn wait_for_palette_index(
    window: &lushtext_core::ui::window::LushtextWindow,
    expected_index: usize,
) {
    use glib::subclass::prelude::ObjectSubclassIsExt;
    wait_until(Duration::from_secs(10), || {
        window.imp().command_palette.file_index_len() == expected_index
    });
}

pub fn find_descendant(
    root: &gtk4::Widget,
    mut pred: impl FnMut(&gtk4::Widget) -> bool,
) -> Option<gtk4::Widget> {
    let mut stack = vec![root.clone()];
    while let Some(current) = stack.pop() {
        if pred(&current) {
            return Some(current);
        }
        let mut child = current.first_child();
        while let Some(next) = child {
            stack.push(next.clone());
            child = next.next_sibling();
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Rendered-row geometry: where a row is drawn, not what an adjustment says.
//
// Adjustment-value assertions cannot see a child that renders against a value
// other than the one its host placed it at. These probes pin a named row's
// on-screen placement across forced layout passes; the sidebar suite and the
// GTK Lush adoption suite share them and differ only in how a label resolves
// to a model index and which chrome the placement is measured against.
// ---------------------------------------------------------------------------

/// Realized rows that are actually drawn. `GtkListView` keeps rows around the
/// selection and focus realized but child-invisible; they have bounds and no
/// pixels, so any placement probe must skip them.
pub fn mapped_list_rows(list: &gtk4::ListView) -> impl Iterator<Item = gtk4::Widget> {
    realized_list_rows(list)
        .into_iter()
        .filter(WidgetExt::is_mapped)
}

/// The first `GtkLabel` in a row's subtree.
pub fn first_label(row: &gtk4::Widget) -> Option<gtk4::Label> {
    find_descendant(row, glib::object::ObjectExt::is::<gtk4::Label>).and_downcast::<gtk4::Label>()
}

/// The mapped row whose first label reads `label`, resolved fresh each call
/// because list rows are recycled and a held handle can be rebound.
pub fn mapped_row_with_label(list: &gtk4::ListView, label: &str) -> Option<gtk4::Widget> {
    mapped_list_rows(list).find(|row| first_label(row).is_some_and(|found| found.text() == label))
}

/// Top and height of a row relative to a fixed reference widget above it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowPlacement {
    pub top: f32,
    pub height: f32,
}

/// Where `row` is drawn relative to `reference`. Relative to chrome above the
/// host rather than to the scroller, so a genuine outer move cannot pass for
/// row stability nor the reverse.
pub fn placement_relative_to(
    row: &gtk4::Widget,
    reference: &impl IsA<gtk4::Widget>,
) -> Option<RowPlacement> {
    row.compute_bounds(reference).map(|bounds| RowPlacement {
        top: bounds.y(),
        height: bounds.height(),
    })
}

/// Pin `scroller`'s own height to `height` pixels. GTK4 cannot shrink a
/// presented window, so a test that needs a smaller or larger viewport pins
/// the scroller instead of resizing the window.
pub fn pin_scroller_height(scroller: &gtk4::ScrolledWindow, height: i32) {
    scroller.set_vexpand(false);
    scroller.set_valign(gtk4::Align::Start);
    scroller.set_propagate_natural_height(true);
    // Lift the old maximum first: GTK rejects a minimum above the current
    // maximum, and a growing height would be one.
    scroller.set_max_content_height(-1);
    scroller.set_min_content_height(height);
    scroller.set_max_content_height(height);
}

/// Run a layout pass now rather than waiting for the compositor to deliver a
/// frame: `flush_after_delay` alone only pumps the main loop, and the queued
/// allocation runs on the next headless frame tick.
pub fn force_layout<'a>(widgets: impl IntoIterator<Item = &'a gtk4::Widget>) {
    for widget in widgets {
        widget.queue_allocate();
    }
    flush_after_delay(Duration::from_millis(16));
}

/// Sample a placement across `samples` forced layout passes, so a row that
/// moves and comes back inside one interval is still caught.
pub fn sample_placements(
    samples: usize,
    mut force: impl FnMut(),
    mut placement: impl FnMut() -> Option<RowPlacement>,
) -> Vec<RowPlacement> {
    (0..samples)
        .map(|_| {
            force();
            placement().expect("the sampled row must stay rendered")
        })
        .collect()
}

/// A rendered list the stillness assertion can drive without knowing the
/// fixture: how to list the fully visible labels, select and focus a row by
/// label, and sample where the anchor row is drawn.
pub struct RowStillnessProbe<'a> {
    pub outer: &'a gtk4::Adjustment,
    pub visible_labels: &'a dyn Fn() -> Vec<String>,
    pub select: &'a dyn Fn(&str),
    pub focus: &'a dyn Fn(&str),
    pub sample: &'a dyn Fn(&str, usize) -> Vec<RowPlacement>,
}

/// Select, then focus, up to seven already-visible rows, and require the first
/// visible row to be drawn at exactly the same place throughout while the
/// outer scroller has not moved.
#[track_caller]
pub fn assert_rows_still_across_selection(probe: &RowStillnessProbe<'_>, when: &str) {
    let resting = probe.outer.value();
    let visible = (probe.visible_labels)();
    assert!(
        visible.len() >= 5,
        "this check needs at least five fully visible rows {when}; saw {}",
        visible.len()
    );
    let anchor = &visible[0];
    let baseline = (probe.sample)(anchor, 3);
    let expected = baseline[0];
    assert!(
        baseline.iter().all(|placement| *placement == expected),
        "the anchor row must rest before selection starts {when}; saw {baseline:?}"
    );
    for target in visible.iter().skip(1).take(7) {
        (probe.select)(target);
        let selected = (probe.sample)(anchor, 3);
        (probe.focus)(target);
        let focused = (probe.sample)(anchor, 3);
        assert!(
            (probe.outer.value() - resting).abs() < gtk_lush_widgets::ADJUSTMENT_EPSILON,
            "selecting an already visible row must not move the outer scroller {when}: {target} \
             moved it from {resting} to {}",
            probe.outer.value()
        );
        for (how, samples) in [("selecting", &selected), ("focusing", &focused)] {
            assert!(
                samples.iter().all(|placement| *placement == expected),
                "{how} {target} must not move the rendered rows {when}: {anchor} rested at \
                 {expected:?} and was drawn at {samples:?}"
            );
        }
    }
}

/// The first mapped row that straddles the bottom edge of `viewport`, with how
/// far it overflows; the fixture for a genuine few-pixel reveal request.
pub fn row_straddling_bottom(
    list: &gtk4::ListView,
    viewport: &impl IsA<gtk4::Widget>,
) -> Option<(gtk4::Widget, f64)> {
    let viewport_bottom = f64::from(viewport.as_ref().height());
    mapped_list_rows(list).find_map(|row| {
        let bounds = row.compute_bounds(viewport)?;
        let overflow = f64::from(bounds.y() + bounds.height()) - viewport_bottom;
        (f64::from(bounds.y()) < viewport_bottom && overflow > gtk_lush_widgets::ADJUSTMENT_EPSILON)
            .then_some((row, overflow))
    })
}

/// The positive control for the stillness checks: an honoured reveal must move
/// the outer at least by the row's overflow and then rest. How far it travels
/// is the list's decision, not the host's; see
/// `gtk_lush_widgets::outer_scroll_request` for why.
#[track_caller]
pub fn assert_reveal_then_rest(resting: f64, overflow: f64, settled: &[f64], label: &str) {
    let moved = settled[0] - resting;
    assert!(
        moved >= overflow - gtk_lush_widgets::ADJUSTMENT_EPSILON,
        "revealing {label}, clipped by {overflow:.1}px, must scroll the outer at least that far; \
         it moved {moved:.1}px"
    );
    assert!(
        settled.iter().all(|value| (value - settled[0]).abs() < 1.0),
        "an honoured request must settle instead of re-asking; saw {settled:?}"
    );
}

/// Restore every process-wide editor-load override when a test exits or unwinds.
///
/// Resets the load delay, the payload-load delay, and the transient weight
/// override together, so a test that sets any of them cannot leak it.
pub struct EditorLoadDelayReset;

impl Drop for EditorLoadDelayReset {
    fn drop(&mut self) {
        use lushtext_core::services::editor_io;
        editor_io::set_load_delay_for_test(0);
        editor_io::set_payload_load_delay_for_test(0);
        editor_io::set_transient_weight_override_for_test(None);
    }
}

/// The boolean state of a stateful window action.
pub fn action_state_bool(window: &lushtext_core::ui::window::LushtextWindow, name: &str) -> bool {
    use gio::prelude::{ActionExt, ActionMapExt};
    window
        .lookup_action(name)
        .unwrap_or_else(|| panic!("action '{name}' not found"))
        .state()
        .unwrap_or_else(|| panic!("action '{name}' should be stateful"))
        .get::<bool>()
        .unwrap_or_else(|| panic!("action '{name}' should use bool state"))
}

/// Activate a parameterless window action and drain the resulting events.
pub fn activate_action(window: &lushtext_core::ui::window::LushtextWindow, name: &str) {
    gio::prelude::ActionGroupExt::activate_action(window, name, None);
    flush_events();
}

/// The full text of an editor's buffer, hidden characters included.
pub fn editor_text(editor: &lushtext_core::ui::editor_page::LushtextEditorPage) -> String {
    use gtk4::prelude::TextBufferExt;
    let buffer = editor.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

/// A `GtkListView` of `rows` labels reading `row 0000`, `row 0001`, …, with the
/// selection model `select` builds around the strings; `setup_label`
/// configures each row label once, when its list item is set up.
pub fn numbered_label_list(
    rows: u32,
    select: impl FnOnce(gtk4::StringList) -> gtk4::SelectionModel,
    setup_label: impl Fn(&gtk4::Label) + 'static,
) -> gtk4::ListView {
    use gtk4::prelude::*;

    let strings: Vec<String> = (0..rows).map(|index| format!("row {index:04}")).collect();
    let model = gtk4::StringList::new(&strings.iter().map(String::as_str).collect::<Vec<_>>());
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(move |_, item| {
        let item = item.downcast_ref::<gtk4::ListItem>().expect("list item");
        let label = gtk4::Label::new(None);
        setup_label(&label);
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
    gtk4::ListView::new(Some(select(model)), Some(factory))
}

/// Restores the first-dirty autosave debounce to its production 750 ms.
pub struct FirstDirtyAutosaveDelayReset;

impl Drop for FirstDirtyAutosaveDelayReset {
    fn drop(&mut self) {
        lushtext_core::ui::window::set_first_dirty_autosave_delay_for_test(750);
    }
}

/// Restores every draft-pipeline test override: the automatic-recovery limit,
/// the lazy-read, body, manifest, and delete delays, and the injected faults.
pub struct DraftPipelinePolicyReset;

impl Drop for DraftPipelinePolicyReset {
    fn drop(&mut self) {
        use lushtext_core::ui::window::{
            fail_next_draft_mutations_for_test, set_automatic_draft_limit_for_test,
            set_draft_manifest_completion_delay_for_test, set_draft_mutation_delays_for_test,
            set_lazy_draft_read_delay_for_test,
        };
        set_automatic_draft_limit_for_test(
            lushtext_core::services::draft_service::MAX_AUTOMATIC_DRAFT_BYTES,
        );
        set_lazy_draft_read_delay_for_test(0);
        set_draft_mutation_delays_for_test(0, 0, 0);
        set_draft_manifest_completion_delay_for_test(0);
        fail_next_draft_mutations_for_test(false, false, false);
    }
}

/// Restores the orphan-cleanup start, follow-up, and worker delays.
pub struct OrphanCleanupPolicyReset;

impl Drop for OrphanCleanupPolicyReset {
    fn drop(&mut self) {
        lushtext_core::ui::window::set_orphan_cleanup_delays_for_test(2_000, 30_000, 0);
    }
}
