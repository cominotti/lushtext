## Context

- **Phase 0 is complete and archived.** It fixed six protocol defects, and the
  audit fixed two more (`openspec/changes/archive/2026-09-23-formal-verification-phase-0`).
- **The programme then consolidated on Kani**, as recorded in
  `docs/next/formal-verification.md` §2. Quint was dropped, and Lean went
  dormant.
- **The feasibility spike ran in this toolbox with Kani 0.68.0 and CBMC
  6.11.0.** Its results:
  - `cargo kani -p gtk-lush-widgets` compiles next to the 1.96 pin in 31 s.
  - Seven harnesses over the real geometry files gave five proofs and two
    domain counterexamples.
  - The Lean micro-model was ported and checked in 1.2 s, with a stronger
    any-sequence theorem.
  - Harness sources are in appendices A and B of
    `docs/next/formal-verification-evolution.md`.

Constraints:
- The single-maintainer budget.
- The workflow readability convention: `policy.rs` purity, coordination
  naming, `evidence.rs`.
- The filesystem-boundary rules.
- GTK Lush governance.
- Headless-only widget tests.
- The mandatory pre-existing-blocker rule.
- The 30-minute CI job cap.

## Goals / Non-Goals

**Goals:**

- Every formal property is a Kani harness over production code, or over a
  pure core that production drives.
- The work is ordered by value, and each group is independently shippable:
  K1, K2, K3, K4, K5, K6, K7, K8.
- Data-safety guarantees are preserved exactly through the K3 and K6
  refactors, and are proved where the change makes that possible.

**Non-Goals:**

- Unbounded proofs, Lean, Quint, Aeneas, or Lean/Rust differential testing.
- Verifying GTK itself. GTK enters verification only as ledger axioms.
- Changing any persisted format.

## Decisions

### D1. Harness placement: in the owning crate, `#[cfg(kani)]`, with `check-cfg`

Harnesses live in the crate that owns the checked code:

- `gtk-lush-widgets` for geometry;
- `lushtext-core` for the journal and the durable write.

Each crate has a `#[cfg(kani)] mod kani_proofs`.

The workspace declares `[lints.rust] unexpected_cfgs = { level = "warn",
check-cfg = ['cfg(kani)'] }` through the workspace lints table, so stable
Clippy stays quiet.

Alternative rejected: a separate `formal/` crate that includes files with
`#[path]`, as the spike did. It duplicates module wiring and can check a
different compilation of the code than the one that ships.

GTK Lush policy: `cfg(kani)` code is test-like and adds nothing to the
public API. Confirm with `make check-gtk-lush-policy`, and update the family
policy if it objects.

### D2. The Kani lane

`make kani` runs `cargo kani` per crate with an explicit harness list, or
runs everything, and exits non-zero on any failure.

CI is a new `kani.yml`, run on a weekly schedule and by `workflow_dispatch`.
Its setup:
- the Fedora 44 container, matching the other workflows;
- Kani installed with `cargo install --locked kani-verifier --version 0.68.0`
  and then `cargo kani setup`, since the official action assumes Ubuntu;
- `timeout-minutes: 30`, satisfying `check-workflow-timeouts.py`.

It stays out of the pull-request gate until run times are known. That can be
promoted after stability review, following the fuzz-smoke precedent. The
version is pinned in one place: an env var in the workflow and a Makefile
variable.

### D3. Domain gaps (K1): state the whole-pixel domain; do not change behaviour

The two counterexamples live at `f64` magnitudes near 10²⁶⁰, and at
non-integer values GTK never produces. Clamping the inputs would change
behaviour for no user benefit. So the rustdoc and the spec state the
whole-pixel domain, the proof harnesses assume it, and the any-`f64`
harnesses keep proving the property that holds universally, no panic.

### D4. GTK axiom ledger location and probes (K2)

The ledger file is
`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`,
next to the existing GTK facts. The programme record's A1–A13 table moves
there and is linked from the record.

Probes go in a new widget-test module, `crates/lushtext/tests/widget/gtk_axioms.rs`.
Each probe builds the smallest GTK fixture that shows its axiom, with no
LushText widget involved:

| Axiom | What the probe does |
|---|---|
| A5 | allocates a `GtkListView` at zero height and observes its adjustment rewrite |
| A9 | sets a pending `scroll_to`, emits `value-changed`, and asserts the request is dropped |
| A11 | asserts `configure` and `set_value` clamp, and that an unchanged `set_value` emits nothing |
| A13 | moves the outer adjustment during layout and asserts whether a relayout was scheduled |

Each probe cites its axiom id. A8 records the phase-0 instrumentation numbers.

### D5. Journal decision core (K3)

The pure module is `ui/window/drafts/policy.rs`, if it fits the facade budget
and the role convention. Otherwise it is a new GTK-free core under
`services/draft_service/` driven by both layers. The decision is recorded in
the workflow matrix row.

The state is fixed-size for Kani: ids and generations are small indices. The
core exposes:
- `decide(state, event) -> Decision`;
- `ownership(state, id) -> Owner`, the single body-ownership check that
  replaces the restore hold, the six preserve call sites, and the overwrite
  copy.

Production maps its `HashMap`s onto the same functions, which are generic
over a small `JournalView` trait or take slices. Kani instantiates them with
arrays of 3.

Absorbed altitude findings:
- The service's body write takes proof of a registered entry, as a typed token
  returned by registration. That makes "no body without an entry"
  unrepresentable.
- Insert-only commits go through `update_manifest`.
- Stale bodies have one owner: set-aside owns the bytes, and local history is
  a view.

Harness bounds: 3 ids, 3 generations, and 8 steps, with events Edit, Snapshot,
BodyWrite, Commit, ReqDelete, DelBody, DelEntry, Inspect, ExecCleanup, Crash,
Startup, and ExternalMtime. L1 is bounded at k = 6 fault-free steps.

Alternative rejected: modelling the journal separately, the Quint approach.
It re-creates the translation gap that the consolidation removed.

### D6. Closed-loop model (K4)

The model lives in `gtk-lush-widgets` behind `cfg(kani)`. The state per bin
and the outer state are those of the phase-0 reading.

Each step:
1. calls the real `viewport_slice` and `classify_child_scroll`;
2. picks the child's reaction as `kani::any()`, constrained by `kani::assume`
   clauses that each cite a ledger axiom;
3. applies the idle step to the outer adjustment.

It uses the integer abstraction. This is sound because allocations are
integer and `f64` add/sub is exact below 2^53, and K1 proves the landing
lemma on whole pixels. Bounds: N ≤ 3 bins, k ≤ 4 allocations.

If the learning-frame residual produces a counterexample, the fix is
designed then, with the counterexample as its failing-first test, or the
residual is recorded with ledger-cited justification.

### D7. Kill points (K5)

A Cargo feature, `crash-kill-points`, is enabled only in the smoke build. Each
kill point is a function called at a named window. It reads
`LUSHTEXT_KILL_AT=<window>` and calls `std::process::abort()`, which is close
enough to SIGKILL because it runs no destructors.

The windows come from the K3 and K6 cores: `draft-body-before-commit`,
`durable-renamed-before-dirsync`, and `stale-preserved-before-retire`.

The smoke driver gains one scenario per window, and each asserts recovery.
The feature is off in the Meson/Flatpak build, and the release build is
verified to contain no kill-point code.

### D8. `WriteProtocol` core (K6), built on the primitive split (K7 first)

K7's split lands before K6, so the core models `copy_durable` and
`move_durable` as distinct actions.

The core is `services/filesystem/write_protocol.rs`, with an `enum Action`
covering CreateTemp, WriteContent, ApplyMetadata, SyncTemp, Rename, SyncDir,
RemoveTemp, and Done(Result). `step(state, outcome) -> (state, Action)` is
the whole decision logic. The shell loop in `durable_write.rs` executes
actions through `sys::`.

Streaming writes keep their closure in the shell. The core sees only the
closure's outcome.

The ported harnesses (appendix B) retarget the real core. The disk
abstraction for crash semantics lives in the `cfg(kani)` module, so
production never carries it.

Existing fault-injection unit tests stay as the shell's evidence.

### D9. K7 cleanups ordering and the Data-page surface

Order: primitive split, then temp-name owner and closed `WriteLabel`, then
sweep coverage, then the Data-page set-aside group.

The Data page reuses the existing `Preferences > Data` page structure and the
grouped-row rules. Delete confirmation uses `AdwAlertDialog`. Open creates an
untitled tab through `new_tab` with the body content, bounded by the
automatic draft limit.

### D10. K8

Add a second writer (window or process) to the K3 harness as a second actor
with its own in-memory state over the shared disk state. Report which
invariants fail, and record a decision in the programme record: accept with
documentation, or add an inter-process data-directory lock as a follow-up
change. No production change is required inside this change unless a failure
is reachable in current production. Production reuses the active window, and
second instances are a `flatpak run` away, so the decision is recorded with
that evidence.

### D11. Cleanup

- Docs keep only the dormant-Lean note. Remove any remaining Quint or Lean
  plan text.
- Remove `~/.elan` with `elan self uninstall`. This is a local toolbox action
  outside the repo.
- Remove `.claude/worktrees/agent-*` only after a per-file check that each
  file's content in the worktree equals the committed `main` version or is an
  older ancestor of it. The phase-0 subagent's worktrees differ from `main`
  only because `main` evolved further. Any other file is reported rather than
  deleted.

## Risks / Trade-offs

- [The K3 and K6 refactors touch the most safety-critical code] → Every group
  starts from failing-first or characterization tests, keeps all existing
  tests green, reruns the data-safety explicit audit, and lands behind
  unchanged public behaviour.
- [Kani run time grows with journal state] → Fixed arrays, small bounds, and
  per-harness unwind limits. CI stays scheduled rather than per-PR until
  measured.
- [Kani contracts and loop contracts are experimental] → Not used. Harnesses
  use only `kani::any`, `assume`, `unwind`, and `should_panic`.
- [The Kani nightly drifts from the stable toolchain] → Kani is pinned to one
  version in one place, and upgrades are deliberate.
- [Ledger probes are fragile across GTK updates] → That fragility is the
  point. A failure is an axiom change and is reviewed as such.
- [The change is large] → Groups are ordered by value and are independently
  shippable. Apply may stop after any group with the programme record updated.

## Migration Plan

- No user-visible migration and no persisted-format change.
- K7 adds a Data-page group that is visible only when set-aside content
  exists.
- Rollback is per group through git. Kani code sits behind `cfg(kani)` and
  never enters builds.

## Open Questions

- Does the journal core fit `ui/window/drafts/policy.rs` under the facade and
  role rules, or does it need a service-level home? Decide at the start of
  K3 and record it in the matrix.
- The exact k for bounded L1. Choose the smallest k that covers the longest
  fault-free autosave-to-clean path in the core.
