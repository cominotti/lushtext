## Why

LushText must stay instantly responsive (open, scroll, every interaction) with documents of any admitted size, and must never change a file's bytes. A 5 MB HTML file with a 1.9-million-character line currently freezes the app for seconds. The long-line programme (C0–C5, from the reviewed proposal under `docs/next/long-line-responsiveness.md`) fixes this in stages, and its maintainer decisions (2026-10-11) leave several values open until they are measured:

- the paragraph thresholds L_syntax, L_minimap, L_invisibles, L_search_highlight, L_protect and N_visible, with the bound on C2's N_inspect;
- the density bounds V_batch, R_batch and N_dense;
- the syntax total-size cutoff S_syntax;
- confirmation of the stricter working budgets: B_interact = 8 ms per interaction dispatch, input-to-presented-frame ≤ 2 frames, and B_settle = 16 ms per settle dispatch.

C0 measures them, under the names C1 (`document-scale-policy`) and C2 (`long-line-protected-view`) declare. It also turns the toolkit beliefs that C1–C3 depend on into pinned GTK axioms, so that no later change rests on an unprobed assumption. C0 changes no application behaviour.

## What Changes

- **GTK axioms A23–A36** are added to the ledger and to the `gtk-lush-axioms` catalogue. The ledger currently ends at A22, so the proposal's numbering applies unchanged. They cover:
  - one PangoLayout per paragraph;
  - invisible text left out of the layout;
  - toggle gravity at a tagged run's boundaries;
  - log attrs computed over the whole paragraph, hidden text included;
  - tag changes that do not dirty the buffer;
  - Pango's int32 width overflow;
  - the context engine's atomic line and its 2000 ms cut-off;
  - IM surrounding-text and spot cost;
  - 2000 px validation batches;
  - the `GtkSourceMap` detach CRITICAL;
  - which text `GtkSourceSearchContext` sees;
  - `move-cursor` with an invisible cursor;
  - AT-SPI offsets over invisible text;
  - btree segment merge.

  Each axiom gets an isolated headless probe and a `--check` sample, or records why it has none. Probe verdicts are structural. Timings are recorded values only. `gtk-lush-axioms` gains `pango` and `sourceview5` dependencies, and its catalogue gains a `slow` marker so a seconds-long probe (A29) can stay out of the 20-minute `GTK Axioms` PR job while still running nightly.
- **A synthetic long-line fixture generator** (`scripts/generate-long-line-fixtures.py`). It is deterministic and seeded, and writes only into the run's temporary scope. It produces:
  - single giant paragraphs;
  - dense near-threshold line sets, for wrap on and wrap off;
  - short-line HTML at several total sizes;
  - CR-only, CRLF and U+2029 variants;
  - multi-document session and eviction sets.

  No user file is ever copied or committed.
- **An in-process responsiveness probe.** It is compiled only under a new smoke-only cargo feature, `responsiveness-probes`, which no shipped build enables. It has four parts:
  - a `G_PRIORITY_HIGH` main-loop lateness watchdog with microsecond ready times, which bounds the longest main-loop stall;
  - frame-clock `after-paint` and presentation timing;
  - input-to-presented-frame stamps, measured from the injection or compositor event time so a stalled loop's queueing is counted;
  - a deterministic probe IM context that requests surrounding text.

  The probe streams bounded JSONL evidence off the GTK thread. The pure, GTK-free half is `model/responsiveness_budget.rs`. It holds the named budget constants (B_interact, B_settle, the input-to-frame limit, the scroll frame-gap limit), stored as integer microseconds, and the verdict and percentile arithmetic. Kani proves that arithmetic.
- **A `make responsiveness-smoke` lane.** It runs headless, nightly or on demand, with no PR gate. It drives the real LushText binary through the automation client and compositor input injection across the sweep in `PROPOSAL.md`:
  - paragraph lengths 2k/10k/50k/250k/1.9M, with bounded bisection;
  - near-threshold dense sets, with wrap off and wrap on;
  - short-line HTML at 1/2/5/10 MB;
  - CR-only;
  - session restore of three or more giant-paragraph documents, with tab switching;
  - eviction and reselection;
  - an external reload.

  It runs each case with the AT bridge and an event listener, and with an IM that requests surrounding text. It checks that no fixture's bytes changed, and never saves: an app holding typed edits is killed, and each case starts from fresh isolated XDG state. It records `btrfs-stall-report` evidence for the run window. Its output is a report that derives candidate values for every open threshold and a confirm-or-revise verdict on the 8/16 ms budgets.
- **A `docs/next/long-line-documents.md` programme record** for C0–C5. It holds the decisions, the gates between changes, the measured values with their derivation, and the pre-existing blockers each later change owns.

## Dependencies and sequencing

- C0 depends on no other change and changes no application behaviour.
- Stream order: C0 → C1 → C2 → (C3, conditional on its adoption gate) → C4 → (C5, optional exploration).
- C1 `document-scale-policy`, C2 `long-line-protected-view`, C3 `sliced-protected-install` and C4 `bound-window-construction-dispatch` are already drafted. They depend on C0's recorded values, budgets, fixtures, watchdog and axioms, so C0 must be applied, and its values recorded, before any of them is implemented. C5 `explore-editing-long-paragraphs` is exploration only.
- `stop-expander-bind-time-child-scans` is unrelated and can land in either order. No other open change claims a ledger id: they use provisional names that are not ledger ids (C1's four D8 axioms, C2's P1–P7, C3's S1–S3, C4's G1–G4, and `stop-expander-bind-time-child-scans`' T1); each takes the next free id when its change is applied. If the stop-expander change is applied first, it takes A23 and C0 renumbers its range (task 1.2).

## Capabilities

### New Capabilities
- `responsiveness-measurement`: the named budget constants and their verdict arithmetic; the feature-gated in-process watchdog, frame timing and probe IM; the synthetic fixture generator; the `make responsiveness-smoke` lane, with its sweep, AT and IM variants, its data-integrity check, its storage-stall evidence and its report; and the rule that the measured thresholds and budget verdict are recorded in the programme record before C1 is proposed.

### Modified Capabilities
- `gtk-axiom-ledger`: the ledger records the text-layout, text-buffer, input-method, accessibility and GtkSourceView axioms that the long-line designs depend on (A23–A36), each written from what its probe observes.
- `gtk-lush-axioms`: probes may build GtkTextView, Pango and GtkSourceView fixtures, may install a probe input-method context, and keep verdicts structural, with timings as recorded values. The PR-gated probe runtime stays inside the CI job cap.

## Impact

- **New code:**
  - `crates/gtk-lush/axioms/src/probes/a23.rs`–`a36.rs`, plus `examples/a23_…`–`a36_…`;
  - `crates/lushtext-core/src/model/responsiveness_budget.rs` (with `kani_proofs.rs`);
  - `crates/lushtext-core/src/ui/responsiveness_probe.rs`, cross-cutting instrumentation with no workflow role, compiled out of shipped builds;
  - `scripts/generate-long-line-fixtures.py`, `scripts/run-responsiveness-smoke.sh`, `scripts/responsiveness-smoke-driver.py`;
  - `.github/workflows/responsiveness-smoke.yml`.
- **Cargo:**
  - `gtk-lush-axioms` gains `pango` and `sourceview5` workspace dependencies;
  - `lushtext-core` and `lushtext` gain a `responsiveness-probes` feature, following the `crash-kill-points` precedent;
  - the measurement binary is built separately from `target/debug/lushtext`.
- **Policy:** `make check-gtk-axioms` (which learns the `slow` marker), `make check-kani-shards` (a new harness assigned to a shard with a measured budget), `make check-workflow-timeouts` (the new workflow ≤ 30 min per job), `make check-automation-docs` (new helper flags), and `make check-filesystem-boundary` (evidence writes go through `services::filesystem`).
- **Docs:**
  - the GTK axiom ledger, the `gtk-lush-axioms` README and CHANGELOG;
  - `docs/next/long-line-documents.md` (new), `docs/next/formal-verification.md` (A23–A36, the new Kani harness);
  - `docs/end-user-coverage.md`, `docs/automation.md` and `docs/automation-reference.md` (helper flags);
  - `.agents/rules/build.md` (the lane, the slow-probe switch), `AGENTS.md` and `README.md` (build commands and test lanes);
  - `sonar-project.properties` scope review for the new scripts and workflow.
- **Unchanged:**
  - application behaviour, and the shipped binary's code paths;
  - D-Bus automation members and snapshot fields;
  - GSettings and persisted formats.
