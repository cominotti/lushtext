# Formal Verification — Programme Record

Status: **active**. Phase 0 is **complete** (OpenSpec change
`formal-verification-phase-0`, implemented and archived 2026-09-23 as
`openspec/changes/archive/2026-09-23-formal-verification-phase-0`). Phases 1–5
and the K5, K7, and K8 candidates of
[`formal-verification-evolution.md`](./formal-verification-evolution.md) were
carried by the OpenSpec change `consolidate-formal-verification-on-kani`
(2026-09-23), and each phase below records its posture and every proof result.
Decided by the maintainer on 2026-09-23.

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
- Kani is the only candidate that needs **no bridge to the code**. A model in
  another specification language re-expresses the system, and every such
  model then needs its own bridge back to Rust.

How Kani covers each target:

| Target | With Kani |
|---|---|
| Pure geometry and policy functions | harnesses over the real functions |
| ViewportSliceBin closed loop | Rust step model calling the real functions; the child is `kani::any()` constrained by `kani::assume` (the adversarial envelope); N bins, k steps |
| Draft journal interleavings and crashes | pure Rust journal decision core that production drives; each step is a nondeterministic action, including `Crash` |
| Durable-write crash atomicity | I/O-free Rust core that production drives. The protocol is finite, so exploring every event sequence is a **complete** proof, not merely a bounded one |

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
are modelled as nondeterministic choice in a sequential model.

**Lean is dormant.** It returns only if a claim genuinely needs to be
unbounded, for example "any number of bins" as evidence for publishing GTK
Lush, and only by a recorded maintainer decision. No other specification
language is planned.

Tool facts as of September 2026:

- Kani 0.68 pins its own nightly, independent of this workspace's toolchain
  pin. In the phase-0 spike, `cargo kani -p gtk-lush-widgets` compiled next to
  the 1.96 pin in 31 s.
- Kani function contracts and loop contracts are still experimental.
- Harnesses live in GTK-free code behind `#[cfg(kani)]`.

## 3. Phases

**Final lane run (2026-09-23, `make kani`, all five shards, Kani 0.68.0 /
CBMC 6.11.0, this toolbox): 27 harnesses, 0 failures, 40 min 29 s wall**
(while other lanes shared the machine). Per shard: widgets-geometry 9
harnesses; widgets-slice-loop-rest 3 (longest `slice_loop_rests_with_three_bins`
310.1 s); widgets-slice-loop-requests 6 (longest
`slice_loop_honours_a_request_with_two_bins` 369.5 s);
core-journal-and-write 8 (longest `journal_invariants_hold_under_crashes`
470.4 s); core-second-writer 1 (532.7 s). The per-phase tables below give
each harness's own time.


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

**Status: complete (K2, 2026-09-23).** The ledger, A1–A13 with statements,
dependent designs, and pinning status, now lives at
[`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`](../../.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md)
and is no longer duplicated here. New isolated probes in
`crates/lushtext/tests/widget/gtk_axioms.rs` pin the four previously unpinned
axioms against real GTK 4.22:

- A5 `test_axiom_a5_zero_height_allocation_rewrites_the_hosts_adjustment` —
  a zero-height `GtkListView` rewrote a published value of 1200 to 34 and the
  page from 300 to 0, emitting `value-changed`.
- A9 `test_axiom_a9_value_changed_drops_a_pending_scroll_to` — a
  `scroll_to(200)` alone reaches the row; a 10px `set_value` before the next
  allocation leaves the list at the nudge.
- A11 `test_axiom_a11_adjustments_clamp_and_skip_unchanged_values`.
- A13 `test_axiom_a13_an_in_layout_scroll_does_not_schedule_a_relayout` — the
  same outer move re-allocates a re-slicing widget from an idle, and does not
  from inside its `size_allocate`.

A8 is backed by the phase-0 instrumentation (39 requests, 0 settles, no
`Defer` across 48 tests) and recorded as not isolable by a probe; A10 and A12
record why they are not pinned separately.

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

**Status: K1 landed (2026-09-23, `consolidate-formal-verification-on-kani`).**
`#[cfg(kani)] mod kani_proofs` in `gtk-lush-widgets`, run by `make kani`
(pinned `KANI_VERSION=0.68.0`) and by the scheduled/manual
`.github/workflows/kani.yml`. `cfg(kani)` is a declared expected cfg in the
workspace lints. Results, Kani 0.68.0 / CBMC 6.11.0, this toolbox:

| Harness | Domain | Result | Time |
|---|---|---|---|
| `no_input_panics` | any `f64`, all three decisions | PROVED | 0.35 s |
| `whole_pixel_band_stays_inside_the_widget` | any `f64` slice, any `i32` height (the bin's `i32::clamp`) | PROVED | 0.42 s |
| `slice_lies_inside_content_on_whole_pixels` | `i32` pixels, overscan 0..=4096 | PROVED | 29.3 s |
| `slice_covers_the_visible_intersection_on_whole_pixels` | `i32` pixels, overscan 0..=4096 | PROVED | 176.9 s |
| `slice_containment_fails_for_general_f64` | finite `f64` | `should_panic`: counterexample found, as documented | 0.63 s |
| `resting_bin_never_requests` | any `f64` | PROVED | 0.33 s |
| `requests_are_at_least_epsilon` | any `f64` | PROVED | 0.08 s |
| `request_lands_exactly_on_whole_pixels` | `i32` pixels, `u16` shift | PROVED | 5.07 s |
| `request_landing_fails_for_general_f64` | any `f64` | `should_panic`: counterexample found, as documented | 0.12 s |

Whole lane: 9 harnesses, 0 failures, 4 min 11 s wall including the crate's
GTK dependency build. The two general-`f64` harnesses are **kept** as
`should_panic` counterexamples rather than removed, so a future rustdoc claim of
a broader domain has to confront them. The rustdoc of `viewport_slice`,
`classify_child_scroll`, and `outer_scroll_request` and the
`gtk-lush-viewport-slice` spec now state the whole-pixel domain; behaviour is
unchanged (design D3). The lane grew past one CI job's 30-minute cap once
phases 3–5 landed, so `scripts/kani-shards.py` now owns a five-shard table
(`widgets-geometry`, `widgets-slice-loop-rest`, `widgets-slice-loop-requests`,
`core-journal-and-write`, `core-second-writer`): `make kani` runs every shard,
`kani.yml` runs one per matrix job, and `make check-kani-shards` (in
`make check-policy`) fails when a harness matches no shard or several. The shard
times on GitHub runners are measured, and `widgets-geometry` is in the
pull-request gate (below, `harden-kani-lane-and-draft-token`). The editor-memory, minimap, window-geometry, and
`clamped_preview_width` harnesses listed above were not part of this change and
remain open candidates.

**Runner budgets and the pull-request gate (2026-09-23,
`harden-kani-lane-and-draft-token`).** `make kani KANI_MEASURE=<json>`
(`scripts/kani-shards.py run --measure`) records each shard's `cargo kani` wall
time, the peak resident memory of its largest descendant (per-child `wait4`
rusage), and every harness's `Verification Time:`; `kani.yml` always runs in
that mode, writes a job-summary table, and uploads one `kani-measure-<shard>`
artifact. Four `workflow_dispatch` runs on `ubuntu-latest` (4 vCPU, 16 GB,
Fedora 44 container), Kani 0.68.0:

- A `35920992670`: cold Kani install cache;
- B `35923346671`: warm install cache;
- C `35925626107`: warm install, `target/kani` cache added (cold);
- D `35927734943`: warm install, warm `target/kani` cache.

| Shard | Gate | Wall A / B / C / D (min) | Peak A / B / C / D (GiB) | Job, longest (min) | Recorded budget |
|---|---|---|---|---|---|
| `widgets-geometry` | pull-request | 8.16 / 7.62 / 6.45 / 7.41 | 1.74 / 1.69 / 1.70 / 0.11 | 9.96 | 8.2 min, 1.8 GiB |
| `widgets-slice-loop-rest` | scheduled | 14.60 / 13.01 / 14.69 / 9.86 | 1.73 / 1.69 / 1.69 / 1.26 | 16.21 | 14.7 min, 1.8 GiB |
| `widgets-slice-loop-requests` | scheduled | 14.82 / 15.10 / 16.02 / 14.22 | 1.73 / 1.69 / 1.69 / 1.21 | 17.51 | 16.1 min, 1.8 GiB |
| `core-journal-and-write` | scheduled | 19.40 / 20.81 / 19.67 / 14.47 | 8.27 / 8.26 / 8.27 / 8.26 | 22.20 | 20.9 min, 8.3 GiB |
| `core-second-writer` | scheduled | 16.53 / 16.30 / 17.28 / 13.90 | 8.81 / 8.81 / 8.80 / 8.78 | 18.78 | 17.3 min, 8.9 GiB |

- The recorded budget in the shard table is the largest figure across the four
  runs, rounded up. The task asked for the larger of A and B; C and D only
  raise it (`widgets-slice-loop-rest`, `widgets-slice-loop-requests`,
  `core-second-writer`), so the table is at least that conservative.
- Every shard fits the margins that `make check-kani-shards` now enforces:
  25 minutes and 12 GiB per shard, and 15 minutes for a `pull-request` shard.
  **No shard was split, no solver changed, and no bound reduced** (the design's
  fit order was not needed). The job adds about 1.5–2 minutes around the shard
  (container, `dnf`, caches); the longest job, 22.2 minutes, is inside the
  30-minute cap.
- The peak is the Kani compiler's (about 1.7 GiB building GTK dependencies) for
  the widget shards and CBMC's for the core shards. With a warm target cache
  (run D) nothing is compiled, so the widget peaks drop to CBMC's own.
- The runner's CBMC is about 1.8x slower than the toolbox: the longest
  harness, `journal_invariants_hold_under_crashes`, took 835 s in run A against
  487 s locally, and `slice_covers_the_visible_intersection_on_whole_pixels`
  328 s against 177 s. Run-to-run CBMC variance on the runner is about ±1.5
  minutes per shard, which is why a single run is not a budget.
- **Kani install cache: adopted.** Keyed `kani-<KANI_VERSION>-fedora-44`; it
  holds `cargo-kani`, `kani`, `~/.kani`, and the nightly `cargo kani setup`
  installs through rustup (the bundle links to it). About 379 MB. Cold, the
  install took 19–22 s plus a 9 s save; warm, an 8 s restore and no install.
  The gain is small because `kani-verifier` downloads a prebuilt bundle rather
  than building from source; the cache is kept because it is version-keyed,
  small, and removes two network downloads from every job.
- **`target/kani` cache: rejected.** The five per-shard caches totalled about
  650 MB (90–189 MB each), inside the 5 GB limit, but a warm cache cut the
  `widgets-geometry` build share from 1.17–1.57 minutes to 0.18: a saving under
  1.5 minutes, below the 3 minutes the design required, and smaller than the
  runner's CBMC variance (the warm job D took 8.80 minutes against the
  cold-cache jobs' 7.61–9.96). The step was removed and its cache entries
  deleted.
- **Pull-request gate.** `kani.yml` now also runs on `pull_request` and on
  pushes to `main`, and those events run only the `pull-request` shards
  (`github-outputs` emits them as `pr-shards`): `widgets-geometry`, 9 harnesses.
  The recommendation for the maintainer (a repository setting, not code) is to
  mark `Kani Proof Harnesses (widgets-geometry)` as a required check. Failing
  first: on pull request #41, a scratch commit (never merged, then dropped from
  the branch) widened `viewport_slice`'s clamp by one pixel, and the
  pull-request run `35929586005` ran only `widgets-geometry` and failed it,
  with `slice_lies_inside_content_on_whole_pixels` reporting the counterexample
  `slice.top + slice.height <= content.max(0.0)`.
- **Harness modules are verification code.** A harness module must be named
  `kani_proofs.rs` and declared `#[cfg(kani)] mod kani_proofs;`.
  `.cargo/mutants.toml` excludes `crates/**/kani_proofs.rs`: `make mutants-list`
  had listed 171 mutants in the two harness modules (138 in
  `draft_service/kani_proofs.rs`, 33 in `write_protocol/kani_proofs.rs`), which
  no test build compiles, plus 1 in `draft_service/fixture.rs`, now excluded
  like its filesystem sibling. The scope went from 6,001 to 5,829 mutants,
  exactly 172, with no other file's count changing. `make
  check-workflow-boundaries` skips a gated `kani_proofs.rs` in its
  decision-logic and role-home rules and fails an ungated or orphaned one, so
  harnesses over `ui/**/policy.rs` (N2) need no ledger entry.

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

**Status: complete (K4, 2026-09-23, `consolidate-formal-verification-on-kani`).**
The step model is `slice_loop` in `crates/gtk-lush/widgets/src/kani_proofs.rs`.
Each allocation calls the real `viewport_slice`, `whole_pixel_band`, and
`classify_child_scroll` in the order `ViewportSliceBin::size_allocate` does,
and applies the bin's own publish / learn-inset / write-back / defer / request
bookkeeping. The child is `kani::any()` restricted only by `kani::assume`
clauses citing ledger axioms (A4–A13). No widget code changed. Results, Kani
0.68.0 / CBMC 6.11.0, this toolbox:

| Harness | Scope | Result | Time |
|---|---|---|---|
| `slice_loop_rests_with_one_bin` | N=1, no input | PROVED: fixed point within two allocations, no oscillation, drawn row = intended | 37.5 s |
| `slice_loop_rests_with_two_bins` | N=2 | PROVED | 154.9 s |
| `slice_loop_rests_with_three_bins` | N=3 | PROVED | 328.7 s |
| `slice_loop_honours_a_request_with_one_bin` | N=1, one request | PROVED: lands where the re-slice republishes the child's value, or clamps at an end, within two frames | 87.8 s |
| `slice_loop_honours_a_request_with_two_bins` | N=2 | PROVED | 366.3 s |
| `slice_loop_learning_frame_request_beyond_the_inset_is_honoured` | first allocation, divergence > inset + 1 | PROVED | 35.6 s |
| `slice_loop_learning_frame_request_within_the_inset_is_erased` | first allocation, divergence ≤ inset | `should_panic`: counterexample, the recorded learning-frame residual | 13.3 s |
| `slice_loop_one_reconfiguring_allocation_leaves_the_outer_alone` | one allocation with a geometry correction and a settle within it | PROVED | 17.2 s |
| `slice_loop_two_reconfiguring_allocations_can_move_the_outer` | two consecutive such allocations | `should_panic`: counterexample, a new residual | 18.8 s |

Envelope assumptions narrower than the ledger, recorded as the ledger requires:
the child's content height does not depend on the outer position or on which
rows are realized (A12 holds only approximately; drift is modelled as a
reconfiguring child); a settle happens only in an allocation whose geometry
the child corrected (A7) and never exceeds that correction (A8); a settle does
not survive into a stable-geometry frame (A8, modelled literally: a held settle
re-anchors on the published offset); only a bin whose band is non-zero can
request (A5). Integer abstraction as above; the landing lemma it relies on is
`request_lands_exactly_on_whole_pixels` (phase 2).

**Both residuals are decided: recorded, not fixed** (design D6). Each is a
real counterexample of the model, and neither is reachable through any real
consumer, so no failing-first widget test can be written for either:

- *Learning-frame residual.* A request applied in the very first allocation,
  within the inset the bin is learning, is indistinguishable there from the
  learning-frame settle (~3px measured on a `navigation-sidebar` list) and is
  erased (A9). Forwarding it instead would scroll `child - viewport_top`
  (~58px) and hide the header on first show. Reachability evidence: the phase-0
  instrumentation of 48 widget tests saw 39 requests, **0 settles and no
  `Defer`**, and no request in a learning frame; ledger A8 records that the
  discriminator cannot be isolated by a probe. Kept as the `should_panic`
  harness above, which fails the day a change makes the case honoured or
  widens it.
- *Two consecutive reconfiguring allocations* (found by K4). Two allocations
  that each correct the child's geometry and each settle within their own
  correction can add up to a divergence larger than the second correction;
  the bin then classifies it as a request and scrolls the chrome above the
  bin away. Reachability evidence: a settle needs the child to re-estimate
  its content height, and both real consumers (the LushText workspace tree
  and the GTK Lush adoption lab) use uniform single-line rows, whose estimate
  never corrects; the phase-0 instrumentation saw no settle at all.
  **Candidate fix, evaluated in the model only:** while a held divergence
  exists and the geometry is still moving, bound the settle by
  `shift + |held − published|` rather than `shift`. With that change the model
  proves `slice_loop_two_reconfiguring_allocations_can_move_the_outer`
  (63.0 s) and keeps the one-bin rest, request, learning-frame, and
  one-reconfigure harnesses proved. It widens what is deferred, so a genuine
  request made while a settle is held would wait one more allocation; that
  trade-off and a real-GTK failing-first test are what adopting it needs.

Still open from this phase's plan: simultaneous requests from two bins in one
frame (the concurrent-request double-count; the harnesses above issue one
request at a time) and the adaptive-shell breakpoint loop.

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

**Status: complete (K3, 2026-09-23, `consolidate-formal-verification-on-kani`).**
The decision core is `crates/lushtext-core/src/services/draft_service/journal_core.rs`
(a service-level home, because `draft_service` drives it too). Every decision
site of the service and of `ui/window/drafts/` routes through it: body
ownership, the body-write decision, registration, delete-while-pending
preservation, the serialized deletion steps, startup stale retirement, the six
unapplied-restore dispositions, orphan-cleanup eligibility, and commit
authority. A body write takes the `RegisteredDraft` token that only
registration mints; insert-only commits run inside the one manifest command;
`services/draft_service/set_aside.rs` owns the set-aside bytes.

The harness `services/draft_service/kani_proofs.rs` drives the **real** core
over a model disk with 3 ids, content ids for up to 3 edits (so 3 generations
per id), and 8 nondeterministic actions after the first startup — Edit,
Register, WriteBody, Commit, Save, Discard, DeletionStep, Inspect,
ExecCleanup, RestoreApply, ExternalMtime, Crash, Startup — with an I/O fault
possible on every step (`unwind(9)`). Results, Kani 0.68.0 / CBMC 6.11.0:

| Harness | Property | Result | Time |
|---|---|---|---|
| `journal_invariants_hold_under_crashes` | S1–S4 after every step | PROVED | 487 s (about 8 GB) |
| `a_dirty_editor_becomes_clean_without_faults` | L1 at k = 7 | PROVED | 57 s |
| `a_dirty_editor_may_need_seven_steps` | L1 at k = 6 | `should_panic`: counterexample, so k = 7 is tight | 53.2 s |

L1's bound is 7, not the design's 6: a pending restore that turns out stale
(1), its preserve / delete-body / remove-entry retirement (3), then register,
write, and commit (3).

What the harness found, all fixed failing-first:

- a model bug in `RemoveEntry` trust, fixed in the model;
- **a real loss path:** an `Unavailable` restore (the file vanished before a
  lazy restore resolved) released the autosave hold without keeping the body,
  so the next autosave, or a Discard, destroyed a body the user never saw.
  S1 counterexample: Startup → RestoreApply(Unavailable) → Discard →
  DeletionStep. It now keeps a set-aside copy; widget test
  `test_an_unavailable_restore_keeps_the_unshown_draft_before_autosave_replaces_it`.

The data-safety audit of the refactor then found two more, both fixed
failing-first:

- **the production hold did not match the proved model:** when the set-aside
  copy of an unrestored body failed, production released the restore hold
  anyway, so the next autosave replaced the only copy. The hold now stays and
  each autosave tick retries the copy (`unrestored_copy_retries` in
  `DraftEvidence`); widget test
  `test_a_failed_set_aside_copy_keeps_autosave_off_the_unshown_draft`;
- **a preserved draft opened from `Preferences > Data` was never draft-dirty**
  (the bounded install suspends the handlers that mark it), so deleting its
  set-aside copy before typing left it in no draft at all. It is now marked and
  scheduled; `test_data_page_opens_a_preserved_draft_in_a_new_untitled_tab_and_keeps_it`.

Known unmodelled assumption: one process per data directory (A6); K8 below
drops it.

**Token closed (2026-09-23, `harden-kani-lane-and-draft-token`).** The
`RegisteredDraft` token no longer has a production bypass: both
`draft_service::fixture` (its `write_body`) and `services::filesystem::fixture`
(whose writers could put bytes at `drafts/<id>.draft` directly) compile only
under `#[cfg(any(test, feature = "test-utils"))]`, `property-tests` implies
`test-utils`, and every benchmark command passes `--features test-utils`.
`make check-filesystem-boundary` checks both gates and that `test-utils` is
neither a default feature nor enabled by any shipping build (the `lushtext`
`[dependencies]`, `build-aux/cargo.sh`, the Flatpak manifest, Snap, Meson), with
a self-test; CI's lint job checks the shipped binary with default features
(`cargo check -p lushtext --bins --locked`), because the all-features Clippy
gate cannot see a production caller. Failing first: the rule failed on the
ungated tree, and a scratch call to `draft_service::fixture::write_body` in
`ui/window/drafts/journal.rs` compiled before the gate and after it failed with
``error[E0433]: cannot find `fixture` in `draft_service` ``. The deferral
inventory entry for `write_body` is closed.

#### K8 — dropping axiom A6

**Status: complete; decision: accept with documentation, no lock in this
change.** `a_second_writer_breaks_the_journal_invariants` adds a second
LushText process with its own editors and in-memory journal over the same
disk (`step_as`). With 8 actions after both startups (1017.5 s, about 18 GB,
17 min wall) Kani fails two checks, each decoded by concrete playback:

- **S1, accepted work lost.** Both processes restored the same draft id.
  Process A edits it, writes its body, and commits (accepted); A crashes;
  process B, whose tab still shows the older restore, discards the draft and
  its deletion steps remove the body A had just committed. Trace: Startup(A),
  Startup(B), A Edit, A Register, A WriteBody, A Commit, A Crash, B Discard,
  B DeletionStep.
- **A body written without an entry.** B edits and registers the id; A
  discards it and its deletion removes the shared manifest entry; B then
  writes its body with the token its own registration minted. Trace:
  Startup(A), Startup(B), B Edit, A Discard, B Register, A DeletionStep,
  B ExternalMtime, B RestoreApply, A DeletionStep, B WriteBody.

Both need **two processes with the same draft id open over one data
directory**. Production does not create that: LushText is a unique
`GApplication`, so a second launch in the same session forwards its files to
the running instance and exits; a Flatpak and a host build use different data
directories. It takes two separate D-Bus sessions of one user sharing a home
(two graphical logins, or a nested or remote session) running LushText at
once. That is documented here and in the deferral inventory rather than fixed
blind; an inter-process data-directory lock (`flock` on a lock file in the
data directory, with a read-only or refuse-to-start second instance) is the
follow-up change to open if that use is ever reported. The harness stays as a
`should_panic` pin, run at 6 actions after both startups (500.8 s, about
9 GB, which a CI runner holds; the body-without-entry counterexample is still
found): the day it passes, A6 is no longer needed and this record changes.

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

**Status: complete (K6, 2026-09-23, `consolidate-formal-verification-on-kani`),
built on the K7 split of `copy_durable` (keeps its source) and `move_durable`
(removes it only after the destination is durable).** The core is
`crates/lushtext-core/src/services/filesystem/write_protocol.rs`:
`WriteProtocol`, `MoveProtocol`, and `RenameProtocol`, each
`step(outcome) -> next action`. `services/durable_write.rs` is now only the
shell loop, one backend call per action with its outcome fed back unchanged;
streaming closures stay in the shell. The existing durable-write unit and
fault-injection tests, plus three new characterization tests (content,
rename, and probe failure), are the shell's evidence.

The harnesses (`write_protocol/kani_proofs.rs`) drive the **real** core
against a `cfg(kani)` disk abstraction — one destination name, its previous
inode, and the temp inode — with POSIX crash semantics: unsynced bytes, and
metadata applied after the last sync, may be anything after a crash (A2), and
a rename whose directory was not synced may or may not survive (A1, A3). Every
backend outcome is `kani::any()` (including `AlreadyExists` for temp creation,
A5), and a crash may land before any step. Results, Kani 0.68.0 / CBMC 6.11.0:

| Harness | Property | Result | Time |
|---|---|---|---|
| `a_crash_never_tears_the_destination` | theorem 1, plus mode non-widening (theorem 3) and metadata-before-final-sync at every step | PROVED | 2.27 s |
| `every_classification_describes_the_destination` | theorem 2: `BeforeRename` ⇒ old visible, `AfterRename` ⇒ new visible, success ⇒ new durable; a failed write leaves a temp only when its removal failed | PROVED | 3.45 s |
| `skipping_the_temp_sync_tears_the_destination` | the same with a shell that answers `SyncTemp` without syncing | `should_panic`: torn-destination counterexample | 2.11 s |
| `a_move_removes_its_source_only_after_the_copy_is_durable` | move safety | PROVED | 0.06 s |
| `a_completed_rename_synced_every_directory_it_mutated` | `rename_durable` syncs both parents across directories | PROVED | 0.06 s |

`kani::cover!` checks confirm the proved branches are reachable (a successful
write, a failed directory sync, a failed temp removal, a post-crash new
destination, a completed move, a completed cross-directory rename). Theorem 4,
guarded-delete safety, is not a property of this core: orphan-cleanup deletion
is S2 of phase 4. A4 (sticky fsync errors) needs no modelling, because every
failed sync already ends the protocol.

### K5 — Deterministic kill points in the crash-recovery smoke

**Status: complete.** `services/kill_point.rs::reach`, behind the smoke-only
`crash-kill-points` feature, aborts the real process inside
`draft-body-before-commit`, `durable-renamed-before-dirsync`, and
`stale-preserved-before-retire`. `make crash-recovery-smoke` builds that binary
into `target/crash-kill-points` and runs one scenario per window, each
relaunching the ordinary binary and requiring no lost work: all three passed
(2026-09-23, `assertions/kill-points.json`). A fresh release build contains no
kill-point code (`strings target/release/lushtext` finds neither
`LUSHTEXT_KILL_AT` nor the abort message; the feature binary finds both), and
no Meson, Flatpak, or Snap build passes the feature.

Next candidates after the Kani consolidation are ranked in
[`formal-verification-next.md`](./formal-verification-next.md).

## 4. Deferral inventory

- Multi-process safety of the draft journal and the target guard (axiom A6).
  K8 showed two concrete losses when two processes open the same draft over
  one data directory (phase 4, K8); accepted with documentation because a
  unique `GApplication` makes that need two D-Bus sessions of one user. The
  follow-up, if ever reported, is an inter-process data-directory lock. The
  target guard is process-local, and cleanup gating is per window.
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
  browsable version, not the content. `services/draft_service/set_aside.rs`
  owns those bytes and `Preferences > Data` lists them with Open and Delete
  (K7); local history stays a separate copy rather than a zero-copy view,
  because a view needs a local-history index change. The set-aside area still
  has no size bound.
- Phase 0 audit: the workspace same-target leftover sweep trusts the local
  clock against a file server's mtime. Across hosts sharing a workspace with
  ≥24 h clock skew, another host's in-flight temp for the same target could
  look stale; that host's rename then fails `BeforeRename` with its target
  intact (no loss). Multi-host workspaces sit outside the single-process
  axiom (A6).
- Slice-bin residuals, **decided by K4 and recorded, not fixed** (phase 3
  above): a request within the inset in the learning frame is erased, and two
  consecutive reconfiguring allocations with settles can be forwarded as an
  unrequested scroll. Both are Kani counterexamples kept as `should_panic`
  harnesses; neither is reachable through a real consumer (0 settles across
  the phase-0 instrumentation; uniform rows in both consumers). The second has
  a model-checked candidate fix. Revisit when a consumer with variable-height
  rows adopts `ViewportSliceBin`, starting from a failing real-GTK test.
- Phase 0: a lineage index repaired from snapshot files derives each
  timestamp from the snapshot id (capture time), so a preserved stale draft's
  `saved_at_secs` stamp does not survive an index repair.
- Phase 0: the startup leftover sweep skips the legacy folder-note sidecar
  directory kept for older releases, and a pass over a directory larger than
  its budget may leave leftovers for a later pass.
- `cargo-gtk-proof` has no sidebar or slice-bin scenario. The screenshot lane
  is waiting on a `reveal-workspace-path` automation action.
