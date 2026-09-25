## 1. Crate skeleton

- [ ] 1.1 Read `crates/gtk-lush/GOVERNANCE.md`, `scripts/check-gtk-lush-policy.py`, `scripts/check-gtk-lush-adoption.py`, `docs/next/gtk-lush.md` and one functional family crate (for example `crates/gtk-lush/proof-harness/`), using the gtk-lush-stewardship skill. List every obligation a new family crate carries: required files, the "Internal Platform Status" / "functional in-tree" / "not a stable external dependency" README phrases, SPDX headers, `#![forbid(unsafe_code)]` and `#![deny(missing_docs)]`, workspace lints and `rust-version`, at least one example, the adoption-matrix fields, and the Makefile package lists. Get the maintainer's answer to design D1's open question (runner in the lab, or a governed dev-dependency exception) before writing code.
- [ ] 1.2 Add `axioms` to `EXPECTED_MEMBERS` in `scripts/check-gtk-lush-policy.py` and `gtk-lush-axioms` to `EXPECTED_PACKAGES` in `scripts/check-gtk-lush-adoption.py`, and confirm both `make check-gtk-lush-policy` and `make gtk-lush-adoption-matrix` fail naming the missing crate.
- [ ] 1.3 Create `crates/gtk-lush/axioms/` (`gtk-lush-axioms`, `0.0.0`, dependencies `gtk4`, `glib`, `libadwaita` only) with README, CHANGELOG, both licences and one placeholder example. Add it to `[workspace].members` and `[workspace.dependencies]`, to `GTK_LUSH_PACKAGES` and `GTK_LUSH_CRATES` in the Makefile, and to the family README list. Add its `docs/gtk-lush-adoption/matrix.toml` row. Run `cargo hakari generate`. Both checks from 1.2 now pass.
- [ ] 1.4 Add `AxiomId`, `Axiom` (with `probe: Option<fn() -> Observation>`), `Verdict`, `Observation` with its one-line JSON encoder, and `catalogue()` with every ledger id A1–A13 and no probes yet. Unit-test: ids unique and ordered, and JSON escaping of measured strings.

## 2. Port the existing probes

- [ ] 2.1 Port `FixedHost` (GType renamed `GtkLushAxiomsFixedHost`), `HostedList`, `probe_list`, and the A5, A9, A11 and A13 probes into the crate as `probe_a05()` and so on. Replace the widget binary's `common.rs` helpers with crate-private ones: present in a plain `adw::Window` with no application, and a bounded main-context wait. Keep every probe step unchanged; a failed control step yields `FixtureInvalid`, and a failed axiom step yields `Violated`. Wire each probe into `catalogue()`.
- [ ] 2.2 Add `crates/gtk-lush-adoption-lab/tests/axiom_probes.rs` (`harness = false`, main on `gtk-lush-proof-harness`), with one test per probed catalogue entry that prints the observation and fails unless it is `Holds`. Exclude `binary(=axiom_probes)` in `.config/nextest.toml`, and narrow `make gtk-lush-adoption-lab` to `--lib --bins`; confirm `cargo nextest run --workspace` no longer lists it as run. Add `make gtk-axioms`. Run it, and confirm all four pass on the toolkit where the old probes pass. Compare each measured value with the ledger's recorded one (A5: value 1200 → 34, page 300 → 0), and record any change. Then run each ported test 5 times in isolation.
- [ ] 2.3 Delete `crates/lushtext/tests/widget/gtk_axioms.rs`, and drop `gtk_axioms` from the `surfaces` shard in `scripts/widget-shards.py`. Confirm `make check-widget-shards` passes and the widget binary's `--list` count fell by exactly four. Repoint the four "Pinned by" cells in the ledger at the crate's probes, keeping every LushText consumer-test reference.

## 3. Samples

- [ ] 3.1 Add `examples/support/mod.rs` (shared window and `--check` handling: the verdict exit codes, and exit 77 unless `GTK_LUSH_AXIOMS_HEADLESS=1`). For A5, A9, A11 and A13 add `examples/aNN_<slug>.rs` using the probe's fixture builder, and remove the placeholder example. Add `make gtk-axiom-sample AXIOM=<id> [CHECK=1]`: `CHECK=1` builds the example and runs it under `dbus-run-session -- mutter --headless` with the variable set and the environment of `scripts/run-widget-tests.sh`. Extend `make gtk-axioms` to run every sample's `--check` after the probe binary.
- [ ] 3.2 Add probes and samples for A1 (realized row count around the 200 + 2 cap) and A4 (`scroll_to` applied inside the list's own allocation).
- [ ] 3.3 Try a minimal isolating fixture for each of A2, A3, A6 and A7, for at most 1 h per axiom. Each that isolates gets a probe and a sample; each that does not gets a ledger note recording the attempt and why it stays "indirectly".
- [ ] 3.4 Confirm `--check` outside a headless session exits 77 without opening a window. Run each sample's interactive mode once under a private headless `mutter` and capture a screenshot with the gtk-agentic-debugging tooling into `build/gtk-axioms/` (not committed). Leave the desktop-session look at each sample to the maintainer.

## 4. Ledger and policy check

- [ ] 4.1 Write `scripts/check-gtk-axioms.py` with `--self-test` covering every failure in design D5. Wire it into `check-policy`. Confirm it fails on the current ledger (no "Verified against" or "Sample" columns yet).
- [ ] 4.2 Add the "Verified against" and "Sample" columns, filling "Verified against" from the `Observation` lines printed by `make gtk-axioms`, with where it ran. Replace the ledger's "probes live in `gtk_axioms.rs`" rule and single "Measured against" line accordingly. Confirm `make check-policy` passes, and that it fails again when one catalogue entry is deleted and when a "Pinned by" cell names `gtk_axioms::`; restore both.

## 5. CI

- [ ] 5.1 Add a `gtk-axioms` job to `.github/workflows/ci.yml` (Fedora 44 container, `gtk4-devel`, `libadwaita-devel`, `mutter`, `dbus-daemon`, a `timeout-minutes` under 30) running `make gtk-axioms`. Add `libadwaita-devel` to the `gtk-lush-msrv` and `gtk-lush-api-advisory` jobs. Run `make check-workflow-timeouts`, and confirm the new job and both changed jobs pass on the pull request.

## 6. Upgrade alarm

- [ ] 6.1 Document the toolkit-update procedure (design D6) in the gtk-testing skill and `.agents/rules/build.md`.
- [ ] 6.2 Runtime spike, 4 h at most: build and run the `--check` samples inside the GNOME 50 SDK and the GNOME nightly SDK. If it works, add a local-only `make gtk-axioms-runtimes` and record per-runtime results in "Verified against". If not, record the attempt and the blocker in `docs/next/gtk-lush.md`.

## 7. Hand-off to the breakpoint change

- [ ] 7.1 Edit `openspec/changes/extend-closed-loop-geometry-verification` wherever it names `gtk_axioms.rs` as the probe home: proposal (the ledger bullet and the Impact list), design D6, tasks 3.1 (probes and samples in `gtk-lush-axioms`) and 3.2 (ledger rows plus catalogue entries), and its `gtk-axiom-ledger` delta spec. If that change has already landed, port its A14–A18 probes here instead. Run `openspec validate extend-closed-loop-geometry-verification --strict`.

## 8. Documentation

- [ ] 8.1 Update AGENTS.md (the crate list, the build-command list, and Recent Changes), README.md (crates and make targets), `docs/next/gtk-lush.md` (a family-inventory section for the crate), `docs/next/formal-verification.md` (where the probes live, and the ledger link), the GOVERNANCE review log (a constitution audit for the crate, including the main-context wait of design D1), the crate's CHANGELOG, and the gtk4-libadwaita-internals skill's pointer to the probes.

## 9. Verification

- [ ] 9.1 Run `make check`, `make test` (no `FLAKY`), `make gtk-axioms`, `make check-gtk-lush-policy`, `make check-gtk-lush-adoption`, `make gtk-lush-examples`, `make gtk-lush-doctests`, `make gtk-lush-msrv`, and `make gtk-lush-api-advisory` (confirm the new crate's public API snapshot is generated).
- [ ] 9.2 Confirm no application behaviour or Kani harness changed, and that every axiom id cited in `crates/gtk-lush/widgets/src/kani_proofs.rs` still names a ledger row.
- [ ] 9.3 Run `openspec validate extract-gtk-lush-axiom-samples --strict`.
