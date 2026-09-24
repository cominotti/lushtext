// -*- mode: Bluespec; -*-
// SPDX-License-Identifier: GPL-3.0-or-later
//
// T1 calibration: the durable-write protocol, re-expressed in Quint.
//
// DISPOSABLE EVALUATION MODEL, NOT A MAINTAINED PROPERTY. The maintained
// proof is the Kani harness set in
// crates/lushtext-core/src/services/filesystem/write_protocol/kani_proofs.rs,
// which drives the real `WriteProtocol` (write_protocol.rs, `step`). This file
// transliterates that `step` and the harness's model disk so the evaluation
// (docs/next/formal-verification-quint-vs-tlaplus.md) can check whether Quint
// reproduces Kani's two known answers:
//
//   1. crash atomicity, mode non-widening, metadata-before-final-sync, and
//      classification soundness hold for the production shell;
//   2. a shell that answers `SyncTemp` with `Done` without syncing tears the
//      destination after a crash (the `should_panic` harness).
//
// Run the mutant with `--init=initSkipsTempSync`.
module write_protocol {
  // --- The protocol (write_protocol.rs) ------------------------------------

  type StepOutcome = Done | AlreadyExists | Failed
  type WriteClass = Success | BeforeRename | AfterRename
  type WriteAction =
    | ProbeMetadata
    | CreateTemp
    | WriteContent
    | ApplyMetadata
    | SyncTemp
    | Rename
    | SyncDir
    | RemoveTemp
    | Finish(WriteClass)

  /// `MAX_TEMP_NAME_ATTEMPTS`.
  pure val MaxTempNameAttempts = 8
  /// `WRITE_STEPS` in the harness: a probe, every temp-name attempt,
  /// content, metadata, sync, rename, and directory sync.
  pure val WriteSteps = MaxTempNameAttempts + 7

  /// `WriteProtocol::step`, arm for arm.
  pure def protocolStep(pending: WriteAction, attempts: int, outcome: StepOutcome): (WriteAction, int) = {
    val ok = outcome == Done
    match pending {
      | ProbeMetadata => if (ok) (CreateTemp, attempts) else (Finish(BeforeRename), attempts)
      | RemoveTemp => (Finish(BeforeRename), attempts)
      | CreateTemp =>
        val tried = attempts + 1
        match outcome {
          | Done => (WriteContent, tried)
          | AlreadyExists => if (tried < MaxTempNameAttempts) (CreateTemp, tried) else (Finish(BeforeRename), tried)
          | Failed => (Finish(BeforeRename), tried)
        }
      | WriteContent => if (ok) (ApplyMetadata, attempts) else (RemoveTemp, attempts)
      | ApplyMetadata => if (ok) (SyncTemp, attempts) else (RemoveTemp, attempts)
      | SyncTemp => if (ok) (Rename, attempts) else (RemoveTemp, attempts)
      | Rename => if (ok) (SyncDir, attempts) else (RemoveTemp, attempts)
      | SyncDir => if (ok) (Finish(Success), attempts) else (Finish(AfterRename), attempts)
      | Finish(c) => (Finish(c), attempts)
    }
  }

  // --- The model disk (write_protocol/kani_proofs.rs) ----------------------

  type Content = Empty | Old | New | Torn
  type Entry = Previous | Written
  type Shell = Production | SkipsTempSync
  type Temp = {
    data: Content,
    durableData: Content,
    metadataApplied: bool,
    durableMetadata: bool,
    destinationMode: bool,
    linked: bool,
  }
  type OptTemp = NoTemp | SomeTemp(Temp)

  var shell: Shell
  var pending: WriteAction
  var attempts: int
  var steps: int
  var probed: bool
  var temp: OptTemp
  var liveEntry: Entry
  var durableEntry: Entry
  var removalFailed: bool
  var secondTemp: bool
  /// The crash view: `Empty` until a crash, then what the name shows.
  var crashed: bool
  var crashContent: Content
  var crashMetadataIntact: bool
  /// For trace readability only.
  var lastOutcome: StepOutcome

  pure def tempOr(t: OptTemp, default: Temp): Temp = match t { | SomeTemp(x) => x | NoTemp => default }
  pure val NoInode: Temp = {
    data: Empty, durableData: Empty, metadataApplied: false,
    durableMetadata: false, destinationMode: false, linked: false,
  }
  pure def hasTemp(t: OptTemp): bool = match t { | SomeTemp(_) => true | NoTemp => false }

  // Two initializers, written out twice: Quint's TLA+ translation rejects an
  // action shared between the chosen `--init` and any other definition
  // (QNT409), so a parameterized `initWith(shell)` cannot be used.
  action init = all {
    shell' = Production,
    pending' = ProbeMetadata,
    attempts' = 0,
    steps' = 0,
    probed' = false,
    temp' = NoTemp,
    liveEntry' = Previous,
    durableEntry' = Previous,
    removalFailed' = false,
    secondTemp' = false,
    crashed' = false,
    crashContent' = Empty,
    crashMetadataIntact' = false,
    lastOutcome' = Done,
  }

  action initSkipsTempSync = all {
    shell' = SkipsTempSync,
    pending' = ProbeMetadata,
    attempts' = 0,
    steps' = 0,
    probed' = false,
    temp' = NoTemp,
    liveEntry' = Previous,
    durableEntry' = Previous,
    removalFailed' = false,
    secondTemp' = false,
    crashed' = false,
    crashContent' = Empty,
    crashMetadataIntact' = false,
    lastOutcome' = Done,
  }


  pure def isFinished(a: WriteAction): bool = match a { | Finish(_) => true | _ => false }

  /// `Disk::execute`: one backend call with a nondeterministic outcome.
  /// Returns the outcome the protocol sees.
  action execute(outcome: StepOutcome): bool = {
    val done = outcome == Done
    val t = tempOr(temp, NoInode)
    all {
      match pending {
        | ProbeMetadata => all {
          probed' = done, temp' = temp, liveEntry' = liveEntry, durableEntry' = durableEntry,
          removalFailed' = removalFailed, secondTemp' = secondTemp,
        }
        | CreateTemp => all {
          probed' = probed,
          temp' = if (done) SomeTemp({
            data: Empty, durableData: Torn, metadataApplied: false,
            durableMetadata: false, destinationMode: probed, linked: true,
          }) else temp,
          secondTemp' = (secondTemp or (done and hasTemp(temp))),
          liveEntry' = liveEntry, durableEntry' = durableEntry, removalFailed' = removalFailed,
        }
        | WriteContent => all {
          temp' = SomeTemp({ ...t, data: if (done) New else Torn }),
          probed' = probed, liveEntry' = liveEntry, durableEntry' = durableEntry,
          removalFailed' = removalFailed, secondTemp' = secondTemp,
        }
        | ApplyMetadata => all {
          temp' = SomeTemp({ ...t, metadataApplied: t.metadataApplied or done }),
          probed' = probed, liveEntry' = liveEntry, durableEntry' = durableEntry,
          removalFailed' = removalFailed, secondTemp' = secondTemp,
        }
        | SyncTemp => all {
          temp' = if (shell == SkipsTempSync or not(done)) temp
                  else SomeTemp({ ...t, durableData: t.data, durableMetadata: t.metadataApplied }),
          probed' = probed, liveEntry' = liveEntry, durableEntry' = durableEntry,
          removalFailed' = removalFailed, secondTemp' = secondTemp,
        }
        | Rename => all {
          temp' = if (done) SomeTemp({ ...t, linked: false }) else temp,
          liveEntry' = if (done) Written else liveEntry,
          probed' = probed, durableEntry' = durableEntry,
          removalFailed' = removalFailed, secondTemp' = secondTemp,
        }
        | SyncDir => all {
          durableEntry' = if (done) liveEntry else durableEntry,
          probed' = probed, temp' = temp, liveEntry' = liveEntry,
          removalFailed' = removalFailed, secondTemp' = secondTemp,
        }
        | RemoveTemp => all {
          removalFailed' = (removalFailed or not(done)),
          temp' = if (not(done)) temp
                  else if (liveEntry == Previous) NoTemp
                  else match temp { | SomeTemp(x) => SomeTemp({ ...x, linked: false }) | NoTemp => NoTemp },
          probed' = probed, liveEntry' = liveEntry, durableEntry' = durableEntry,
          secondTemp' = secondTemp,
        }
        | Finish(_) => all {
          probed' = probed, temp' = temp, liveEntry' = liveEntry, durableEntry' = durableEntry,
          removalFailed' = removalFailed, secondTemp' = secondTemp,
        }
      },
      // The mutant shell answers `SyncTemp` with `Done` whatever happened.
      val seen = if (shell == SkipsTempSync and pending == SyncTemp) Done else outcome
      val advanced = protocolStep(pending, attempts, seen)
      all {
        pending' = advanced._1,
        attempts' = advanced._2,
        lastOutcome' = seen,
      },
    }
  }

  action writeStep = all {
    not(crashed),
    not(isFinished(pending)),
    nondet outcome = oneOf(Set(Done, AlreadyExists, Failed))
    execute(outcome),
    steps' = steps + 1,
    crashed' = crashed, crashContent' = crashContent, crashMetadataIntact' = crashMetadataIntact,
    shell' = shell,
  }

  /// `Disk::after_crash`: an unsynced directory may or may not keep the
  /// rename, so the name shows either the durable or the live entry.
  action crash = all {
    not(crashed),
    nondet keepsRename = oneOf(Set(true, false))
    val entry = if (keepsRename) liveEntry else durableEntry
    val t = tempOr(temp, NoInode)
    all {
      crashed' = true,
      crashContent' = if (entry == Previous) Old else t.durableData,
      crashMetadataIntact' = if (entry == Previous) true else t.durableMetadata and t.destinationMode,
    },
    shell' = shell, pending' = pending, attempts' = attempts, steps' = steps, probed' = probed,
    temp' = temp, liveEntry' = liveEntry, durableEntry' = durableEntry,
    removalFailed' = removalFailed, secondTemp' = secondTemp, lastOutcome' = lastOutcome,
  }

  /// Terminal stutter, so a finished or crashed run is not a deadlock.
  action idle = all {
    crashed or isFinished(pending),
    shell' = shell, pending' = pending, attempts' = attempts, steps' = steps, probed' = probed,
    temp' = temp, liveEntry' = liveEntry, durableEntry' = durableEntry,
    removalFailed' = removalFailed, secondTemp' = secondTemp, crashed' = crashed,
    crashContent' = crashContent, crashMetadataIntact' = crashMetadataIntact,
    lastOutcome' = lastOutcome,
  }

  action step = any { writeStep, crash, idle }

  // --- Properties -----------------------------------------------------------

  /// Theorem 1 plus metadata-before-final-sync (`a_crash_never_tears_the_destination`).
  val crashAtomicity = crashed implies (
    (crashContent == Old or crashContent == New)
    and (crashContent == New implies crashMetadataIntact)
  )

  /// Theorem 3 (`assert_new_bytes_are_never_wider`).
  val modeNonWidening = match temp {
    | SomeTemp(t) => (t.data == New or t.durableData == New) implies t.destinationMode
    | NoTemp => true
  }

  /// The harness's "a second temp was created" assertion.
  val oneTempAtATime = not(secondTemp)

  /// The harness's "the write did not finish within WRITE_STEPS actions".
  val finishesWithinBound = steps <= WriteSteps

  val visible: Content = if (liveEntry == Previous) Old else tempOr(temp, NoInode).data

  /// Theorem 2 (`every_classification_describes_the_destination`).
  val classificationSound = crashed or match pending {
    | Finish(c) => match c {
      | BeforeRename => visible == Old and (match temp { | NoTemp => true | SomeTemp(t) => not(t.linked) } or removalFailed)
      | AfterRename => visible == New
      | Success =>
        val t = tempOr(temp, NoInode)
        visible == New and durableEntry == Written and t.durableData == New
          and t.durableMetadata and t.destinationMode and not(t.linked)
    }
    | _ => true
  }

  /// What `run_write` asserts, so what every harness (the mutant's included) checks.
  val runWriteAssertions = crashAtomicity and modeNonWidening and oneTempAtATime
    and finishesWithinBound

  val safety = runWriteAssertions and classificationSound

  // --- Witnesses (the harness's kani::cover! checks) -----------------------

  val witnessSuccess = not(crashed) and pending == Finish(Success)
  val witnessAfterRename = not(crashed) and pending == Finish(AfterRename)
  val witnessRemovalFailed = not(crashed) and isFinished(pending) and removalFailed
  val witnessCrashShowsNew = crashed and crashContent == New

  // A witness in `verify` form: "this is never reached" must be violated.
  val neverSucceeds = not(witnessSuccess)
  val neverAfterRename = not(witnessAfterRename)
  val neverRemovalFailed = not(witnessRemovalFailed)
  val neverCrashShowsNew = not(witnessCrashShowsNew)

  // --- A temporal property: every run finishes or crashes ----------------

  temporal eventuallyTerminal = eventually(crashed or isFinished(pending))
  temporal eventuallyTerminalFair = weakFair(step, (pending, crashed, attempts, steps)) implies eventuallyTerminal
}
