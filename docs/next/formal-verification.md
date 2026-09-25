# Formal Verification — Programme Record

Status: **active**. Phase 0 is **complete** (OpenSpec change
`formal-verification-phase-0`, implemented and archived 2026-09-23 as
`openspec/changes/archive/2026-09-23-formal-verification-phase-0`). Phases 1–5
and the K5, K7, and K8 candidates of
[`formal-verification-evolution.md`](./formal-verification-evolution.md) were
carried by the OpenSpec change `consolidate-formal-verification-on-kani`
(2026-09-23), and each phase below records its posture and every proof result.
Decided by the maintainer on 2026-09-23. `extend-closed-loop-geometry-verification`
(2026-09-25) extended phase 3 (two-bin requests, the A7/A8 envelope revisited
against real consumers, the adaptive-shell breakpoint loop, and both in-Kani
unbounded attempts); the lane is now 92 harnesses in twelve shards.

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

**Lean is dormant.** An unbounded claim is first attempted inside Kani, twice:
with Kani's loop contracts, and with a compositional argument (small harnesses
plus a written proof note that extends a bounded proof to any size). Lean, or
any other tool, is re-discussed only if **both** attempts fail **and** an
unbounded claim is actually needed (for example "any number of bins" as
evidence for publishing GTK Lush), and only by a recorded maintainer decision.
`extend-closed-loop-geometry-verification` made both attempts for the slice
loop (phase 3): loop contracts failed on resources, and the compositional
argument carried the claim within stated per-bin bounds, so no other tool is
proposed. No other specification language is planned.

Tool facts as of September 2026:

- Kani 0.68 pins its own nightly, independent of this workspace's toolchain
  pin. In the phase-0 spike, `cargo kani -p gtk-lush-widgets` compiled next to
  the 1.96 pin in 31 s.
- Kani function contracts and loop contracts are still experimental.
- Loop contracts in the pinned Kani 0.68.0 (checked 2026-09-25 against its
  `docs/src/reference/experimental/loop-contracts.md` at tag `kani-0.68.0` and
  by compiling harnesses with the installed 0.68.0 / CBMC 6.11.0): they need
  `-Z loop-contracts` on the command line and
  `#![feature(stmt_expr_attributes, proc_macro_hygiene)]` on the crate (under
  `cfg_attr(kani, ...)`), and provide `#[kani::loop_invariant(..)]`,
  `#[kani::loop_decreases(..)]` (integer measures only; struct-field
  projections and multi-dimensional measures are known-broken, as is combining
  it with `loop_modifies`), `#[kani::loop_modifies(..)]`, and `on_entry(..)` /
  `prev(..)`, on `while`, `loop`, and `for` over ranges, arrays, slices, and
  common iterator adaptors (not `while let`). Loop modifies are inferred by
  alias analysis. A shard enables the flag through its `kani_flags` entry in
  `scripts/kani-shards.py`, which no other shard receives.
- Harnesses live in GTK-free code behind `#[cfg(kani)]`.

**Measured on 2026-09-24.** The OpenSpec change
`evaluate-quint-and-tlaplus-empirically` compared Quint and TLA+ with Kani
on three of this project's own targets, and on nine exploration ideas (E1–E9). The
record is
[`formal-verification-quint-vs-tlaplus.md`](./formal-verification-quint-vs-tlaplus.md).

- **Kani stays the only maintained formal tool.**
- **TLA+ with TLC** is the recorded tool for the disposable N6 design sketch,
  if that trigger ever fires.
- **Quint is not adopted,** and cannot replace Kani.
- **A narrow exception to the Kani-only rule** lets the disposable evaluation
  models stay in `formal/evaluation/`. They sit outside every gate, and no
  claim here rests on them (maintainer decision D7, option A).

## 3. Phases

**Final lane run (2026-09-23, `make kani`, all five shards, Kani 0.68.0 /
CBMC 6.11.0, this toolbox): 27 harnesses, 0 failures, 40 min 29 s wall**
(the lane has since grown to 74 harnesses in seven shards; see N2 in phase 2)
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
and is no longer duplicated here. Isolated probes pinned the four previously
unpinned axioms against real GTK 4.22 (first in LushText's widget binary; since
`extract-gtk-lush-axiom-samples`, 2026-09-24, in the GTK Lush family crate
`gtk-lush-axioms`, which added probes for A1, A2, A3, A4, A6, and A7, one
interactive/`--check` sample per probe, `make gtk-axioms` in CI, and
`make check-gtk-axioms` keeping ledger and crate in agreement; the four moved
probes measured identical values before and after the move):

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

The new probes also measured two behaviours the ledger did not record
(2026-09-24, GTK 4.22.5 and the GNOME 50 SDK's 4.22.2):

- **A1's cap is 205, not 202.** `GtkListView` realized rows 0–204 both at rest
  and in a viewport needing 294 rows; the ledger's "200 + 2" was imprecise. The
  first host `set_value` past the realized rows is also not followed: the list
  keeps its old anchor and maps no row until the next `value-changed`.
  Recorded, not asserted, because no design relies on it.
- **A7's alignment is not confined to `[0, 1]`.** After a host `set_value`, the
  list re-derived its value along `value = anchor − align × page` with
  align −4.42, so a 100 px page change moved it 442 px. That contradicts A8's
  "a settle is bounded by the correction that caused it" as a GTK guarantee:
  A8 stays an envelope assumption of the slice-bin model, to be revisited by
  `extend-closed-loop-geometry-verification`, whose A14–A18 probes are written
  in `gtk-lush-axioms`.

A8 is backed by the phase-0 instrumentation (39 requests, 0 settles, no
`Defer` across 48 tests) and recorded as not isolable by a probe; A12 records
why it is not pinned. A10 was rewritten from measurement on 2026-09-25 and now
has its own probe: its old statement ("off-screen rows stay realized but
unmapped") hid that a `GtkListView` also **maps** rows wholly outside its own
allocation, so rendered-row tests intersect mapped rows with the viewport. The
same change pinned A19 (a `GtkViewport` notifies `page-size` only after
allocating its child) and A20 (a host `set_value` leaves the list's anchor
outside `[0, 1]` until the value is re-announced after allocation).

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
`clamped_preview_width` harnesses listed above were not part of this change;
they landed in `extend-kani-to-pure-policies` (N2, below).

**Runner budgets and the pull-request gate (2026-09-23,
`harden-kani-lane-and-draft-token`).** `make kani KANI_MEASURE=<json>`
(`scripts/kani-shards.py run --measure`) records each shard's `cargo kani` wall
time, the peak resident memory of its largest descendant (per-child `wait4`
rusage), and every harness's `Verification Time:`; `kani.yml` always runs in
that mode, writes a job-summary table, and uploads one `kani-measure-<shard>`
artifact. Five `workflow_dispatch` runs on `ubuntu-latest` (4 vCPU, 16 GB,
Fedora 44 container), Kani 0.68.0:

- A `35920992670`: cold Kani install cache;
- B `35923346671`: warm install cache;
- C `35925626107`: warm install, `target/kani` cache added (cold);
- D `35927734943`: warm install, warm `target/kani` cache;
- E `35930757261`: the final run, warm install cache, `target/kani` cache
  removed (the shipped configuration).

| Shard | Gate | Wall A / B / C / D / E (min) | Peak A / B / C / D / E (GiB) | Job, longest (min) | Recorded budget |
|---|---|---|---|---|---|
| `widgets-geometry` | pull-request | 8.16 / 7.62 / 6.45 / 7.41 / 7.70 | 1.74 / 1.69 / 1.70 / 0.11 / 1.70 | 9.96 | 8.2 min, 1.8 GiB |
| `widgets-slice-loop-rest` | scheduled | 14.60 / 13.01 / 14.69 / 9.86 / 16.14 | 1.73 / 1.69 / 1.69 / 1.26 / 1.69 | 17.53 | 16.2 min, 1.8 GiB |
| `widgets-slice-loop-requests` | scheduled | 14.82 / 15.10 / 16.02 / 14.22 / 16.71 | 1.73 / 1.69 / 1.69 / 1.21 / 1.70 | 17.96 | 16.8 min, 1.8 GiB |
| `core-journal-and-write` | scheduled | 19.40 / 20.81 / 19.67 / 14.47 / 13.17 | 8.27 / 8.26 / 8.27 / 8.26 / 8.27 | 22.20 | 20.9 min, 8.3 GiB |
| `core-second-writer` | scheduled | 16.53 / 16.30 / 17.28 / 13.90 / 13.05 | 8.81 / 8.81 / 8.80 / 8.78 / 8.80 | 18.78 | 17.3 min, 8.9 GiB |

- The recorded budget in the shard table is the largest figure across the five
  runs, rounded up. The task asked for the larger of A and B; C and E only
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
  On pull request #41 only that job ran (run `35930740981`: 7.66-minute shard,
  9.18-minute job), while the dispatched run E still ran all five shards.
  `Kani Proof Harnesses (widgets-geometry)` is a **required check**
  (maintainer decision, 2026-09-23). `main` had no branch protection and no
  ruleset before this. The repository ruleset `main: Kani geometry proofs
  required` (id `23911547`) targets the default branch and has one rule: that
  status check must pass, and only the GitHub Actions app (integration
  `15368`) can report it. Repository admins bypass it, so direct pushes to
  `main` and release pushes keep working; in practice it gates pull requests.
  Deleting the ruleset undoes this. Failing
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

**The pure geometry and budget policies (2026-09-24,
`extend-kani-to-pure-policies`, N2).** Harnesses over five GTK-free policies,
each a `kani_proofs.rs` child of the checked module (so it reaches private
helpers without widening them), in two new shards. Local times are from the
final full-lane run (`make kani`, Kani 0.68.0 / CBMC 6.11.0, this toolbox, one
shard at a time); the runner budgets follow the table.

| Module | Harness | Domain | Result | Local time |
|---|---|---|---|---|
| `model/editor_memory` | `estimate_is_bookkeeping_when_evicted_and_floored_by_file_size_otherwise` | every `u64`, `Option<u64>`, `bool` | PROVED | 0.08 s |
| | `budget_never_selects_protected_or_bookkeeping_pages` | three pages, distinct ids in `0..3`, every `u64` | PROVED | 40.8 s |
| | `budget_selects_least_recently_used_first` | same | PROVED | 55.7 s |
| | `budget_stops_at_the_lower_watermark` | same | PROVED | 38.2 s |
| | `budget_outcome_matches_the_projected_total` (covers `WithinBudget`, `Converged`, `NoProgress`) | same | PROVED | 212.7 s |
| | `within_budget_selects_nothing` | same | PROVED | 34.8 s |
| | `ledger_totals_match_a_recomputation` | three upserts/removes on ids `{0, 1}`, every `u64` | PROVED | 40.1 s (262.7 s before the per-step full scan was replaced by the transitive `saturating_total` link) |
| | `ledger_crossing_flag_is_exact` | same | PROVED | 0.6 s |
| `ui/editor_page/minimap/policy` | `native_slider_fit_never_panics`, `marker_fit_never_panics`, `projected_fit_never_panics`, `native_slider_estimate_never_panics` | any `f64` and `i32` | PROVED | 0.2 / 1.1 / 1.5 / 5.1 s |
| | `min_height_expansion_never_panics_inside_its_band` | finite band and span, any `f64` minimum | PROVED | 98.5 s |
| | `min_height_expansion_panics_on_an_inverted_band` | finite, `lower > upper` | `should_panic`, as documented | 1.4 s |
| | `marker_bounds_stay_in_content_for_finite_f64` | finite `f64` | PROVED | 3.0 s |
| | `projected_fit_rejects_a_span_outside_the_band` | finite `f64` | PROVED | 4.0 s |
| | `projected_bounds_stay_in_content_on_whole_pixels` | integers, magnitude ≤ 2^20, minimum ≤ 2^16 | PROVED | 27.0 s |
| | `native_slider_stays_in_the_source_map_on_whole_pixels` | same | PROVED | 143.9 s |
| | `min_height_expansion_reaches_the_minimum_on_small_whole_pixels` | integers, magnitude ≤ 2^8, minimum ≤ 2^6 | PROVED | 93.8 s |
| | `projected_containment_fails_for_general_f64`, `native_slider_containment_fails_for_general_f64`, `marker_min_height_fails_for_general_f64` | fractional, magnitude ≤ 2^20 | `should_panic`, as documented | 6.8 / 7.3 / 4.6 s |
| `ui/window/geometry/policy` | `focus_mode_renders_no_secondary_surface`, `layout_never_renders_an_unrequested_surface`, `compact_layout_renders_at_most_one_surface`, `wide_layout_renders_every_requested_surface`, `sheet_presentation_matches_the_breakpoint` (covers sheet and pane) | every `i32` width, preset, intent | PROVED | 0.1–0.2 s each |
| | `breakpoint_is_monotone_and_bounded` | workspace width in [0, 440] sp | PROVED | 0.2 s |
| | `pane_shares_are_positive`, `shell_policy_never_panics` | every `i32` width, preset, intent | PROVED | 0.9 / 0.6 s |
| `ui/sidebar/width_preset` | `clamp_matches_the_spec_formula_and_bounds`, `clamp_is_monotone_in_window_width`, `percent_is_the_hint_fraction`, `index_round_trips`, `fraction_round_trips` | every preset, `i32`, `u32` | PROVED | 0.3 / 0.3 / 0.02 / 0.03 / 0.03 s |
| | `from_fraction_picks_the_nearest_preset` | every `f64` (nearness for magnitude ≤ 2) | PROVED after the fix | 8.1 s |
| `ui/markdown_preview/policy` | `preview_width_respects_the_floor`, `preview_width_is_at_most_a_third_above_three_sp`, `preview_width_is_the_floor_below_three_sp` (covers the floor winning), `preview_width_keeps_an_in_band_preference`, `preview_width_is_monotone_in_preference` | every `i32` preferred and available width | PROVED | under 0.2 s each |
| | `preview_width_is_not_always_a_third` | same | `should_panic`, as documented | 0.03 s |

Every `kani::cover!` was satisfied, so no proof is vacuous. Findings, each
triaged like a failing test:

- **Defect, fixed failing-first: a non-finite stored sidebar width meant
  `Large`.** `from_fraction_picks_the_nearest_preset` failed on the unchanged
  code (the `DEFAULT` assertion, and Kani's NaN-on-subtraction checks); the
  unit test `non_finite_stored_fraction_resolves_to_default` failed with NaN →
  `Large`. `from_fraction` now returns `Comfy` for any non-finite value (the
  key has no schema range, and GVariant text parses `nan` and `inf`).
- **Defect, fixed failing-first: the native-slider fit returned an infinite
  height.** `native_slider_fit_never_panics` found a raw slider and a source map
  both `f64::MAX` tall whose clamped edges were finite but whose difference
  overflowed; `native_slider_fit_never_returns_an_infinite_height` reproduced it
  (`height: inf`), and two finiteness guards fixed it. `make visual-geometry-smoke`
  passed (80 cases).
- **Precondition, recorded:** `expanded_to_min_height` panics in `f64::clamp` on
  an inverted band. Both callers exclude that, so it is proved on its
  precondition and the panic is kept as a `should_panic` harness; the spec and
  rustdoc say so.
- **The preview "≤ 1/3" rule keeps its floor (design D5).** The scratch proof of
  the unconditional rule failed with `preferred = 1073741824, available = 0`
  (counted as 1 sp, width 1). The rule is stated in `adaptive-editor-geometry`
  with its 3 sp exception, and `preview_width_is_not_always_a_third` pins it.
- **Tractability, by restructuring rather than by weakening:**
  - the budget harnesses did not finish in 20 minutes through
    `sort_unstable_by_key`'s pivot recursion, so selection now takes the
    least-recently-used remaining page per step (same prefix, pinned against a
    sorting reference by `least_recently_used_loop_selects_the_sorted_prefix`);
  - a second insert into the ledger's `BTreeMap` exhausted 20 GiB, so the
    accounting moved into a private `ResidencyTotals` value the ledger drives,
    proved over a fixed-array record set that displaces records as the map
    does (the map is trusted).
- **A bound reduced, stated with the property it weakens:** the minimum-height
  guarantee did not finish in 30 minutes at 2^20 (nor 15 at 2^12), through a
  fit or on the helper, so it is proved on `expanded_to_min_height` at 2^8 /
  2^6 and sampled to 2^20 by the unit test
  `expanded_to_min_height_reaches_the_minimum_on_whole_pixels`. Containment
  keeps the 2^20 domain.
- **Whole pixels at the policy boundary (maintainer decisions, design D8 and
  D9).** The preview clamp, the width presets, the shell geometry, and the
  local-history viewer share became integer-only; the split-view fraction is
  formed once in `geometry::execution::split_fraction`, unit-tested finite and
  in (0, 1]. The preview harnesses fell from up to 12 minutes each (CaDiCaL on
  the `f64` multiply and floor) to under a second, and `from_fraction` from
  82 s to 8 s. The suspected `3k → k - 1` flooring of the old preview form was
  unreachable for `i32` (the in-band harness had proved the `f64` form), and
  the comparison form of `from_fraction` agrees with the nearest-delta form
  everywhere except that form's own `f64::EPSILON` tie band, where it had
  said `Comfy` (`tests/properties/width_preset.rs`). The geometry and budget
  policy modules deny `clippy::float_arithmetic`, and rule 10 of
  `make check-workflow-boundaries` fails a listed module or a new geometry
  `policy.rs` without the deny — proved failing first on the real tree.
- **The whole-pixel enforcement, hardened (design D10).** An adversarial review
  found that both layers could be walked past: a reasoned `expect` on an
  `impl`, an inline `mod`, or a `mod x;` line; a child file's own inner
  `expect`; a `cfg_attr` or group-level (`restriction`) lowering; a module-level
  inner `allow`, which Clippy's `allow_attributes` ignores (re-verified with a
  scratch crate; the simplify pass had wrongly dropped rule 10's scan for it);
  float arithmetic by method call (`mul_add`, `powi`, `sqrt`, `midpoint`, …),
  which `float_arithmetic` does not see; and a deny that counted inside a block
  comment. Six of the seven modules now **forbid**
  `clippy::float_arithmetic` and `clippy::disallowed_methods` (the root
  `clippy.toml` lists the float methods, allowed workspace-wide), which rustc
  lets nothing beneath lower. The minimap keeps the pair as `deny` with a
  recorded ceiling of **10** admitted functions. Rule 10 now reads
  comment-stripped code, scans child files, rejects every lint-lowering
  attribute but a function-level `expect`, and checks the convention's table
  against its own list; all 25 escape fixtures are findings, 20 of which the
  previous rule passed. Three exceptions were removed, each against a dense
  equivalence test with the replaced `f64` form: `format_bytes` (integer
  tenths, ties to even; identical below 2^53 bytes), the minimap's wide-editor
  ratio (`map * 5 > editor`; identical for every `i32` pair), and
  `MinimapMarkerBounds::height` (the subtraction moved to its one cairo
  caller). Focus Mode's `readable_column_margin`, an unlisted whole-pixel
  decision, moved into `ui/window/focus_mode/policy.rs` on Pango units
  (identical for every `i32` input). The minimap's marker lane widths stay
  fractional and admitted; making them whole-pixel would change what is drawn,
  so it is recorded as a maintainer option. No harnessed function changed.
  **Option taken (2026-09-25, `integer-minimap-marker-lanes`):**
  `marker_lane_width` and `marker_lane_x` now return whole pixels, rounded to
  the nearest pixel (8 / 7 / 5 / 4 px lanes on the 8 px strip, at most 0.44 px
  from the fractional edges), with the one `f64` conversion at the cairo call
  in `projection_execution.rs`; both admissions are gone and the minimap's
  ceiling is **8**.

Mutation scope (`make mutants-list`, 5,919 → 5,919 mutants): `ui/markdown_preview/policy.rs`
177 → 188, a **gain from zero** (the clamp had 0 mutants in
`ui/window/preview.rs`); `ui/window/geometry/policy.rs` 81 → 66, because the
integer shares replaced the `f64` fraction arithmetic and its dead
rebased-properties floor, and the properties quarter became a divisor
(`width / 4`) instead of a percentage; `model/editor_memory.rs` 43 → 50, from the selection
loop and `ResidencyTotals`; `ui/editor_page/minimap/policy.rs` 412 → 411 (the
defect guards added some, and `gtk_f64_to_milli` moved into the minimap's
coordination adapter); `ui/window/local_history/policy.rs` 92 → 92. No
`kani_proofs.rs` mutant is listed. A focused run (`scripts/run-mutants.sh full`
with `MUTANTS_RE` over `ui/window/geometry/policy.rs`, `model/editor_memory.rs`,
the two preview-width functions, `fit_native_slider_to_source_map_bounds`, and
`parent_relative_dialog_axis_size`) left **0 missed**: 235 caught, 16 unviable.
It first needed a pre-existing blocker fixed: `cargo-mutants` builds
`--package lushtext-core`, and the `persistent_json_format` integration test
uses the `test-utils`-gated filesystem fixture, so the unmutated baseline did
not compile; that target now declares `required-features = ["test-utils"]`,
which workspace test runs still satisfy.

**Runner budgets.** Four `workflow_dispatch` runs of `kani.yml`: `36052583128`
(A) and `36054671084` (B) on the first form of the change, `36059814296` (C) and
`36061831286` (D) on the final, whole-pixel form.

| Shard | Harnesses | Gate | Wall A / B / C / D (min) | Peak (GiB) | Recorded budget | Local |
|---|---|---|---|---|---|---|
| `core-memory-policy` | 8 | scheduled | 14.86 / 16.15 / 13.05 / 15.05 | 8.9–8.91 | 16.2 min, 9.0 GiB (all four; the harnesses did not change) | 11.1 min, 8.91 GiB |
| `core-geometry-policies` | 34 | **pull-request** | 15.15 / 15.73 / 13.51 / 12.49 | 2.17–2.18 | 13.6 min, 2.2 GiB (C and D; the rewrite changed its harnesses) | 7.45 min, 2.35 GiB |

- One shard was projected over the 25-minute margin from the first local times
  and the runner's 1.8x factor, so the design's split came first: the memory
  harnesses (dominated by the ledger and the outcome harness) and everything
  else.
- `core-geometry-policies` measured 13.6 minutes at most on the final form, within
  the 15-minute pull-request margin, so it joins `widgets-geometry` in the
  pull-request gate (design D7). It was not a required check at first; on
  2026-09-25 the maintainer approved making it one, and the ruleset
  `main: Kani geometry proofs required` (id `23911547`, integration `15368`,
  admins bypass) now requires **both** `Kani Proof Harnesses (widgets-geometry)`
  and `Kani Proof Harnesses (core-geometry-policies)`.
- Runs C and D also re-measured the older shards on the runner.
  `core-journal-and-write` reached 21.12 minutes (recorded 20.9) and
  `core-second-writer` 17.39 (recorded 17.3), with no change to their
  harnesses: runner variance, as the phase-2 note on ±1.5 minutes predicts.
  Their recorded budgets were raised to 21.2 and 17.4 minutes, and their
  `measured_in` now names all seven runs.
- Local full lane (`make kani`, all seven shards, one heavy job at a time):
  74 harnesses, 0 failures, about 66 minutes.

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

**Extended by `extend-closed-loop-geometry-verification` (2026-09-25).** The
two items this phase left open are closed, the A7/A8 settle envelope was
revisited against real consumers, and both in-Kani attempts at an unbounded
claim were made. Kani 0.68.0 / CBMC 6.11.0, this toolbox, times per harness
from one `cargo kani` run of the final model while other lanes shared the
machine. The anchor model made the slice-loop shards slower, so they are split
into six (`widgets-slice-loop-rest`, `-rest-three`, `-requests`,
`-requests-two`, `-pairs`, `-unbounded`) to stay inside the runner margins:

| Harness | Scope | Result | Time |
|---|---|---|---|
| `slice_loop_two_simultaneous_requests_do_not_add_up` | N=2, both bins request in one frame (A4, A5) | counterexample on the pre-fix model (outer at the sum); PROVED with anchored forwarding | 442 s |
| `slice_loop_viewport_resize_leaves_the_outer_alone` | N=1, rest, then any viewport height 40–200 px | PROVED | 61 s |
| `slice_loop_forwarding_a_published_anchor_settle_moves_the_outer` | the same on the pre-fix bin | `should_panic`: the outer jumps | 100 s |
| `slice_loop_request_coinciding_with_a_resize_is_erased` | a request in the frame of a resize | `should_panic`: the recorded residual | 42 s |
| `slice_bin_allocation_touches_only_its_own_bin` | L1, L2 | PROVED | 35 s |
| `slice_bin_decision_depends_only_on_its_own_inputs` | L3 | PROVED | 89 s |
| `slice_bin_rests_from_any_origin` | L4, up to 400 px above and below | PROVED | 37 s |
| `slice_bin_rests_after_any_outer_jump` | L4 | PROVED | 55 s |
| `slice_bin_honours_a_request_from_any_origin` | L4 | PROVED | 250 s |
| `slice_loop_rests_with_{one,two,three}_bins` | as K4, on the anchor model | PROVED | 67 / 339 / 577 s |
| `slice_loop_honours_a_request_with_{one,two}_bin(s)` | as K4 | PROVED | 170 / 617 s |
| `slice_loop_learning_frame_*`, `slice_loop_{one,two}_reconfiguring_*` | as K4 | unchanged (proof / `should_panic` residuals) | 40 / 19, 15 / 27 s |

**Two bins requesting in one frame (N4, second half).** Predicted by code
reading and confirmed by Kani on the pre-fix model: every bin measures its
request against the same pre-move outer value, and each idle added its delta to
the value the previous bin's idle had already moved, so the outer landed at
`outer₀ + d_A + d_B`. Concrete playback: viewport 40, bin contents 90 and 84,
outer 147; bin 0 asks for value 1 (δ −126), bin 1 for 8 (δ −9); the outer landed
at 12 = 147 − 126 − 9 instead of bin 1's 138. Failing first in real GTK:
`gtk_lush_adoption::test_adoption_two_slice_bins_requesting_in_one_turn_do_not_add_up`
(two bins with the boundary mid-viewport, `scroll_to` in both lists in one turn:
the outer settled at 6855 and the second list's row was off screen). The fix
(design D2), model-checked first: the first request of a batch records the outer
value it was measured against and the idle sets `anchor + pending`. Within-bin
accumulation is unchanged. With it the harness is a proof and every earlier
slice-loop harness kept its result on that model (two-bin 284 s; one/two/three-bin
rest 54/192/515 s; one/two-bin request 109/474 s; learning-frame 39/19 s;
reconfiguring 27/26 s). LushText path: `scan_execution.rs` calls
`select_and_scroll_to` from a per-section timeout, so two sections refreshed in
one turn can each request; no existing test-utils surface drives two sections'
refreshes into one turn without a new seam, so the adoption-lab fixture (a real
GTK Lush consumer) is the reproduction (task 2.4).

**The A7/A8 settle envelope, revisited.** The A7 probe had measured 4.42 px of
value per pixel of page change after a host move, contradicting A8's "a settle
is bounded by the correction that caused it", which K4 used as an envelope.
Instrumenting the 118 slice-bin widget tests headless (every allocation's
published offset, the child's value, the page and upper before and after the
child's allocation, and the decision) gave 1 115 allocations, 39 requests, 217
child-corrected allocations whose every divergence lay within the correction,
and **no page change at a non-zero offset in the whole suite**: the A7
amplification needs a host page change after a host move, which the suite never
made. Targeted experiments made it: after any outer scroll of 100–7000 px, a
600 → 450 px viewport moved the adoption-lab list's value by ≈ 25 % of the
offset (−6 … −1760 px), a maximize after a 3000 px scroll moved it +4664 px, and a
CSS inset change mid-content moved it −79 px for a 24 px correction; in
LushText's sidebar a 665 → 400 px change after a 9000 px scroll moved it 9 px.
The bin forwarded each as a request, so the outer scroller jumped. **Verdict:
reachable in real consumers**, through the bin's own page changes: its publish
is a host move whose `value-changed` the list sees before it is allocated at the
new value, which leaves the anchor stray (A20, pinned by a probe), and a later
page-only change re-derives the value along it while `reconfigure_shift` is zero.

The model was widened to what GTK does where it matters: the child carries an
explicit anchor (A7), stray after a publish (A20), and a stray anchor
re-derives the value **anywhere** in the child's range once the page differs
from the one it was set at; the A8 bound applies only to the child's own
estimate correction. One narrowing is recorded: an anchor inside the view
(after a request or a post-allocation announcement) is assumed to keep the
value on a page change, where GTK moves it by up to the page change times an
alignment in `[0, 1]`. A first form modelled that move exactly
(`P − (P − v₀) × page / page₀`); its symbolic division tripled the slice-loop
times (the two-bin request harness took 61 minutes) and three harnesses then
failed only on that interplay, so the frames after such a page change are the
model's claims, not GTK's (the padded clipped-row widget test forwards −7 px of
it after a reveal near the header, and comes to rest).
On the pre-fix classification the widened model finds the jump
(`slice_loop_forwarding_a_published_anchor_settle_moves_the_outer`, kept as a
`should_panic` counterexample). Failing first in real GTK:
`gtk_lush_adoption::test_adoption_slice_bin_keeps_the_outer_still_when_the_viewport_height_changes`
(3000 → 2256 at a 450 px viewport) and, in LushText,
`workspace_tree_virtualization::test_a_sidebar_height_change_after_scrolling_keeps_the_scroll_position`.
The fix, model-checked first
(`slice_loop_viewport_resize_leaves_the_outer_alone`): the bin re-announces the
value after the child's allocation whenever its publish moved it (A20: the list
then anchors inside its view; this alone left a +5 px drift), and the
crate-private `classify_child_scroll_in_frame` writes back, never forwards, a
divergence after an emitting publish (A9: no request can follow it) or after a
page/upper change the bin made under an anchor it set. The public
`classify_child_scroll` contract is unchanged. **New residual, pinned**
(`slice_loop_request_coinciding_with_a_resize_is_erased`, `should_panic`): a
child request applied in the very allocation of such a page change is erased.
Reachability: it needs a `scroll_to` in the exact frame of a window resize after
an outer scroll; no test or user flow was found that does it. Candidate fix if
it matters: re-anchor before the child's allocation instead, which needs the
list's own rows to be realized at the new value first.

Two more defects surfaced on the way, each failing first:

- **A request in a scrolled-past bin was a no-op.** The band never shrank at the
  bottom edge, so a bin the viewport had left still showed its child a full
  viewport of its last rows, and `scroll_to` of one of them asked for nothing.
  Rendered-row tests had hidden it by counting realized rows; A10 was wrong
  (mapped does not mean drawn; now pinned by a probe), and last-row assertions
  now intersect mapped rows with the viewport (`drawn_labels`). Failing first:
  `workspace_tree_virtualization::test_two_sections_each_above_the_cap_render_completely`
  (hardened) and
  `gtk_lush_adoption::test_adoption_slice_bin_honours_a_request_in_a_bin_scrolled_past`.
  Fix: the band is what shows at both edges, one pixel plus the learned inset at
  the nearest edge when nothing does (never zero, A5), and a full viewport's band
  until a non-zero page has revealed the inset (otherwise an off-screen bin
  learned its inset one pixel per frame). **Residual:** the one row covering that
  pixel — GTK's `scroll_to` treats a row that covers its whole view as already
  visible. It is GTK scroll logic, outside the model's adversarial child, so it
  is recorded here and in the widget CHANGELOG rather than pinned by a harness.
- **A late re-slice on an outer page change.** `GtkViewport` delivers
  `notify::page-size` after allocating its child (A19, pinned by a probe), so
  the bin's `queue_allocate` from that handler was pending while the frame was
  drawn (`Trying to snapshot GtkLushViewportSliceBin ... without a current
  allocation` on maximize in `workspace_tree_virtualization`). The bin now
  re-slices from an idle.

- **A snapshot without a current allocation on maximize.** Maximizing and
  unmaximizing a LushText window with the sidebar scrolled printed `Trying to
  snapshot GtkBox ... without a current allocation` for the sidebar's
  `sections_box` (found by the A7/A8 experiments; the window's own
  allocation-time writes were ruled out by elimination). A re-slice reaches the
  bin through `gtk_widget_ensure_allocate`, because its ancestors keep their
  sizes; the new band moves the list's anchor, the list rebinds rows, and their
  resize leaves ancestors GTK only walked through marked until the next layout
  pass, while `AdwBreakpointBin` snapshots its child inside its own allocation
  when its breakpoint changes. Failing first:
  `workspace_tree_virtualization::test_maximize_round_trip_while_scrolled_keeps_the_sidebar_allocated`.
  Fix: a re-slice marks the ancestor chain for allocation (to the outer
  scroller inside layout, to the root outside), and the bin allocates its child
  once more after a re-announcement or write-back, so it returns with the child
  allocated.

**Unbounded attempt 1: loop contracts — failed on resources.**
`slice_loop_rest_persists_for_any_number_of_frames` (`while kani::any()` over
resting frames, invariant "the rested model is unchanged") and
`slice_loop_spaced_requests_stay_honoured_for_any_number_of_requests` (body:
one admissible request plus two resting frames; invariant: the rest set R — at
rest, the child and the published offset on the band top, the inset learned, the
anchor inside the view — plus `on_entry` equalities for the geometry) compiled
under `-Z loop-contracts` with the crate features above. Kani accepted both
contracts; CBMC did not finish either:

- rest persists, whole-struct invariant (`model == rested`): after 20 s of
  symbolic execution, 8 109 VCCs (517 after simplification) and 502 s of SSA
  conversion, "CBMC appears to have run out of memory";
- rest persists, field-wise invariant (to avoid the derived comparison's own
  loop): 7 502 VCCs (517 after simplification), then "Solver ran out of memory
  during propositional reduction" under a 25 GB address-space cap;
- spaced requests: symbolic execution had not finished after 80 minutes.

The inductive step havocs the whole model, so the encoded body of `frame` is
the fully symbolic step (arbitrary geometry, anchors, and the anchor division)
rather than the constrained one the bounded harnesses see. Stronger invariants
constraining every field, `loop_modifies` restricted to the bins, or a smaller
per-bin domain are the untried workarounds. The two harnesses are not in the
lane (a failing harness cannot be), and no claim rests on them. Their text, for
rerunning the attempt:

```rust
#[kani::proof]
#[kani::unwind(4)]
fn slice_loop_rest_persists_for_any_number_of_frames() {
    let mut model = slice_loop::Loop::<1>::any();
    model.frame(slice_loop::resting_child);
    model.frame(slice_loop::resting_child);
    kani::assume(model.at_rest());
    let (outer, child, published) = (model.outer, model.bins[0].child, model.bins[0].published);
    #[kani::loop_invariant(
        !model.bins[0].needs_allocation && !model.bins[0].idle_scheduled
            && model.bins[0].deferred.is_none() && model.outer == outer
            && model.bins[0].child == child && model.bins[0].published == published
    )]
    while kani::any::<bool>() {
        model.frame(slice_loop::resting_child);
    }
    assert!(model.outer == outer && model.bins[0].child == child);
}
```

(the spaced-requests form wraps one admissible request and two resting frames
in the same `while kani::any()`, with the rest set R and `on_entry` equalities
for the viewport, content, and inset as the invariant.)

**Unbounded attempt 2: bin independence — the claim carried within bounds.**
The argument (design D9), with the harnesses it rests on. Content heights are
fixed (the A12 envelope), and every step is the model's, so the claims hold on
its per-bin domain: content 0–400 px, inset 0–10 px, viewport 40–200 px, up to
400 px of other content above and below a bin (L4), whole pixels.

- **L1, frame locality** (`slice_bin_allocation_touches_only_its_own_bin`):
  allocating bin `i` changes no other bin and not the outer value, from any
  state two bins reach in a frame with reconfiguring children.
- **L2, single writer** (same harness, ghost `outer_writes`): only the
  forwarding idle writes the outer value.
- **L3, locality of the decision**
  (`slice_bin_decision_depends_only_on_its_own_inputs`): bin 1's allocation is
  a function of its own state, its viewport top (`outer − origin`), and the
  viewport height; a model whose other bin differs and whose outer differs by
  the same amount as the origin gives bin 1 the same state (its request anchor,
  an absolute outer value, shifted by exactly that amount).
- **L4, one bin anywhere** (`slice_bin_rests_from_any_origin`,
  `slice_bin_rests_after_any_outer_jump`,
  `slice_bin_honours_a_request_from_any_origin`): one bin at any origin in any
  outer range rests within two frames without moving the outer, rests again
  within two frames at an arbitrary jumped outer value, and honours a request.
- **L5, simultaneous requests** (`slice_loop_two_simultaneous_requests_do_not_add_up`):
  two requests in one frame end at the last applied one. With anchored idles
  every forwarding idle of a frame performs an absolute `set_value(anchor +
  delta)` from the same anchor, so k simultaneous requests reduce to the last
  applied one by induction over the FIFO idles.

**Composition.** With no request, no idle runs (a bin forwards only on a
divergence, and by L4 a resting bin has none), so by L2 the outer value is
constant; by L1 and L3 each bin then evolves exactly as a one-bin loop with a
constant viewport top, which L4 covers: **any number of bins rests within two
frames and never oscillates**. A request, or a user scroll, is an outer jump
for every other bin (L1–L3), which L4 covers again; the requester's own outcome
is L4's request harness, and several requests in one frame reduce to the last
by L5. **Not covered:** the recorded residuals (learning frame, two consecutive
reconfiguring allocations, a request coinciding with a resize, the off-screen
edge row); geometry outside the per-bin domain; and an outer jump that arrives
while a bin is still settling (L4 jumps from rest; the bounded two- and
three-bin harnesses cover interleavings only at their own size). The claim is
"any number of bins within the per-bin bounds", not "any geometry".


#### The adaptive-shell breakpoint loop (N3)

The shell's second closed loop — allocated width → derived layout →
properties breakpoint threshold and Adwaita breakpoint state → rendered
surfaces → allocated width — now has a Kani step model,
`crates/lushtext-core/src/ui/window/geometry/kani_proofs.rs`
(`extend-closed-loop-geometry-verification`, design D4/D5). It drives
production logic: the reconciliation decision `sync_secondary_surfaces` and
`sync_properties_breakpoint` used to make and apply in one place is now the
pure `plan_shell_reconciliation(RenderedShellState, AdaptiveShellLayout,
installed_threshold) -> ShellReconciliation` in `geometry/policy.rs`
(integer-only, no toolkit imports; `execution.rs` applies its writes in the old
order, and focus restoration stays there). Five characterization tests written
before the move, plus an exhaustive check of the plan against the
pre-extraction branches over 14 976 combinations, and the 80 shell-geometry
widget tests pinned it as behaviour-preserving; `cargo mutants` on `policy.rs`:
80 mutants, 69 caught, 11 unviable, 0 missed (66 predate the plan; the matrix's
old 81 was the `f64` form). The model calls the real
`derive_adaptive_shell_layout` and `plan_shell_reconciliation`, mirrors
`size_allocate`, the settle completion, and the notify handlers for
`layout-name`, workspace `show-sidebar`, and `collapsed`, and restricts Adwaita
only by clauses citing the new axioms A14–A18 (all pinned by probes):

- A14: `max-width: N sp` holds iff the allocated width in px ≤ N × s,
  inclusive, s = `gtk-xft-dpi` / 98304 from {1, 1.25, 1.5, 2};
- A15: `set_condition` does nothing synchronously; conditions are
  re-evaluated in the allocation it schedules;
- A16: setters apply inside the bin's allocation, after its child's; unapply
  restores the add-time value; a direct switch between two breakpoints setting
  one property runs unapply-old, one write of the new value, apply-new (never
  the add-time value in between);
- A17: `show-sidebar` and `collapsed` never change the toplevel width;
- A18: the window minimum is its `width-request` (640 px) and only the
  last-added matching breakpoint applies (LushText adds properties, workspace,
  then the Open-button one).

Local run, Kani 0.68.0 / CBMC 6.11.0, one CBMC at a time: 474 s wall, peak
2.65 GiB.

| Harness | Domain | Result | Time |
|---|---|---|---|
| `shell_loop_settles_at_a_stable_width` | widths 640–2560 px, every preset, intent, and compact slot, s ∈ {1, 1.25, 1.5, 2}, cached threshold 0–4096, arbitrary start, 3 allocations and a settle | PROVED: fixed point, and one more allocation changes nothing | 48.7 s |
| `shell_loop_sweep_does_not_flap` | 3 monotone widths either way from rest, 4 allocations | PROVED | 82.5 s |
| `shell_loop_preserves_requested_visibility` | compact width then wide, from rest | PROVED | 39.0 s |
| `shell_loop_layout_agrees_with_policy_at_rest` | as settles | PROVED | 43.5 s |
| `shell_loop_allocation_never_persists` | from rest, one allocation | PROVED: the settings-write ghost stays 0 | 12.5 s |
| `shell_loop_workspace_collapses_with_its_breakpoint` | as settles | PROVED | 31.0 s |
| `shell_loop_layout_is_stable_under_its_own_compact_slot` | step lemma | PROVED | 0.7 s |
| `shell_loop_layout_setter_flaps` | the pre-fix properties setter | `should_panic`: flaps | 145.0 s |
| `shell_loop_open_button_breakpoint_uncollapsed_the_workspace` | the pre-fix Open-button breakpoint | `should_panic`: uncollapses | 31.9 s |

Every required `kani::cover!` is reached: pane↔sheet transition, compact slot
handed to properties, collapsed workspace, threshold reinstall (and the
Open-button breakpoint current). Two counterexamples, both reachable in real
GTK, were fixed failing-first:

1. **The properties layout setter flapped.** Under text scale (the
   `(T, T·s]` px band), and at scale 1 when the workspace breakpoint took over
   and the setter's add-time `pane` was restored, one allocation changed
   `layout-name` twice. Failing first:
   `shell_geometry::test_large_text_breakpoint_never_flips_the_pane_to_the_sheet`
   (Large Text, 1500 px). Fix: the properties breakpoint has no setter; its
   `apply`/`unapply` signals run `sync_secondary_surfaces`, so the plan is the
   only writer.
2. **The Open-button breakpoint uncollapsed the workspace** at text scale ≥ 1.6
   at the narrowest windows (A18: only the last-added matching breakpoint
   applies). Failing first:
   `shell_geometry::test_doubled_text_scale_keeps_the_workspace_collapsed_at_the_minimum_width`
   (scale 2, 700 px, then switching 2.0 → 1.5 → 2.0 at a fixed width). Fix: the
   Open-button breakpoint also sets `collapsed = true` (every width matching
   400 sp matches 860 sp).

Not fixed, recorded in the deferral inventory: the policy still compares px
against sp, so it and the breakpoint conditions agree only at text scale 1; with
fix 1 this no longer affects the loop.

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

### Step 8a — Set-aside retention (`bound-draft-set-aside-retention`)

**Status: complete (2026-09-24).** N10's set-aside size bound, decided as
**surface, never auto-delete**: a soft bound (100 bodies or 256 MiB) makes one
status notice due, which points to `app.review-preserved-drafts`; only the
user's confirmed per-row Delete or "Delete All Preserved Drafts…" removes a
body. The pure core is `services/draft_service/set_aside_retention.rs`
(`bound_status`, `notice_due`, `may_delete`), and its
harnesses sit in `set_aside_retention/kani_proofs.rs`, over four bodies with
arbitrary fingerprints, listing, and change facts, and an arbitrary decision:

| Harness | Property | Result | Time |
|---|---|---|---|
| `set_aside_retention_deletes_only_confirmed_bodies` | R1 no decision, no deletion; R2 only confirmed fingerprints; R3 never a changed or unlisted body | PROVED | 1.9 s |
| `set_aside_retention_bound_and_notice_never_plan_a_deletion` | R4 bound and notice are total, and feed no plan | PROVED | 0.2 s |
| `set_aside_retention_deleting_every_body_breaks_r2` (`should_panic`) | a decision that deletes whatever the user decided breaks R2 | fails as expected | 1.1 s |

A first draft of the model kept the plan's `Vec` and a cloned confirmed set;
its main harness ran past 14 minutes. The shipped core borrows the confirmed
slice and the harnesses check the per-body `may_delete` over fixed arrays,
which the service applies to each body immediately before removing it, for the
per-row Delete as well as the bulk one (both go through
`set_aside::delete_confirmed`); its composition over every confirmed body is
covered by the proptest mirror of R1–R3 (which a deliberately broken decision
fails). A separate `deletion_plan` over a pre-read slice was removed because
nothing executed it. The E1 fix in the same change added
K9 (`journal_set_aside_keeps_every_body_it_reports_kept`, 3.1 s) and its
`should_panic` twin (3.0 s), recorded in the deferral inventory. All five join
the `core-journal-and-write` shard (by its `journal_` and
`set_aside_retention::kani_proofs::` filters). Local shard run
(`make kani KANI_SHARD=core-journal-and-write`, 2026-09-24, this toolbox):
13 harnesses, 0 failures, 9.71 min wall, 8.31 GiB peak; the five new
harnesses add about 8 s, so the recorded CI budget (20.9 min) stands. The same change fixed, failing-first, the pre-existing
listing defect: `set_aside::list` stopped after 256 directory entries before
sorting, so past 256 bodies it showed an arbitrary subset as the newest.

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
  (K7), plus a summary row and a confirmed Delete All (step 8a); local
  history stays a separate copy rather than a zero-copy view,
  because a view needs a local-history index change. **Resolved 2026-09-24
  (step 8a) as "soft bound, no automatic deletion":** past 100 bodies or
  256 MiB the user is asked, once per launch, to review the area; nothing is
  deleted without a confirmed decision, and which bodies were opened is not
  recorded (no new persisted file).
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
- **Fixed (2026-09-24, `bound-draft-set-aside-retention`): the set-aside
  copy of a newer body was skipped.** The Quint vs TLA+ evaluation found it
  (E1): the real `draft_service`, driven against a Rust abstract disk model.
  - **Where:** `preserve_stale_draft_body` keeps an unapplied restore's body
    under `set_aside::keep_copy(id, entry.saved_at_secs)`, and `keep_copy`
    treated an existing `{id}.{stamp}.draft` as already kept.
  - **The failure:** a crash falls between a body write and its manifest
    commit, so the body on disk is newer than its entry's stamp. A second
    unapplied restore then reported `SetAside` without copying it, and
    released the hold. Autosave could then replace the only copy.
  - **Why the proofs missed it:** S1 does not flag it, because that body was
    never committed, and the journal model keeps preserved content as a set,
    so it never modelled set-aside naming.
  - **The fix:** a name counts as kept only when it holds a byte-identical
    copy; a different body takes the next free `-{n}` name, and neither is
    overwritten. The decision is the pure `journal_core::set_aside_name_step`,
    which `set_aside::place` drives.
  - **Evidence:** two failing-first `draft_service` tests
    (`set_aside_copy_of_a_changed_body_under_the_same_stamp_keeps_both` and
    `unapplied_restore_keeps_an_uncommitted_newer_body_under_the_same_stamp`,
    the promoted reproduction). A new Kani harness, K9
    (`journal_set_aside_keeps_every_body_it_reports_kept`), checks over an
    abstract set-aside area for one id, 2 stamps, 3 names per stamp, and 4
    `keep_copy` calls with arbitrary contents that every body reported kept
    is in the area and no kept body is replaced (about 3 s). Its
    `should_panic` twin (`journal_set_aside_stamp_only_naming_loses_a_newer_body`)
    shows the stamp-only rule breaks K9. Both run in the
    `core-journal-and-write` shard. The evaluation's
    `formal/evaluation/stateright/tests/e1_findings.rs` now asserts the fixed
    behaviour, and its seed-12 weighted Quint Connect run replays 1000 traces
    with no divergence.
- `bound-draft-set-aside-retention` hardening candidate: `set_aside::place`
  (dedupe by bytes) and `set_aside::delete_confirmed` (fingerprint check,
  then removal) do not hold the stable target write guard across their
  check-then-act windows. Every deletion re-checks the fingerprint the user
  confirmed, so no confirmed-other body is removed, but the window between
  that re-check and the removal is unguarded. No failing sequence is known;
  take it up with a design for acquiring the guard inside `place()`.
- Slice-bin residuals added by `extend-closed-loop-geometry-verification`
  (phase 3): a child request applied in the very allocation in which a window
  resize changes the band, after an outer scroll, is written back and erased
  (pinned `should_panic`); and the one row covering the one-pixel band an
  off-screen bin keeps cannot be revealed by `scroll_to`, because GTK treats a
  row that covers its whole view as visible (outside the model; recorded in the
  widgets CHANGELOG). The earlier "simultaneous requests from two bins" gap is
  closed (anchored forwarding, proved).
- The adaptive-shell policy compares the window's px width against whole-sp
  thresholds, so under text scaling it and Adwaita's `max-width: N sp`
  conditions (A14) agree only at scale 1. Since the properties breakpoint lost
  its setter (phase 3, N3) this no longer makes the loop flap; converting the
  width to sp is the open text-scale follow-up.
- `cargo-gtk-proof` has no sidebar or slice-bin scenario. The screenshot lane
  is waiting on a `reveal-workspace-path` automation action.
