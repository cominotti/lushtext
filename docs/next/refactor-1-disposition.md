# Refactor-1 audit: disposition

An external code-quality audit of LushText ("refactor-1", 2026-11-18, kept outside the repository) graded the workspace and proposed refactors in four fronts. This record is the repository's own account of it:

- every audit item, with the fact measured in the repository, re-measured on `origin/main` `761866ff`;
- the change that takes the item, or the reason it is declined;
- the evidence behind each decision, so that none depends on the audit text, a session scratchpad or a conversation.

The six changes it names live under `openspec/changes/`. None starts before the programme coordinator's queued work has finished and the maintainer places it in `PARALLEL-LEDGER.md` (maintainer, 2026-11-19). Each starts by re-measuring the figures below, and its task 5.1 or 0.1 updates this record.

## Corrections to the audit's figures

| Audit claim | Measured (`origin/main` 761866ff) |
| --- | --- |
| 22 of 32 `check-policy` audits outside CI | 33 prerequisites. CI's `Lint` runs 8 as `make` steps and 2 more through their scripts (`install-cargo-public-api.sh --self-test`, `widget-shards.py github-outputs`); 23 run only in the local hook. |
| 659 manual `.downgrade()` sites in 136 files | 581 in 134 files of `ui/`. 705 `.upgrade()` calls: 301 plain `if let`, 176 `let … else`, 27 in `if let` chains, 96 split over lines. |
| 43 `file:line:column` exclusions in `.cargo/mutants.toml` | 12. A moved anchor stops matching and its mutant reappears as a survivor, so the failure is loud. |
| `model/` has zero `pub(crate)` | 10. 33 `model` modules are named from `crates/lushtext`, benches or fuzz, so most of the `pub` surface is used across crates. |
| 174 `Result<_, String>` in `cargo-gtk-proof` | 172: `live.rs` 52, `runner.rs` 36, `automation.rs` 16, `capture.rs` 16, `policy.rs` 14, `process.rs` 12, others 26. |
| 23 `Arc<PathBuf>` in the palette index | 26, about half in the test-only oracle `index/reference.rs`. |
| "Zero `.unwrap()`" as the panic-safety measure | True, but about 1,969 `.expect(` calls exist in `lushtext-core` (tests included), and `expect_used` is not denied. |
| `transient_dismissal/` is "an `if` of about 15 lines" | Its `policy.rs` has about 150 production lines and four public functions; the directory has 760 lines with one documented async inversion. |
| The Open-dialog future holds no strong window reference | False. `clone!(#[weak])` upgrades inside the generated `async` block (glib-macros 0.22.9 `clone.rs:545-549`) and holds the window across the `.await`. |

## Taken

| Item | Change |
| --- | --- |
| CI format check misses `crates/lushtext/tests/widget/*.rs` (`ci.yml:67` runs `cargo fmt --all`) | `close-audited-gate-gaps` |
| 23 policy audits only in the hook | `close-audited-gate-gaps` (`check-policy-ci` plus covered-elsewhere and local-only lists, checked by a script) |
| No `workspace-hack` check | `close-audited-gate-gaps` (see "workspace-hack" below) |
| `proof-strength.py --self-test` runs twice (`Makefile:460`, `:570`) | `close-audited-gate-gaps` |
| Window leaks two handlers per close on the shared settings (`ui/window/imp.rs:853`) | `close-audited-gate-gaps` |
| `DurableWriteError` lacks `source()` | `close-audited-gate-gaps` |
| Print test hook restores without a `Drop` guard (`print/test_policy.rs:27-37`) | `close-audited-gate-gaps` |
| Production comments cite the gitignored `.claude/handoffs/` (`protected_fold.rs:74`, `sr_measure.rs:8`) | `close-audited-gate-gaps` (the other 49 references in `docs/` are a separate audit) |
| `kind: Cell<u8>` row models with parallel fields | `adopt-typed-row-state-and-dialog-futures` |
| Rows built by hand and navigated by `next_sibling` (34 calls) | `spike-list-row-construction`, then `adopt-typed-row-state-and-dialog-futures` |
| `unsafe set_data` for row state (4 sites) | `adopt-typed-row-state-and-dialog-futures` (to zero, with an audit) |
| Continuation-passing Open dialog | `adopt-typed-row-state-and-dialog-futures` (behind a characterization test) |
| Manual weak captures instead of `glib::clone!` | `adopt-glib-clone-weak-captures` (after the typed rows, with a ratchet) |
| `Result<_, String>` and free-string statuses in `cargo-gtk-proof` | `type-cargo-gtk-proof-errors` |
| `SettleBurst::schedule` keeps superseded sources (`settle/src/lib.rs:281-299`) | `fix-settle-burst-sources-and-weak-refs` |
| `clone().upcast()` before `downgrade()` (`signals/src/lib.rs:73`, `viewport/src/lib.rs:271`) | `fix-settle-burst-sources-and-weak-refs` |

## workspace-hack

cargo-hakari 0.9.37 on `main`: `generate --diff` exits 0, `verify` exits 0, and `manage-deps --dry-run` exits 1. It asks to add `workspace-hack` to ten crates: the eight GTK Lush family crates, `gtk-lush-adoption-lab` and `cargo-gtk-proof`. `.agents/rules/rust.md:12` says every crate depends on it.

**Findings** (investigation of 2026-11-19, read-only, with experiments on disposable copies of the tree):

- **The family never had the dependency.**
  - None of the ten manifests ever named `workspace-hack` (`git log -S workspace-hack` on each is empty).
  - The family arrived in `3589824c`, `16003e97`, `3c5efff5` and `a7fdb525` (2026-06-10 to 06-12) and `7bda64d8` (2026-09-25). The rule dates from `4714ac79` (2026-04-01).
  - `.config/hakari.toml` has one commit (`90343269`) and no exclusions.
  - The archived family changes ran `generate` and `verify`, never `manage-deps`.
- **Adding it to the family breaks the family's contract.**
  - `make gtk-lush-stock-fixtures` fails under `--locked`. Regenerating the stock fixture's lock adds 252 lines, and the fixture's packages go from 84 to 143 (`clap`, `serde`, `tracing-core`, `syn`, `toml_*`, `winnow` …).
  - It would contradict `crates/gtk-lush/GOVERNANCE.md:3-5`, and MIT/Apache crates would depend on a GPL package.
- **hakari can record the exclusion two ways, and only one fits.**
  - `[traversal-excludes]` makes `manage-deps` pass but changes the generated crate: it drops `gio`, `glib`, `pango`, the `*-sys` crates, `bitflags`, `memchr` and more.
  - `[final-excludes]` makes `manage-deps` pass and leaves `workspace-hack/Cargo.toml` unchanged. Measured with the eight family crates excluded, and again with all ten.
- **`cargo-gtk-proof` has none of the family's reasons.** It is `publish = false` and GPL, no stock fixture depends on it, and it is built on its own by the hook's `check-visual-proof-policy` (`Makefile:941`).
- **The adoption lab is excluded by choice.**
  - `docs/gtk-lush-adoption/matrix.toml:77` describes the lab as a second consumer that "adds one dependency".
  - The cost is that `scripts/run-gtk-axioms.sh:382` builds the lab alone, with its own feature resolution.

**Decision (maintainer, 2026-11-19):**
- the eight family crates and the lab go into `[final-excludes]`;
- `cargo-gtk-proof` gains the dependency;
- `rust.md` and `build.md` say so;
- CI runs all three hakari commands, and the family policy check compares the exclusion list with the family packages.

The configuration "nine excluded plus the `cargo-gtk-proof` dependency" was not itself measured. `close-audited-gate-gaps` task 1b.1 verifies that `workspace-hack/Cargo.toml` stays unchanged.

## Declined, with reasons

- **`#[gtk4::template_callbacks]`.** A template callback's return value is converted with `From` and checked only at emission.
  - In glib 0.22.10 (`closure.rs:254`), a closure that answers `None` where the signal expects a value panics inside the C trampoline, which aborts the process. That is the class of defect behind GTK axiom ledger A122.
  - Template signal handlers also connect by runtime name, outside `ui::signal_connect` and `make check-signal-connections`.
- **A pre-commit hook of under ten seconds, with CI as the authority.** It contradicts merging on local evidence (maintainer, 2026-11-14), and the smoke audit found every real visual failure in local implementation runs. `smoke-gate-coherence` owns the smoke demand.
- **Moving `run_self_tests()` and `run_embedded_png_corpus()` behind `#[cfg(test)]`.** They back `cargo gtk-proof policy --self-test` and `corpus`, and the `cargo-gtk-proof` spec requires the first ("Proof policy target has Rust parity evidence").
- **Removing the `file:line:column` anchors in `.cargo/mutants.toml`.** There are 12. Name-keyed re-keying is already the rule when a file changes (`testing.md`, "Path-keyed gates").
- **A thread pool in `gtk-lush-tasks`.** Outstanding work is already capped at eight, and no measurement shows thread creation costing anything next to the I/O.
- **`Arc<Path>` in the palette index.** It is a 16-byte fat pointer against 8, about half the sites are in the test-only oracle, and sizes are charged in the index budgets. No gain is measured.
- **Narrowing `model/` to `pub(crate)` wholesale.** Most of the surface is named across crates.
- **A flat PNG buffer in `cargo-gtk-proof`.** No measurement shows a cost.
- **Rewording the family crates' internal references (21 lines).** It edits Kani inputs read by scheduled shards, for no measured gain.
- **Unifying `Debounce` and `SupersedingTimer` internals.** There is no observable gain.
- **Tier Compacto** (a lighter workflow convention for small workflows).
  - Only one of 22 migrated workflows (`ui/window/print/`) would qualify.
  - The convention's amendment rule would re-migrate every workflow.
  - A single file removes the workflow's decisions from the mutation scope, which reaches `ui/` only through files named `policy.rs`.
  - The cheaper path: trim the small facades' prose, correct the stale matrix cells (`WFR-PRINT` records 426 physical lines, measured 474; `WFR-TRANSIENT-DISMISSAL` records 691, measured 760), and write a contributor quickstart.
- **Continuous Miri.** The one pure-Rust `unsafe` kernel (`services/editor_io/text_scans/health_avx2.rs`) was run under Miri: 410 s, no UB reported (`openspec/changes/archive/2026-11-06-avx2-health-scan-kernel/tasks.md:43`). Miri cannot execute GTK FFI. It is run by hand when that kernel or new pure `unsafe` changes.
- **Archiving `formal/evaluation/`.** The models stay as disposable evidence by maintainer decision D7 (option A), and `formal/evaluation/verus/kani-step.patch` is the first step on any revisit trigger (`docs/next/formal-verification.md`).

## Open, with no change yet

Each needs its own decision or change:
- the `docs/` audit of the other 49 `.claude/handoffs/` references;
- splitting `crates/lushtext/tests/widget/window.rs` (21,939 lines) and replacing `crates/lushtext/build.rs`'s textual rewrite with a declarative registry;
- moving process archaeology out of doc comments while keeping every `Kind:` line and recorded exception reason `make check-thresholds` requires;
- the scope of `docs/thresholds.md` (kept as it is for now);
- SonarQube Cloud (kept until there is a count of its actionable findings);
- a one-page contributor quickstart;
- splitting the large `constructed()` methods into named setup functions;
- consolidating `impl` blocks spread over many files (bounded by the role-home convention);
- grouping evidence-only state fields into a telemetry sub-struct;
- `OnceCell` for set-once fields;
- the audit's defects 3 and 4: the `expect` in `model/workspace.rs` `generate_id` (practically unreachable), and the borrow held while invoking the search progress callback (a latent reentrancy risk; `rust.md` prescribes that pattern today);
- `SupersedingTimer::is_armed()`, which ships with the change that migrates the draft evidence flags in `ui/window/imp.rs`, its first consumer.
