## 1. Preconditions

- [ ] 1.1 Confirm C0, C1 and C2 are applied (or archived): budget constants recorded, fixture generator, lateness watchdog, frame-clock timing, `make responsiveness-smoke`, ledger axioms A23–A36, `DocumentScale` with `protected_view`, the GTK-matching paragraph boundary function, Protected View and the Line Inspector; record the final budget values and `L_protect` in the evaluation record
- [ ] 1.2 Create `docs/next/long-paragraph-editing.md` with the context, the budgets, the rubric (design D1), and an empty Decision section; link it from the long-line programme record created by C0
- [ ] 1.3 Ask the maintainer Open Question 1 (is editing wanted at all) and Open Question 7 (time box); record the answers; if editing is not wanted, skip to 7.1 with "keep view-only"

## 2. Paper analysis

- [ ] 2.1 Write the consumer inventory (design Context table, including the line-number gutter, status-bar position, document statistics, live editor memory estimate and print) against the current code, with file references for each consumer, and verify no consumer is missing by grepping for buffer text reads (`text(`, `slice(`), line/offset iter constructors, and persisted line/column fields
- [ ] 2.2 Candidate A: settle delimiter choice (including a real `\r` directly before a synthetic `\n`, which GTK reads as one `\r\n` delimiter), tag-identified breaks, line-identity mapping, cross-break editing, undo interaction (GTK history records text only and cannot take unrecorded insertions without clearing its queues, so show that no recorded action can delete a break, or that an untagged re-insertion is repaired before any read; include `begin_irreversible_action` above the undo threshold), visual wrapping and gutter line numbers; fill the per-consumer obligations; record a reject-or-spike verdict with reasons
- [ ] 2.3 Candidate B: list every GtkSourceView and editor_page workflow feature that must be rebuilt, confirm `GtkAccessibleText` availability in the GTK 4.22 sources and gtk4-rs 0.11, name candidate text-model crates with their `cargo deny` status (none added); record verdict
- [ ] 2.4 Candidate C: settle window cutting on grapheme boundaries, the `SpliceRecord` and paragraph-generation ticket, fold-tag maintenance at splice (A25, at both boundaries of the hidden run), fold re-planning when the visible head grows, fold re-application after undo and redo (history restores text untagged), draft/autosave/local-history on a modified protected document, undo composition, file-monitor staleness; record verdict
- [ ] 2.5 Record, per candidate, the toolkit facts its viability depends on and whether an existing ledger axiom pins each

## 3. Spike scaffold (disposable)

- [ ] 3.1 Create `spikes/long-paragraph-editing/` as a standalone Cargo package with its own `[workspace]`, `Cargo.lock` and `.cargo/config.toml` targeting `build/long-paragraph-editing/`; add it to the root `Cargo.toml` `exclude`; add `/build/long-paragraph-editing/` to `.gitignore`
- [ ] 3.2 Add a local-only `make long-paragraph-editing-spike SPIKE_TARGET=a|b|c|all` that runs under private `mutter --headless`, `GSK_RENDERER=cairo`, pinned to at most 16 cores, with a temp scope; confirm it is absent from CI, `make check`, policy audits, widget shards, Kani/mutation lanes and Sonar scope
- [ ] 3.3 Wire C0's fixture generator (`scripts/generate-long-line-fixtures.py`: 50k, 250k, 1.9M, 10M characters; mixed-script; CR-only; U+2029; eight giant paragraphs among short lines; plus the short-line control fixture of each total size), C0's budget constants, watchdog and frame-clock timing (through the `responsiveness-probes` feature, or an identical reimplementation if the module is not reachable) into the spike harness; add the AT-bridge-plus-listener and surrounding-text-IM configurations
- [ ] 3.3a Calibrate first: re-measure one case C0 recorded (stock GtkTextView, one unfolded 250k-character paragraph, Right ×100) in the spike harness and record both values; if the result falls outside C0's recorded spread, fix the harness before any candidate run
- [ ] 3.4 Implement the interaction script (design D6) as a driver shared by all candidates, with each step's budget class (interaction or settle) fixed in the driver before any run, emitting per-step max dispatch, input-to-frame latency, scroll frame gaps, and budget verdict, plus a `btrfs-stall-report --json` capture per run
- [ ] 3.5 Run the script in the stock editor on the short-line control fixtures; record every step that misses its budget there in the long-line programme record's pre-existing-blocker list with an owning change

## 4. Spikes per surviving candidate

- [ ] 4.1 Candidate C (if surviving): editable inspector over a copied window, splice commit into the main buffer with fold maintenance, generation check, fold re-application after undo/redo with a frame-level check that no unfolded giant run is ever presented; run the script on every fixture and configuration
- [ ] 4.2 Candidate A (if surviving): tagged synthetic breaks at install, `SegmentMap` prototype, strip on every text read in the spike's save/draft/history stubs, re-segmentation on growth; run the script
- [ ] 4.3 Candidate B (if surviving): minimal virtualized widget core (model, viewport layout, caret, word motion, IM with bounded surrounding text, `GtkAccessibleText`); run the script
- [ ] 4.4 Repeat any run whose stall report does not clear storage, then record all numbers in the rubric table

## 5. Integrity checks

- [ ] 5.1 For every spiked candidate, run the no-edit round trip, the single-edit oracle comparison, and the draft/local-history equals-save check on every fixture
- [ ] 5.2 Seed the candidate-specific defects (design D7) and record whether each is caught; mark the candidate failed if any escapes
- [ ] 5.3 Write the production pure-core proof plan for each passing candidate (Kani harness statements with their domains and property tests for the map or splice planning) into the evaluation record, using design D10: for A, K-SEG1–K-SEG4, the cover list and the three `should_panic` regressions at `crates/lushtext-core/src/model/segment_map/kani_proofs.rs`; for B, the four pure cores D10 names; for C, the splice-planner harnesses D10 states
- [ ] 5.4 Candidate A (if spiked and the core fits the time box): make the prototype core pure in the shape D10 requires (byte offsets with the overflow-rejecting constructor, `break_width`, `n_segment` and the permitted cut positions as parameters, `apply_splice` / `resegment` split); add `kani_proofs.rs` beside it in the spike package behind `#[cfg(kani)]` with K-SEG1–K-SEG4 and every `kani::cover!` from D10, domains stated in the harness and the prototype rustdoc
- [ ] 5.5 Failing-first: run the prototype harnesses with the D7 seeded inclusive-bound defect in place and confirm K-SEG1 fails; revert the defect and confirm all four verify and every cover is satisfied; add the three `should_panic` regressions and confirm each finds its counterexample
- [ ] 5.6 Run the prototype with the pinned Kani version through the spike's own manifest only (never `make kani`, no shard, no `ORACLES` entry); record verdicts, local wall time and peak memory in the evaluation record as estimates, not shard measurements; triage any unexpected counterexample in `docs/next/formal-verification.md` and the record

## 6. Toolkit axioms

- [ ] 6.1 For each toolkit fact from 2.5 not already pinned, write the probe first and confirm it observes the behaviour (and fails on a deliberately inverted expectation), then add the GTK axiom ledger entry with its `gtk-lush-axioms` probe and interactive/`--check` sample, register it in the adoption lab's `axiom_probes` runner, update the ledger reference under `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`, the `gtk-lush-axioms` README and CHANGELOG, and `docs/next/formal-verification.md` where it lists the ledger
- [ ] 6.2 Run `make gtk-axioms` and `make check-gtk-axioms` (headless only, at most 16 cores) and record the verdicts in the evaluation record

## 7. Decision and records

- [ ] 7.1 Complete the evaluation record: rubric table with measurements, consumer obligations, integrity and seeded-defect results, cost and reversibility, proposed production placement under the workflow role convention for any passing candidate, and the remaining Open Questions
- [ ] 7.1a State in the evaluation record, for the candidate it proposes, the Kani obligations the follow-up change inherits (D10): every harness in exactly one shard of `scripts/kani-shards.py` with a gate and CI-runner-measured `ci_minutes` / `ci_peak_gib` / `measured_in` within 25 min / 12 GiB (15 min if gated `pull-request`); the module and every harness in `ORACLES` with measured local seconds and tier; a recorded proof-strength run with every Kani survivor triaged; `make check-kani-shards` passing
- [ ] 7.2 Present the record to the maintainer and record the Decision verbatim: adopt (with the follow-up change name and the capabilities it will modify), keep view-only, or defer, with re-open triggers and the fate of the spike package
- [ ] 7.3 Apply the spike-package fate (delete, or keep as disposable evidence with the exclusion intact)
- [ ] 7.4 Update the long-line programme record with C5's outcome, and `docs/next/formal-verification.md` with the planned harnesses of the adopted candidate (or a note that none are planned because view-only was kept or the question deferred) and the spike prototype's verdicts; if the spike make target is kept, document it as LOCAL ONLY in the Makefile header and `help`, AGENTS.md Build Commands, `.agents/rules/build.md` and README.md, as `make formal-evaluation` is; if it is deleted, remove the target, the exclusion and the ignore entry together
- [ ] 7.5 Verify no LushText application crate source, dependency, schema, action, D-Bus member, snapshot field or persisted format changed (`git diff --stat` against the base, outside `docs/`, `spikes/`, the axioms crate and its adoption-lab runner registration, the ledger, the Makefile target and the exclusion and ignore lines); confirm `cargo metadata --no-deps` at the root does not list the spike package and no CI workflow or policy script references `spikes/`; run `make check` and `make check-gtk-axioms` to confirm the gates pass and ignore the spike
