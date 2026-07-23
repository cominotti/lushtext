# Close Deferred Simplification Gaps

## Why

A four-angle quality review of the guarded-payload and bounded-admission work
surfaced four maintenance hazards that were deliberately deferred because
their fixes restructure types in data-safety-critical code or shared proof
infrastructure: guarded draft-restore enums that can represent forbidden
states (policed by `unreachable!` arms at every consumer), a load-pipeline
result that carries a known-always-empty document-sized `content` field
through every layer (policed by a `debug_assert!` at destructuring), palette
note-source loops whose byte-admission construction charges are released by
eight hand-placed calls that any new early exit can silently skip, and an
accessibility-smoke warning allowlist duplicated verbatim across two embedded
Python heredocs in one script — the review itself had to patch both copies
identically, proving the next allowlist change can silently diverge. Each is
a contract enforced by vigilance instead of by structure, and each will
regress the first time it is touched without the tribal knowledge.

## What Changes

- Restructure the guarded draft-restore wrappers so the compact (non-body)
  side can no longer represent a body-carrying variant: consumers lose their
  `unreachable!` arms and the "eager bodies cross GTK only with transferable
  disposal ownership" invariant becomes type-enforced.
- Split the editor load service result into metadata and content so the
  guarded GTK-side result carries `LoadMetadata` plus `DisposalOwned<String>`
  instead of a `LoadResult` whose `content` is always empty; the
  `debug_assert!(empty_content.is_empty())` seams disappear.
- Replace the manual `release_construction(...)` calls in the palette
  note-source sidecar loops with a scope-owned charge guard so every exit
  path (admit, filter-out, `continue`, early `return`) releases exactly once
  by ownership.
- Consolidate the accessibility-smoke warning allowlist
  (`warning_line_is_allowlisted`, currently duplicated in two Python
  heredocs inside `scripts/run-accessibility-smoke.sh`) into one shared
  Python module under `scripts/`, imported by both call sites — following
  the import pattern the script already uses for
  `accessibility_source_fingerprint`.
- No user-visible behavior changes; all existing responsiveness, disposal,
  admission, and smoke-lane evidence (widget tests, coordinator snapshots,
  benchmarks, allowlist classification results) must remain green and
  unchanged in meaning.

## Capabilities

### New Capabilities

- `typed-payload-ownership-contracts`: type-level contracts for guarded
  cross-thread payloads and bounded-admission charges — illegal guarded
  states are unrepresentable, service results do not carry dead
  document-sized passenger fields, and scoped byte charges release by
  ownership rather than by manual calls on every exit path.

### Modified Capabilities

- `accessibility-keyboard-coverage`: adds a requirement that the smoke
  lane's warning allowlist classification has a single source of truth
  shared by every scan and summary path (existing warning-scan behavior is
  unchanged).
- `main-thread-responsiveness`: bounded buffer installation slices become
  paragraph-aligned, and a paragraph larger than the slice byte budget
  installs in one turn. This fixes a pre-existing crash-recovery smoke
  failure (quadratic re-layout of giant single-line drafts) discovered and
  bisected during this change's verification; see the design addendum.

## Impact

- `crates/lushtext-core/src/model/draft.rs` (or sibling model module):
  `PreloadedDraftRestore` / restore-resolution enums gain a body-free skip
  shape consumed by the guarded wrappers.
- `crates/lushtext-core/src/ui/window/drafts.rs`: `GuardedPreloadedDraftRestore`,
  `GuardedDraftRestoreResolution`, `take_preloaded_draft`, and both restore
  call sites lose `unreachable!` arms.
- `crates/lushtext-core/src/services/editor_io.rs`: `LoadResult` splits into
  metadata + content (service-internal callers and tests update).
- `crates/lushtext-core/src/ui/editor_page/{load_runtime,load_save}.rs`:
  `GuardedLoadResult` holds `LoadMetadata`; empty-content asserts removed.
- `crates/lushtext-core/src/services/palette/notes.rs`: sidecar loops use a
  construction-charge guard; the admission type gains the guard constructor.
- `scripts/run-accessibility-smoke.sh`: both Python heredocs import the
  allowlist from a new `scripts/accessibility_warning_allowlist.py` module;
  classification behavior is byte-identical.
- Tests: existing draft-restore, load-pipeline, and palette admission tests
  keep passing unchanged; new compile-time shape is covered by the type
  system itself plus small unit updates where constructors changed.
