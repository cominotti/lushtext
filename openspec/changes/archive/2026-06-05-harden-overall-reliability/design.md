## Context

LushText already routes app-owned persistence through durable writes, preserves file metadata, keeps draft/session state isolated under the app data directory, and has strong widget/property/fuzz/smoke coverage. The reliability gaps left by that work are mostly cross-cutting:

- `json_store::load()` correctly returns errors for malformed JSON, but startup restore currently defaults session and draft manifest failures to empty state in `draft_service::load_restore_state()`.
- Sidecar migrations for bookmarks, document notes, workspace notes, and local history run after the user-visible rename succeeds. If a sidecar rewrite or cleanup fails, the user gets a warning, but there is no durable retry state for restart reconciliation.
- Close-time draft flushing is strict, while session save failures are logged without a durable retry flag or visible warning.
- Tests cover many restore and safety scenarios in-process, but there is no smoke lane that kills the real app process and verifies recovery after relaunch.

The design keeps the existing filesystem boundary and `spawn_blocking_then` model. It adds shared service-layer primitives and small UI/reporting adapters instead of letting each feature invent its own recovery behavior.

```
             app-owned reliability state
             ===========================

  session.json     drafts/manifest.json     sidecars/indexes
      │                    │                     │
      └────────────┬───────┴────────────┬────────┘
                   ▼                    ▼
        recovery_metadata service   migration_ledger service
                   │                    │
                   └─────────┬──────────┘
                             ▼
                startup diagnostics + repair summary
                             │
                             ▼
              status / inline warnings / smoke artifacts
```

## Goals / Non-Goals

**Goals:**
- Preserve evidence when recovery metadata is malformed, unreadable, or only partially repairable.
- Keep startup non-destructive: missing or bad metadata must not erase surviving drafts, sidecars, local-history snapshots, or session files merely because restore could not parse one file.
- Provide user-visible and test-visible diagnostics for session, draft, sidecar, local-history, and transient recovery-journal problems.
- Add retryable, crash-safe migration state for sidecars and local-history lineage moves after in-app renames.
- Reduce the initial draft crash-loss window after first modification while preserving chunked snapshots and background writes for large buffers.
- Add a real-process crash/restart smoke lane with isolated app state, diagnostic artifacts, and explicit skip/fail behavior.
- Add unit, integration, widget, property or fuzz-adjacent, smoke, and performance coverage where each level is useful.

**Non-Goals:**
- No attempt to guarantee zero-keystroke loss after kernel panic or power loss before the app has captured any text.
- No user-facing manual JSON repair editor.
- No migration away from JSON for existing app-owned state.
- No cross-process write coordination beyond the existing process-local target guards.
- No broad rewrite of notes/bookmarks/local-history UI.
- No default PR requirement for host-sensitive crash/restart smoke until it proves stable and cheap enough.

## Decisions

### 1. Add a recovery metadata service instead of widening `json_store`

Introduce a small service module, tentatively `services::recovery_metadata`, that wraps selected loads with richer outcomes:

```rust
enum RecoveryLoad<T> {
    Loaded(T),
    MissingDefault(T),
    QuarantinedDefault {
        default: T,
        original_path: PathBuf,
        quarantine_path: PathBuf,
        reason: RecoveryProblem,
    },
    Partial {
        value: T,
        diagnostics: Vec<RecoveryDiagnostic>,
    },
}
```

The shared `json_store::load/save` contract stays simple and strict. Callers that need recovery behavior opt into the new wrapper and must inspect diagnostics. This avoids silently changing every JSON caller while creating one clear place for quarantine naming, diagnostic formatting, file-size limits, and durability requirements.

Alternatives considered:
- Change `json_store::load` to always quarantine: too broad; many callers need a hard error.
- Keep per-service ad hoc handling: repeats policy and makes future repair behavior drift.

### 2. Quarantine by durable rename/copy, then default or repair

When a present metadata file cannot be parsed or is a non-file path, the service will try to move it under a deterministic app-owned quarantine directory, for example:

```
$XDG_DATA_HOME/lushtext/recovery-quarantine/
  2026-06-05T12-22-10Z-session-json-<hash>.json
```

Quarantine must use the filesystem write/mutation boundary. If moving the file is not possible, the app must leave the original untouched, report that quarantine failed, and avoid writing a replacement until the user has been warned or a successful later repair makes the situation safe.

Repair is conservative:
- Missing metadata returns defaults without diagnostics.
- Malformed `session.json` defaults to empty restore only after the original is quarantined or explicitly preserved.
- Malformed `drafts/manifest.json` defaults to an empty manifest only after preserving evidence and scanning draft files for recoverable untitled or file-backed candidates when possible.
- Malformed sidecar files are hidden from normal lists after quarantine, but a diagnostic remains visible.
- Malformed local-history `index.json` can be rebuilt only from intact snapshot files and snapshot metadata encoded in filenames or recoverable adjacent data; otherwise the lineage is quarantined, not deleted.

Alternatives considered:
- Fail startup on malformed metadata: preserves evidence but makes the editor unavailable.
- Overwrite malformed files immediately: convenient but destroys the best debugging and recovery evidence.

### 3. Return startup restore diagnostics to the window

`draft_service::load_restore_state()` should return a struct instead of a tuple, for example:

```rust
struct RestoreState {
    manifest: DraftManifest,
    session: SessionData,
    preloaded_drafts: HashMap<String, PreloadedDraftRestore>,
    diagnostics: Vec<RecoveryDiagnostic>,
}
```

The window applies restore as it does today, then publishes one summarized status or inline notification when diagnostics exist. Detailed diagnostics go to tracing and smoke artifacts. Startup must not show one alert per corrupt sidecar; the UI should group them into clear messages like "Some recovery data could not be loaded. The original files were preserved for inspection."

Alternatives considered:
- Surface each service failure directly from the background worker: noisy and harder to test.
- Keep diagnostics only in logs: users lose trust because visible state changed with no explanation.

### 4. Add a durable pending-migration ledger for sidecars and local history

After a successful in-app rename, create a compact ledger entry before starting sidecar migration:

```json
{
  "generation": 7,
  "old_path": "...",
  "new_path": "...",
  "kinds": ["bookmarks", "document-notes", "workspace-notes", "local-history"],
  "created_at_secs": 1780000000,
  "attempts": 0
}
```

Each kind records completion independently. Startup reconciliation loads pending entries and retries incomplete kinds before normal browse surfaces rely on sidecar lists. Successful completion removes the entry durably. Partial failures remain bounded by attempt count and are reported in status/logs.

This ledger belongs in the service layer so bookmark/document-note/workspace-note/local-history logic can share retry and cleanup behavior. Individual services still own their sidecar formats and move logic.

Alternatives considered:
- Make the file/directory rename wait for all sidecar migrations before returning: stronger atomic UX but risks blocking the UI flow on large history stores or slow disks.
- Ignore failures and rely on later manual repair: too easy to leave annotations detached from files forever.

### 5. Reconcile duplicate or orphaned sidecar state conservatively

Startup reconciliation should detect these cases:
- A new sidecar exists and the old sidecar cleanup failed.
- Both old and new sidecars exist with overlapping records.
- A local-history target lineage exists and an old lineage directory remains.
- A pending ledger entry references paths that no longer exist.

The default policy is:
- Prefer the sidecar identity matching the current path.
- Merge records when record identities make merging deterministic.
- Never delete the only copy of a non-empty sidecar without a successful write of the merged target.
- Quarantine corrupt duplicates rather than dropping them.
- Report unresolved duplicates as diagnostics instead of hiding them.

### 6. Make session-save failure visible and retryable

Session persistence should keep a window-local failure state when debounced or close-time saves fail. A later tab open/close/switch/detach or an explicit close attempt should retry with the newest generation. Close-time session save failure should be visible through the status bar or close-flow warning, while not blocking close if dirty drafts and modified documents have already reached safe states.

Session save remains less critical than draft/file save: losing session layout is bad, but it must not trap the user in a window that cannot close after draft safety has succeeded.

### 7. Add first-dirty draft autosave without one long GTK tick

Keep the 5-second periodic autosave timer, but schedule a short first-dirty debounce when an editor transitions from clean-draft to dirty-draft. The first-dirty path reuses the existing autosave machinery:

- If an autosave is already in flight, set the existing pending flag.
- Small buffers may snapshot directly.
- Large buffers use the existing chunked snapshot path.
- Failed writes leave `draft_dirty` true for retry.
- The production debounce should be long enough to coalesce a burst of typing but short enough to shrink the current 5-second first-recovery window.

Tests should use the existing test-only `autosave_tick_for_test` and a narrow timing override rather than sleeping for production intervals.

### 8. Add a real-process crash/restart smoke lane

Create a script, tentatively `scripts/run-crash-recovery-smoke.sh`, that follows existing smoke conventions:

1. Build or receive a debug binary path.
2. Create an isolated state directory with `smoke_prepare_isolated_state`.
3. Launch LushText inside an isolated desktop session.
4. Use a deterministic driver to create:
   - a file-backed modified draft,
   - an untitled modified draft,
   - a saved session with multiple tabs and selected tab,
   - at least one bookmark or note sidecar if feasible.
5. Wait for recovery metadata to exist or invoke a controlled test action.
6. Terminate the process abruptly with `SIGKILL`.
7. Relaunch with the same isolated data directory.
8. Assert restored content, selected tab/session state, diagnostic absence or expected warning state, and no unexpected GTK/GIO/accessibility warnings.

The first implementation may use debug/test-only actions or a small driver helper rather than coordinate-based input. The important property is that it crosses a real process boundary and verifies disk state after restart.

The lane should be available locally through `make crash-recovery-smoke` and scheduled/manual CI as part of end-user smoke once stable. Host-sensitive tooling belongs on the host when it needs compositor/session access; build/test CLI helpers can stay inside the toolbox.

### 9. Testing strategy is layered by risk

Unit/service tests:
- Recovery metadata load outcomes for missing, valid, malformed, non-file, quarantine-success, quarantine-failure, and repairable cases.
- Migration ledger create/update/remove ordering and stale-generation behavior.
- Service reconciliation for duplicate bookmark/note sidecars and local-history lineages.
- Session save failure retry state and ordered-generation preservation.

Integration tests:
- `load_restore_state` returns diagnostics while preserving session/draft evidence.
- Corrupt draft manifest with intact draft files produces recoverable candidates or explicit diagnostics.
- Pending sidecar migration retries after simulated failure and removes ledger after success.
- Local-history migration merge keeps snapshots and prunes only after durable target index write.

Widget tests:
- Startup recovery diagnostics surface one grouped visible warning.
- Session-save failure status is visible and clears after retry.
- First-dirty autosave does not block interaction and marks failed drafts retryable.
- Notes/bookmarks browser remains usable when one sidecar is corrupt and reports partial results.

Property/fuzz-adjacent tests:
- Generated bounded malformed JSON bytes never panic recovery loaders.
- Generated sidecar duplicate sets reconcile deterministically.
- Generated migration ledger state machines do not delete the last non-empty copy before a durable target exists.

Smoke/performance:
- Real-process crash/restart smoke verifies recovery across process boundaries.
- Performance smoke records startup repair cost for bounded corrupt metadata sets.
- Responsiveness smoke or widget timing proves first-dirty autosave does not reintroduce large synchronous snapshots.

## Risks / Trade-offs

- Recovery diagnostics could become noisy -> group messages by category and keep detailed paths in logs/artifacts.
- Quarantine can fail on read-only or damaged data directories -> preserve originals, avoid replacement writes, and surface explicit diagnostics.
- Repair logic could accidentally invent incorrect state -> prefer quarantine plus partial restore over aggressive reconstruction.
- Pending migration ledgers add another app-owned state file -> route through durable JSON writes and include it in recovery-metadata coverage.
- Faster first-dirty autosave increases I/O churn -> debounce, reuse dirty-generation guards, skip clean/evicted editors, and keep the periodic timer as the backstop.
- Real-process smoke may be flaky under headless desktops -> use existing smoke skip/fail helpers, stable actions, generous waits, preserved artifacts, and keep it scheduled/manual until proven stable.
- Confined crash/restart coverage may be limited by Flatpak/Snap runtime availability -> require clear skips and never count unsupported lanes as verified.

## Migration Plan

1. Land service-level recovery metadata primitives and tests without changing startup behavior.
2. Convert session/draft startup restore to return diagnostics and preserve malformed metadata.
3. Add pending sidecar migration ledgers and reconciliation for bookmark/document-note/workspace-note/local-history flows.
4. Add UI/status integration for grouped diagnostics and retry visibility.
5. Add first-dirty draft autosave using existing chunked snapshot and dirty-generation machinery.
6. Add crash/restart smoke locally, then wire it into scheduled/manual end-user smoke after local stability.
7. Refresh stale developer docs and OpenSpec canonical specs.

Rollback strategy: because this change only adds app-owned metadata and diagnostics, rollback should leave quarantined originals and pending ledgers as inert files. Older builds may ignore them. The implementation must avoid changing existing session/draft/sidecar formats in a way that older builds cannot read unless a compatibility fallback is explicitly added.

## Open Questions

- What should the production first-dirty autosave debounce be: 500ms, 1s, or another value?
- Should close-time session-save failure ever block close, or only warn and retry while the window is still alive?
- What is the user-facing affordance for viewing quarantine details: status-only, logs-only, or a future diagnostics dialog?
- Should crash/restart smoke drive the app via debug actions, accessibility events, or a small dedicated test binary?
