// SPDX-License-Identifier: MIT OR Apache-2.0

//! A41: when a `GSettings` instance writes a keyfile-backed key on the thread
//! that owns the main context, `changed` reaches a second instance on the same
//! backend synchronously, inside the writer's `set_*` and before the settings
//! file is rewritten, and the backend's file monitor adds no `changed` for that
//! write (it recognizes its own contents by digest). A `delay()`ed instance
//! emits `changed` on itself inside `set_*`; other instances receive it only
//! inside `apply()`, and `apply()` emits no second `changed` on the delayed
//! instance itself.
//!
//! LushText's settings writer (`bound-window-construction-dispatch`) keeps one
//! delayed instance that every LushText reader and subscriber shares, so a
//! write's effects reach every subscriber at once, before any flush, and the
//! flush (`apply()`) does not re-notify them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::gsettings::{INTEGER_KEY, KeyfileSettingsFixture, drain_main_context};
use crate::{AxiomId, Observation};

/// How long the probe lets late `changed` emissions arrive.
pub const LATE_EMISSION_WINDOW: Duration = Duration::from_millis(400);

/// One `changed` emission, and where it happened.
#[derive(Clone, Copy, Debug)]
struct Emission {
    /// Which instance emitted: 0 the writer's peer, 1 the delayed instance.
    instance: u8,
    /// The phase the probe was in when it arrived.
    phase: Phase,
    /// Whether the settings file already held the expected value.
    value_on_disk: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    InsideSet,
    InsideApply,
}

fn settle() {
    let deadline = Instant::now() + LATE_EMISSION_WINDOW;
    while Instant::now() < deadline {
        drain_main_context();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Probe A41.
#[must_use]
pub fn probe_a41() -> Observation {
    Recorder::run(AxiomId::new(41), |recorder| {
        let fixture = Rc::new(recorder.require(
            KeyfileSettingsFixture::new("a41", 0),
            "control: the GTK 4 file-chooser schema is installed and a private keyfile exists",
        )?);
        let writer = fixture.settings();
        let peer = fixture.settings();
        let delayed = fixture.settings();
        delayed.delay();
        let phase = Rc::new(Cell::new(Phase::Idle));
        let expected = Rc::new(Cell::new(0i32));
        let emissions = Rc::new(RefCell::new(Vec::<Emission>::new()));
        for (instance, settings) in [(0u8, &peer), (1u8, &delayed)] {
            let (phase, expected, emissions, fixture) = (
                Rc::clone(&phase),
                Rc::clone(&expected),
                Rc::clone(&emissions),
                Rc::clone(&fixture),
            );
            settings.connect_changed(Some(INTEGER_KEY), move |_, _| {
                emissions.borrow_mut().push(Emission {
                    instance,
                    phase: phase.get(),
                    value_on_disk: fixture.file_has(INTEGER_KEY, &expected.get().to_string()),
                });
            });
        }
        let _ = (peer.int(INTEGER_KEY), delayed.int(INTEGER_KEY));
        settle();
        emissions.borrow_mut().clear();

        // 1. A plain write on this thread.
        expected.set(411);
        phase.set(Phase::InsideSet);
        let _ = writer.set_int(INTEGER_KEY, 411);
        phase.set(Phase::Idle);
        settle();
        let plain: Vec<Emission> = emissions.borrow_mut().drain(..).collect();
        let peer_plain: Vec<&Emission> = plain.iter().filter(|e| e.instance == 0).collect();

        // 2. A write on the delayed instance, then apply().
        expected.set(412);
        phase.set(Phase::InsideSet);
        let _ = delayed.set_int(INTEGER_KEY, 412);
        phase.set(Phase::Idle);
        settle();
        let pending: Vec<Emission> = emissions.borrow_mut().drain(..).collect();
        phase.set(Phase::InsideApply);
        delayed.apply();
        phase.set(Phase::Idle);
        settle();
        let applied: Vec<Emission> = emissions.borrow_mut().drain(..).collect();

        let describe = |list: &[Emission]| {
            list.iter()
                .map(|e| format!("{}:{:?}:{}", e.instance, e.phase, e.value_on_disk))
                .collect::<Vec<_>>()
                .join(" | ")
        };
        recorder.measure("plain_write_emissions", describe(&plain));
        recorder.measure("delayed_set_emissions", describe(&pending));
        recorder.measure("apply_emissions", describe(&applied));
        recorder.control(
            peer.int(INTEGER_KEY) == 412 && fixture.file_has(INTEGER_KEY, "412"),
            "control: both writes reached the backend",
        )?;
        recorder.axiom(
            peer_plain.len() == 1
                && peer_plain[0].phase == Phase::InsideSet
                && !peer_plain[0].value_on_disk,
            "a write emits changed on another instance once, inside set_*, before the file is rewritten",
        )?;
        recorder.axiom(
            pending.len() == 1 && pending[0].instance == 1 && pending[0].phase == Phase::InsideSet,
            "a delayed instance emits changed on itself inside set_*, and on no other instance",
        )?;
        recorder.axiom(
            applied.iter().all(|e| e.instance == 0 && e.phase == Phase::InsideApply)
                && applied.len() == 1,
            "apply() emits changed on the other instances inside apply(), and none on the delayed one",
        )
    })
}
