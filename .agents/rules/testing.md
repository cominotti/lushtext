---
description: Rules every Rust, script or fuzz edit needs — test isolation, temporary files, runtime warnings, proof obligations, gates before a commit, and authoring rules for Kani harnesses and GTK axiom probes
paths:
  - "**/*.rs"
  - "scripts/**"
  - "fuzz/**"
---

# Testing and Gate Rules

How to run each lane, and the lanes' internals, are in
[`build.md`](./build.md); which Kani, axiom and timing runs a change makes is in
[`verification-runs.md`](./verification-runs.md).

## Widget Tests and Formatting

- Widget tests never run against the developer's live desktop session.
  `scripts/run-widget-tests.sh` has no native mode, and the widget harness
  self-supervises into a private `mutter --headless` before GTK initializes.
- Use **`make fmt`**, not bare `cargo fmt --all`. The widget test modules enter
  `crates/lushtext/tests/widget.rs` through a registry `build.rs` generates, so
  rustfmt's module walk never reaches `tests/widget/*.rs`; `make fmt` and
  `make check-fmt` run `rustfmt --edition 2024` over them as well.

## Temporary Files

Every temporary entry a test, harness, or smoke run creates must be removed by
its owner on every exit path; a leak is a defect even when each run leaks one
directory ([why](../../docs/memorials/2026-10-20-test-data-incidents.md)).

- **One root per run, owned by the supervisor.** The `gtk-lush-proof-harness`
  parent creates one `<prefix><pid>-<random>` root (`lushtext-test-` for the
  widget suite), hands every child attempt its own empty subdirectory as
  `TMPDIR`, removes it when the child exits, and removes the root when the run
  ends. Tests therefore keep using `std::env::temp_dir()`, `tempfile`, and the
  filesystem `fixture` helpers unchanged.
- **Drop guards in Rust, traps in shell, `try`/`finally` in Python.** A temp
  directory outside an artifact tree is owned by a value whose `Drop` removes
  it, so `?` early returns cannot skip cleanup. Shell scripts register their
  temp paths with an `EXIT` trap plus `INT`/`TERM` traps; Python drivers remove
  runtime roots in `finally`, keeping only a listing in the artifacts.
- **Release session runtime dirs only after the session is gone.**
  `dbus-run-session` returns before the services its private bus activated have
  exited, and `xdg-document-portal` still holds its FUSE mount on `doc/`. Wait
  until no process carries the directory as `XDG_RUNTIME_DIR` (the harness and
  `scripts/with-temp-scope.sh --session-runtime` both do), then remove it.
- **Keep runtime dirs short.** Portal and headless smoke scripts keep their
  temporary runtime directories directly under `$XDG_RUNTIME_DIR` or `/tmp`,
  never under long artifact paths: Mutter/Wayland socket paths are
  length-limited. The session helper's private system bus socket
  (`gtk-lush-session/system-bus`) is the deepest, and `dbus-daemon` binds at
  most 99 bytes (`DBUS_SOCKET_PATH_MAX`, below the kernel's 107). The helper
  puts an owned runtime directory that would not fit under `/tmp` and refuses
  an inherited one; `with-temp-scope.sh` makes its whole scope under `/tmp`
  when `TMPDIR` is too deep for a session inside it (an agent stage's
  `$H/<change>/tmp/<stage>`, where widget tests that fix folder lengths or
  charge path bytes against their budgets failed deterministically), and
  `--session-runtime` refuses only a scope whose sockets would not fit even
  there; and every other creator of a session runtime directory falls back to `/tmp`
  when the scope's `TMPDIR` is too deep (which a seven-digit PID can make it):
  the Python drivers through `smoke_runtime_dir.runtime_base`, the Mutter
  capture helper of the `gtk-agentic-debugging` skill with the same rule, and
  `cargo-gtk-proof`'s live runner (`host.rs`, `runtime_dir_for_artifact`).
  A new one follows the same rule; a refused session names the directory.
- **PID-bearing names and a conservative sweep.** Names carry the owning PID
  after an exact prefix so the next run can reclaim what a SIGKILLed run left:
  only real directories owned by the current user, whose PID is gone and which
  have been idle for an hour, are swept (a day for the one legacy PID-less
  `gtk-lush-proof-runtime-*` form). A live run is never swept.
- **Every test lane runs under `scripts/with-temp-scope.sh`**, which gives the
  command a private `TMPDIR`, removes it on every exit (on Ctrl-C or SIGTERM
  after the session's services release the runtime directory, exiting 130 or
  143), and fails with `TEMP-LEAK:` when anything is left inside it. The check
  is scoped to its own run, so parallel runs and CI shards cannot blame one
  another. `make test`, `make test-unit`/`test-int`/`test-prop`,
  `make fuzz-corpus-replay`, `scripts/run-widget-tests.sh` (every CI widget
  shard and every shard of a parallel local run, each in its own scope), the CI
  non-widget step, `make gtk-axioms`, and the visual-geometry, visual,
  crash-recovery, and accessibility smoke targets all use it; wrap a new test
  lane the same way. `make check-temp-scope` (in `make check`) self-tests it.
  Mutation runs are deliberately not wrapped: a mutant that breaks cleanup must
  be caught by the tests, not fail the lane.

## Test Isolation From Real App Data

Tests must never read or write the developer's real application data or
desktop state: `~/.local/share/lushtext`, `~/.config`, `~/.cache`,
`~/.local/state`, or `~/.var/app/dev.cominotti.lushtext`. Isolation is a
property of the test **process**, established before any test code runs, never
a helper a test must remember to call first
([why](../../docs/memorials/2026-10-20-test-data-incidents.md)):

- **Widget tests.** The first statement of `main` in
  `crates/lushtext/tests/widget.rs` points `LUSHTEXT_DATA_DIR` and
  `XDG_{DATA,CONFIG,CACHE,STATE}_HOME` at a root inside the process's own
  `TMPDIR`, from `lushtext_core::services::test_isolation::isolated_environment`,
  before GTK starts (GLib caches the XDG variables on first use). A child
  re-roots the values it inherits from the parent, and a hand-launched child
  isolates itself the same way. `ensure_gtk_init()` asserts the isolation;
  `common::isolated_data_dir()` gives one test a fresh directory, and
  `common::keyfile_settings_backend()` its own `XDG_CONFIG_HOME`, both inside
  the same `TMPDIR`. Widget tests use `GSETTINGS_BACKEND=memory` (set in
  `ensure_gtk_init()`) unless a test selects the keyfile backend that way.
- **Unit and integration tests** (libtest binaries) take explicit temporary
  directories (`TestContext`, `tempfile`) and never resolve the global data
  directory.
- **Defence in depth.** In test builds only (`cfg(test)` or the `test-utils`
  feature), `json_store::data_dir()` panics when the directory it resolves lies
  under `$HOME`'s XDG trees or the Flatpak home above, comparing after
  resolving symlinks (`/home` versus `/var/home`). Production builds compile
  none of it: `services::test_isolation` exists only under those cfgs, which the
  default-feature `cargo check -p lushtext --bins` in CI proves. A test that
  needs a different `HOME` fakes it with a temporary directory and is held to
  that fake home in turn.
- **Prove a fix failing-first** with `HOME` pointing at a temporary directory
  and the XDG variables and `LUSHTEXT_DATA_DIR` unset, never against the real
  home.
- **Scripts and self-tests that run git forget the caller's repository first.**
  The pre-commit hook runs `make check-policy`, and inside a hook git exports
  `GIT_DIR`, `GIT_INDEX_FILE` and their siblings, which override `git -C`. A
  fixture's `git init` or `git add` then rewrites the repository being
  committed: a self-test once set `core.bare=true` in the checkout's
  `.git/config` and replaced its worktree index that way
  (`flatpak-stage-git-visible-tree`). Unset every name
  `git rev-parse --local-env-vars` prints before the first git call
  (`scripts/flatpak-source-tree.sh`; the Python scripts scrub the same names),
  run fixture repositories with `GIT_CONFIG_GLOBAL=/dev/null`,
  `GIT_CONFIG_NOSYSTEM=1` and `-c core.hooksPath=/dev/null`, and pin it with a
  case that runs the script under a decoy repository's `GIT_DIR`,
  `GIT_INDEX_FILE` and `GIT_WORK_TREE`.

## Test-Only Code Never Ships

- The fixture modules `services::filesystem::fixture` and
  `services::draft_service::fixture` compile only under
  `#[cfg(any(test, feature = "test-utils"))]`: their writers skip the durable
  write and the `RegisteredDraft` token, so production must not be able to name
  them. `test-utils` is never a default feature, and no shipping build enables
  it directly or through a feature that implies it. `make
  check-filesystem-boundary` and CI's default-feature
  `cargo check -p lushtext --bins --locked` enforce it (all-feature Clippy
  cannot see a production caller of a fixture). Details: build.md, "Property
  Testing".
- Failure-path filesystem fixtures must not depend on ambient Unix permissions
  to make operations fail: CI containers may run as root. Use feature-gated,
  per-invocation fault seams with no global mutable state; a seam visible
  across threads is keyed by the exact operation target and returns cleanup
  ownership from registration.
- The smoke-only Cargo features `smoke-read-faults`, `smoke-list-budgets`,
  `crash-kill-points` and `responsiveness-probes` are never enabled in release,
  Meson, Flatpak, or Snap builds. There, code gated on them compiles as empty
  inline functions (`services::read_fault::denied`,
  `services::smoke_list_budgets`, `services::kill_point::reach`,
  `ui::responsiveness_probe::install`). Smoke fixtures are synthetic; never copy
  a user file into one.

## Proof Obligations of a Change

- **Rustdoc lint gate, run by hand.** It is not in `make check`,
  `make pre-commit` or `make check-policy`, but CI's `Lint` job enforces it:

  ```
  RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links -D rustdoc::bare_urls" \
    cargo doc --workspace --no-deps
  ```

  Run it whenever a change adds or edits doc comments on a public module, and
  always for a new workflow facade or role home: public module documentation
  must not intra-doc-link private items. The fix is **always** to drop the link
  and keep the name in backticks; never widen an item's visibility to satisfy
  documentation.
- **`git add -N` new files before any diff-aware gate.**
  `make check-visual-proof-policy`, the diff-aware half of
  `make check-accessibility-policy`, and `make mutants-diff` build their
  changed-file set from `git diff <base>`, which omits untracked paths. A green
  diff-aware gate on a worktree with untracked new source files is not
  evidence. When adding the files changes a gate's digest and it starts
  failing, re-run the lane; never unstage the files to take the earlier green.
- **Visual proof.** Screenshot-reported or geometry-sensitive UI fixes run
  `make visual-geometry-smoke` when the invariant is same-session pixel
  stability across a layout action; a pixel-visible effect with a named
  invariant must appear in the root summary's `pixel_verified_invariant_ids`.
  `make check-visual-proof-policy` (in `make check-policy`) fails visual-sensitive
  worktree changes until a passing, unfiltered summary under
  `build/smoke/visual-geometry` matches the current diff; skipped, filtered,
  name-only, rectangle-only, or stale runs do not count. Runners start from a
  clean artifact root. A bug first seen in a live window is captured with
  `scripts/lushtext-automation.py visual-geometry-capture ...` and replayed
  headless.
- **Accessibility proof.** `make check-accessibility-policy` (in
  `make check-policy`) requires, while accessibility-relevant files are dirty,
  accessibility and visual smoke summaries whose source fingerprint matches
  the current relevant-file contents.
- **Files under the UI prefixes need the three smokes.** The visual-sensitive
  and accessibility-relevant sets are selected by **prefix**
  (`crates/lushtext-core/src/ui/`, `crates/lushtext/tests/widget/`,
  `resources/ui/`, `resources/style/`), so any file there, documentation
  included, makes a commit need `make visual-geometry-smoke`,
  `make accessibility-smoke` and `make visual-smoke` current. The proofs digest
  file contents in `git status`, not a commit range: plan a series so those
  files change in as few commits as possible.
- Filesystem-sensitive changes pass `make check-agent-docs`;
  automation-surface changes pass `make check-automation-docs`, and a change to
  `scripts/lushtext-automation.py` also `make automation-client-self-test`
  (both in `make check-policy`).
- **Path-keyed gates.** A gate is path-keyed when a configuration file or policy
  implementation selects what it protects by naming files literally (exact
  path, an explicit scope entry, a `line:column` anchor) rather than by a
  convention: `.cargo/mutants.toml` (`examine_globs`, `exclude_globs`, path- or
  symbol-anchored `exclude_re`), the native-minimap and geometry predicates
  implemented **twice** in `scripts/check-visual-proof-policy.py` and
  `crates/cargo-gtk-proof/src/policy.rs`, `scripts/check-accessibility-policy.py`,
  `scripts/accessibility_source_fingerprint.py`,
  `scripts/check-filesystem-boundary.sh`, `scripts/run-performance-smoke.sh`
  (keyed on test names and evidence labels), and
  `scripts/check-session-hygiene.py` (its exemptions for the session helper's
  directory and for itself are path-keyed and fail-closed: a renamed helper
  becomes a violator, and the gate fails when the helper has zero sanctioned
  spawns). When a change relocates, renames,
  or splits a file such a gate names: re-key or retire the entry in the same
  change and record which; re-key **every** implementation, with a parity
  assertion in each one's self-tests; prove the re-key by running the gate on
  the final state, because a gate that matches nothing passes; and do not
  weaken the gate. Prefer a prefix or naming-convention key when re-keying.
- Workflow-readability changes pass `make check-workflow-boundaries` (in
  `make check-policy`); the convention is
  [`workflow-convention.md`](./workflow-convention.md).

## Gate Order Before a Commit

Run the cheap gates first, with the same `make` targets the hook runs:
`make fmt`, `make check-fmt`, `make check-clippy` (it adds project lints a plain
`cargo clippy` lacks), `make check-workflow-boundaries`,
`make check-session-hygiene` when a lane, a workflow or the session helper
changed, `make check-automation-docs` when the automation surface changed, and
`cargo check -p lushtext --bins --locked` with default features (all-feature
Clippy hides `test-utils`-only imports). Only then run the accessibility,
visual and visual-geometry smokes, and commit right after them: their proofs
fingerprint file contents, so any edit after a smoke, `make fmt` included,
makes them stale and the hook refuses the commit. Commit through
`scripts/heavy.sh` (see verification-runs.md, "Shared-machine agent work").

## Kani Harnesses

How the lane runs and how shards are budgeted is in build.md, "Formal
Verification (Kani)".

- **Placement.** Harnesses live in a `#[cfg(kani)] mod kani_proofs` of the
  crate that owns the checked code, over GTK-free, I/O-free code that ships or
  a pure core production drives — never a copy of the code. A harness module is
  a file named `kani_proofs.rs`, declared by its parent as
  `#[cfg(kani)] mod kani_proofs;`, in the checked module's **child** directory
  (`ui/window/geometry/policy/kani_proofs.rs`, not a sibling of `policy.rs`),
  so it reaches private helpers without widening their visibility and no
  `cfg(kani)` code lands inside a `ui/**/policy.rs` mutation-scope file. The
  name is how `.cargo/mutants.toml` and `make check-workflow-boundaries` treat
  it as verification code; an ungated or orphaned `kani_proofs.rs` is a finding.
- **Harness style.** Use only `kani::any`, `kani::assume`, `kani::cover!`,
  `#[kani::unwind]`, `#[kani::should_panic]`, and `#[kani::solver(...)]` (the
  last only under the fit order). No function or loop contracts. An
  experimental Kani flag comes only through the shard's `kani_flags` entry in
  `scripts/kani-shards.py`, never a global flag. Put a `kani::cover!` on each
  interesting branch. A harness proving a property on a restricted domain
  states that domain in the harness **and** in the checked function's rustdoc
  or spec; a rustdoc promise never claims more than was proved. Pin a
  known-unsafe ordering or guard removal with a `should_panic` harness.
- A model that treats GTK as nondeterministic constrains it only with
  `kani::assume` clauses citing ledger axiom ids from
  `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`.
- **Membership.** Every harness belongs to exactly one shard and appears in
  the `ORACLES` table (or an explicit `NOT_ORACLE_*` exception) of
  `scripts/kani-shards.py`; `make check-kani-shards` fails otherwise. Run new
  harnesses through `make kani`, never a bare `cargo kani`, so the memory cap
  applies.

## GTK Axiom Probes and Samples

The lane and the toolkit-update procedure are in build.md, "GTK Axioms and
Toolkit Updates".

- A sample with its own measurement modes declares them in a `SampleFlags` and
  calls `support::guard(&FLAGS)` as the first statement of `main`;
  `make check-gtk-axioms` fails a sample that reads flags without it.
- **Never run a sample without `--check` (or a measurement mode) outside a
  private headless session**: with no argument it opens its interactive window
  on whatever desktop it was started from.
- Never adjust a probe to match new toolkit behaviour first: a failing probe is
  an axiom change. Revisit the ledger entry, its dependent designs and every
  Kani `kani::assume` that cites the id before editing the probe, and
  version-qualify the entry rather than rewrite it.
- A probe whose fixture must run for seconds may be marked `slow` in the
  catalogue, with its ledger row saying `**slow**` and where it runs (nightly);
  never raise the `GTK Axioms` job's `timeout-minutes` instead.
- A probe that provokes a toolkit CRITICAL on purpose captures it through the
  crate's log-capture guard and restores the previous writer. A new warning
  class gets a `scripts/run-gtk-axioms.sh --self-test` case, never a wider
  benign list.

## Runtime Warnings

**GTK/pixman warnings are bugs, not noise.** When running the app via
`make run`, stderr must be free of:

- `*** BUG *** In pixman_region32_init_rect: Invalid rectangle passed` — a
  widget is being allocated with zero or negative dimensions, typically a raw
  pane child animated to 0 instead of a clipping wrapper (for example
  `GtkRevealer`) hidden at the collapsed endpoint;
- `Gtk-CRITICAL` or `Gtk-WARNING` — usually incorrect widget lifecycle, invalid
  property access, or constraint violations;
- `GLib-GObject-WARNING` — usually signal or property misuse.

**Development is not finished if any of these appear during normal usage.**
Before considering a UI change complete, run the app and exercise the affected
feature (toggle sidebar, resize window, open/close tabs, etc.) while watching
stderr, and fix the root cause rather than suppressing the warning. A change
touching a `GtkRevealer` or a heavy animated sidebar subtree also needs a real
`make run` cycle with restored workspaces and an stderr check for
`Trying to measure …` warnings; widget tests are not sufficient.
