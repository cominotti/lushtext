// SPDX-License-Identifier: MIT OR Apache-2.0

//! A38: a keyfile-backend GSettings write rewrites the settings file
//! synchronously, on the calling thread, exactly once per `set_*` call, and
//! does so again when the value written equals the stored one.
//!
//! Each write replaces the file (`g_file_replace_contents`: a temporary file
//! renamed over it, with an `fsync` first when the file already has content),
//! so a write's cost is the backend's, paid by whichever thread called
//! `set_*`. LushText's settings writer (`bound-window-construction-dispatch`)
//! keeps writes off window construction and interaction dispatches because of
//! it, and drops writes whose value equals the persisted one because GSettings
//! does not.

use std::time::Instant;

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::gsettings::{BOOLEAN_KEYS, INTEGER_KEY, KeyfileSettingsFixture};
use crate::probes::join_entries;
use crate::{AxiomId, Observation};

/// How many distinct-value writes the probe makes before the equal one.
pub const WRITES: i32 = 3;

/// Probe A38.
#[must_use]
pub fn probe_a38() -> Observation {
    Recorder::run(AxiomId::new(38), |recorder| {
        let fixture = recorder.require(
            KeyfileSettingsFixture::new("a38"),
            "control: the GTK 4 file-chooser schema is installed and a private keyfile exists",
        )?;
        let watch = recorder.require(
            fixture.watch_writes(),
            "control: the settings directory can be monitored",
        )?;
        let settings = fixture.settings();
        recorder.control(
            !settings.boolean(BOOLEAN_KEYS[0]),
            "control: the seeded value reads back through the keyfile backend",
        )?;
        recorder.control(
            watch.settled_count() == 0,
            "control: nothing replaced the settings file before the first write",
        )?;

        // Distinct values, then the last value again: GSettings does not
        // compare before it writes.
        let mut values: Vec<i32> = (0..WRITES).map(|step| 100 + step).collect();
        values.push(100 + WRITES - 1);
        let mut replacements_per_set = Vec::new();
        let mut on_disk_at_return = Vec::new();
        let mut durations = Vec::new();
        let mut seen = 0;
        for width in values {
            let started = Instant::now();
            let ok = settings.set_int(INTEGER_KEY, width).is_ok();
            durations.push(started.elapsed().as_micros());
            // Read before any main-context iteration: what the file holds
            // when `set_*` returns is what the calling thread wrote.
            on_disk_at_return.push(ok && fixture.file_has(INTEGER_KEY, &width.to_string()));
            let count = watch.settled_count();
            replacements_per_set.push(count - seen);
            seen = count;
        }

        recorder.measure("replacements_per_set", join_entries(&replacements_per_set));
        recorder.measure(
            "value_on_disk_when_set_returned",
            join_entries(&on_disk_at_return),
        );
        recorder.measure("set_durations_us", join_entries(&durations));
        recorder.axiom(
            on_disk_at_return.iter().all(|on_disk| *on_disk),
            "each written value is in the settings file when set_* returns",
        )?;
        recorder.axiom(
            replacements_per_set.iter().all(|count| *count == 1),
            "each set_* replaces the settings file exactly once, an equal value included",
        )
    })
}
