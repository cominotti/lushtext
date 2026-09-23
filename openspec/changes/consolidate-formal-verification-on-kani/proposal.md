## Why

On 2026-09-23 the formal-verification programme consolidated on **Kani as its
only formal tool** (`docs/next/formal-verification.md` §2). Three tools
(Kani, Quint, Lean) would each cost a toolchain, a CI lane, and a bridge back
to the Rust code. Kani needs no bridge, because it checks the Rust that ships.

The phase-0 spike showed the consolidation is practical:

- Kani compiles `gtk-lush-widgets` next to the workspace toolchain pin in 31 s.
- Over the real geometry code, it found two domain gaps in rustdoc promises.
- It reproduced the Lean spike's theorems, plus a stronger one, in 1.2 s.

This change carries out the whole Kani-only programme, K1–K8 of
`docs/next/formal-verification-evolution.md`, in order of value. It also
migrates the Lean spike and cleans up what the consolidation retired.

## What Changes

- **K1 — the Kani lane.**
  - In-tree `#[cfg(kani)]` harnesses in `gtk-lush-widgets` over
    `slice_geometry.rs` and `scroll_request.rs`, with `#[kani::should_panic]`
    regression harnesses.
  - A `make kani` target and a scheduled or manual CI job.
  - The two whole-pixel domain gaps get stated in rustdoc and in the spec, and
    proved.
- **K2 — GTK axiom ledger.** A normative ledger of the GTK behaviour the
  geometry designs rely on. Each axiom is pinned by an isolated headless
  widget probe, with new probes for A5, A9, A11 and A13, and A8 is backed by
  the phase-0 evidence.
- **K3 — a pure draft-journal decision core.** The journal's ordering and
  ownership decisions are extracted into a GTK-free, I/O-free state machine,
  which `draft_service` and `ui/window/drafts/` both drive. It absorbs the
  phase-0 `/simplify` altitude findings:
  - one body-ownership check;
  - "no body without an entry" enforced in the service;
  - insert-only commits through `update_manifest`;
  - one owner for stale bodies.
  Kani checks invariants S1–S4 and a bounded liveness property L1 over action
  sequences that include `Crash`.
- **K4 — the ViewportSliceBin loop in Kani.** A step model calls the real pure
  functions, with the child as an adversarial envelope drawn from the ledger,
  N ∈ {1, 2, 3} bins and k allocations. Its first target is the phase-0
  learning-frame residual.
- **K5 — deterministic crash injection.** Feature-gated kill points in the
  crash-recovery smoke build, at the protocol windows that K3 and K6 name.
- **K6 — an I/O-free `WriteProtocol` core for `durable_write`.** A thin `sys::`
  shell executes the core's actions. The protocol harnesses ported from Lean
  check the real core, and crash atomicity becomes a complete proof.
- **K7 — base cleanups.**
  - Split `copy_durable` (keeps the source) from `move_durable` (removes it).
    **BREAKING (internal API):** `copy_file_durable` is retired.
  - One owner for the temp-name format, and closed `WriteLabel` construction.
  - Leftover sweep coverage for `style-schemes/`, `format-upgrade-backups/`
    and `drafts/set-aside/`.
  - A set-aside recovery surface on the Preferences Data page.
- **K8 — drop axiom A6.** The K3 machine gains a second writer. The report
  records what breaks and decides whether the data directory needs an
  inter-process lock.
- **Migration and cleanup.**
  - Drop the remaining Lean and Quint plans from the docs, keeping only the
    dormant-Lean note.
  - Remove the Lean toolchain that was installed only for the spike.
  - Remove the stale agent worktrees under `.claude/worktrees/agent-*`, but
    only after verifying their content is contained in `main`.

## Capabilities

### New Capabilities

- `formal-verification-kani`: the Kani lane, covering where harnesses live,
  how they are run locally and in CI, required regression harnesses, domain
  statements, and failure triage.
- `gtk-axiom-ledger`: the normative ledger of GTK behavioural axioms. Each is
  pinned by an isolated headless probe or recorded as unpinnable, and the
  ledger is the source of the adversarial envelope used by verification
  models.

### Modified Capabilities

- `gtk-lush-viewport-slice`: slice geometry and request decisions state their
  whole-pixel domain and are Kani-proved on it. A closed-loop requirement is
  added: rest, no oscillation, fidelity, and bounded liveness, verified
  against the ledger's envelope.
- `draft-session-recovery`: journal decisions live in a pure, Kani-checked
  state machine with stated invariants. Preserved set-aside drafts get a
  recovery surface.
- `durable-file-write-contract`: the protocol lives in an I/O-free core whose
  crash atomicity is verified. The copy and move primitives are distinct. The
  leftover sweep covers every app-data directory that holds durable writes.
- `crash-restart-recovery-coverage`: the smoke can kill the real process
  deterministically inside named protocol windows.

## Impact

- **Code:**
  - `crates/gtk-lush/widgets/` (harnesses, rustdoc, loop model)
  - `crates/lushtext-core/src/services/{durable_write.rs,filesystem/,draft_service.rs,app_data_leftovers.rs,local_history_service.rs}`
  - `crates/lushtext-core/src/ui/window/drafts/`
  - `crates/lushtext-core/src/ui/preferences/` (Data page)
  - the crash-recovery smoke driver and its binary feature gates
- **Build and CI:**
  - a `make kani` target;
  - a new scheduled or `workflow_dispatch` Kani workflow within the 30-minute
    job budget;
  - `unexpected_cfgs` check-cfg for `cfg(kani)`;
  - Kani pins its own nightly, so the stable 1.96 gate is untouched.
- **Docs:**
  - `docs/next/formal-verification.md` and `formal-verification-evolution.md`
  - the new axiom ledger under the gtk4-libadwaita-internals references
  - AGENTS.md: build commands and the design decisions for drafts and the
    durable write
  - `.agents/rules/build.md` (the Kani lane)
  - `docs/workflow-readability-matrix.md` (the drafts row)
  - GTK Lush governance files
  - README
  - accessibility and automation docs, for the Data page surface
- **Persisted formats:** none.
- **Local environment:** the spike's Lean toolchain (`~/.elan`) is removed,
  and so are the stale agent worktrees.
