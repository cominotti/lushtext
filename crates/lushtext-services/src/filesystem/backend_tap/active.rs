// SPDX-License-Identifier: GPL-3.0-or-later

//! The tap's test-build half: thread-local state, plans, the trace, and the
//! injection and crash matching. See the parent module for the contract.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Debug;
use std::io;
use std::path::{Path, PathBuf};

use super::{Call, ProtocolKind};

/// The text every tap crash carries; the crash-recovery smoke's marker guard
/// fails when a shipping binary contains it.
pub(crate) const TAP_CRASH_MARKER: &str = "lushtext conformance tap crash";

/// The payload a simulated crash unwinds with. Drivers catch it with
/// `catch_unwind` and tell it from a real panic by downcasting.
pub(crate) struct TapCrash;

impl Debug for TapCrash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(TAP_CRASH_MARKER)
    }
}

/// Whether a caught unwind is a tap crash rather than a real panic.
pub(crate) fn is_tap_crash(payload: &(dyn Any + Send)) -> bool {
    payload.is::<TapCrash>()
}

/// The error an injection makes a call return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Injected {
    /// An error of this kind, with this message.
    Kind(io::ErrorKind, &'static str),
    /// `EXDEV`, the error a rename across filesystems returns.
    CrossDevice,
}

impl Injected {
    /// A plain injected failure.
    pub(crate) const FAIL: Self = Self::Kind(io::ErrorKind::Other, "injected backend failure");
    /// A taken name.
    pub(crate) const ALREADY_EXISTS: Self =
        Self::Kind(io::ErrorKind::AlreadyExists, "injected name collision");

    fn error(self) -> io::Error {
        match self {
            Self::Kind(kind, message) => io::Error::new(kind, message),
            Self::CrossDevice => super::super::sys::cross_device_error_for_test(),
        }
    }
}

/// Where an injection fires: the call it replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Address {
    /// Protocol-scoped: call `call` of action `ordinal` of the `run`-th run
    /// of `kind` started since the plan was installed (0-based each).
    Protocol {
        kind: ProtocolKind,
        run: usize,
        ordinal: usize,
        call: usize,
    },
    /// Destination-scoped: call `call` of the action named `action` (its
    /// `Debug` name, without fields) in a run of `kind` whose destination,
    /// learned from the run's rename, is `destination`.
    Destination {
        kind: ProtocolKind,
        destination: PathBuf,
        action: &'static str,
        call: usize,
    },
    /// Scope-wide: the `call`-th tap point since the plan was installed, any
    /// primitive, inside or outside a protocol run.
    ScopeWide { call: usize },
    /// The `sync_parent_dir` hook: let `succeeding` parent-directory syncs
    /// succeed, then fail the next one.
    ParentSync { succeeding: u32 },
    /// The next `call` made inside an action named `action` of any run of
    /// `kind`.
    NextInAction {
        kind: ProtocolKind,
        action: &'static str,
        call: Call,
    },
}

/// One injected fault.
#[derive(Clone, Debug)]
pub(crate) struct Injection {
    pub(crate) at: Address,
    pub(crate) error: Injected,
}

/// Which action a hook or crash is addressed to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ActionAt {
    /// Action `ordinal` of the `run`-th run of `kind` since the plan was
    /// installed.
    Ordinal {
        kind: ProtocolKind,
        run: usize,
        ordinal: usize,
    },
    /// The `nth` action named `name` of any run of `kind` since the plan was
    /// installed.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "only unit tests address an action by name")
    )]
    Named {
        kind: ProtocolKind,
        name: &'static str,
        nth: usize,
    },
}

/// A simulated crash: the process stops before or after an action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CrashAt {
    pub(crate) at: ActionAt,
    pub(crate) after: bool,
}

type Hook = Box<dyn FnOnce()>;
type ContentHook = Box<dyn FnMut(&Path)>;
type Observer = Box<dyn FnMut(usize)>;

/// What one installed tap does.
#[derive(Default)]
pub(crate) struct TapPlan {
    injections: Vec<Injection>,
    hooks: Vec<(ActionAt, Hook)>,
    crash: Option<CrashAt>,
    content_hook: Option<ContentHook>,
    observer: Option<Observer>,
}

impl TapPlan {
    /// A plan that only records.
    #[must_use]
    pub(crate) fn record() -> Self {
        Self::default()
    }

    /// Also inject `error` at `at`.
    #[must_use]
    pub(crate) fn inject(mut self, at: Address, error: Injected) -> Self {
        self.injections.push(Injection { at, error });
        self
    }

    /// Also run `hook` just before the addressed action executes.
    #[must_use]
    #[cfg_attr(not(test), expect(dead_code, reason = "only unit tests install hooks"))]
    pub(crate) fn before_action(mut self, at: ActionAt, hook: impl FnOnce() + 'static) -> Self {
        self.hooks.push((at, Box::new(hook)));
        self
    }

    /// Also crash before (or, with `after`, after) the addressed action.
    #[must_use]
    pub(crate) fn crash(mut self, crash: CrashAt) -> Self {
        self.crash = Some(crash);
        self
    }

    /// Also call `observer` after every recorded action, with the action's
    /// index in [`TapTrace::steps`], before any crash addressed after it.
    /// The tap neither records nor injects into the calls `observer` makes,
    /// so it may read the disk through the fixtures.
    #[must_use]
    pub(crate) fn observe_after_each_action(
        mut self,
        observer: impl FnMut(usize) + 'static,
    ) -> Self {
        self.observer = Some(Box::new(observer));
        self
    }

    /// Also call `hook` with the temp path once a write's content is written.
    #[must_use]
    #[cfg_attr(not(test), expect(dead_code, reason = "only unit tests install hooks"))]
    pub(crate) fn on_content_written(mut self, hook: impl FnMut(&Path) + 'static) -> Self {
        self.content_hook = Some(Box::new(hook));
        self
    }
}

/// One tap point's record.
#[derive(Clone, Debug)]
pub(crate) struct CallRecord {
    pub(crate) call: Call,
    pub(crate) path: PathBuf,
    /// The tap returned an injected error instead of making the call.
    pub(crate) injected: bool,
    /// The kind of the error returned, if the call failed.
    pub(crate) failed: Option<io::ErrorKind>,
}

/// One executed action.
pub(crate) struct StepRecord {
    pub(crate) kind: ProtocolKind,
    /// Index of the run in [`TapTrace::runs`].
    pub(crate) run: usize,
    /// The action's position in its run.
    pub(crate) ordinal: usize,
    pub(crate) action: Box<dyn Any>,
    pub(crate) action_name: String,
    /// The outcome fed back; `None` when a crash stopped the action.
    pub(crate) outcome: Option<Box<dyn Any>>,
    /// The tap points the action made outside any nested run.
    pub(crate) calls: Vec<CallRecord>,
}

impl StepRecord {
    /// The action, downcast to the protocol's action type.
    pub(crate) fn action<A: Copy + 'static>(&self) -> Option<A> {
        self.action.downcast_ref::<A>().copied()
    }

    /// The outcome fed back, downcast to the protocol's outcome type.
    pub(crate) fn outcome<O: Copy + 'static>(&self) -> Option<O> {
        self.outcome
            .as_ref()
            .and_then(|outcome| outcome.downcast_ref::<O>().copied())
    }

    /// The fallible backend calls, without the content point.
    pub(crate) fn backend_calls(&self) -> impl Iterator<Item = &CallRecord> {
        self.calls
            .iter()
            .filter(|record| record.call != Call::ContentWritten)
    }
}

/// One `drive` run.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RunRecord {
    pub(crate) kind: ProtocolKind,
    /// The run whose action started this one (a move's copy).
    pub(crate) parent: Option<usize>,
}

/// Everything one installed plan saw.
#[derive(Default)]
pub(crate) struct TapTrace {
    pub(crate) runs: Vec<RunRecord>,
    pub(crate) steps: Vec<StepRecord>,
    /// Tap points made outside every protocol run.
    pub(crate) loose_calls: Vec<CallRecord>,
    /// Injections of the plan that never fired.
    pub(crate) unfired: Vec<Injection>,
}

struct ActiveRun {
    kind: ProtocolKind,
    /// Index in the installed plan's run list, when a plan is installed.
    serial: Option<usize>,
    occurrence: usize,
    ordinal: Option<usize>,
    action_name: String,
    call_index: usize,
    destination: Option<PathBuf>,
    /// Index of the current action's step record.
    step: Option<usize>,
}

struct Installed {
    plan: TapPlan,
    trace: TapTrace,
    scope_calls: usize,
    occurrences: HashMap<ProtocolKind, usize>,
    named: HashMap<(ProtocolKind, String), usize>,
}

#[derive(Default)]
struct TapState {
    /// The tap is running a plan's observer: calls pass straight through.
    suspended: bool,
    /// Guardless injections armed by the `*_for_test` setters.
    legacy: Vec<Injection>,
    installed: Option<Installed>,
    runs: Vec<ActiveRun>,
}

thread_local! {
    static TAP: RefCell<TapState> = RefCell::new(TapState::default());
}

/// Uninstalls the plan it was returned for when dropped.
pub(crate) struct TapGuard {
    armed: bool,
    _not_send: std::marker::PhantomData<*const ()>,
}

impl TapGuard {
    /// Uninstall the plan and return what it saw.
    pub(crate) fn finish(mut self) -> TapTrace {
        self.armed = false;
        take_installed()
    }
}

impl Drop for TapGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = take_installed();
        }
    }
}

fn take_installed() -> TapTrace {
    TAP.with(|tap| {
        let mut tap = tap.borrow_mut();
        tap.installed
            .take()
            .map_or_else(TapTrace::default, |mut installed| {
                installed.trace.unfired = std::mem::take(&mut installed.plan.injections);
                installed.trace
            })
    })
}

/// Install `plan` on this thread until the returned guard is dropped.
///
/// # Panics
///
/// Panics if a plan is already installed on this thread.
pub(crate) fn install(plan: TapPlan) -> TapGuard {
    TAP.with(|tap| {
        let mut tap = tap.borrow_mut();
        assert!(tap.installed.is_none(), "a tap plan is already installed");
        tap.installed = Some(Installed {
            plan,
            trace: TapTrace::default(),
            scope_calls: 0,
            occurrences: HashMap::new(),
            named: HashMap::new(),
        });
    });
    TapGuard {
        armed: true,
        _not_send: std::marker::PhantomData,
    }
}

/// Arm a guardless injection on this thread, consumed when it fires: the
/// semantics of the durable-write `*_for_test` setters.
pub(crate) fn arm(injection: Injection) {
    TAP.with(|tap| tap.borrow_mut().legacy.push(injection));
}

/// The armed parent-sync injection, as `(syncs that still succeed, kind)`.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "only the durable-write unit tests' `*_for_test` setters use it"
    )
)]
pub(crate) fn armed_parent_sync() -> Option<(u32, io::ErrorKind)> {
    TAP.with(|tap| {
        tap.borrow()
            .legacy
            .iter()
            .find_map(|injection| match injection {
                Injection {
                    at: Address::ParentSync { succeeding },
                    error: Injected::Kind(kind, _),
                } => Some((*succeeding, *kind)),
                _ => None,
            })
    })
}

/// The `Debug` name of an action, without its fields.
fn action_name(action: &dyn Debug) -> String {
    let full = format!("{action:?}");
    let end = full.find(['(', ' ', '{']).unwrap_or(full.len());
    full[..end].to_string()
}

/// A `drive` run as the tap tracks it; dropping it ends the run, also while
/// a crash unwinds.
pub(crate) struct RunGuard {
    _not_send: std::marker::PhantomData<*const ()>,
}

impl RunGuard {
    /// Start a run of `kind`.
    pub(crate) fn start(kind: ProtocolKind) -> Self {
        TAP.with(|tap| {
            let mut tap = tap.borrow_mut();
            let parent = tap.runs.last().and_then(|run| run.serial);
            let (serial, occurrence) = match tap.installed.as_mut() {
                Some(installed) => {
                    let occurrence = installed.occurrences.entry(kind).or_insert(0);
                    let this = *occurrence;
                    *occurrence += 1;
                    installed.trace.runs.push(RunRecord { kind, parent });
                    (Some(installed.trace.runs.len() - 1), this)
                }
                None => (None, 0),
            };
            tap.runs.push(ActiveRun {
                kind,
                serial,
                occurrence,
                ordinal: None,
                action_name: String::new(),
                call_index: 0,
                destination: None,
                step: None,
            });
        });
        Self {
            _not_send: std::marker::PhantomData,
        }
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        TAP.with(|tap| {
            tap.borrow_mut().runs.pop();
        });
    }
}

fn matches_action(at: &ActionAt, run: &ActiveRun, nth: usize) -> bool {
    match at {
        ActionAt::Ordinal {
            kind,
            run: occurrence,
            ordinal,
        } => *kind == run.kind && *occurrence == run.occurrence && Some(*ordinal) == run.ordinal,
        ActionAt::Named {
            kind,
            name,
            nth: wanted,
        } => *kind == run.kind && *name == run.action_name && *wanted == nth,
    }
}

fn crash() -> ! {
    std::panic::resume_unwind(Box::new(TapCrash))
}

/// The action site, before `execute`: record the action, run its hooks, and
/// crash if a crash is addressed before it.
pub(crate) fn before<A: Copy + Debug + 'static>(kind: ProtocolKind, action: &A) {
    let (hooks, crash_now) = TAP.with(|tap| {
        let mut tap = tap.borrow_mut();
        let tap = &mut *tap;
        if tap.suspended {
            return (Vec::new(), false);
        }
        let Some(run) = tap.runs.last_mut() else {
            return (Vec::new(), false);
        };
        debug_assert_eq!(run.kind, kind);
        run.ordinal = Some(run.ordinal.map_or(0, |ordinal| ordinal + 1));
        run.call_index = 0;
        if tap.installed.is_none() && tap.legacy.is_empty() {
            return (Vec::new(), false);
        }
        run.action_name = action_name(action);
        let Some(installed) = tap.installed.as_mut() else {
            return (Vec::new(), false);
        };
        let nth = {
            let count = installed
                .named
                .entry((kind, run.action_name.clone()))
                .or_insert(0);
            let this = *count;
            *count += 1;
            this
        };
        if let Some(serial) = run.serial {
            installed.trace.steps.push(StepRecord {
                kind,
                run: serial,
                ordinal: run.ordinal.unwrap_or(0),
                action: Box::new(*action),
                action_name: run.action_name.clone(),
                outcome: None,
                calls: Vec::new(),
            });
            run.step = Some(installed.trace.steps.len() - 1);
        }
        let hooks: Vec<_> = installed
            .plan
            .hooks
            .extract_if(.., |(matcher, _)| matches_action(matcher, run, nth))
            .map(|(_, action)| action)
            .collect();
        // A named crash counts its occurrence here; `after` re-checks the
        // ordinal form only.
        let crash_now = installed
            .plan
            .crash
            .as_ref()
            .is_some_and(|crash| !crash.after && matches_action(&crash.at, run, nth));
        if let Some(crash) = installed.plan.crash.as_mut()
            && crash.after
            && let ActionAt::Named { nth: wanted, .. } = crash.at
            && matches_action(&crash.at, run, nth)
        {
            // Pin the named crash to this run's ordinal for `after`.
            crash.at = ActionAt::Ordinal {
                kind,
                run: run.occurrence,
                ordinal: run.ordinal.unwrap_or(wanted),
            };
        }
        (hooks, crash_now)
    });
    for hook in hooks {
        hook();
    }
    if crash_now {
        crash();
    }
}

/// The action site, after `execute`: record the outcome fed back, and crash
/// if a crash is addressed after this action.
pub(crate) fn after<A: Copy + Debug + 'static, O: Copy + Debug + 'static>(
    _kind: ProtocolKind,
    _action: &A,
    outcome: &O,
) {
    let (crash_now, observed) = TAP.with(|tap| {
        let mut tap = tap.borrow_mut();
        let tap = &mut *tap;
        if tap.suspended {
            return (false, None);
        }
        let (Some(run), Some(installed)) = (tap.runs.last(), tap.installed.as_mut()) else {
            return (false, None);
        };
        if let Some(step) = run.step {
            installed.trace.steps[step].outcome = Some(Box::new(*outcome));
        }
        let crash_now = installed.plan.crash.as_ref().is_some_and(|crash| {
            crash.after && matches!(crash.at, ActionAt::Ordinal { .. }) && {
                matches_action(&crash.at, run, usize::MAX)
            }
        });
        let observed = run.step.and_then(|step| {
            installed
                .plan
                .observer
                .take()
                .map(|observer| (step, observer))
        });
        if observed.is_some() {
            tap.suspended = true;
        }
        (crash_now, observed)
    });
    if let Some((step, mut observer)) = observed {
        observer(step);
        TAP.with(|tap| {
            let mut tap = tap.borrow_mut();
            tap.suspended = false;
            if let Some(installed) = tap.installed.as_mut() {
                installed.plan.observer = Some(observer);
            }
        });
    }
    if crash_now {
        crash();
    }
}

/// Whether `injection` fires for this tap point. `scope_call` is the point's
/// index in the installed scope, if any.
fn fires(
    injection: &Injection,
    call: Call,
    run: Option<&ActiveRun>,
    scope_call: Option<usize>,
) -> bool {
    match &injection.at {
        Address::Protocol {
            kind,
            run: occurrence,
            ordinal,
            call: index,
        } => run.is_some_and(|run| {
            run.serial.is_some()
                && run.kind == *kind
                && run.occurrence == *occurrence
                && run.ordinal == Some(*ordinal)
                && run.call_index == *index
        }),
        Address::Destination {
            kind,
            destination,
            action,
            call: index,
        } => run.is_some_and(|run| {
            run.kind == *kind
                && run.destination.as_deref() == Some(destination.as_path())
                && run.action_name == *action
                && run.call_index == *index
        }),
        Address::ScopeWide { call: index } => scope_call == Some(*index),
        Address::ParentSync { .. } => false,
        Address::NextInAction {
            kind,
            action,
            call: wanted,
        } => run
            .is_some_and(|run| run.kind == *kind && run.action_name == *action && call == *wanted),
    }
}

/// The call site: record one tap point and either make the call or return
/// the injected error armed for it.
pub(crate) fn call<T>(
    call: Call,
    path: &Path,
    primitive: impl FnOnce() -> io::Result<T>,
) -> io::Result<T> {
    let (injected, suspended) = TAP.with(|tap| {
        let mut tap = tap.borrow_mut();
        let tap = &mut *tap;
        if tap.suspended {
            return (None, true);
        }
        if tap.installed.is_none() && tap.legacy.is_empty() {
            return (None, false);
        }
        let scope_call = tap.installed.as_mut().map(|installed| {
            let index = installed.scope_calls;
            installed.scope_calls += 1;
            index
        });
        let mut run = tap.runs.last_mut();
        if let Some(run) = run.as_deref_mut()
            && matches!(call, Call::Rename | Call::RenameNoReplace)
        {
            run.destination = Some(path.to_path_buf());
        }
        let run = run.map(|run| &*run);
        let mut error = None;
        if let Some(installed) = tap.installed.as_mut()
            && let Some(position) = installed
                .plan
                .injections
                .iter()
                .position(|injection| fires(injection, call, run, scope_call))
        {
            error = Some(installed.plan.injections.remove(position).error);
        }
        if error.is_none()
            && let Some(position) = tap
                .legacy
                .iter()
                .position(|injection| fires(injection, call, run, None))
        {
            error = Some(tap.legacy.remove(position).error);
        }
        (error, false)
    });
    if suspended {
        return primitive();
    }
    let result = match injected {
        Some(error) => Err(error.error()),
        None => primitive(),
    };
    record(call, path, injected.is_some(), result.as_ref().err());
    result
}

fn record(call: Call, path: &Path, injected: bool, error: Option<&io::Error>) {
    TAP.with(|tap| {
        let mut tap = tap.borrow_mut();
        let tap = &mut *tap;
        let run = tap.runs.last_mut();
        let step = run.as_ref().and_then(|run| run.step);
        if let Some(run) = run {
            run.call_index += 1;
        }
        let Some(installed) = tap.installed.as_mut() else {
            return;
        };
        let record = CallRecord {
            call,
            path: path.to_path_buf(),
            injected,
            failed: error.map(io::Error::kind),
        };
        match step {
            Some(step) => installed.trace.steps[step].calls.push(record),
            None => installed.trace.loose_calls.push(record),
        }
    });
}

/// The post-content point of a write: an injected content failure, and the
/// content hook, if one is installed.
pub(crate) fn content_written(path: &Path) -> io::Result<()> {
    let hook = TAP.with(|tap| {
        let mut tap = tap.borrow_mut();
        let hook = tap
            .installed
            .as_mut()
            .and_then(|installed| installed.plan.content_hook.take());
        tap.suspended |= hook.is_some();
        hook
    });
    if let Some(mut hook) = hook {
        hook(path);
        TAP.with(|tap| {
            let mut tap = tap.borrow_mut();
            tap.suspended = false;
            if let Some(installed) = tap.installed.as_mut() {
                installed.plan.content_hook = Some(hook);
            }
        });
    }
    call(Call::ContentWritten, path, || Ok(()))
}

/// The `sync_parent_dir` site: the guardless parent-sync injection counts
/// these calls, not every directory sync.
pub(crate) fn parent_sync() -> io::Result<()> {
    let error = TAP.with(|tap| {
        let mut tap = tap.borrow_mut();
        let position = tap
            .legacy
            .iter()
            .position(|injection| matches!(injection.at, Address::ParentSync { .. }))?;
        match &mut tap.legacy[position].at {
            Address::ParentSync { succeeding } if *succeeding > 0 => {
                *succeeding -= 1;
                None
            }
            _ => Some(tap.legacy.remove(position).error),
        }
    });
    error.map_or(Ok(()), |error| Err(error.error()))
}
