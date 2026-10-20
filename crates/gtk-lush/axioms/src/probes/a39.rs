// SPDX-License-Identifier: MIT OR Apache-2.0

//! A39: in `delay()` mode a `GSettings` keeps written values in memory
//! (reads on that instance return them, another instance on the same backend
//! still reads the stored values, and the file is untouched) until `apply()`,
//! which writes every pending key to the backend in one file replacement.
//!
//! LushText's settings writer (`bound-window-construction-dispatch`) flushes a
//! batch of pending keys with one `delay()`/`apply()` pair, so a flush costs
//! one backend write however many keys changed.

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::gsettings::{BOOLEAN_KEYS, INTEGER_KEY, KeyfileSettingsFixture};
use crate::{AxiomId, Observation};

/// The integer value the probe writes.
pub const WIDTH: i32 = 321;

/// Probe A39.
#[must_use]
pub fn probe_a39() -> Observation {
    Recorder::run(AxiomId::new(39), |recorder| {
        let fixture = recorder.require(
            KeyfileSettingsFixture::new("a39"),
            "control: the GTK 4 file-chooser schema is installed and a private keyfile exists",
        )?;
        let watch = recorder.require(
            fixture.watch_writes(),
            "control: the settings directory can be monitored",
        )?;
        let delayed = fixture.settings();
        let other = fixture.settings();
        delayed.delay();
        recorder.control(
            !delayed.has_unapplied() && watch.settled_count() == 0,
            "control: a fresh delayed instance has nothing pending and the file is untouched",
        )?;

        let mut writes_ok = true;
        for key in BOOLEAN_KEYS {
            writes_ok &= delayed.set_boolean(key, true).is_ok();
        }
        writes_ok &= delayed.set_int(INTEGER_KEY, WIDTH).is_ok();
        let pending = delayed.has_unapplied();
        let delayed_reads_pending = BOOLEAN_KEYS.iter().all(|key| delayed.boolean(key))
            && delayed.int(INTEGER_KEY) == WIDTH;
        let other_reads_stored = !other.boolean(BOOLEAN_KEYS[0]) && other.int(INTEGER_KEY) != WIDTH;
        let replacements_while_pending = watch.settled_count();
        recorder.measure("keys_pending", BOOLEAN_KEYS.len() + 1);
        recorder.measure("has_unapplied", pending);
        recorder.measure("replacements_while_pending", replacements_while_pending);
        recorder.measure("delayed_instance_reads_pending", delayed_reads_pending);
        recorder.measure("other_instance_reads_stored", other_reads_stored);
        recorder.control(writes_ok && pending, "control: the writes are pending")?;

        delayed.apply();
        let replacements_by_apply = watch.settled_count() - replacements_while_pending;
        let all_on_disk = BOOLEAN_KEYS.iter().all(|key| fixture.file_has(key, "true"))
            && fixture.file_has(INTEGER_KEY, &WIDTH.to_string());
        recorder.measure("replacements_by_apply", replacements_by_apply);
        recorder.measure("all_keys_on_disk_after_apply", all_on_disk);
        recorder.measure(
            "other_instance_reads_applied",
            other.boolean(BOOLEAN_KEYS[0]),
        );

        recorder.axiom(
            replacements_while_pending == 0,
            "the settings file is not rewritten while the writes are pending",
        )?;
        recorder.axiom(
            delayed_reads_pending && other_reads_stored,
            "the delayed instance reads its pending values; another instance reads the stored ones",
        )?;
        recorder.axiom(
            replacements_by_apply == 1 && all_on_disk && !delayed.has_unapplied(),
            "apply() writes every pending key in one file replacement",
        )
    })
}
