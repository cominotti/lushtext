# Design: Close Deferred Simplification Gaps

## Context

Three contracts introduced by the guarded-payload and bounded-admission work
are currently enforced by vigilance instead of by types:

1. `GuardedPreloadedDraftRestore::Compact(PreloadedDraftRestore)` and
   `GuardedDraftRestoreResolution::Compact(FileDraftRestoreResolution)`
   (`ui/window/drafts.rs`) wrap the *full* model enums, including the
   body-carrying `Content(String)` / `Restore { content }` variants the
   compact side must never hold. Both restore consumers and
   `take_preloaded_draft` police this with `unreachable!` arms
   ("eager bodies cross GTK only with transferable disposal ownership").
2. `GuardedLoadResult` (`ui/editor_page/load_runtime.rs`) holds
   `metadata: LoadResult` whose `content: String` field is always drained
   into the sibling `content: DisposalOwned<String>` before transfer. The
   dead passenger rides the whole pipeline and `load_save.rs` destructures it
   as `content: empty_content` guarded by `debug_assert!(empty_content.is_empty())`.
3. The palette note-source sidecar loops (`services/palette/notes.rs`)
   charge construction bytes via `try_charge_construction` /
   `admit_parsed_sidecar` and release them through eight hand-placed
   `release_construction(bytes)` calls, one per exit path. A new early exit
   silently leaks the charge until `complete()`, skewing
   `current_construction_bytes` and the peak evidence for the rest of the
   load.
4. `scripts/run-accessibility-smoke.sh` embeds `warning_line_is_allowlisted`
   verbatim in two separate Python heredocs (the summary composer near line
   180 and the final warning scan near line 1760). The quality review's own
   ANSI-stripping fix had to be applied to both copies identically; the next
   allowlist edit can silently land in only one, making the scan and the
   summary disagree about the same warning line.

All four were flagged by an adversarial quality review and deferred because
the fixes restructure types in draft-restore and load-pipeline code where a
mistake loses user data, or reshape shared proof infrastructure.

## Goals / Non-Goals

**Goals:**

- Make the compact/guarded payload split type-enforced: delete every
  `unreachable!` arm and empty-content `debug_assert!` that exists only to
  reject states the types should not represent.
- Make construction-charge release exhaustive over exit paths by
  construction, not by convention.
- Give the accessibility-smoke warning allowlist one importable source of
  truth so scan and summary classification cannot diverge.
- Keep every existing behavior requirement, test assertion, coordinator
  snapshot meaning, benchmark, and allowlist classification unchanged.

**Non-Goals:**

- No changes to draft-restore policy (stale/oversized/lazy-budget decisions),
  load slicing, disposal-lane behavior, or admission byte limits.
- No serialization-format changes (the affected enums are in-memory only).
- No widening or narrowing of the warning allowlist's entries, and no
  restructuring of the smoke script beyond extracting the shared predicate
  (the other embedded heredocs stay as they are).

## Decisions

### D1: Split skip shapes out of the body-carrying model enums

Introduce body-free skip enums in `model/draft.rs`:

- `PreloadedDraftSkip { StaleFile, Oversized, LazyAggregateBudget }`
- `FileDraftRestoreSkip { Stale, Oversized, Unavailable, MissingDraft }`

and re-shape the model enums as
`PreloadedDraftRestore { Content(String), Skip(PreloadedDraftSkip) }` and
`FileDraftRestoreResolution { Restore { content: String }, Skip(FileDraftRestoreSkip) }`.
The guarded wrappers become
`GuardedPreloadedDraftRestore { Content(DisposalOwned<String>), Compact(PreloadedDraftSkip) }`
(and the resolution twin), so the compact case is body-free by type.

*Alternative considered:* keep the model enums flat and define the skip
enums only beside the guarded wrappers with `From` conversions. Rejected:
`take_preloaded_draft` would still need a partial re-match (the second
`let ... else { unreachable!() }`), because classification and body removal
would remain two separate reads of the same map entry. Nesting the skip
shape in the model enum lets one `match` move the body or copy the skip.

### D2: Split `LoadResult` into `LoadMetadata` + content at the service boundary

`services/editor_io.rs` gains
`pub struct LoadMetadata { size, size_check, canonical_path, mtime, encoding_state, has_bom, file_health }`
and `LoadResult` becomes `{ pub metadata: LoadMetadata, pub content: String }`.
`GuardedLoadResult` holds `metadata: LoadMetadata` +
`content: DisposalOwned<String>`; the worker moves `result.content` into
disposal ownership and passes `result.metadata` through untouched. The
`empty_content` destructuring and its `debug_assert!` disappear.

*Alternative considered:* leave `LoadResult` unchanged and have
`GuardedLoadResult` copy out the scalar fields it needs. Rejected: the
metadata bundle is cohesive and already flows as a unit to editor-state
apply; duplicating seven fields invites drift, and the service signature
should say what callers actually receive.

### D3: Closure-scoped construction charge, not an RAII drop-guard

The admission ledger is a plain struct mutated through `&mut self` methods,
and the loop bodies keep calling `admission.admit(...)` while the charge is
outstanding. An RAII guard holding `&mut admission` would alias that borrow;
one holding shared interior mutability would need to restructure the
ledger's fields. Instead, add a closure-scoped helper on the admission
type:

```rust
fn with_construction_charge<T>(
    &mut self,
    bytes: u64,
    body: impl FnOnce(&mut Self) -> ControlFlow<T, ()>,
) -> ChargeOutcome<T>
```

The helper charges (or reports budget exhaustion), runs `body` with the
re-borrowed admission, and releases the charge on every return path —
including `ControlFlow::Break` values that model the loops' `continue` /
`return Ok(admission.complete())` exits. The three sidecar loops (folder
notes, scoped document notes, open-tab document notes) route their
parse-charge/filter/admit/release choreography through it.

*Alternative considered:* RAII guard with `Rc<Cell<u64>>` construction
counter. Rejected: converts deterministic scalar accounting into shared
mutable state and changes the ledger's field types only to serve a test-free
convenience; the closure shape keeps ownership single-threaded and explicit.

### D4: Shared allowlist module imported by both heredocs

Extract `warning_line_is_allowlisted` into
`scripts/accessibility_warning_allowlist.py` and import it from both
heredocs. The summary heredoc already receives `REPO_ROOT` and does
`sys.path.insert(0, str(repo_root / "scripts"))` to import
`accessibility_source_fingerprint`, so the same mechanism serves the
allowlist; the warning-scan heredoc gains the repo-root argument it
currently lacks. The module keeps the ANSI-stripping line and both entry
patterns byte-identical to today's copies.

*Alternative considered:* replacing both heredocs with one standalone
Python script invoked twice. Rejected: the two call sites do different jobs
(summary composition vs scan-and-fail), and merging them would restructure
the script's flow for no classification benefit; sharing only the predicate
is the minimal single-source fix.

### D5: Sequence the work behind the existing evidence

Each decision lands as its own commit with the full existing test suite
green in between: D1 (draft enums), D2 (load result split), D3 (charge
scope), D4 (allowlist module). The draft-restore widget tests
(`test_startup_restore_*`, `test_document_sized_preloaded_draft_*`), the
load-pipeline slice tests, and the palette admission metrics tests already
assert the behavior these refactors must preserve; no assertion values may
change.

## Risks / Trade-offs

- [Draft enum reshape touches restore persistence paths] → The enums are
  in-memory only (manifest JSON has its own serde types); grep confirms no
  serde derive on `PreloadedDraftRestore` / `FileDraftRestoreResolution`.
  Verify by running the crash-recovery smoke lane in addition to widget
  tests.
- [`LoadResult` is a public service type; splitting it churns service tests]
  → All callers are in-crate; update mechanically and lean on the existing
  result-equivalence tests (chunked vs reference decode) which compare both
  metadata and content.
- [Closure-scoped helper obscures the admit path] → Keep the helper small
  and local to `palette/notes.rs`'s admission impl; the loops' filtering
  logic stays inline in the closure so readers still see the flow in place.
- [Behavioral drift hidden by refactor] → No assertion, snapshot field, or
  benchmark ID changes are allowed in the same commits; any test edit is
  limited to constructor shape.
- [Allowlist extraction changes classification subtly] → The module body is
  copied byte-identically from the current predicates; prove equivalence by
  rerunning `make accessibility-smoke` and diffing the run's
  `unexpected-warnings.txt` / summary `warning_status` against a pre-change
  run on the same host. New `scripts/*.py` content may also shift the
  accessibility source fingerprint — rerun the fingerprinted lanes rather
  than hand-editing summaries.

## Migration Plan

Internal refactor, no data migration. Land as four reviewable commits in
the order D1 → D2 → D3 → D4 (independent; any subset can ship). Rollback is
`git revert` per commit.

## Implementation Addendum: Pre-existing Crash-Recovery Blocker (D1 verification)

Task 1.5's `make crash-recovery-smoke` run failed on clean `main` as well —
`recovery-restore-complete` timed out "blocked by draft-autosave". Bisection
pinned the regression to `a280672` ("perf: complete end-to-end boundedness"),
which replaced draft-restore's single `set_text` with byte-bounded sliced
installs. GTK text layout validates whole paragraphs, so appending 256 KiB
slices into the smoke fixture's 33 MB single-line draft re-laid-out the whole
growing paragraph on every slice (measured ~3 s per slice, quadratic overall);
the restore never finished, and the held disposal reservation starved the lazy
restore queue. Per `preexisting-blockers.md` the fix landed in this change:
`next_replacement_boundary` now ends every slice just after a newline and
installs a paragraph larger than the byte budget in one turn (the same
one-time layout cost its first render already pays), and both clear paths
extend deletion to the next line start. See the `main-thread-responsiveness`
delta spec in this change for the modified requirement.

## Open Questions

- Whether `ChargeOutcome` should also carry the settled-bytes evidence the
  metrics tests want to assert directly, or whether the existing
  `peak_construction_bytes` / `current_construction_bytes` metrics remain
  sufficient (default: keep existing metrics only).
