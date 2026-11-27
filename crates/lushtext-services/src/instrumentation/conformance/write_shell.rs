// SPDX-License-Identifier: GPL-3.0-or-later

//! Write-shell conformance (`verify-shell-conformance-against-proven-cores`
//! D4): run one real durable-write primitive over a caller-owned directory,
//! with the conformance tap recording every action and injecting the case's
//! faults and crash, then replay the recorded outcomes through the shared disk
//! model (`write_protocol::disk_model`) and the production cores, and compare.
//!
//! The oracle is the proven code: the protocols' own `step` and the disk
//! model the Kani harnesses run. After every action the driver checks that
//!
//! - the outcome fed back equals the result of the action's backend calls
//!   (a temp removal is fed back `Done` whatever its call returned, which the
//!   core provably ignores; a probe that finds an absent destination it does
//!   not require succeeds; content writing reports its stream's result);
//! - the action made exactly the pinned number of backend calls
//!   ([`expected_calls`]);
//! - the destination holds the model's visible content, the temp file exists
//!   exactly while the model's temp is linked, and the temp's mode is no wider
//!   than the destination's;
//! - a move's source and a rename's name are where `MoveDisk` and
//!   `RenameNamespace` put them.
//!
//! At the end the returned result must be the core's class. After a crash the
//! destination must hold the model's live crash view, and every leftover must
//! parse as LushText's own temp file that a later launch's sweep removes.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use crate::filesystem::backend_tap::{
    self as tap, ActionAt, Address, Call, CrashAt, Injected, ProtocolKind, StepRecord, TapPlan,
    TapTrace,
};
use crate::filesystem::leftovers::{
    LeftoverClock, LeftoverFacts, LeftoverScope, STALE_LEFTOVER_AGE, is_sweepable_leftover,
};
use crate::filesystem::write::DurableWriteError;
use crate::filesystem::write_protocol::disk_model::{
    Content, Disk, MoveDisk, RenameNamespace, Shell,
};
use crate::filesystem::write_protocol::{
    MoveAction, MoveProtocol, RenameAction, RenameClass, RenameProtocol, StepOutcome, WriteAction,
    WriteClass, WriteProtocol,
};
use crate::filesystem::{WriteLabel, fixture, temp_name, write as fs_write};

/// The durable-write primitive a case runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WritePrimitive {
    /// `write::atomic_replace`.
    Replace,
    /// `write::atomic_replace_stream`.
    ReplaceStream,
    /// `write::copy_durable`.
    Copy,
    /// `write::move_durable`.
    Move,
    /// `write::rename_durable`.
    Rename { cross_directory: bool },
    /// `write::rename_durable_no_replace`.
    RenameNoReplace { cross_directory: bool },
}

impl WritePrimitive {
    /// Every primitive, both directory layouts of the renames.
    pub const ALL: [Self; 8] = [
        Self::Replace,
        Self::ReplaceStream,
        Self::Copy,
        Self::Move,
        Self::Rename {
            cross_directory: false,
        },
        Self::Rename {
            cross_directory: true,
        },
        Self::RenameNoReplace {
            cross_directory: false,
        },
        Self::RenameNoReplace {
            cross_directory: true,
        },
    ];

    const fn source_dir(self) -> &'static str {
        match self {
            Self::Rename {
                cross_directory: true,
            }
            | Self::RenameNoReplace {
                cross_directory: true,
            } => "b",
            _ => "a",
        }
    }

    const fn has_source(self) -> bool {
        !matches!(self, Self::Replace | Self::ReplaceStream)
    }
}

/// Which core a fault or crash addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteCore {
    /// The atomic write (also a move's copy).
    Write,
    /// The durable move.
    Move,
    /// The durable rename.
    Rename,
}

impl WriteCore {
    const fn kind(self) -> ProtocolKind {
        match self {
            Self::Write => ProtocolKind::Write,
            Self::Move => ProtocolKind::Move,
            Self::Rename => ProtocolKind::Rename,
        }
    }

    fn of(kind: ProtocolKind) -> Option<Self> {
        match kind {
            ProtocolKind::Write => Some(Self::Write),
            ProtocolKind::Move => Some(Self::Move),
            ProtocolKind::Rename => Some(Self::Rename),
            _ => None,
        }
    }
}

/// The error a fault makes its call return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteFaultKind {
    /// Any failure.
    Fail,
    /// A taken name (meaningful at a temp creation).
    AlreadyExists,
    /// `EXDEV` (meaningful at a rename).
    CrossDevice,
}

/// One injected fault: call `call` of action `ordinal` of the `run`-th run of
/// `core`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteFault {
    pub core: WriteCore,
    pub run: usize,
    pub ordinal: usize,
    pub call: usize,
    pub kind: WriteFaultKind,
}

/// A simulated crash before (or after) action `ordinal` of the `run`-th run
/// of `core`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteCrash {
    pub core: WriteCore,
    pub run: usize,
    pub ordinal: usize,
    pub after: bool,
}

/// One conformance case.
#[derive(Clone, Debug)]
pub struct WriteCase {
    pub primitive: WritePrimitive,
    /// The destination exists before the call, holding `old`.
    pub destination_exists: bool,
    /// The permission bits of the existing destination and of the source.
    pub mode: u32,
    pub old: Vec<u8>,
    pub new: Vec<u8>,
    /// A streaming write's producer fails halfway.
    pub failing_producer: bool,
    pub faults: Vec<WriteFault>,
    pub crash: Option<WriteCrash>,
}

impl WriteCase {
    /// A fault-free case with small payloads.
    #[must_use]
    pub fn plain(primitive: WritePrimitive, destination_exists: bool) -> Self {
        Self {
            primitive,
            destination_exists,
            mode: 0o640,
            old: b"old bytes\n".to_vec(),
            new: b"new bytes, longer\n".to_vec(),
            failing_producer: false,
            faults: Vec::new(),
            crash: None,
        }
    }
}

/// One executed action, as the enumeration addresses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionShape {
    pub core: WriteCore,
    pub run: usize,
    pub ordinal: usize,
    pub name: String,
    /// Tap points the action made: backend calls plus the content point.
    pub points: usize,
}

/// What one case did.
#[derive(Clone, Debug, Default)]
pub struct WriteCaseReport {
    pub actions: Vec<ActionShape>,
    /// Faults that fired.
    pub injected: usize,
    pub crashed: bool,
}

/// The per-action backend call table: `(core, action, a metadata source is
/// present) -> number of fallible backend calls`. A shell change that adds or
/// removes a call fails [`check_write_case`] and the table's own test.
#[must_use]
pub fn expected_calls(core: WriteCore, action: &str, metadata_source: bool) -> Option<usize> {
    Some(match (core, action) {
        (
            WriteCore::Write,
            "ProbeMetadata" | "CreateTemp" | "SyncTemp" | "Rename" | "SyncDir" | "RemoveTemp",
        )
        | (WriteCore::Move, "Copy" | "RemoveSource" | "SyncSourceDir")
        | (WriteCore::Rename, "Rename" | "SyncSourceDir" | "SyncDestinationDir") => 1,
        (WriteCore::Write, "WriteContent") => 0,
        (WriteCore::Write, "ApplyMetadata") => usize::from(metadata_source),
        _ => return None,
    })
}

/// The disk as one action left it.
#[derive(Clone, Debug, Default)]
struct Snapshot {
    destination: Option<(Vec<u8>, u32)>,
    source: Option<Vec<u8>>,
    temps: Vec<(String, u32)>,
    /// Entries of either directory that are neither the destination, the
    /// source, nor a durable temp of the destination.
    strays: Vec<String>,
}

struct Layout {
    destination: PathBuf,
    source: PathBuf,
}

impl Layout {
    fn new(root: &Path, primitive: WritePrimitive) -> Self {
        Self {
            destination: root.join("a").join("dest.bin"),
            source: root.join(primitive.source_dir()).join("source.bin"),
        }
    }

    fn destination_name(&self) -> String {
        self.destination
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default()
            .to_string()
    }

    fn snapshot(&self) -> Snapshot {
        let read = |path: &Path| fixture::exists(path).then(|| fixture::read_bytes(path));
        let name = self.destination_name();
        let directory = self
            .destination
            .parent()
            .expect("the layout has directories");
        let temps = fixture::entry_names(directory)
            .into_iter()
            .filter(|entry| {
                temp_name::parse(entry).is_some_and(|parsed| parsed.target_file == name)
            })
            .map(|entry| {
                let mode = fixture::mode(&directory.join(&entry)) & 0o7777;
                (entry, mode)
            })
            .collect();
        let known = |entry: &str| {
            entry == name
                || Some(std::ffi::OsStr::new(entry)) == self.source.file_name()
                || temp_name::parse(entry).is_some_and(|parsed| parsed.target_file == name)
        };
        let mut strays = Vec::new();
        if let Some(root) = directory.parent() {
            for subdirectory in ["a", "b"] {
                strays.extend(
                    fixture::entry_names(&root.join(subdirectory))
                        .into_iter()
                        .filter(|entry| !known(entry))
                        .map(|entry| format!("{subdirectory}/{entry}")),
                );
            }
        }
        Snapshot {
            destination: read(&self.destination)
                .map(|bytes| (bytes, fixture::mode(&self.destination) & 0o7777)),
            source: read(&self.source),
            temps,
            strays,
        }
    }
}

fn injected(kind: WriteFaultKind) -> Injected {
    match kind {
        WriteFaultKind::Fail => Injected::FAIL,
        WriteFaultKind::AlreadyExists => Injected::ALREADY_EXISTS,
        WriteFaultKind::CrossDevice => Injected::CrossDevice,
    }
}

/// The result of the primitive, reduced to the classes the cores decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ended {
    Ok,
    BeforeRename,
    AfterRename,
    Failed,
}

fn classify(result: &Result<(), DurableWriteError>) -> Ended {
    match result {
        Ok(()) => Ended::Ok,
        Err(DurableWriteError::BeforeRename(_)) => Ended::BeforeRename,
        Err(DurableWriteError::AfterRename(_)) => Ended::AfterRename,
    }
}

fn run_primitive(case: &WriteCase, layout: &Layout) -> Ended {
    let label = WriteLabel::SAVE;
    let flat = |result: std::io::Result<()>| {
        if result.is_ok() {
            Ended::Ok
        } else {
            Ended::Failed
        }
    };
    match case.primitive {
        WritePrimitive::Replace => classify(&fs_write::atomic_replace(
            &layout.destination,
            label,
            &case.new,
        )),
        WritePrimitive::ReplaceStream => {
            let new = case.new.clone();
            let failing = case.failing_producer;
            classify(&fs_write::atomic_replace_stream(
                &layout.destination,
                label,
                move |writer| {
                    let half = new.len() / 2;
                    writer.write_all(&new[..half])?;
                    if failing {
                        return Err(std::io::Error::other("the producer failed"));
                    }
                    writer.write_all(&new[half..])
                },
            ))
        }
        WritePrimitive::Copy => flat(fs_write::copy_durable(
            &layout.source,
            &layout.destination,
            label,
        )),
        WritePrimitive::Move => flat(fs_write::move_durable(
            &layout.source,
            &layout.destination,
            label,
        )),
        WritePrimitive::Rename { .. } => flat(fs_write::rename_durable(
            &layout.source,
            &layout.destination,
        )),
        WritePrimitive::RenameNoReplace { .. } => classify(&fs_write::rename_durable_no_replace(
            &layout.source,
            &layout.destination,
        )),
    }
}

/// Typed divergence reported by write-shell conformance replay against the proven disk models.
#[derive(Debug, thiserror::Error)]
pub enum WriteShellConformanceError {
    /// Model destination content was empty or torn where concrete bytes were expected.
    #[error("the model shows {content} at the destination")]
    InvalidModelDestinationContent {
        /// Unexpected model content state.
        content: String,
    },
    /// Trace contained a run kind outside the write-shell cores.
    #[error("an unexpected {kind} run")]
    UnexpectedRunKind {
        /// Observed protocol kind.
        kind: String,
    },
    /// Completed step count did not match the observed snapshot count.
    #[error("{completed} actions completed, {observed} were observed")]
    ObservedActionCountMismatch {
        /// Number of completed trace steps.
        completed: usize,
        /// Number of observed post-action snapshots.
        observed: usize,
    },
    /// One observed step diverged from its pinned call table or disk model.
    #[error("{context}: {source}")]
    Step {
        /// Core, run, ordinal, and action name for the failing step.
        context: String,
        /// Underlying conformance divergence.
        #[source]
        source: Box<WriteShellConformanceError>,
    },
    /// Unexpected directory entries appeared in the sandbox root.
    #[error("unexpected entries: {strays:?}")]
    UnexpectedStrays {
        /// Stray entry names observed in the sandbox.
        strays: Vec<String>,
    },
    /// A fault-free action made a different number of backend calls than pinned.
    #[error("{calls} backend calls, the table pins {expected}")]
    UnexpectedBackendCallCount {
        /// Observed backend call count.
        calls: usize,
        /// Pinned call count.
        expected: usize,
    },
    /// A faulted action exceeded the pinned call ceiling.
    #[error("{calls} backend calls, at most {expected} pinned")]
    ExcessBackendCallCount {
        /// Observed backend call count.
        calls: usize,
        /// Pinned maximum call count.
        expected: usize,
    },
    /// Trace emitted an action missing from the call table.
    #[error("an action the call table does not pin")]
    UnpinnedAction,
    /// Trace or replay state was missing an expected record.
    #[error("{detail}")]
    MissingProtocolState {
        /// Description of the missing protocol record.
        detail: &'static str,
    },
    /// Outcome fed back to the write core disagreed with observed backend calls.
    #[error("fed {fed}, its calls say {expected} (failed call: {failed})")]
    FedWriteOutcomeMismatch {
        /// Outcome fed to the write core.
        fed: String,
        /// Outcome implied by backend calls.
        expected: String,
        /// First failed backend call, if any.
        failed: String,
    },
    /// Boolean outcome fed to a move or rename core disagreed with observed calls.
    #[error("fed {fed}, its calls say {expected}")]
    FedBoolOutcomeMismatch {
        /// Outcome fed to the core.
        fed: bool,
        /// Expected boolean outcome.
        expected: bool,
    },
    /// Core expected a different pending action than the trace executed.
    #[error("the core asked for {pending}")]
    UnexpectedCoreAction {
        /// Debug representation of the expected core action.
        pending: String,
    },
    /// Real destination bytes did not match the model's visible destination state.
    #[error("the destination holds {real:?}, the model shows {visible}")]
    DestinationMismatch {
        /// Actual destination bytes on disk, if present.
        real: Option<Vec<u8>>,
        /// Expected model visibility state.
        visible: String,
    },
    /// Number of temp files on disk disagreed with the model's linked temp flag.
    #[error("{actual} temp files, the model's temp is {state}linked")]
    TempCountMismatch {
        /// Observed temp file count on disk.
        actual: usize,
        /// `""` when linked, `"not "` when unlinked.
        state: &'static str,
    },
    /// Temp file permissions were wider than the metadata source mode.
    #[error("temp {name} has mode {mode:o}, wider than {max_mode:o}")]
    TempModeTooWide {
        /// Temp file name.
        name: String,
        /// Observed mode bits.
        mode: u32,
        /// Source mode mask ceiling.
        max_mode: u32,
    },
    /// Source file presence disagreed with the move model.
    #[error("the source is {presence}present, the model says {model_present}")]
    SourcePresenceMismatch {
        /// `""` when present, `"not "` when absent.
        presence: &'static str,
        /// Model's `source_present` value.
        model_present: bool,
    },
    /// Move reached a state where neither source nor durable destination existed.
    #[error("the move lost its only durable copy")]
    MoveLostOnlyCopy,
    /// Rename state on disk disagreed with the rename model.
    #[error("the entry {verb} its new name, the model says renamed = {model_renamed}")]
    RenameStateMismatch {
        /// `"has"` or `"lacks"`.
        verb: &'static str,
        /// Model's `renamed` value.
        model_renamed: bool,
    },
    /// Top-level core did not reach a terminal finish action.
    #[error("the {kind} core did not finish: {state}")]
    CoreDidNotFinish {
        /// Core kind (`write`, `copy`, `move`, or `rename`).
        kind: &'static str,
        /// Remaining core state.
        state: String,
    },
    /// Primitive return classification disagreed with the core's terminal classification.
    #[error("returned {returned}, the core ended {expected}")]
    EndedMismatch {
        /// Classification returned by the real primitive.
        returned: String,
        /// Expected classification from the core.
        expected: String,
    },
    /// Post-run harness invariant failed.
    #[error("{detail}")]
    Invariant {
        /// Invariant failure message.
        detail: &'static str,
    },
    /// Simulated crash left a torn destination according to the disk model.
    #[error("a crash left a torn destination: {content}")]
    CrashTornDestination {
        /// Torn destination content state.
        content: String,
    },
    /// Simulated crash destination check failed.
    #[error("after the crash: {source}")]
    AfterCrash {
        /// Underlying destination mismatch.
        #[source]
        source: Box<WriteShellConformanceError>,
    },
    /// Simulated crash of a move lost both source and destination.
    #[error("after the crash the move lost its only copy")]
    CrashMoveLostCopy,
    /// Simulated crash left entries outside the sweepable temp-file contract.
    #[error("after the crash, entries LushText cannot sweep: {strays:?}")]
    CrashUnsweepableStrays {
        /// Unsweepable stray entry names.
        strays: Vec<String>,
    },
    /// Leftover temp file lacked modification time metadata.
    #[error("leftover {name} has no modification time")]
    LeftoverMissingMtime {
        /// Leftover temp filename.
        name: String,
    },
    /// Leftover temp file did not parse as a durable temp name.
    #[error("leftover {name} is not a durable temp name")]
    LeftoverInvalidTempName {
        /// Leftover filename.
        name: String,
    },
    /// Leftover temp file was not accepted by the startup leftover sweeper.
    #[error("leftover {name} is not one a later sweep removes")]
    LeftoverNotSweepable {
        /// Leftover filename.
        name: String,
    },
}

/// Seed `root`, run the case, and check every action against the models.
///
/// # Errors
///
/// Returns the first divergence, described.
///
/// # Panics
///
/// Panics when a shared model's own assertion fails (a second temp, new
/// bytes in a wider file), which is a divergence too.
pub fn check_write_case(
    root: &Path,
    case: &WriteCase,
) -> Result<WriteCaseReport, WriteShellConformanceError> {
    let layout = Layout::new(root, case.primitive);
    fixture::create_dir_all(&root.join("a"));
    fixture::create_dir_all(&root.join("b"));
    if case.destination_exists {
        fixture::write_bytes(&layout.destination, &case.old);
        fixture::set_mode(&layout.destination, case.mode);
    }
    if case.primitive.has_source() {
        fixture::write_bytes(&layout.source, &case.new);
        fixture::set_mode(&layout.source, case.mode);
    }

    let snapshots: Rc<RefCell<Vec<(usize, Snapshot)>>> = Rc::default();
    let mut plan = TapPlan::record();
    for fault in &case.faults {
        plan = plan.inject(
            Address::Protocol {
                kind: fault.core.kind(),
                run: fault.run,
                ordinal: fault.ordinal,
                call: fault.call,
            },
            injected(fault.kind),
        );
    }
    if let Some(crash) = case.crash {
        plan = plan.crash(CrashAt {
            at: ActionAt::Ordinal {
                kind: crash.core.kind(),
                run: crash.run,
                ordinal: crash.ordinal,
            },
            after: crash.after,
        });
    }
    let observed = Rc::clone(&snapshots);
    let observed_layout = Layout::new(root, case.primitive);
    plan = plan.observe_after_each_action(move |step| {
        observed
            .borrow_mut()
            .push((step, observed_layout.snapshot()));
    });

    let guard = tap::install(plan);
    let ended = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_primitive(case, &layout)
    }));
    let trace = guard.finish();
    let ended = match ended {
        Ok(ended) => Some(ended),
        Err(payload) if tap::is_tap_crash(payload.as_ref()) => None,
        Err(payload) => std::panic::resume_unwind(payload),
    };
    let snapshots = std::mem::take(&mut *snapshots.borrow_mut());
    Replay::new(case, &layout, &trace, snapshots).check(ended)
}

/// The models stepped from one trace.
struct Replay<'a> {
    case: &'a WriteCase,
    layout: &'a Layout,
    trace: &'a TapTrace,
    snapshots: Vec<(usize, Snapshot)>,
    /// One disk model per write run, by run index.
    disks: Vec<Option<Disk>>,
    moves: Vec<Option<MoveDisk>>,
    renames: Vec<Option<RenameNamespace>>,
    /// The core's view of each run: its protocol, stepped with the fed outcomes.
    write_cores: Vec<Option<(WriteProtocol, WriteAction)>>,
    move_cores: Vec<Option<(MoveProtocol, MoveAction)>>,
    rename_cores: Vec<Option<(RenameProtocol, RenameAction)>>,
}

impl<'a> Replay<'a> {
    fn new(
        case: &'a WriteCase,
        layout: &'a Layout,
        trace: &'a TapTrace,
        snapshots: Vec<(usize, Snapshot)>,
    ) -> Self {
        let runs = trace.runs.len();
        let cross_directory = matches!(
            case.primitive,
            WritePrimitive::Rename {
                cross_directory: true
            } | WritePrimitive::RenameNoReplace {
                cross_directory: true
            }
        );
        let mut replay = Self {
            case,
            layout,
            trace,
            snapshots,
            disks: vec![None; runs],
            moves: vec![None; runs],
            renames: vec![None; runs],
            write_cores: vec![None; runs],
            move_cores: vec![None; runs],
            rename_cores: vec![None; runs],
        };
        for (index, run) in trace.runs.iter().enumerate() {
            match run.kind {
                ProtocolKind::Write => {
                    replay.disks[index] = Some(Disk::before_write());
                    replay.write_cores[index] = Some(WriteProtocol::start());
                }
                ProtocolKind::Move => {
                    replay.moves[index] = Some(MoveDisk::before_move());
                    replay.move_cores[index] = Some(MoveProtocol::start());
                }
                ProtocolKind::Rename => {
                    replay.renames[index] = Some(RenameNamespace::before_rename());
                    replay.rename_cores[index] = Some(RenameProtocol::start(cross_directory));
                }
                _ => {}
            }
        }
        replay
    }

    /// Whether the write runs with a metadata source (an existing destination,
    /// or a copy's explicit source).
    fn metadata_source(&self) -> bool {
        self.case.destination_exists
            || matches!(
                self.case.primitive,
                WritePrimitive::Copy | WritePrimitive::Move
            )
    }

    fn content_bytes(
        &self,
        content: Content,
    ) -> Result<Option<Vec<u8>>, WriteShellConformanceError> {
        match content {
            Content::Old => Ok(self.case.destination_exists.then(|| self.case.old.clone())),
            Content::New => Ok(Some(self.case.new.clone())),
            Content::Empty | Content::Torn => {
                Err(WriteShellConformanceError::InvalidModelDestinationContent {
                    content: format!("{content:?}"),
                })
            }
        }
    }

    fn check(
        mut self,
        ended: Option<Ended>,
    ) -> Result<WriteCaseReport, WriteShellConformanceError> {
        let mut report = WriteCaseReport {
            crashed: ended.is_none(),
            injected: self.case.faults.len() - self.trace.unfired.len(),
            ..WriteCaseReport::default()
        };
        let mut occurrence = vec![0usize; self.trace.runs.len()];
        let mut seen = std::collections::HashMap::new();
        for (index, run) in self.trace.runs.iter().enumerate() {
            let counter = seen.entry(run.kind).or_insert(0usize);
            occurrence[index] = *counter;
            *counter += 1;
        }
        for step in &self.trace.steps {
            let core = WriteCore::of(step.kind).ok_or_else(|| {
                WriteShellConformanceError::UnexpectedRunKind {
                    kind: format!("{:?}", step.kind),
                }
            })?;
            report.actions.push(ActionShape {
                core,
                run: occurrence[step.run],
                ordinal: step.ordinal,
                name: step.action_name.clone(),
                points: step.calls.len(),
            });
        }
        // Actions complete in observation order: a move's copy completes
        // after the nested write it runs.
        let observed = std::mem::take(&mut self.snapshots);
        let completed = self
            .trace
            .steps
            .iter()
            .filter(|step| step.outcome.is_some())
            .count();
        if observed.len() != completed {
            return Err(WriteShellConformanceError::ObservedActionCountMismatch {
                completed,
                observed: observed.len(),
            });
        }
        for (index, snapshot) in &observed {
            let step = &self.trace.steps[*index];
            let core = report.actions[*index].core;
            let context = format!(
                "{core:?} run {} action {} {}",
                occurrence[step.run], step.ordinal, step.action_name
            );
            self.step(step, core, snapshot)
                .map_err(|source| WriteShellConformanceError::Step {
                    context,
                    source: Box::new(source),
                })?;
        }
        match ended {
            Some(ended) => self.check_end(ended)?,
            None => self.check_crash()?,
        }
        Ok(report)
    }

    fn step(
        &mut self,
        step: &StepRecord,
        core: WriteCore,
        snapshot: &Snapshot,
    ) -> Result<(), WriteShellConformanceError> {
        if !snapshot.strays.is_empty() {
            return Err(WriteShellConformanceError::UnexpectedStrays {
                strays: snapshot.strays.clone(),
            });
        }
        let failed = step
            .backend_calls()
            .find(|record| record.failed.is_some())
            .cloned();
        let calls = step.backend_calls().count();
        if let Some(expected) = expected_calls(core, &step.action_name, self.metadata_source()) {
            if failed.is_none() && calls != expected {
                return Err(WriteShellConformanceError::UnexpectedBackendCallCount {
                    calls,
                    expected,
                });
            }
            if failed.is_some() && calls > expected {
                return Err(WriteShellConformanceError::ExcessBackendCallCount { calls, expected });
            }
        } else {
            return Err(WriteShellConformanceError::UnpinnedAction);
        }
        match core {
            WriteCore::Write => self.step_write(step, failed.as_ref(), snapshot),
            WriteCore::Move => self.step_move(step, failed.as_ref(), snapshot),
            WriteCore::Rename => self.step_rename(step, failed.as_ref(), snapshot),
        }
    }

    fn step_write(
        &mut self,
        step: &StepRecord,
        failed: Option<&tap::CallRecord>,
        snapshot: &Snapshot,
    ) -> Result<(), WriteShellConformanceError> {
        let action: WriteAction =
            step.action()
                .ok_or(WriteShellConformanceError::MissingProtocolState {
                    detail: "not a write action",
                })?;
        let fed: StepOutcome =
            step.outcome()
                .ok_or(WriteShellConformanceError::MissingProtocolState {
                    detail: "no fed outcome",
                })?;
        let required_source = matches!(
            self.case.primitive,
            WritePrimitive::Copy | WritePrimitive::Move
        );
        let expected = match (action, failed) {
            (WriteAction::RemoveTemp, _) => StepOutcome::Done,
            (WriteAction::WriteContent, _) => {
                let content_failed = step
                    .calls
                    .iter()
                    .any(|record| record.call == Call::ContentWritten && record.failed.is_some());
                if content_failed || self.case.failing_producer {
                    StepOutcome::Failed
                } else {
                    StepOutcome::Done
                }
            }
            (WriteAction::ProbeMetadata, Some(record))
                if !required_source
                    && !record.injected
                    && record.failed == Some(std::io::ErrorKind::NotFound) =>
            {
                StepOutcome::Done
            }
            (WriteAction::CreateTemp, Some(record))
                if record.failed == Some(std::io::ErrorKind::AlreadyExists) =>
            {
                StepOutcome::AlreadyExists
            }
            (_, Some(_)) => StepOutcome::Failed,
            (_, None) => StepOutcome::Done,
        };
        if fed != expected {
            return Err(WriteShellConformanceError::FedWriteOutcomeMismatch {
                fed: format!("{fed:?}"),
                expected: format!("{expected:?}"),
                failed: format!("{:?}", failed.map(|record| (record.call, &record.path))),
            });
        }
        // The shell feeds a temp removal back as `Done` whatever happened; the
        // model is stepped with what the removal call really did.
        let model_outcome = if action == WriteAction::RemoveTemp && failed.is_some() {
            StepOutcome::Failed
        } else {
            fed
        };
        let run = step.run;
        let disk =
            self.disks[run]
                .as_mut()
                .ok_or(WriteShellConformanceError::MissingProtocolState {
                    detail: "no disk for the run",
                })?;
        let shell_outcome = disk.execute(action, Shell::Production, model_outcome);
        disk.assert_new_bytes_are_never_wider();
        let disk = *disk;
        let (protocol, pending) =
            self.write_cores[run].ok_or(WriteShellConformanceError::MissingProtocolState {
                detail: "no write core",
            })?;
        if pending != action {
            return Err(WriteShellConformanceError::UnexpectedCoreAction {
                pending: format!("{pending:?}"),
            });
        }
        self.write_cores[run] = Some(protocol.step(shell_outcome));
        // Nested in a move, the copy writes the move's destination too.
        self.check_destination(disk.visible(), snapshot)?;
        let linked = disk.temp.is_some_and(|temp| temp.linked);
        if snapshot.temps.len() != usize::from(linked) {
            return Err(WriteShellConformanceError::TempCountMismatch {
                actual: snapshot.temps.len(),
                state: if linked { "" } else { "not " },
            });
        }
        if self.metadata_source() {
            for (name, mode) in &snapshot.temps {
                if mode & 0o777 & !self.case.mode != 0 {
                    return Err(WriteShellConformanceError::TempModeTooWide {
                        name: name.clone(),
                        mode: *mode,
                        max_mode: self.case.mode,
                    });
                }
            }
        }
        Ok(())
    }

    fn check_destination(
        &self,
        visible: Content,
        snapshot: &Snapshot,
    ) -> Result<(), WriteShellConformanceError> {
        let expected = self.content_bytes(visible)?;
        let real = snapshot
            .destination
            .as_ref()
            .map(|(bytes, _)| bytes.clone());
        if real != expected {
            return Err(WriteShellConformanceError::DestinationMismatch {
                real,
                visible: format!("{visible:?}"),
            });
        }
        Ok(())
    }

    fn step_move(
        &mut self,
        step: &StepRecord,
        failed: Option<&tap::CallRecord>,
        snapshot: &Snapshot,
    ) -> Result<(), WriteShellConformanceError> {
        let action: MoveAction =
            step.action()
                .ok_or(WriteShellConformanceError::MissingProtocolState {
                    detail: "not a move action",
                })?;
        let fed: bool = step
            .outcome()
            .ok_or(WriteShellConformanceError::MissingProtocolState {
                detail: "no fed outcome",
            })?;
        let expected = match action {
            // The copy's result is its nested write's class.
            MoveAction::Copy => {
                failed.is_none()
                    && self
                        .trace
                        .runs
                        .iter()
                        .enumerate()
                        .filter(|(_, run)| run.parent == Some(step.run))
                        .all(|(child, _)| {
                            matches!(
                                self.write_cores[child],
                                Some((_, WriteAction::Finish(WriteClass::Success)))
                            )
                        })
            }
            _ => failed.is_none(),
        };
        if fed != expected {
            return Err(WriteShellConformanceError::FedBoolOutcomeMismatch { fed, expected });
        }
        let disk = self.moves[step.run].as_mut().ok_or(
            WriteShellConformanceError::MissingProtocolState {
                detail: "no move model",
            },
        )?;
        disk.execute(action, fed);
        let disk = *disk;
        let (protocol, pending) =
            self.move_cores[step.run].ok_or(WriteShellConformanceError::MissingProtocolState {
                detail: "no move core",
            })?;
        if pending != action {
            return Err(WriteShellConformanceError::UnexpectedCoreAction {
                pending: format!("{pending:?}"),
            });
        }
        self.move_cores[step.run] = Some(protocol.step(fed));
        if snapshot.source.is_some() != disk.source_present {
            return Err(WriteShellConformanceError::SourcePresenceMismatch {
                presence: if snapshot.source.is_some() {
                    ""
                } else {
                    "not "
                },
                model_present: disk.source_present,
            });
        }
        if disk.destination_durable {
            self.check_destination(Content::New, snapshot)?;
        }
        if !(disk.source_present || disk.destination_durable) {
            return Err(WriteShellConformanceError::MoveLostOnlyCopy);
        }
        Ok(())
    }

    fn step_rename(
        &mut self,
        step: &StepRecord,
        failed: Option<&tap::CallRecord>,
        snapshot: &Snapshot,
    ) -> Result<(), WriteShellConformanceError> {
        let action: RenameAction =
            step.action()
                .ok_or(WriteShellConformanceError::MissingProtocolState {
                    detail: "not a rename action",
                })?;
        let fed: bool = step
            .outcome()
            .ok_or(WriteShellConformanceError::MissingProtocolState {
                detail: "no fed outcome",
            })?;
        if fed != failed.is_none() {
            return Err(WriteShellConformanceError::FedBoolOutcomeMismatch {
                fed,
                expected: failed.is_none(),
            });
        }
        let namespace = self.renames[step.run].as_mut().ok_or(
            WriteShellConformanceError::MissingProtocolState {
                detail: "no namespace",
            },
        )?;
        namespace.execute(action, fed);
        let namespace = *namespace;
        let (protocol, pending) = self.rename_cores[step.run].ok_or(
            WriteShellConformanceError::MissingProtocolState {
                detail: "no rename core",
            },
        )?;
        if pending != action {
            return Err(WriteShellConformanceError::UnexpectedCoreAction {
                pending: format!("{pending:?}"),
            });
        }
        self.rename_cores[step.run] = Some(protocol.step(fed));
        let moved = snapshot.source.is_none()
            && snapshot.destination.as_ref().map(|(bytes, _)| bytes) == Some(&self.case.new);
        if moved != namespace.renamed {
            return Err(WriteShellConformanceError::RenameStateMismatch {
                verb: if moved { "has" } else { "lacks" },
                model_renamed: namespace.renamed,
            });
        }
        Ok(())
    }

    /// The returned result against the top-level core's end.
    fn check_end(&self, ended: Ended) -> Result<(), WriteShellConformanceError> {
        let top = self.trace.runs.iter().position(|run| run.parent.is_none());
        let expected = match (self.case.primitive, top) {
            // A rename refused by an injected or real cross-device error
            // before any action is still a rename run, so `top` exists.
            (_, None) => Ended::Failed,
            (WritePrimitive::Replace | WritePrimitive::ReplaceStream, Some(run)) => {
                match self.write_cores[run] {
                    Some((_, WriteAction::Finish(WriteClass::Success))) => Ended::Ok,
                    Some((_, WriteAction::Finish(WriteClass::BeforeRename))) => Ended::BeforeRename,
                    Some((_, WriteAction::Finish(WriteClass::AfterRename))) => Ended::AfterRename,
                    other => {
                        return Err(WriteShellConformanceError::CoreDidNotFinish {
                            kind: "write",
                            state: format!("{other:?}"),
                        });
                    }
                }
            }
            (WritePrimitive::Copy, Some(run)) => match self.write_cores[run] {
                Some((_, WriteAction::Finish(WriteClass::Success))) => Ended::Ok,
                Some((_, WriteAction::Finish(_))) => Ended::Failed,
                other => {
                    return Err(WriteShellConformanceError::CoreDidNotFinish {
                        kind: "copy",
                        state: format!("{other:?}"),
                    });
                }
            },
            (WritePrimitive::Move, Some(run)) => match self.move_cores[run] {
                Some((_, MoveAction::Finish(true))) => Ended::Ok,
                Some((_, MoveAction::Finish(false))) => Ended::Failed,
                other => {
                    return Err(WriteShellConformanceError::CoreDidNotFinish {
                        kind: "move",
                        state: format!("{other:?}"),
                    });
                }
            },
            (WritePrimitive::Rename { .. }, Some(run)) => match self.rename_cores[run] {
                Some((_, RenameAction::Finish(RenameClass::Success))) => Ended::Ok,
                Some((_, RenameAction::Finish(_))) => Ended::Failed,
                other => {
                    return Err(WriteShellConformanceError::CoreDidNotFinish {
                        kind: "rename",
                        state: format!("{other:?}"),
                    });
                }
            },
            (WritePrimitive::RenameNoReplace { .. }, Some(run)) => match self.rename_cores[run] {
                Some((_, RenameAction::Finish(RenameClass::Success))) => Ended::Ok,
                Some((_, RenameAction::Finish(RenameClass::BeforeRename))) => Ended::BeforeRename,
                Some((_, RenameAction::Finish(RenameClass::AfterRename))) => Ended::AfterRename,
                other => {
                    return Err(WriteShellConformanceError::CoreDidNotFinish {
                        kind: "rename",
                        state: format!("{other:?}"),
                    });
                }
            },
        };
        if ended != expected {
            return Err(WriteShellConformanceError::EndedMismatch {
                returned: format!("{ended:?}"),
                expected: format!("{expected:?}"),
            });
        }
        // The harness's own classification facts, on the real run.
        for (run, disk) in self.disks.iter().enumerate() {
            let (Some(disk), Some((_, WriteAction::Finish(class)))) = (disk, self.write_cores[run])
            else {
                continue;
            };
            if disk.every_step_done && class != WriteClass::Success {
                return Err(WriteShellConformanceError::Invariant {
                    detail: "a write with no failure did not succeed",
                });
            }
            if disk.temp_name_attempts > 0
                && disk.every_temp_name_taken
                && disk.temp_name_attempts
                    != crate::filesystem::write_protocol::MAX_TEMP_NAME_ATTEMPTS
            {
                return Err(WriteShellConformanceError::Invariant {
                    detail: "a write gave up on taken names early",
                });
            }
            if class == WriteClass::BeforeRename
                && disk.temp.is_some_and(|temp| temp.linked)
                && !disk.removal_failed
            {
                return Err(WriteShellConformanceError::Invariant {
                    detail: "a failed write left its temp behind",
                });
            }
        }
        Ok(())
    }

    /// After a crash: the live crash view, and leftovers a later launch sweeps.
    fn check_crash(&self) -> Result<(), WriteShellConformanceError> {
        let now = self.layout.snapshot();
        let write = self
            .disks
            .iter()
            .enumerate()
            .rev()
            .find_map(|(run, disk)| disk.map(|disk| (run, disk)));
        if let Some((_, disk)) = write {
            let (content, _) = disk.after_crash(false);
            if !matches!(content, Content::Old | Content::New) {
                return Err(WriteShellConformanceError::CrashTornDestination {
                    content: format!("{content:?}"),
                });
            }
            self.check_destination(content, &now).map_err(|source| {
                WriteShellConformanceError::AfterCrash {
                    source: Box::new(source),
                }
            })?;
        }
        if let Some(disk) = self.moves.iter().flatten().next()
            && !(disk.source_present || disk.destination_durable)
        {
            return Err(WriteShellConformanceError::CrashMoveLostCopy);
        }
        let directory = self
            .layout
            .destination
            .parent()
            .expect("the layout has directories");
        let target = self.layout.destination_name();
        // Every leftover, not only those that parse: a stray name fails.
        if !now.strays.is_empty() {
            return Err(WriteShellConformanceError::CrashUnsweepableStrays { strays: now.strays });
        }
        for (name, _) in &now.temps {
            let path = directory.join(name);
            let modified = crate::filesystem::metadata::file_stat(&path)
                .ok()
                .and_then(|facts| facts.modified_at_nanos)
                .map(|nanos| {
                    std::time::UNIX_EPOCH
                        + Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
                });
            let Some(modified) = modified else {
                return Err(WriteShellConformanceError::LeftoverMissingMtime {
                    name: name.clone(),
                });
            };
            let parsed = temp_name::parse(name).ok_or_else(|| {
                WriteShellConformanceError::LeftoverInvalidTempName { name: name.clone() }
            })?;
            let clock = LeftoverClock {
                current_pid: std::process::id(),
                // A later launch: a launch never sweeps its own temps.
                current_launch: parsed.launch_nonce().wrapping_add(1),
                now: modified + STALE_LEFTOVER_AGE + Duration::from_secs(1),
            };
            let facts = LeftoverFacts {
                is_regular_file: true,
                modified: Some(modified),
            };
            if !is_sweepable_leftover(
                std::ffi::OsStr::new(name),
                facts,
                LeftoverScope::Target(std::ffi::OsStr::new(&target)),
                clock,
            ) {
                return Err(WriteShellConformanceError::LeftoverNotSweepable {
                    name: name.clone(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn replay_returns_structured_conformance_errors_on_invalid_inputs() {
        let root = tempfile::tempdir().expect("tempdir");
        let case = WriteCase::plain(WritePrimitive::Replace, false);
        let layout = Layout::new(root.path(), case.primitive);
        let trace = TapTrace::default();
        let replay = Replay::new(&case, &layout, &trace, Vec::new());

        assert!(matches!(
            replay.content_bytes(Content::Torn),
            Err(WriteShellConformanceError::InvalidModelDestinationContent { ref content })
                if content == "Torn"
        ));
        assert!(matches!(
            replay.check(Some(Ended::Ok)),
            Err(WriteShellConformanceError::EndedMismatch {
                ref returned,
                ref expected
            }) if returned == "Ok" && expected == "Failed"
        ));
    }
}
