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
  runtime roots in `finally`, keeping only a listing in the artifacts. A
  `( … )` subshell has no traps (bash resets them), so a path is created and
  registered in the shell that holds the trap, never inside a subshell, or it
  is an unlinked descriptor and needs no registration.
- **A runner never captures a command's whole output.** A scan of the output
  of a command it runs (warnings, a phrase, result lines) goes through
  `scripts/bounded-log-scan.sh`: a stage beside the live stream that keeps an
  exact count, at most 64 KiB of unexpected lines and at most 1 MiB of the
  lines the runner parses later, on files unlinked as soon as they are open.
  A whole-output temp file in `/tmp` is tmpfs charged to the run's memory
  scope, and a flooding test once turned four of them into 4 GiB OOM kills
  ([why](../../docs/memorials/2026-10-20-test-data-incidents.md)).
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
  `gtk-lush-proof-runtime-*` form). A live run is never swept, nor is a plain
  `mktemp` name (`tmp.*`), which nothing proves is ours. The prefixes are in
  `with-temp-scope.sh`'s `PID_PREFIXES`; besides the test roots they cover the
  scan stage's open window (`lt-scan-`), `heavy.sh`'s admission report
  (`lt-heavy-`) and `with-memory-cap.sh`'s envelope pid file
  (`lt-envelope-`). A runner's other temp entry that a child must name lives
  in such a directory too.
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
  lane the same way. `make check-temp-scope` (in `make check`) self-tests it,
  through `scripts/run-widget-tests.sh --self-test`, which also runs the
  bounded scan stage's self-test.
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
  `TMPDIR`, from `lushtext_services::test_isolation::isolated_environment`,
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
  ownership from registration. The durable-write seam is the conformance tap
  (`services/filesystem/backend_tap.rs`): per-invocation and thread-local.
  Its crashes unwind with `resume_unwind`, so use one only where no lock or
  I/O-doing `Drop` is on the unwind path (the draft service's manifest lock
  `expect`s on poison, so draft-level crashes fall between service calls).
- The smoke-only Cargo features `smoke-read-faults`, `smoke-list-budgets`,
  `crash-kill-points` and `responsiveness-probes` are never enabled in release,
  Meson, Flatpak, or Snap builds. There, code gated on them compiles as empty
  inline functions (`services::read_fault::denied`,
  `services::smoke_list_budgets`, `services::kill_point::reach`,
  `ui::responsiveness_probe::install`). Smoke fixtures are synthetic; never copy
  a user file into one.

## Proof Obligations of a Change

- **Rustdoc lint gate and local CI parity (`make verify-local-fast`,
  `make verify-local`).** CI's `Lint` job enforces:

  ```
  RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links -D rustdoc::bare_urls" \
    cargo doc --workspace --no-deps
  ```

  `make check` / `make pre-commit` keep fast static iteration (fmt, Clippy,
  `check-policy`); `make verify-local-fast` adds `cargo check -p lushtext
  --bins --locked`, the `rustdoc` gate above, and `cargo bench --no-run`;
  `make verify-local` defaults to `VERIFY_RUN_SMOKES=auto` (short-circuiting
  clean docs-only branches to `make pre-commit-docs` unless `VERIFY_FORCE_FULL=1`,
  else running `verify-local-fast`, `cargo deny`, `make test`, `make test-prop`,
  `make fuzz-corpus-replay`, conditional GTK Lush checks via `precommit-scope.py`,
  only the smoke lane(s) needed by the diff, and `make check-smoke-proof`). Run
  `make verify-local-fast` before opening or updating a PR, and
  `make verify-local` before merging. Public module docs must never
  intra-doc-link private items — drop the link and keep the name in
  backticks; never widen visibility for docs.
- **`git add -N` new files before any diff-aware gate.**
  `make check-smoke-proof` (`scripts/check-visual-proof-policy.py`,
  `scripts/check-accessibility-policy.py`) and `make mutants-diff` build their
  changed-file set from `git diff` + `git status`, which omits untracked paths.
  A green diff-aware gate on a worktree with untracked new source files is not
  evidence. When adding files changes a digest, re-run the affected smoke lane;
  never unstage files to keep an earlier green.
- **Branch-level smoke freshness (`make check-smoke-proof`) and S7(b) closure.**
  Live smoke freshness is enforced at branch scope over `origin/main...HEAD` (or
  `SMOKE_PROOF_BASE_REF`) unioned with working-tree status by
  `make check-smoke-proof` — called automatically by `.githooks/pre-push` and
  `make verify-local` — rather than inside `.githooks/pre-commit`. This closes
  recorded hole S7(b) (committed UI diffs can no longer bypass smoke freshness on
  a clean branch) while letting intermediate and `/swarm` commits iterate without
  Mutter churn. Because `changed_files_digest` unions `base_ref...HEAD` with
  working-tree status, a passing `make smoke-ui` summary survives `git add` and
  `git commit` as long as file bytes remain unchanged.
- **Visual proof.** Screenshot-reported or geometry-sensitive UI fixes run
  `make visual-geometry-smoke` (or `make smoke-ui`) when the invariant is
  same-session pixel stability across a layout action; a pixel-visible effect
  with a named invariant must appear in the root summary's
  `pixel_verified_invariant_ids`. `make check-smoke-proof` fails branches with
  visual-sensitive changes until a passing, unfiltered
  `build/smoke/visual-geometry/summary.json` matches the branch diff; skipped,
  filtered, name-only, rectangle-only, or stale runs do not count. Runners
  start from a clean artifact root. A bug first seen in a live window is
  captured with `scripts/lushtext-automation.py visual-geometry-capture ...` and
  replayed headless.
- **Accessibility proof.** `make check-smoke-proof` runs
  `check-accessibility-policy.py --strict-current-tree --base-ref ...` and
  requires accessibility and visual smoke summaries whose source fingerprint
  matches the current relevant-file contents when any smoke-relevant path
  changed on the branch or worktree. Meanwhile `make check-policy` runs
  `make check-accessibility-policy` (which invokes `--static-only`) and
  `make check-visual-proof-policy` (`--self-test` parity checks only).
- **Files under the UI prefixes need the three smokes (Decision 7 note).**
  The visual-sensitive and accessibility-relevant smoke sets are selected by
  prefix (`crates/lushtext-ui/src/ui/`, `resources/ui/`, `resources/style/`),
  plus `scripts/lushtext-automation.py` and the executable smoke/AT-SPI scripts
  in `accessibility_source_fingerprint.py`. Pure Markdown documentation files
  (`docs/*.md`) stay outside runtime smoke fingerprints and are audited statically
  by `check-accessibility-policy --static-only`, `check-automation-docs`, and
  `check-end-user-smoke-workflow` (including under `check-policy-docs`). Per
  2026-11-18 Decision 7,
  `crates/lushtext/tests/widget/` is excluded from both smoke fingerprints
  while retained in `is_policy_path` (static accessibility policy audits +
  `make test-widget-headless`); residual risk is bounded because widget tests do
  not compile into the shipped `lushtext` binary (`cargo check -p lushtext
  --bins --locked`) and test behavior is verified by `make test-widget-headless`.
  On non-visual branches, `make check-smoke-proof` exits early in pure Python
  without invoking Cargo. Any edit under `crates/lushtext-ui/src/ui/`,
  `resources/ui/`, or `resources/style/` requires `make smoke-ui` (or
  `make verify-local`, which auto-selects it via `VERIFY_RUN_SMOKES=auto`).
- Filesystem-sensitive changes pass `make check-agent-docs`;
  automation-surface changes pass `make check-automation-docs`, and a change to
  `scripts/lushtext-automation.py` also `make automation-client-self-test`;
  Python script changes pass `make check-python-hygiene` (all in
  `make check-policy`).
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

Run the cheap static gates while iterating:
`make fmt`, `make check` (`make check-fmt` + all-feature `make check-clippy` +
`make check-policy`, which writes a 30-minute clean-worktree `git write-tree`
stamp so an unchanged `git commit -S` can reuse it without re-running Clippy),
and `make verify-local-fast` (`cargo check -p lushtext --bins --locked`,
strict `rustdoc`, `cargo bench --no-run`). Complete `/swarm` code review and
review fixes (plus any standalone In-PR archive) **before** running
`make smoke-ui GATE_TREES_CHANGE=<change>` once and `make verify-local`
(including `make check-smoke-proof`) on the exact final `HEAD^{tree}`, then push
(enforced by `.githooks/pre-push`) and merge. Commit through `scripts/heavy.sh`
(see verification-runs.md, "Shared-machine agent work").

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
