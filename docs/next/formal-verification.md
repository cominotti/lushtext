# Formal Verification — Programme Record

Status: **active**. Phase 0 is **complete** (OpenSpec change
`formal-verification-phase-0`, implemented and archived 2026-09-23 as
`openspec/changes/archive/2026-09-23-formal-verification-phase-0`). The ranked
next moves and the measured tool feasibility spike live in
[`formal-verification-evolution.md`](./formal-verification-evolution.md). Phases 1–5 are planned
and have no change yet. Decided by the maintainer on 2026-09-23.

## 1. Motivation

The goal is correctness of the program as a whole. That means data safety
first, and also the geometry work that consumed months of troubleshooting in
2026 and led to GTK Lush. A proof only closes one of the two gaps a bug can
come from:

```
   The real world (GTK, Mutter, the filesystem, the user)
          │  ◄── gap A: "is our model of the environment right?"
          ▼
   Specification
          │  ◄── gap B: "does the code meet the spec?"   ← a proof closes this
          ▼
   Rust code
```

Most LushText geometry bugs came from gap A:

- d8e57861 — GtkListView realizes at most about 200 rows.
- 129a7e61 — a geometry settle was forwarded as a request.
- ecf2c791 — content-box versus border-box frames.
- the reverted `child − published` attempt — GtkListBase drops a pending
  `scroll_to` on any `value-changed`.

Only 78af12c7's resting-bin request was code violating its own spec.

The programme therefore does two things:

1. It turns gap A into an explicit, finite **axiom ledger**, and pins every
   axiom against real GTK.
2. It models the environment as an **adversarial envelope**: "the child may
   move its value by up to k whenever geometry changes", not "the child
   behaves the way we believe". A model built on believed behaviour proves the
   bug correct. Checked against an envelope, 129a7e61 fails in two steps.

## 2. Tool: Kani only (decided 2026-09-23, supersedes the pragmatic mix)

**Current decision.** Kani is the programme's single formal tool. The testing
stack the project already runs covers the rest: proptest, cargo-fuzz, and the
headless widget harness, whose probes pin the GTK axioms.

Why one tool:

- Every extra tool costs a toolchain, a CI lane, version upkeep, a skill, and
  a mental model. For a single maintainer, that fixed cost outweighed the
  marginal fit of a per-target "best tool".
- Kani is the only candidate that needs **no bridge to the code**. Quint and
  Lean re-express the system in another language, and every such model then
  needs its own bridge back to Rust (differential testing, or Aeneas
  extraction).

How Kani covers what the mix assigned to other tools:

| Target | Earlier assignment | With Kani |
|---|---|---|
| Pure geometry and policy functions | Kani | unchanged |
| ViewportSliceBin closed loop | Rust model plus proptest | Rust model; the child is `kani::any()` constrained by `kani::assume` (the adversarial envelope); N bins, k steps |
| Draft journal interleavings and crashes | Quint | pure Rust journal state machine; each step is a nondeterministic action, including `Crash` |
| Durable-write crash atomicity | Lean 4 | I/O-free Rust core. The protocol is finite, so exploring every event sequence is a **complete** proof, not merely a bounded one |

What this gives up:

- Unbounded proofs. The draft journal is checked for small scopes, for example
  up to 3 ids, 3 generations, and 8 actions. This relies on the small-scope
  hypothesis, and crash-consistency studies find most bugs within 3
  operations.
- Unbounded liveness. It becomes bounded liveness: L1 reads "clean within k
  steps".
- Collection-heavy models. They must use fixed arrays instead of `HashMap` or
  `Vec`.

Kani runs no real threads. That does not matter here, because interleavings
are modelled as nondeterministic choice in a sequential model, the same way
TLA+ and Quint work.

**Lean is dormant, and Quint is dropped.** Lean returns only if a claim
genuinely needs to be unbounded, for example "any number of bins" as evidence
for publishing GTK Lush. The Lean spike's micro-model was ported to Kani in
minutes and got stronger (see the evolution record).

**Superseded decision** (2026-09-23, earlier the same day): a pragmatic mix of
Kani for pure functions, a Rust model for the slice bin, Quint for drafts, and
Lean for the durable write. It optimised per-target fit and ignored per-tool
fixed cost.

Tool facts as of September 2026:

- Kani 0.68 pins its own nightly, independent of this workspace's toolchain
  pin. In the phase-0 spike, `cargo kani -p gtk-lush-widgets` compiled next to
  the 1.96 pin in 31 s.
- Kani function contracts and loop contracts are still experimental.
- Harnesses live in GTK-free code behind `#[cfg(kani)]`.

## 3. Phases

### Phase 0 — Fix what the reading found (`formal-verification-phase-0`)

Writing the subsystems down as state machines surfaced these defects:

1. **Draft journal wedge.** A crash between a new file-backed body write and
   the single post-batch manifest commit leaves an ambiguous body. Every later
   update then fails as `Partial`, and the body is never offered.
2. **ViewportSliceBin drops a request.** A request suppressed by
   `reconfigure_shift` is erased by the settle write-back instead of being
   deferred. This is latent in the sidebar and reachable with variable-height
   rows.
3. **Local-history copy fallback.** Migration falls back to a copy on any
   rename error, including after a rename that took effect.
4. **Leftover temp files.** Durable-write leftovers are never swept.
5. **Bare relative paths.** The parent of a bare relative path is `""`, so
   `openat("")` fails.
6. **Stale drafts deleted.** Stale file-backed drafts are deleted without
   confirmation.

Exit: every defect is fixed, each with a test that failed before its fix.

**Status: complete (2026-09-23).** Each defect's failing-first test:

1. Wedge: `unregistered_path_hash_body_is_attributed_from_session_and_journal_stays_trusted`,
   `unattributable_path_hash_body_is_set_aside_and_journal_stays_trusted`
   (integration, failed with `Partial`), and the widget test
   `test_new_file_backed_drafts_are_registered_before_their_first_body_write`
   (failed: "written without a registered entry"). Fixed by write-ahead
   registration of file-backed ids (with an additive, non-authoritative
   fallback while the journal is untrusted), session-hash attribution
   (entries rebuilt with `UNPROVEN_BACKING_MTIME_SECS`), and
   `drafts/set-aside/` for unattributable bodies.
2. Swallowed request: `gtk_lush_adoption::test_adoption_slice_bin_honours_a_request_made_while_the_child_reconfigures`
   (synthetic `GtkScrollable`; neither the sidebar nor a variable-height
   `GtkListView` reached the case — `reconfigure_shift` stayed 0). Fixed by
   `classify_child_scroll` → `ChildScrollDecision::Defer`, which holds the
   child's value for one re-allocation.
3. Copy fallback: `move_path_tree_parent_sync_failure_after_rename_fails_retryably_without_copy`.
   Fixed by `filesystem::write::is_cross_device` (`EXDEV` only).
4. Leftovers: `saving_a_document_clears_its_own_stale_leftovers` plus the
   `filesystem::leftovers` predicate tests. Startup sweep of app-data
   directories off GTK; same-target sweep after workspace writes.
5. Bare relative path: `atomic_write_to_bare_relative_name_syncs_current_directory`
   and `missing_ancestors_of_a_relative_path_stop_before_the_empty_prefix`.
   Fixed by `parent_or_current`.
6. Stale drafts: `stale_file_draft_is_preserved_as_periodic_local_history_snapshot`
   and `stale_draft_for_file_outside_local_history_policy_is_set_aside`
   (integration); the alert's Show in Local History action is covered by
   `test_startup_restore_skips_stale_file_backed_draft_once` and
   `test_lazily_opened_stale_draft_is_kept_in_local_history_before_retirement`.

### Phase 1 — GTK axiom ledger

This phase writes a normative ledger of the GTK behaviour that the
ViewportSliceBin and adaptive-geometry designs depend on. Each entry is pinned
by an isolated widget probe.

| Id | Axiom | Pinned today |
|---|---|---|
| A1 | GtkListView realizes at most 200+2 rows per range; "visible" means its own vadjustment | yes |
| A2 | a list outside a scroller reports its whole content as its minimum height | indirectly |
| A3 | GtkViewport allocates a non-scrollable child its minimum | indirectly |
| A4 | the list applies `scroll_to` and focus scrolling inside its own allocate | positive controls |
| A5 | a zero-height allocation makes the list rewrite the host's adjustment | **no** |
| A6 | a GtkScrollable works in its CSS content box, so `page = alloc − inset` | indirectly |
| A7 | the child re-derives its value from its scroll anchor when page or upper change | indirectly |
| A8 | a settle is bounded by the geometry correction that caused it, and does not survive into a stable-geometry frame | **partially** (phase 0: across all 48 slice-bin widget tests, instrumented, real consumers produced 39 requests, 0 settles, and no `Defer`; the only `Defer` came from the synthetic probe — so no discriminator was needed) |
| A9 | GtkListBase drops a pending `scroll_to` on any `value-changed` | only via a focus-traversal test |
| A10 | rows around focus and selection stay realized but unmapped | by probe discipline |
| A11 | `configure`/`set_value` clamp, and an unchanged `set_value` emits nothing | **no** |
| A12 | the child's natural height is independent of the outer position | not pinned; only approximately true |
| A13 | moving the outer adjustment inside layout does not reliably schedule a relayout | **no** |

Exit: every axiom has an isolated probe, or is recorded as unpinnable with a
reason. The ledger lives beside the gtk4-libadwaita-internals references and is
cited by the Phase 3 model.

### Phase 2 — Kani lane

- Add a `make kani` target, and an optional CI job using
  `model-checking/kani-github-action`.
- Write harnesses for:
  - no panic for any `f64`: the `f64::clamp` min > max case in
    `slice_geometry.rs:80` and the i32 clamp in `imp.rs`;
  - exact landing, `t + d == child` bit for bit;
  - band ≤ visible height;
  - band never zero while content exceeds the viewport;
  - the invariants of `editor_memory`, the fit functions in `minimap/policy.rs`,
    `window/geometry/policy.rs`, and the width-preset clamps.
- Also: `clamped_preview_width` violates its "≤ 1/3" rule when the available
  width is below 3·MIN. Either state that in its spec or fix it.

Exit: harnesses run in CI or a documented lane, and their counterexamples are
triaged.

### Phase 3 — ViewportSliceBin closed-loop model (Kani)

This is a state-machine model:

- **State:** outer value, and per bin the origin, content height, inset,
  published offset, child value, anchor, pending intent, and pending delta.
- **Step:** it calls the real `viewport_slice` and `outer_scroll_request`,
  with the child as an adversarial envelope drawn from the ledger.
- **Scope:** N ∈ {1, 2, 3} bins, integer abstraction (sound because
  allocations are integer and `f64` add/sub is exact below 2^53).
- **Properties:**
  - fixed point at rest within 2 frames;
  - no oscillation for any N;
  - request fidelity, including outer clamping, rounding, and the band-height
    jump when t crosses 0;
  - liveness: every request is honoured or clamped;
  - render fidelity.
- Also: the concurrent-request double-count across bins.

Candidate follow-on: the adaptive-shell breakpoint loop in
`window/geometry/policy.rs`.

### Phase 4 — Draft journal model (Kani)

A pure Rust journal state machine, checked with Kani, covers two or three ids of mixed kind, generations up to
about 3, and about 18 actions, including Crash and Startup. Invariants:

- **S1 acceptance durability:** an accepted generation stays recoverable
  until discard, save, or a stale file.
- **S2 cleanup safety:** a delete happens only under the lock and guard, with
  the id not in the manifest, the inode matching, the inventory complete, and
  no write in flight.
- **S3 delete ordering:** delete intent never coexists with a present body
  and a missing manifest entry.
- **S4 trust:** a trusted manifest means the last reconciliation was complete.
- **L1 liveness:** without I/O faults, an open dirty editor eventually
  becomes clean. This is false before phase 0, because of the wedge.

The state machine is the journal's decision core, extracted out of the service
and GTK coordination and used by them, so the harness checks production logic.
It is not a separate model. L1 becomes bounded: "clean within k steps".

Known unmodelled assumption: one process and one window per data directory.

### Phase 5 — Crash-atomicity of durable_write (Kani)

The I/O-free `WriteProtocol` core covers states S0–S6, the copy fallback, and
`rename_durable` over a volatile/durable disk abstraction. A thin shell
executes each action through `sys::`. Kani explores every event and crash
sequence of the finite protocol.

POSIX axioms:

- A1: `rename` is atomic.
- A2: `fsync(file)` persists data and metadata.
- A3: `fsync(dir)` persists the namespace.
- A4: sticky fsync errors.
- A5: `O_EXCL` is atomic.
- A6: a single process.
- A7: a cross-directory rename is one transaction.

Theorems:

1. **Crash atomicity:** after any crash the target is old or new, never
   partial.
2. **Classification soundness:** `BeforeRename` implies the old content is
   visible. This breaks under the NFS axiom, which is known.
3. **Mode non-widening.**
4. **Guarded-delete safety,** assuming unique inodes. Dropping that
   assumption exposes the inode ABA gap.

The Lean spike's micro-model and its two theorems were already ported to Kani,
along with a stronger third result, so this phase adopts them against the real
core rather than a model.

## 4. Deferral inventory

- Multi-process and multi-window safety of the draft journal and the target
  guard. The guard is process-local, and cleanup gating is per window.
- NFS rename-retransmit misclassification (`BeforeRename` while the new bytes
  are live).
- suid/sgid are cleared by `fchown` after `fchmod`. This is documented as
  best-effort.
- The target guard is not prefix-aware: a directory rename can race a save
  inside it. That causes a leak, not a loss.
- Inode ABA in orphan cleanup. It compares only the inode, and is mitigated by
  the manifest reload.
- A failed body delete is never re-queued, so the discarded draft resurrects
  after restart. This is documented as intentional.
- An untitled draft absent from `session.json` is registered but never offered.
- Stale file-backed drafts are preserved as `Periodic` local-history snapshots
  (phase 0). Those snapshots remain subject to ordinary retention; exempting
  them would need an index format change. The byte-identical copy phase 0 also
  keeps in `drafts/set-aside/` is never pruned, so retention costs the
  browsable version, not the content. The set-aside area has no size bound or
  cleanup UI yet.
- Phase 0 audit: the workspace same-target leftover sweep trusts the local
  clock against a file server's mtime. Across hosts sharing a workspace with
  ≥24 h clock skew, another host's in-flight temp for the same target could
  look stale; that host's rename then fails `BeforeRename` with its target
  intact (no loss). Multi-host workspaces sit outside the single-process
  axiom (A6).
- Phase 0 slice-bin residual: the **learning frame** (the first allocation
  after the bin learns the child's content-box inset) still republishes the
  offset instead of holding a divergence, because holding the measured ~3px
  settle there would forward as a ~58px scroll that hides the header on first
  show. A `scroll_to` applied in that very first allocation could therefore
  still be erased. Telling the two apart needs a discriminator (A8), and phase
  0 found no evidence the case is reachable; revisit with the phase 3 model.
- Phase 0: a lineage index repaired from snapshot files derives each
  timestamp from the snapshot id (capture time), so a preserved stale draft's
  `saved_at_secs` stamp does not survive an index repair.
- Phase 0: the startup leftover sweep skips the legacy folder-note sidecar
  directory kept for older releases, and a pass over a directory larger than
  its budget may leave leftovers for a later pass.
- `cargo-gtk-proof` has no sidebar or slice-bin scenario. The screenshot lane
  is waiting on a `reveal-workspace-path` automation action.
