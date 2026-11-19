# Design

## Context

See `proposal.md`, "Why", for the measured gaps. Three facts shape the approach:

- **The local gate is the authority before merge** (`.agents/rules/parallel-agent-work.md`, "Merge on local evidence"). CI closes the hole left by a push without the hook; it does not replace the hook.
- **Some `check-policy` prerequisites cannot run, or prove nothing, on a GitHub runner** (table in D1).
- **The visibility subscription is the only application-settings subscription in a window or widget whose handler id is discarded.**
  - The editor page's `let id = settings.connect_changed(...)` sites track the id in a `SignalBag`, sometimes with the call on the next line (`editor_page/imp.rs:1065-1066`).
  - The sidebar tracks its pair, and the writer's `connect_changed_for` holds its owner weakly.
  - `ui/theme.rs` has three discarded sites, all installed once per process by `load_css`:
    - line 99, a loop over three keys;
    - line 131, a loop over two keys;
    - line 104, on the desktop interface settings rather than the application's.
  - About 25 other `connect_changed` calls in `lushtext-core` are on entries, buffers, adjustments and file monitors, several as bare statements.

## Goals / Non-Goals

**Goals:** one place that says, per audit, where it runs; a subscription API that cannot leak; committed evidence for the two code comments.

**Non-Goals:** changing what any audit checks, or the hook's scope (`proposal.md`, "Non-goals").

## Decisions

### D1. An explicit local-only registry with a coverage check, not a copied list in `ci.yml`

- **Shape.** `check-policy-ci` is a Makefile target whose prerequisites are the CI-runnable audits. CI runs it as one `Lint` step. A local-only table lists every remaining prerequisite with its reason.
- **The coverage check.** `scripts/check-policy-ci.py` with a `--self-test` reads three lists: the `check-policy-ci` prerequisites, a covered-elsewhere table (target and the CI job that runs it), and the local-only table (target and reason). It also reads `check-policy`'s prerequisites from the Makefile. It fails on a prerequisite on none of the lists or on more than one, and on an entry that is not a prerequisite. The covered-elsewhere table is what keeps a target from running twice in CI.
- **One per line.** `check-policy` and `check-policy-ci` list one prerequisite per line, so the parser reads a stable shape. This was promised to `smoke-gate-coherence` (PARALLEL-LEDGER, 2026-11-15 03:30); taking it here is a **proposal to the coordinator**, not a decision of this change. If the coordinator keeps it there, this change parses the current one-line form and leaves the layout alone. No parser of the `check-policy:` line exists in `scripts/`, `.githooks/` or `.github/` on `origin/main` (comments only); task 1.1 re-checks.
- **Preliminary classification of the 33 prerequisites,** to be settled by the first CI run:

  | Class | Prerequisites |
  | --- | --- |
  | Already in CI as `make` steps (8) | `check-filesystem-boundary`, `check-workflow-timeouts`, `check-flatpak-ci-builds`, `check-explicit-simd`, `check-workflow-boundaries`, `check-blueprint`, `check-gtk-lush-policy`, `gtk-lush-adoption-matrix` |
  | Covered by another CI job (3) | `check-cargo-public-api-install` (the public-API advisory job runs its script), `check-widget-shards` (the widget shard-table job), `check-terminology` (its `workspace_terminology` test runs in the non-widget nextest step; confirm in the first run) |
  | Static, expected to run in CI (13) | `check-precommit-scope`, `check-session-hygiene`, `check-settings-writes`, `check-font-zoom-registration`, `check-signal-connections`, `check-thresholds`, `check-responsiveness-lane`, `check-kani-shards`, `check-gtk-axioms`, `check-automation-docs`, `check-flatpak-permissions`, `check-end-user-smoke-workflow`, `automation-client-self-test` |
  | Decided by the first run (4) | `check-kill-point-gating`, `check-flatpak-source-tree`, `check-temp-scope`, `check-rustc-frontend-threads` |
  | Local-only (5) | `check-agent-helpers` and `check-build-cache-helpers` (host systemd user slices and build-cache root); `test-flatpak-install-origin` (prints `SKIP:` without `flatpak`, so it proves nothing on the runner); `check-accessibility-policy` and `check-visual-proof-policy` (they verify local smoke summaries against the dirty tree; on a clean checkout they pass without proving anything) |

- **Baseline for the job's budget.** In the last three green `main` runs (44479238042, 44416653444, 44413424957) the `Lint` job took 1 min 43 s, 1 min 57 s and 2 min 03 s, against `timeout-minutes: 25`. Of `check-policy`'s recipes only `cargo run -p cargo-gtk-proof -- policy` and the terminology test call `cargo`; the rest are scripts. So the aggregate joins `Lint`. A separate parallel job remains the fallback, decided by the two runs task 1.6 measures.
- **First run.** A prerequisite that fails on the runner for an environmental reason moves to the local-only table with that reason. One that fails on a real finding is fixed (`preexisting-blockers.md`).
- **Rejected:** six more `run:` steps in `ci.yml` (the audit's proposal). Nothing would keep the two lists in step, and the next new audit would repeat the gap.

### D2. Run `check-fmt` itself in CI

- CI replaces `cargo fmt --all -- --check` with `make check-fmt`, so both sides run the same recipe. The recipe already refuses an empty widget glob (`Makefile:842`).
- `rustfmt` comes from the toolchain step already installed there.

### D3. `workspace-hack`: record the family's exclusion, add the missing dependency, check all three in CI

The investigation of 2026-11-19, recorded with its figures in `docs/next/refactor-1-disposition.md` ("workspace-hack"), answered the open question.

- **The family's exclusion is right but unrecorded.** None of the eight family crates ever had the dependency (`git log -S workspace-hack` on each manifest is empty). The rule `rust.md:12` (`4714ac79`, 2026-04-01) predates the family by two months.
- **Why the family stays out.** With the dependency:
  - `make gtk-lush-stock-fixtures` fails under `--locked`;
  - the stock consumer goes from 84 to 143 packages, against `GOVERNANCE.md:3-5`.
- **The adoption lab is excluded by choice** (maintainer, 2026-11-19). `docs/gtk-lush-adoption/matrix.toml:77` describes it as a second consumer that "adds one dependency". The cost is that `scripts/run-gtk-axioms.sh:382` builds it alone, with its own feature resolution.
- **`cargo-gtk-proof` lacks the dependency by omission.** It is `publish = false`, GPL, and built alone by the hook's `check-visual-proof-policy`, so it gains it.
- **Recording:** `[final-excludes] workspace-members = [<8 family crates>, "gtk-lush-adoption-lab"]` in `.config/hakari.toml`, with a comment naming the reason.
  - `final-excludes` keeps them in the unification. The generated `workspace-hack/Cargo.toml` was measured unchanged with eight excluded and with all ten. The chosen nine plus the `cargo-gtk-proof` dependency is verified by task 1b.1.
  - `traversal-excludes` would drop them from the unification and change the generated crate. It was rejected.
- **CI.** `CARGO_HAKARI_VERSION: "0.9.37"`. The step runs `cargo hakari generate --diff`, `verify` and `manage-deps --dry-run`, all expected to exit 0 after the edit.
  - It runs only when a diff against the base touches `**/Cargo.toml`, `Cargo.lock`, `.config/hakari.toml` or `rust-toolchain.toml`.
  - The guard is a shell `git diff --name-only` inside `Lint`, not a job `paths` filter, because a required check skipped that way stays pending.
- **Policy check.** `scripts/check-gtk-lush-policy.py` rejects `workspace-hack` in a family crate's dependencies. It also fails when the `final-excludes` list differs from the family packages plus the lab (`GTK_LUSH_PACKAGES`, `Makefile:166`), with `--self-test` cases.
- **Rejected:**
  - giving all ten crates the dependency, which breaks the stock fixtures and the family's contract;
  - `traversal-excludes` (above).

### D4. The subscription takes its owner

- **Signature.** `connect_visibility_changed(owner, handler)` mirrors `settings_writes::connect_changed_for`: it holds the owner weakly and disconnects both handlers from `add_weak_ref_notify_local`, which GLib runs when the owner's weak references are notified at dispose.
  - The handler is `Fn(&O)` and is shared by two connections, so it is held in an `Rc`.
  - The function returns `()`.
- **Callers.** The sidebar drops its `visibility_signals` tracking for this pair, and the window passes `&*obj`.
- **The gate.** `check-settings-writes.py` gains one rule:
  - **Scope.** It matches only the two-argument settings form `.connect_changed(\s*Some(`, outside `ui/settings_writes.rs` and `ui/workspace_visibility.rs`, the owner helpers.
  - **What fails.** It walks back to the start of the statement, across lines. It fails when the id is discarded: `let _ =`, `let _<name> =` (an underscore binding drops the id at the end of its scope, and dropping a `SignalHandlerId` does not disconnect), or a bare statement ending in `;`. It passes when the id is bound to a used name (`let id =`), iterated (`for … in`), passed as an argument, or returned, including as a block's tail expression with no `;` (the form `workspace_visibility.rs` uses).
  - **Allowlist.** It accepts `ui/theme.rs` with an expected count of 3, so a fourth discarded site there fails.
  - **Self-test cases:** `let _ =`, `let _unused =`, a bare statement, `let id =` on the previous line, `for id in …`, a block's tail expression, `buffer.connect_changed(move |_|` (single argument, not matched), the owner-form call, and the theme count.
- **Rejected:**
  - Keeping the returned ids and asking the window to track them. Correct, but the next caller can discard them again.
  - A rule matching every `connect_changed(`. It would flag about 25 legitimate widget calls.
  - Matching `connect_visibility_changed` calls. They return `()` after D4, so every correct call would look like a discard.

### D5. Commit the C2 sweep table

- `sweep-table.md` (176 lines, 17,341 bytes) moves to `docs/benchmarks/c2-head-sweep.md` with a header naming its origin, date and the `measure.sh` runs it summarises. The raw `runs/` directories stay out of the repository.
- `protected_fold.rs:74` and `docs/next/long-line-documents.md:1737` cite the committed file.
- `sr_measure.rs:8` cites `main_m9.rs.add`: `main`'s side of the M9 measurement, as the file's own header says, and still present on this workstation (3,017 bytes). It is committed next to the stream-replace-all record and cited there.

### D6. `DurableWriteError`: a `Display` per phase, `source()` to the I/O error

- **Shape.** `#[derive(Debug, thiserror::Error)]` with:
  - `#[error("durable write failed before the destination was replaced")] BeforeRename(#[source] io::Error)`;
  - `#[error("new bytes are in place but the parent-directory sync failed, so durability is unconfirmed")] AfterRename(#[source] io::Error)`.
- **Existing API stays.** `io_error()` and `into_io_error()` stay, so the 18 flattening call sites are untouched.
- **Where it is rendered.** Editor saves and set-aside copies match on the variants (`editor_io.rs:1838`, `set_aside.rs:193`), so their messages, which come from `EditorSaveError`, do not change. Other callers render it:
  - `search_backup.rs:1805` returns it through `?` into an `anyhow::Result`;
  - `draft_service.rs:2082` and `local_history_service.rs:1345` wrap it with `.with_context`.
  
  With today's `Display`, those messages end in the I/O text. With a phase `Display` and `source()`:
  - a `{:#}` or `{:?}` rendering shows the phase and then the I/O cause;
  - a plain `{}` rendering of an error whose root is the `DurableWriteError` would show only the phase.
- **Inventory and pin first.** Task 4.0 lists every path from these functions to a renderer (`tracing` fields, notifications, `{}`, `{:#}`) and pins each message with a test. A path that renders with `{}` and would lose the I/O text switches to the chain form (`{:#}`), so the cause stays visible.
- **Rejected:** `#[error(transparent)]`. Its `source()` is the I/O error's own source (`None` for an OS error), so it adds nothing.

### D7. The print test hook restores through a guard

`with_print_runner_for_test` installs the runner, keeps the previous one in a `Drop` guard that puts it back, then runs `f()`. This matches the audit's Example 4 "After" without the file merge, which the disposition record declines (Tier Compacto).

### D8. The disposition record

`docs/next/refactor-1-disposition.md`, in English, has one row per audit item (report §3.5 defects, §4.4 CI gaps, proposal Fronts A-D, the Miri section). Each row gives the item, the measured fact, and either the change that takes it or the reason it is declined, with evidence.

- **Corrections.** Where the audit's figure was wrong, the row gives the measured one: 12 `mutants.toml` anchors, not 43; 581 `.downgrade()` sites in 134 files.
- **What stays out.** The Portuguese audit is not committed. The record links to nothing outside the repository.

## Risks / Trade-offs

- **The `Lint` job grows past its 25-minute timeout.** → Measure two runs before and after. Move `check-policy-ci` to its own job if needed. Never raise `timeout-minutes` past the policy ceiling.
- **An audit green locally fails on the runner.** → A found defect or an environmental limit; D1 handles both, and the change does not merge until CI is green.
- **The leak test finds that a window is never finalized.** It depends on the window and sidebar being finalized after close. If something else keeps the window alive, that is a pre-existing blocker to fix in this change (`preexisting-blockers.md`), not a reason to weaken the test.
- **\`verify-shell-conformance-against-proven-cores\` touches \`durable_write.rs\`.** → D6 starts only after it merges (the Start line), and task 0.1 re-reads the file then.
- **The owner form changes when the window's handler can run.** It can no longer run after the window is disposed. That is the intended behaviour: the old closure already did nothing then.

## Migration Plan

- Each task group lands on its own; the CI group needs its own green runs (CI-evidence exception).
- Rollback is a revert. No persisted state changes.

## Verification scope

- **Kani:** `core-protected-fold` (comment-only edit in `model/protected_fold.rs`; a pull-request shard, left to CI). `services/durable_write.rs` is the shell over the Kani-checked `write_protocol.rs`. Task 0.1 checks with `scripts/kani-shards.py` whether `verify-shell-conformance-against-proven-cores` made it a harness input, and runs that shard once if it did.
- **Timing:** none (no timed path touched).
- **Axioms:** none relied on or added.
