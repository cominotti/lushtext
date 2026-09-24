// SPDX-License-Identifier: GPL-3.0-or-later

//! E1 part A: Quint Connect replay of `../quint/journal.qnt` against the E8
//! port (`src/env.rs`), whose every decision is the real `journal_core`.
//!
//! Each Quint step's nondet picks are replayed through `JournalModel::start`
//! (the `init` picks) or `Journal::step_as` (the `step` picks), and after
//! every step the whole Quint `j` record, plus the loop counter `n`, is
//! compared with the Rust journal converted to the same shape.
//!
//! State mapping decisions:
//! - `other`: Quint always carries a window and a `hasOther` flag; the Rust
//!   `Option<Window>` maps `None` to Quint's `ClosedWindow` with
//!   `hasOther = false`, so both the flag and the parked window are compared.
//! - content ids: both sides use the same numbering (0 = empty buffer,
//!   `1 + id` = the previous session's body, then one per edit); Quint's
//!   `NoContent = -1` is Rust's `None` (`body`, `written`, `cleanupCandidate`).
//! - ancestors: the Rust bitset `ancestors[c]` becomes the set of its bits;
//!   Quint's map domain `0..IdCount + MaxEdits` equals `CONTENTS`.
//! - `preserved`/`accepted`/`resolved`: bitsets become sets.
//!
//! Run from this directory with Quint on PATH (see README.md).

#![expect(
    non_snake_case,
    reason = "`switch!` binds nondet picks by their Quint names (entryPresent, actorB)"
)]

use std::collections::{BTreeMap, BTreeSet};

use lushtext_core::services::draft_service::journal_core::{DeletionStep, RestoreEnding};
use lushtext_journal_stateright::env::{
    Action, CONTENTS, ContentSet, Editor, IDS, Journal, PreviousId, Window,
};
use lushtext_journal_stateright::model::JournalModel;
use quint_connect::{Config, Driver, Result, State, Step, quint_run, quint_test, switch};
use serde::Deserialize;

mod common;
use common::{QAction, QDeletion, QDeletionStep, QDiskEntry, QEditor, QEnding, QJournal, QWindow};

/// What is compared after every step: the journal and the loop counter.
#[derive(Debug, PartialEq, Eq)]
struct Observed {
    j: QJournal,
    n: i64,
}

/// `State` requires `DeserializeOwned`, but `from_spec` is overridden below
/// (the variable names are module-qualified), so this is never called.
impl<'de> Deserialize<'de> for Observed {
    fn deserialize<D: serde::Deserializer<'de>>(_: D) -> core::result::Result<Self, D::Error> {
        Err(serde::de::Error::custom("unused: Observed::from_spec is overridden"))
    }
}

// --- Rust -> Quint shape --------------------------------------------------------

const NO_CONTENT: i64 = -1;

fn content(c: Option<u8>) -> i64 {
    c.map_or(NO_CONTENT, i64::from)
}

fn set(bits: ContentSet) -> BTreeSet<i64> {
    (0..16).filter(|b| bits & (1 << b) != 0).collect()
}

fn deletion(d: Option<DeletionStep>) -> QDeletion {
    match d {
        None => QDeletion::NoDeletion,
        Some(step) => QDeletion::Deleting(match step {
            DeletionStep::Preserve => QDeletionStep::Preserve,
            DeletionStep::DeleteBody => QDeletionStep::DeleteBody,
            DeletionStep::RemoveEntry => QDeletionStep::RemoveEntry,
            DeletionStep::Done => QDeletionStep::DeletionDone,
            DeletionStep::Stopped => QDeletionStep::Stopped,
        }),
    }
}

fn editor(e: &Editor) -> QEditor {
    QEditor {
        open: e.open,
        content: i64::from(e.content),
        dirty: e.dirty,
        token: e.token,
        written: content(e.written),
        restore_pending: e.restore_pending,
        deletion: deletion(e.deletion),
        preserve_queued: e.preserve_queued,
        known_entry: e.known_entry,
        cleanup_candidate: content(e.cleanup_candidate),
    }
}

fn per_id<T>(f: impl Fn(usize) -> T) -> BTreeMap<i64, T> {
    (0..IDS).map(|id| (id as i64, f(id))).collect()
}

fn window(w: &Window) -> QWindow {
    QWindow {
        editors: per_id(|id| editor(&w.editors[id])),
        running: w.running,
        trusted: w.trusted,
        last_reconciliation_complete: w.last_reconciliation_complete,
    }
}

fn journal(j: &Journal) -> QJournal {
    let closed = Window {
        editors: [lushtext_journal_stateright::env::CLOSED; IDS],
        running: false,
        trusted: false,
        last_reconciliation_complete: false,
    };
    QJournal {
        has_other: j.other.is_some(),
        other: window(j.other.as_ref().unwrap_or(&closed)),
        entry: per_id(|id| QDiskEntry {
            present: j.entry[id].present,
            backing: i64::from(j.entry[id].backing),
        }),
        body: per_id(|id| content(j.body[id])),
        backing: per_id(|id| i64::from(j.backing[id])),
        preserved: set(j.preserved),
        editors: per_id(|id| editor(&j.editors[id])),
        running: j.running,
        trusted: j.trusted,
        last_reconciliation_complete: j.last_reconciliation_complete,
        accepted: set(j.accepted),
        resolved: set(j.resolved),
        ancestors: (0..CONTENTS)
            .map(|c| (c as i64, set(j.ancestors[c])))
            .collect(),
        next_content: i64::from(j.next_content),
        edits: j.edits as i64,
        body_without_entry: j.body_without_entry,
        cleanup_unsafe: j.cleanup_unsafe,
    }
}

// --- The driver -----------------------------------------------------------------

/// Where the choices live: `quint run --mbt`'s builtin variables, or the
/// `choice` variable of `k8_traces_connect` (`quint test` writes no mbt
/// variables).
trait Layout {
    const TWO_PROCESSES: bool;
    const NONDET: &'static [&'static str];
}

struct OneProcessRun;
impl Layout for OneProcessRun {
    const TWO_PROCESSES: bool = false;
    const NONDET: &'static [&'static str] = &[];
}

struct TwoProcessRun;
impl Layout for TwoProcessRun {
    const TWO_PROCESSES: bool = true;
    const NONDET: &'static [&'static str] = &[];
}

struct TwoProcessTest;
impl Layout for TwoProcessTest {
    const TWO_PROCESSES: bool = true;
    const NONDET: &'static [&'static str] = &["choice"];
}

struct CoreDriver<L: Layout> {
    journal: Option<Journal>,
    n: i64,
    traces: usize,
    steps: usize,
    /// Steps that changed the journal (the rest are no-ops).
    effective: usize,
    started: std::time::Instant,
    _layout: core::marker::PhantomData<L>,
}

impl<L: Layout> Default for CoreDriver<L> {
    fn default() -> Self {
        Self {
            journal: None,
            n: 0,
            traces: 0,
            steps: 0,
            effective: 0,
            started: std::time::Instant::now(),
            _layout: core::marker::PhantomData,
        }
    }
}

fn ending(e: QEnding) -> RestoreEnding {
    match e {
        QEnding::Applied => RestoreEnding::Applied,
        QEnding::Stale => RestoreEnding::Stale,
        QEnding::Oversized => RestoreEnding::Oversized,
        QEnding::ReadFailed => RestoreEnding::ReadFailed,
        QEnding::EditedOver => RestoreEnding::EditedOver,
        QEnding::InstallCancelled => RestoreEnding::InstallCancelled,
        QEnding::Unavailable => RestoreEnding::Unavailable,
        QEnding::MissingBody => RestoreEnding::MissingBody,
    }
}

fn action(a: QAction) -> (Action, Option<RestoreEnding>) {
    let plain = match a {
        QAction::Edit => Action::Edit,
        QAction::Register => Action::Register,
        QAction::WriteBody => Action::WriteBody,
        QAction::Commit => Action::Commit,
        QAction::Save => Action::Save,
        QAction::Discard => Action::Discard,
        QAction::DeletionStepAction => Action::DeletionStep,
        QAction::Inspect => Action::Inspect,
        QAction::ExecCleanup => Action::ExecCleanup,
        QAction::RestoreApply(e) => return (Action::RestoreApply, Some(ending(e))),
        QAction::ExternalMtime => Action::ExternalMtime,
        QAction::Crash => Action::Crash,
        QAction::Startup => Action::Startup,
    };
    (plain, None)
}

impl<L: Layout> CoreDriver<L> {
    fn init(
        &mut self,
        entry_present: BTreeMap<i64, bool>,
        body_present: BTreeMap<i64, bool>,
        backing_moved: BTreeMap<i64, bool>,
        startup_fault: bool,
    ) {
        let previous: [PreviousId; IDS] = core::array::from_fn(|id| PreviousId {
            entry: entry_present[&(id as i64)],
            body: body_present[&(id as i64)],
            backing_moved: backing_moved[&(id as i64)],
        });
        let model = JournalModel {
            max_steps: 0,
            two_processes: L::TWO_PROCESSES,
            count_steps: true,
        };
        self.journal = Some(model.start(previous, startup_fault));
        self.n = 0;
        self.traces += 1;
        self.steps += 1;
    }

    fn step(&mut self, actor_b: bool, a: QAction, id: i64, fault: bool) {
        let (act, end) = action(a);
        let journal = self.journal.as_mut().expect("init runs first");
        let before = journal.clone();
        journal.step_as(actor_b, act, id as usize, fault, end);
        if *journal != before {
            self.effective += 1;
        }
        self.n += 1;
        self.steps += 1;
    }
}

/// Prints the replay volume once the runner drops the driver.
impl<L: Layout> Drop for CoreDriver<L> {
    fn drop(&mut self) {
        eprintln!(
            "E1-REPORT two_processes={}: {} traces, {} steps (init included; {} state-changing loop steps) replayed in {:?} (trace generation included)",
            L::TWO_PROCESSES,
            self.traces,
            self.steps,
            self.effective,
            self.started.elapsed()
        );
    }
}

impl<L: Layout> Driver for CoreDriver<L> {
    type State = Observed;

    fn config() -> Config {
        Config {
            state: &[],
            nondet: L::NONDET,
        }
    }

    fn step(&mut self, step: &Step) -> Result {
        switch!(step {
            // `quint run --mbt`: the action names are `init` and `step`.
            init(entryPresent, bodyPresent, backingMoved, startupFault) =>
                self.init(entryPresent, bodyPresent, backingMoved, startupFault),
            step(actorB, a, id, fault) => CoreDriver::step(self, actorB, a, id, fault),
            // `k8_traces_connect`: the `choice` variants.
            Init(entryPresent, bodyPresent, backingMoved, startupFault) =>
                self.init(entryPresent, bodyPresent, backingMoved, startupFault),
            Step(actorB, a, id, fault) => CoreDriver::step(self, actorB, a, id, fault),
        })
    }
}

impl<L: Layout> State<CoreDriver<L>> for Observed {
    fn from_driver(driver: &CoreDriver<L>) -> Result<Self> {
        Ok(Self {
            j: journal(driver.journal.as_ref().expect("init runs first")),
            n: driver.n,
        })
    }

    /// The ITF variables are module-qualified (`k3_8::journal::j`), so pick
    /// them by suffix instead of a fixed `Config::state` path.
    fn from_spec(value: itf::Value) -> Result<Self> {
        let itf::Value::Record(record) = value else {
            anyhow::bail!("expected the state to be a record");
        };
        let find = |suffix: &str| {
            record
                .iter()
                .find(|(k, _)| k.ends_with(suffix))
                .map(|(_, v)| v.clone())
                .ok_or_else(|| anyhow::anyhow!("no `*{suffix}` variable in the trace"))
        };
        Ok(Self {
            j: QJournal::deserialize(find("journal::j")?)?,
            n: i64::deserialize(find("journal::n")?)?,
        })
    }
}

// --- The tests ------------------------------------------------------------------

#[quint_run(spec = "../quint/journal.qnt", main = "k3_8", max_samples = 2000, max_steps = 8)]
fn k3_8_simulation() -> impl Driver {
    CoreDriver::<OneProcessRun>::default()
}

#[quint_run(spec = "../quint/journal.qnt", main = "k8_6", max_samples = 2000, max_steps = 6)]
fn k8_6_simulation() -> impl Driver {
    CoreDriver::<TwoProcessRun>::default()
}

#[quint_test(spec = "../quint/journal.qnt", main = "k8_traces_connect", test = "s1LossTest", max_samples = 1)]
fn k8_s1_loss() -> impl Driver {
    CoreDriver::<TwoProcessTest>::default()
}

#[quint_test(
    spec = "../quint/journal.qnt",
    main = "k8_traces_connect",
    test = "bodyWithoutEntry8Test",
    max_samples = 1
)]
fn k8_body_without_entry_8() -> impl Driver {
    CoreDriver::<TwoProcessTest>::default()
}

#[quint_test(
    spec = "../quint/journal.qnt",
    main = "k8_traces_connect",
    test = "bodyWithoutEntry6Test",
    max_samples = 1
)]
fn k8_body_without_entry_6() -> impl Driver {
    CoreDriver::<TwoProcessTest>::default()
}
