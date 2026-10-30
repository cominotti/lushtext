// SPDX-License-Identifier: MIT OR Apache-2.0

//! A56: the **first** write through a `GKeyfileSettingsBackend` whose
//! settings file does not exist yet (a fresh profile).
//!
//! The probe builds the fixture's keyfile backend over a missing file —
//! once in an existing directory, once with the directory missing too — and
//! holds, on that one backend, the writer, a peer instance of the same schema,
//! and an instance of another schema (`org.gnome.desktop.interface` when it
//! is installed, the schema GDK reads its own settings from, else a GTK 4
//! schema). It counts `changed` and `writable-changed` on each, inside the
//! writer's `set_*` and over the following second of main-context draining,
//! in which the backend's file and directory monitors deliver the file's
//! creation, and `notify` on the process's `GtkSettings`.
//!
//! What the first write does is recorded, and the steps check it: the peer
//! receives `changed` once, inside `set_*` (A41 on a fresh profile), and the
//! file's creation adds **no** `changed` on any instance of the backend, the
//! other schema's included, so the monitors do not re-deliver the backend's
//! own first write. `GtkSettings` is recorded for completeness; GDK reads it
//! through its own default backend or the settings portal, never through an
//! application's backend object, so a restyle after the first flush cannot
//! come from this backend.
//!
//! LushText's post-open settle (`bound-post-open-settle`, D7) looked for the
//! dispatch that follows the first flush of a fresh keyfile profile here.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio;
use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::gsettings::{
    INTEGER_KEY, KeyfileSettingsFixture, drain_main_context, pump_main_context_for,
};
use crate::{AxiomId, Observation};

/// The value the first write stores.
pub const FIRST_VALUE: i32 = 5_656;
/// How long the probe lets monitor events arrive after the write.
pub const MONITOR_WINDOW: Duration = Duration::from_millis(1_000);
/// The schemas tried, in order, for the instance of another schema.
pub const OTHER_SCHEMAS: [&str; 2] = [
    "org.gnome.desktop.interface",
    "org.gtk.gtk4.Settings.ColorChooser",
];

/// `changed` and `writable-changed` emissions on one instance, inside the
/// write and after it.
#[derive(Clone, Debug, Default)]
pub struct InstanceCounts {
    /// `changed` while the writer's `set_*` ran.
    pub changed_inside: Rc<Cell<u32>>,
    /// `changed` after the write returned.
    pub changed_after: Rc<Cell<u32>>,
    /// `writable-changed` at any time.
    pub writable_changed: Rc<Cell<u32>>,
}

/// Count `settings`' emissions, phased by `inside`.
#[must_use]
pub fn count_emissions(settings: &gio::Settings, inside: &Rc<Cell<bool>>) -> InstanceCounts {
    let counts = InstanceCounts::default();
    {
        let (inside, during, after) = (
            Rc::clone(inside),
            Rc::clone(&counts.changed_inside),
            Rc::clone(&counts.changed_after),
        );
        settings.connect_changed(None, move |_, _| {
            let counter = if inside.get() { &during } else { &after };
            counter.set(counter.get() + 1);
        });
    }
    {
        let writable = Rc::clone(&counts.writable_changed);
        settings.connect_writable_changed(None, move |_, _| writable.set(writable.get() + 1));
    }
    counts
}

/// What one first write observed.
#[derive(Clone, Debug)]
pub struct FirstWrite {
    /// Whether the file existed before the write.
    pub existed_before: bool,
    /// Whether the file holds the value after the write.
    pub written: bool,
    /// The peer instance of the writer's schema.
    pub peer: (u32, u32, u32),
    /// The instance of another schema, and its schema id.
    pub other: (u32, u32, u32, &'static str),
    /// `notify` on the process's `GtkSettings` over the whole window.
    pub gtk_settings_notifies: u32,
}

/// Make the first write through a backend over a missing file.
#[must_use]
pub fn first_write(create_dir: bool) -> Option<FirstWrite> {
    let fixture = KeyfileSettingsFixture::fresh("a56", create_dir)?;
    let existed_before = fixture.file_exists();
    let (other_id, other) = OTHER_SCHEMAS
        .iter()
        .find_map(|id| fixture.settings_for(id).map(|settings| (*id, settings)))?;
    let writer = fixture.settings();
    let peer = fixture.settings();
    let inside = Rc::new(Cell::new(false));
    let peer_counts = count_emissions(&peer, &inside);
    let other_counts = count_emissions(&other, &inside);
    let gtk_notifies = Rc::new(Cell::new(0u32));
    let gtk_settings = gtk4::Settings::default();
    let handler = gtk_settings.as_ref().map(|settings| {
        let counter = Rc::clone(&gtk_notifies);
        settings.connect_notify_local(None, move |_, _| counter.set(counter.get() + 1))
    });
    let _ = (
        peer.int(INTEGER_KEY),
        other.settings_schema().map(|schema| schema.list_keys()),
    );
    drain_main_context();
    inside.set(true);
    let _ = writer.set_int(INTEGER_KEY, FIRST_VALUE);
    inside.set(false);
    pump_main_context_for(MONITOR_WINDOW);
    if let (Some(settings), Some(handler)) = (gtk_settings.as_ref(), handler) {
        settings.disconnect(handler);
    }
    let read = |counts: &InstanceCounts| {
        (
            counts.changed_inside.get(),
            counts.changed_after.get(),
            counts.writable_changed.get(),
        )
    };
    let (other_inside, other_after, other_writable) = read(&other_counts);
    Some(FirstWrite {
        existed_before,
        written: fixture.file_has(INTEGER_KEY, &FIRST_VALUE.to_string()),
        peer: read(&peer_counts),
        other: (other_inside, other_after, other_writable, other_id),
        gtk_settings_notifies: gtk_notifies.get(),
    })
}

fn describe(run: &FirstWrite) -> String {
    format!(
        "file existed {} written {}; peer changed inside {} after {} writable {}; {} changed \
         inside {} after {} writable {}; GtkSettings notify {}",
        run.existed_before,
        run.written,
        run.peer.0,
        run.peer.1,
        run.peer.2,
        run.other.3,
        run.other.0,
        run.other.1,
        run.other.2,
        run.gtk_settings_notifies
    )
}

/// Probe A56.
#[must_use]
pub fn probe_a56() -> Observation {
    Recorder::run(AxiomId::new(56), |recorder| {
        let in_directory = recorder.require(
            first_write(true),
            "control: the GTK 4 schemas are installed",
        )?;
        let without_directory = recorder.require(
            first_write(false),
            "control: the GTK 4 schemas are installed",
        )?;
        recorder.measure("existing_directory", describe(&in_directory));
        recorder.measure("missing_directory", describe(&without_directory));
        recorder.control(
            !in_directory.existed_before && !without_directory.existed_before,
            "control: neither settings file existed before its first write",
        )?;
        recorder.control(
            in_directory.written && without_directory.written,
            "control: the first write created the settings file, directory included",
        )?;
        for run in [&in_directory, &without_directory] {
            recorder.axiom(
                run.peer.0 == 1,
                "the first write emits changed on the peer once, inside set_*",
            )?;
            recorder.axiom(
                run.peer.1 == 0 && run.other.1 == 0 && run.other.0 == 0,
                "the file's creation adds no changed on any instance of the backend",
            )?;
        }
        Ok(())
    })
}
