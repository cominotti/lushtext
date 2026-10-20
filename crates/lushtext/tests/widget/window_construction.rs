// SPDX-License-Identifier: GPL-3.0-or-later

//! The window-construction write-count gate (`bound-window-construction-dispatch`).
//!
//! Every test here runs on GSettings' **keyfile** backend over a private config
//! directory (`common::keyfile_settings_backend`), where each backend write
//! replaces the settings file (ledger A38). The file's identity and a directory
//! monitor counting renames onto it are the write counter, so the gate counts
//! real backend writes, not calls into LushText:
//!
//! - a steady-state profile constructs and presents a window with **zero**
//!   writes, before and after the first frame;
//! - a fresh profile writes **nothing before the first frame**, then exactly
//!   **one** coalesced flush whose values equal the pure restore oracle's;
//! - a resize burst writes at most once per persistence window;
//! - the last window's close persists the exact size synchronously, and closing
//!   a non-last window flushes nothing synchronously;
//! - a write's effects reach other readers (a window's preset, a new editor
//!   page's invisible-character mode) before any flush;
//! - a `changed` handler on the shared instance lives only as long as its
//!   owner (`settings_writes::connect_changed_for`).
//!
//! Before this change a steady-state construction made three writes and a
//! fresh one eight, every one of them inside the constructing dispatch.

use crate::common::{
    active_editor, fixture, flush_after_delay, flush_events, fs_metadata, keyfile_settings_backend,
    present_window, test_application, test_window, wait_for_startup_data_flow, wait_until,
};
use gio::prelude::*;
use gtk4::prelude::*;
use lushtext_core::config::{APP_ID, keys};
use lushtext_core::ui::settings_writes;
use lushtext_core::ui::window::{LushtextWindow, shell_geometry_evidence};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// A profile a previous launch wrote: migrated, with the workspace preset and
/// properties fraction the restore computes for the schema's default window
/// width (1200 sp: Comfy is 0.3, and a quarter of the window is 0.25).
const STEADY_PROFILE: &str = "[dev/cominotti/lushtext]\n\
    split-view-layout-migrated=true\n\
    workspace-sidebar-visible=true\n\
    workspace-sidebar-width-fraction=0.3\n\
    properties-sidebar-visible=false\n\
    properties-sidebar-width-fraction=0.25\n";

/// Long enough for a debounced flush to have run.
const AFTER_PERSISTENCE_WINDOW: Duration = Duration::from_millis(1_500);

/// The settings file's identity: a backend write replaces it (A38), so the
/// inode changes; the contents ride along so a reused inode cannot hide one.
fn identity(keyfile: &Path) -> Option<(u64, String)> {
    let inode = fs_metadata::inode(keyfile).ok()?;
    Some((inode, fixture::read_text(keyfile)))
}

/// Counts renames onto the settings file: one per backend write.
struct WriteCounter {
    _monitor: gio::FileMonitor,
    count: Rc<Cell<u32>>,
}

impl WriteCounter {
    fn new(keyfile: &Path) -> Self {
        let directory = gio::File::for_path(keyfile.parent().expect("keyfile has a parent"));
        let monitor = directory
            .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
            .expect("monitor the settings directory");
        let count = Rc::new(Cell::new(0));
        let counter = Rc::clone(&count);
        let target: PathBuf = keyfile.to_path_buf();
        monitor.connect_changed(move |_, file, other, event| {
            let onto_target = match event {
                gio::FileMonitorEvent::Renamed => {
                    other.and_then(gio::File::path).as_deref() == Some(target.as_path())
                }
                gio::FileMonitorEvent::MovedIn | gio::FileMonitorEvent::Created => {
                    file.path().as_deref() == Some(target.as_path())
                }
                _ => false,
            };
            if onto_target {
                counter.set(counter.get() + 1);
            }
        });
        Self {
            _monitor: monitor,
            count,
        }
    }

    fn settled(&self) -> u32 {
        flush_after_delay(Duration::from_millis(300));
        self.count.get()
    }
}

/// Present `window` and wait for its first painted frame. This is also the
/// harness precondition the gate depends on: presented widget tests must
/// deliver the frame clock's `after-paint`.
fn present_and_wait_for_first_frame(window: &LushtextWindow) {
    present_window(window);
    wait_until(Duration::from_secs(10), || {
        shell_geometry_evidence(window)
            .window_construction
            .first_frame_us
            .is_some()
    });
    assert!(
        shell_geometry_evidence(window)
            .window_construction
            .first_frame_us
            .is_some(),
        "precondition: the harness delivers the window frame clock's after-paint"
    );
}

/// What the backend holds now, read through an instance of its own.
fn backend() -> gio::Settings {
    gio::Settings::new(APP_ID)
}

#[test]
fn test_steady_state_construction_writes_nothing() {
    let keyfile = keyfile_settings_backend(Some(STEADY_PROFILE));
    let before = identity(&keyfile);
    let writes = WriteCounter::new(&keyfile);
    let window = test_window();
    flush_events();
    assert_eq!(identity(&keyfile), before, "construction wrote the backend");
    assert_eq!(
        settings_writes::evidence().pending,
        0,
        "a steady-state construction has nothing to persist"
    );
    present_and_wait_for_first_frame(&window);
    flush_after_delay(AFTER_PERSISTENCE_WINDOW);
    assert_eq!(writes.settled(), 0, "a steady-state launch writes nothing");
    assert_eq!(identity(&keyfile), before);
    let construction = shell_geometry_evidence(&window).window_construction;
    assert_eq!(construction.settings_writes_before_first_frame, 0);
    assert_eq!(construction.settings_flushes, 0);
}

#[test]
fn test_fresh_construction_writes_once_after_the_first_frame() {
    let keyfile = keyfile_settings_backend(None);
    let writes = WriteCounter::new(&keyfile);
    let window = test_window();
    flush_events();
    assert!(
        identity(&keyfile).is_none(),
        "construction wrote the backend"
    );
    // The migration's values that differ from the schema defaults (the
    // migrated flag and the properties fraction for this width) wait for the
    // first frame; the ones equal to a default are already what every reader
    // sees, so the change-only writer drops them.
    assert!(
        settings_writes::evidence().pending >= 1,
        "the migration keys wait for the first frame"
    );
    // Every reader already sees the migrated values.
    let shared = settings_writes::shared();
    assert!(shared.boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED));

    present_and_wait_for_first_frame(&window);
    assert_eq!(
        shell_geometry_evidence(&window)
            .window_construction
            .settings_writes_before_first_frame,
        0
    );
    wait_until(Duration::from_secs(10), || {
        settings_writes::evidence().flushes >= 1
    });
    assert_eq!(writes.settled(), 1, "one coalesced flush");
    assert_eq!(settings_writes::evidence().flushes, 1);

    // The values equal the pure restore oracle's for a fresh profile at the
    // default width: the workspace shown, the default preset, properties
    // hidden at a quarter of the window, migrated.
    let persisted = backend();
    assert!(persisted.boolean(keys::SPLIT_VIEW_LAYOUT_MIGRATED));
    assert!(persisted.boolean(keys::WORKSPACE_SIDEBAR_VISIBLE));
    assert!(!persisted.boolean(keys::PROPERTIES_SIDEBAR_VISIBLE));
    assert!((persisted.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION) - 0.3).abs() < 1e-9);
    assert!((persisted.double(keys::PROPERTIES_SIDEBAR_WIDTH_FRACTION) - 0.25).abs() < 1e-9);
}

#[test]
fn test_a_resize_burst_writes_at_most_once_per_persistence_window() {
    let keyfile = keyfile_settings_backend(Some(STEADY_PROFILE));
    let writes = WriteCounter::new(&keyfile);
    let window = test_window();
    present_and_wait_for_first_frame(&window);
    let flushes_before = settings_writes::evidence().flushes;
    let (width, height) = window.default_size();
    for step in 1..=20 {
        window.set_default_size(width - step, height);
        flush_after_delay(Duration::from_millis(10));
    }
    assert_eq!(
        settings_writes::evidence().flushes,
        flushes_before,
        "no write while the burst continues"
    );
    wait_until(Duration::from_secs(10), || {
        settings_writes::evidence().flushes > flushes_before
    });
    assert_eq!(writes.settled(), 1, "one write for the whole burst");
    assert_eq!(backend().int(keys::WINDOW_WIDTH), width - 20);
}

#[test]
fn test_the_last_window_close_persists_the_exact_state() {
    let _keyfile = keyfile_settings_backend(Some(STEADY_PROFILE));
    let window = test_window();
    present_and_wait_for_first_frame(&window);
    window.set_default_size(1000, 700);
    flush_events();
    assert_eq!(
        backend().int(keys::WINDOW_WIDTH),
        1200,
        "precondition: the new size is still pending"
    );
    wait_for_startup_data_flow(&window);
    window.close();
    wait_until(Duration::from_secs(10), || !window.is_visible());
    // The confirmed close of the last window flushed synchronously, well
    // inside the persistence window.
    assert_eq!(backend().int(keys::WINDOW_WIDTH), 1000);
    assert_eq!(backend().int(keys::WINDOW_HEIGHT), 700);
    assert_eq!(settings_writes::evidence().pending, 0);
}

#[test]
fn test_closing_a_non_last_window_does_not_flush() {
    let _keyfile = keyfile_settings_backend(Some(STEADY_PROFILE));
    let app = test_application();
    let first = LushtextWindow::new(&app);
    let second = LushtextWindow::new(&app);
    present_and_wait_for_first_frame(&first);
    present_and_wait_for_first_frame(&second);
    first.set_default_size(900, 650);
    flush_events();
    let flushes_before = settings_writes::evidence().flushes;
    wait_for_startup_data_flow(&first);
    first.close();
    wait_until(Duration::from_secs(10), || !first.is_visible());
    assert_eq!(
        settings_writes::evidence().flushes,
        flushes_before,
        "closing one of two windows performs no synchronous flush"
    );
    assert!(settings_writes::evidence().pending > 0);
    wait_until(Duration::from_secs(10), || {
        settings_writes::evidence().flushes > flushes_before
    });
    assert_eq!(backend().int(keys::WINDOW_WIDTH), 900);
    drop(second);
}

#[test]
fn test_effects_reach_other_readers_before_any_flush() {
    let keyfile = keyfile_settings_backend(Some(STEADY_PROFILE));
    let window = test_window();
    present_and_wait_for_first_frame(&window);
    let before = identity(&keyfile);

    // A Preferences preset change reaches the window at once.
    settings_writes::set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, 0.4);
    flush_events();
    let evidence = shell_geometry_evidence(&window);
    assert!((evidence.persisted_workspace_fraction - 0.4).abs() < 1e-9);
    assert!(evidence.workspace_fraction > 0.0);

    // An invisible-characters change reaches a new editor page at once.
    settings_writes::set_string(keys::INVISIBLE_CHARACTERS_MODE, "all");
    window.new_tab();
    flush_events();
    let editor = active_editor(&window);
    assert_eq!(editor.invisible_characters_mode().id(), "all");

    assert_eq!(identity(&keyfile), before, "nothing was flushed yet");
    assert_eq!(
        backend().string(keys::INVISIBLE_CHARACTERS_MODE).as_str(),
        "off",
        "another instance sees the value only at the flush (A39)"
    );
}

/// The shared settings instance outlives every window, so a handler connected
/// through `connect_changed_for` must be disconnected, and its captures
/// released, when its owner is finalized.
#[test]
fn test_a_changed_handler_lives_only_as_long_as_its_owner() {
    let _keyfile = keyfile_settings_backend(Some(STEADY_PROFILE));
    let calls = Rc::new(Cell::new(0u32));
    let owner = gtk4::Label::new(None);
    {
        let calls = Rc::clone(&calls);
        settings_writes::connect_changed_for(&owner, keys::ZOOM_LEVEL, move |_, _| {
            calls.set(calls.get() + 1);
        });
    }
    // GSettings emits `changed` only for a key read since the handler was
    // connected.
    let zoom = settings_writes::shared().uint(keys::ZOOM_LEVEL);
    settings_writes::set_uint(keys::ZOOM_LEVEL, zoom + 10);
    flush_events();
    assert_eq!(calls.get(), 1, "the handler runs while its owner lives");
    assert_eq!(
        Rc::strong_count(&calls),
        2,
        "the handler holds its captures"
    );

    drop(owner);
    flush_events();
    assert_eq!(
        Rc::strong_count(&calls),
        1,
        "finalizing the owner disconnected the handler and released its captures"
    );
    let _ = settings_writes::shared().uint(keys::ZOOM_LEVEL);
    settings_writes::set_uint(keys::ZOOM_LEVEL, zoom);
    flush_events();
    assert_eq!(calls.get(), 1, "a disconnected handler never runs again");
}
