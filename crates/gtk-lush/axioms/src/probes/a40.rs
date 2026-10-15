// SPDX-License-Identifier: MIT OR Apache-2.0

//! A40: `GKeyfileSettingsBackend` holds no lock around its in-memory key file
//! (GLib 2.88 `gio/gkeyfilesettingsbackend.c`: neither `read` nor `write`
//! takes a mutex, and `write` rewrites the file in the caller's frame), so one
//! backend object must not be written from one thread while another thread
//! uses it. A second backend object over the same file does not share that
//! state: a write through it on a worker thread leaves the first backend
//! answering with its previous value until the first backend's file monitor
//! delivers the replacement on the main context, which reloads the file and
//! emits `changed`.
//!
//! LushText's settings writer (`bound-window-construction-dispatch`) chose its
//! flush path from this: a worker flush through the shared default backend is
//! a data race, and a worker flush through a private backend over the same
//! file leaves every other reader stale for a file-monitor round trip (and lets
//! a write made meanwhile through the default backend overwrite the file with
//! stale values). The flush therefore runs on the GTK thread, after the first
//! frame, as one `delay()`/`apply()` batch (A39).
//!
//! The lock-free shared-object half is source-cited, not probed: observing it
//! needs one `GObject` on two threads, which safe gtk-rs refuses (neither
//! `gio::Settings` nor `gio::SettingsBackend` is `Send`) and GTK Lush forbids
//! `unsafe`. The probe pins the second half, which is what makes a
//! private-backend worker flush unsound.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::gio;
use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::gsettings::{
    INTEGER_KEY, KeyfileSettingsFixture, SCHEMA_ID, drain_main_context,
};
use crate::{AxiomId, Observation};

/// The value the worker thread writes.
pub const WORKER_VALUE: i32 = 4_040;
/// How long the probe waits for the first backend to notice the replacement.
pub const RELOAD_DEADLINE: Duration = Duration::from_secs(5);

/// Write `value` through a backend object of the worker's own over `path`.
/// Returns how long the write took, or `None` when the schema is missing.
#[must_use]
pub fn write_on_a_worker(path: std::path::PathBuf, value: i32) -> Option<Duration> {
    std::thread::spawn(move || {
        let schema = gio::SettingsSchemaSource::default()?.lookup(SCHEMA_ID, true)?;
        let backend = gio::keyfile_settings_backend_new(path.to_str()?, "/", None);
        let settings = gio::Settings::new_full(&schema, Some(&backend), None);
        let started = Instant::now();
        settings.set_int(INTEGER_KEY, value).ok()?;
        Some(started.elapsed())
    })
    .join()
    .ok()
    .flatten()
}

/// Probe A40.
#[must_use]
pub fn probe_a40() -> Observation {
    Recorder::run(AxiomId::new(40), |recorder| {
        let fixture = recorder.require(
            KeyfileSettingsFixture::new("a40", 0),
            "control: the GTK 4 file-chooser schema is installed and a private keyfile exists",
        )?;
        let reader = fixture.settings();
        let before = reader.int(INTEGER_KEY);
        let changes = Rc::new(Cell::new(0u32));
        let counter = Rc::clone(&changes);
        reader.connect_changed(Some(INTEGER_KEY), move |_, _| {
            counter.set(counter.get() + 1);
        });
        drain_main_context();
        recorder.control(before != WORKER_VALUE, "control: the stored value differs")?;

        let write = recorder.require(
            write_on_a_worker(fixture.path().to_path_buf(), WORKER_VALUE),
            "control: the worker thread wrote through its own backend",
        )?;
        let on_disk = fixture.file_has(INTEGER_KEY, &WORKER_VALUE.to_string());
        // No main-context iteration yet: what the first backend answers now
        // is what every reader sharing it sees until its monitor fires.
        let read_right_after = reader.int(INTEGER_KEY);
        let changes_right_after = changes.get();

        let waited = Instant::now();
        while changes.get() == 0 && waited.elapsed() < RELOAD_DEADLINE {
            drain_main_context();
            std::thread::sleep(Duration::from_millis(5));
        }
        let reload_after = waited.elapsed();
        let read_after_reload = reader.int(INTEGER_KEY);

        recorder.measure("worker_write_us", write.as_micros());
        recorder.measure("value_on_disk_after_worker_write", on_disk);
        recorder.measure("first_backend_read_right_after", read_right_after);
        recorder.measure("changed_right_after", changes_right_after);
        recorder.measure("changed_after_reload", changes.get());
        recorder.measure("reload_latency_us", reload_after.as_micros());
        recorder.measure("first_backend_read_after_reload", read_after_reload);
        recorder.control(on_disk, "control: the worker's write reached the file")?;
        recorder.axiom(
            read_right_after == before && changes_right_after == 0,
            "the first backend still answers with its previous value right after the write",
        )?;
        recorder.axiom(
            changes.get() >= 1 && read_after_reload == WORKER_VALUE,
            "its file monitor, on the main context, reloads the file and emits changed",
        )
    })
}
