## 1. Preconditions and axioms

- [ ] 1.1 Confirm C0 (`probe-long-line-toolkit-costs`) and C1 (`document-scale-policy`) are archived. Read the recorded values of `N_visible`, `N_dense`, `V_batch`, `R_batch`, `L_protect`, `B_interact`, `B_settle` and the find batch constants, and adopt C1's actual names for `DocumentScale`, the profile and the edit-time hook. Confirm that C0's `R_batch` counts the partially covered edge row of an unaligned 2000 px batch; if it does not, the fold window is `R_batch + 1` (design D12, row-window assumption). Compute `E = V_batch − R_batch·N_dense` from C0's values and record in this change's design D1 which consequence holds: the per-paragraph cap `N_dense + E` is accepted, the fold uses a lookahead of `R_batch − 1` lengths (then H1, H1i and H2 range over the lookahead step), or C0 records a guaranteed head below `V_batch ÷ R_batch`.
- [ ] 1.2 Run `make gtk-axioms` on the current toolkit before touching GTK code.
- [ ] 1.3 Add ledger axioms P1–P7 (design D11), numbered after C0's last entry, to the ledger. Add one probe each in `gtk-lush-axioms`, one interactive/`--check` sample each, and catalogue rows, except P4, which (like C0's A35) has no in-crate probe: pin it with the lane's AT case and say so in its ledger row. Run `make gtk-axioms` and `make check-gtk-axioms`.
- [ ] 1.4 If any P-axiom is falsified, record it in the ledger and update design D3 with the extra interception or the non-focusable fallback before continuing.

## 2. Pure fold policy (`model/protected_fold.rs`)

- [ ] 2.1 Implement `FoldThresholds` (validated `new`, `const PRODUCTION` that fails to compile when C0's values break `n_dense ≥ 1`, `n_dense ≤ n_visible`, `r_batch·n_dense ≤ v_batch`), `FoldState` (excess ring plus `window_excess`), `FoldRange`, and the streaming `fold_step(state, paragraph_chars)` as the two production stages `length_cap` and `density_cap` (the excess budget of design D1), with a `plan_folds` wrapper for tests. Keep it integer-only, run it on the worker that prepares each body over every paragraph (not from the bounded `DocumentProfile` offset list), charge the compact plan to the install's transient admission, add `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]`, and use C1's paragraph-boundary function. The rustdoc of `fold_step`, `density_cap` and `row_span` states the proved domain of D12, including the step-induction argument.
- [ ] 2.2 Implement `row_span(visible_chars, WrapModel)`, `FoldRange::locate`, `FoldLocation::column_of` and `fold_byte_offset` (design D12).
- [ ] 2.3 Add unit tests for the threshold edges (`N_visible`, `N_visible` + 1, `N_dense`, density at and over `V_batch`), CR-only, U+2029 and CRLF. Write a failing-first unit test from the recorded counterexample (`V_batch = 10`, `R_batch = 2`, `N_dense = 5`, `N_visible = 8`, two 8-character paragraphs): it must fail against the draft rule ("fold beyond `N_dense` when the window would overflow") before `density_cap` lands, and pass after.
- [ ] 2.4 Add a proptest for slicing independence, the window bound under both wrap modes, and the fold-shape invariants at large sizes and at the production `R_batch`.
- [ ] 2.5 Add `#[cfg(kani)] mod kani_proofs;` to `model/protected_fold.rs` and write `model/protected_fold/kani_proofs.rs` with harnesses H1, H1i, H1c, H2, H2b, H2c and H2s (design D12), each over the production functions with its domain stated in its doc comment, the `#[kani::unwind]` its D12 row states and a `kani::cover!` on each interesting branch (warm-up, full and evicting windows; wrap off and on; folded and unfolded; visible, hidden and past-end locations; 1-, 2-, 3- and 4-byte scalars).
- [ ] 2.6 Add the `should_panic` harness H1x (`density_bound_fails_without_the_density_fold`), which runs `length_cap` alone under the H1 assertion. Run it first, before `density_cap` exists, to confirm H1 fails on the length cap alone.
- [ ] 2.7 Add a `core-protected-fold` shard (gate `pull-request`) to `SHARDS` in `scripts/kani-shards.py`. Run `make kani KANI_SHARD=core-protected-fold KANI_MEASURE=…` locally, then record `ci_minutes`, `ci_peak_gib` and `measured_in` from two dispatched runner runs (the larger of each). If the wall time is over 15 minutes, split the shard as design D12 states, so H1/H1i/H1c/H1x stay PR-gated. Run `make check-kani-shards`.
- [ ] 2.8 Add the `ORACLES` entry for `crates/lushtext-core/src/model/protected_fold.rs`, with the harnesses cheapest first and their measured local seconds and `ci`/`local` tier. Run `make proof-strength-list` and then `make proof-strength PROOF_STRENGTH_MODULE=crates/lushtext-core/src/model/protected_fold.rs`. Triage every survivor: fix a real defect failing-first, or add an assertion or harness to the shard and re-measure it.
- [ ] 2.9 Record in `docs/next/formal-verification.md` the hand-found counterexample to the draft density rule and its triage (the code was wrong; it was fixed by the excess budget), plus any Kani counterexample from 2.5–2.8 with its outcome (code, stated domain or envelope).
- [ ] 2.10 Add `model/protected_fold.rs` to the whole-pixel table in `.agents/rules/workflow-convention.md` and the check script at ceiling 0.

## 3. Fold executor at insertion (`ui/protected_fold.rs`)

- [ ] 3.1 Create the fold tag and the head/tail insertion helper (`insert` + `insert_with_tags`). The helper must be callable from any install turn.
- [ ] 3.2 Route load `execution.rs`, `buffer_replacement/execution.rs`, eviction reload, `monitor.rs` reload and reopen-with-encoding through the executor when the facts say protected.
- [ ] 3.3 Implement the edit-growth entry: from C1's `insert-text` and `delete-range` after-handler watch, apply the fold tag to the grown paragraph's tail in the same emission. Add failing-first widget tests for a paste and for a delimiter deletion that joins two paragraphs over `L_protect`.
- [ ] 3.4 Add widget tests: no paragraph has more than `N_visible` visible characters at any install turn boundary, for 2k/10k/50k/250k/1.9M, the dense wrap-off fixture, CR-only and U+2029. On the dense fixture, with wrap off and wrap on, check that `row_span` matches GtkTextView's laid-out line y-ranges, and that every 2000 px window holds at most `V_batch` visible characters. This ties the Kani row model to the toolkit. Assign the tests to a shard in `scripts/widget-shards.py`.

## 4. Protected View role home (`ui/editor_page/protected_view/`)

- [ ] 4.1 Create `mod.rs` (the facade, naming every resumption point), `seams.rs` (`ProtectedViewTicket`, `ProtectedViewFacts`, `*_is_current`) and `evidence.rs` (`ProtectedViewEvidence`).
- [ ] 4.2 Write `policy.rs` (pure, whole-pixel `forbid`):
  - `viewport_step`, `press_disposition` and `action_disposition`;
  - `navigation_target` and inspector window arithmetic;
  - the find batch and copy chunk constants (`B_interact` and `B_settle` are read from C0's responsiveness-budget module, never restated);
  - notice text.

  - the closed `ProtectedAction` enum, `toolkit_effect`, `action_disposition`, `ProtectedSession::on_event`, `inspector_window`/`inspector_window_count`/`inspector_window_containing`, `admit_find_record` and `next_find_batch` (design D12), with `const` assertions `MAX_FIND_RECORD_BYTES ≤ FIND_BYTES_PER_TURN` and `N_inspect ≤ min(N_visible, L_protect)`.

  Add unit tests, and add the module to the whole-pixel table. `execution.rs` only ever replaces its stored session with the result of `on_event`.
- [ ] 4.2a Add `#[cfg(kani)] mod kani_proofs;` to `policy.rs` and write `ui/editor_page/protected_view/policy/kani_proofs.rs` with H3, H3m, H4, H4n and H4f, each over the production functions with its domain in its doc comment, the `#[kani::unwind]` its D12 row states, the domain repeated in the checked function's rustdoc, and a `kani::cover!` on each disposition, each event kind in both states, single-window and multi-window paragraphs, and the count-cap, byte-cap and result-cap stops. Derive `kani::Arbitrary` for `ProtectedAction` and `SessionEvent` under `cfg(kani)` only.
- [ ] 4.2b Add the `should_panic` harnesses H3x (eviction reload routed through the from-disk re-derivation) and H4x (a record that bypassed `admit_find_record`). Write H3m and H4f first, against a stub `on_event` that re-derives on eviction reload and a planner without record admission, to see them fail before the real rules land.
- [ ] 4.2c Add a `core-protected-view` shard (gate `scheduled`) to `scripts/kani-shards.py`, with `ci_minutes`, `ci_peak_gib` and `measured_in` from two dispatched runner runs (the larger of each), within 25 min and 12 GiB. Add the `ORACLES` entry for `crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs` with measured local seconds and tiers. Run `make check-kani-shards` and `make check-workflow-boundaries`, which must accept the gated `kani_proofs.rs` in the role home. Then run `make proof-strength PROOF_STRENGTH_MODULE=crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs` and triage its survivors in `docs/next/formal-verification.md`.
- [ ] 4.3 In `execution.rs`, implement entry and monotone state: `editable=false`, `cursor-visible=false`, the `move-cursor` intercept, capture-phase click and drag gestures, DnD off, and key-binding pre-emption: stop every mark-moving or buffer-changing keybinding signal of GtkTextView and GtkSourceView (including `select-all`, `toggle-cursor-visible`, `change-number`, `change-case`, `join-lines`, `move-to-matching-bracket`), and claim the binding actions (`menu.popup`, `text.undo`/`text.redo`, `source.change-case`) with a capture-phase key controller. Do not rely on per-instance `action_set_enabled`, which GtkTextView recomputes. Leave touchscreen drag sequences to the scrolled window's pan gesture. Count observed insert-mark moves in the evidence. Add the `protected-nav` mark and `scroll_to_mark` navigation, and never move the insert mark.
- [ ] 4.4 Replace the context menu with the Protected View popover menu (Inspect Line, Copy Whole Line, Find, insensitive disabled entries).
- [ ] 4.5 Add `fold_gutter.rs` (the fold-marker renderer, bounded by visible rows, with an accessible label).
- [ ] 4.6 Add `test_policy.rs` only if a test-only value is required. Keep the externally reachable `*_for_test` count within the matrix ratchet.
- [ ] 4.7 Add widget tests for the precondition gates:
  - the properties and the intercept with `gtk-keynav-use-caret` on;
  - single, double and triple clicks and drags leave the marks unmoved;
  - Ctrl+V, Ctrl+X, Ctrl+A, Ctrl+Shift+A, Ctrl+Shift+X, Ctrl+%, F7, Shift+F10, Menu, Ctrl+Z, Alt+Up, Backspace, Delete and typing leave the buffer unchanged, the marks unmoved, `cursor-visible` false and `modified` false (failing-first: Ctrl+Shift+A on a number and Ctrl+A must fail before the pre-emption lands);
  - an enumeration of the view's class shortcut controllers finds every bound action and signal classified by `action_disposition`;
  - a touch drag scrolls the view and leaves the marks unmoved;
  - DnD drop is refused;
  - the notice appears once.

## 5. Immutable source, Line Inspector and whole-line copy

- [ ] 5.1 Implement `ProtectedTextSource` (`Arc<str>` + paragraph index + char checkpoints), captured lazily through the chunked `buffer_snapshot`, charged to live memory and transient admission, and released through plain disposal on close, eviction or reload.
- [ ] 5.2 Add `resources/ui/line-inspector.blp` and its generated `.ui` (`make blueprint-generate`, `make check-blueprint`), plus `inspector.rs`, a called presentation surface.
- [ ] 5.3 In `inspector_execution.rs`, implement worker window reads, generation guards, Previous/Next/Go to Column, bounded Copy Window, and the keyboard default line chooser.
- [ ] 5.4 Implement the lazy whole-line `GdkContentProvider` that writes in bounded chunks (at most one chunk copied per GTK dispatch), with its retained `Arc<str>` charged as long-lived ownership rather than transient admission, and add a test that reads the clipboard and compares the exact characters.
- [ ] 5.5 Add the Inspect Line action and shortcut to the action catalogue (`services/action_catalog/`) and the command palette, with `make check-automation-docs`.

## 6. Worker in-tab find

- [ ] 6.1 Add the protected mode to the search bar: skip `attach` when protected, hide the replace row, and make `win.begin-replace` open find plus the notice.
- [ ] 6.2 In `find_execution.rs`, implement a single-flight worker search over the source, using the existing option model, C1's paragraph boundaries, bounded batches, a result cap, cancellation, and the counter states.
- [ ] 6.3 Implement result activation through `navigation_target`: highlight the current match with the tag when it is visible, otherwise open the inspector.
- [ ] 6.4 Unify the two prefill sites (`ui/editor_page/search.rs:82`, `ui/window/search.rs:593`) on one helper that reads `visible=true` text under the prefill bound.
- [ ] 6.5 Hold the existing `editor-search` blocker for protected requests and first-use source capture (covered by the `search-complete` predicate), and hold a blocker for inspector window reads (an existing one, or a new `line-inspector` blocker included in `idle` and `accessibility-settled`, documented in task 8.2).
- [ ] 6.6 Add widget tests: no search context is attached; supersession keeps one pending request; a hit in a hidden tail opens the inspector; batches stay within the constants.

## 7. Surfaces, save and session

- [ ] 7.1 Add `ProtectedSaveMode` in `ui/editor_page/save/`: Save is disabled while unmodified; every save of a protected document (including edit-growth documents and the close-with-changes save) suppresses EditorConfig formatting, line-ending normalization and mirror-back, and writes `Mixed` line endings verbatim instead of refusing; Save As of a non-round-trippable decode is refused. Add the round-trip exactness fact to `LoadMetadata`. Add a byte-identity integration test over the synthetic fixture set (including mixed endings and U+2029), and a failing-first test that an edit-growth document with mixed endings saves.
- [ ] 7.2 Run the `data-safety` skill in explicit mode on the edit-growth path (modified buffer that becomes protected) and on Save As. Fix every finding.
- [ ] 7.3 Force invisibles off while folds exist, and make `win.cycle-invisible-characters` publish the notice.
- [ ] 7.4 Add `PrintOutcome::RefusedProtectedView` in `window/print/policy.rs` and its execution, with a test.
- [ ] 7.5 Make the local-history preview install through the fold executor, suppress capture for installed-protected documents, and route restore through re-profiling.
- [ ] 7.6 Verify the Markdown preview `Simplified`/limited terminal on a giant paragraph, verify the file-peek sample bound, and add read-only notes editors with tests.
- [ ] 7.7 Disable bookmark toggles and line-ending normalization in Protected View, and make bookmark navigation go through `navigation_target`.
- [ ] 7.8 For session restore, scroll through the navigation mark, keep the persisted cursor offset on later session saves, and create no draft for installed-protected tabs. Add a widget test restoring three protected documents and switching tabs.
- [ ] 7.9 For eviction and external reload, release the source, re-fold at insertion and re-derive the facts. Add the eviction-reselection and monitor-reload widget tests.

## 8. Automation and accessibility

- [ ] 8.1 Add `protected_view`, `protected_view_reason`, `folded_paragraph_count` and `hidden_character_count` to `AutomationTabSnapshot`, and `line_inspector_visible` to the surface snapshot. Project them from `evidence.rs`, and update `scripts/lushtext-automation.py`.
- [ ] 8.2 Update `docs/automation.md` and `docs/automation-reference.md` (the new fields, the `editor-search` blocker's protected scope, and the inspector blocker). Run `make check-automation-docs` and `make automation-client-self-test`.
- [ ] 8.3 Add the accessible description of the protected view, the inspector labels and announcements, and the gutter labels through the shared accessibility boundary.
- [ ] 8.4 Update `docs/accessibility.md`, add the `docs/accessibility-matrix.md` rows, add the `docs/accessibility-orca-checklist.md` Protected View entry, and add the accessibility-smoke inspector keyboard path.

## 9. Convention, docs and verification

- [ ] 9.1 Add the `WFR-PROTECTED-VIEW` row to `docs/workflow-readability-matrix.md`. Re-derive the measured cells of `WFR-DOCUMENT-LOAD`, `WFR-BUFFER-REPLACEMENT`, `WFR-EDITOR-FIND`, `WFR-DOCUMENT-SAVE`, `WFR-LOCAL-HISTORY`, `WFR-PRINT` and `WFR-SESSION-RESTORE`, and of `WFR-NOTES-BOOKMARKS`, `WFR-ENCODING` and `WFR-SEARCH-REPLACE` if their gating changes more than a call site, then run `make check-workflow-boundaries`.
- [ ] 9.2 Update `AGENTS.md` (the module layout and a Protected View design decision), `.agents/rules/ui.md` (large-buffer guardrails), `docs/end-user-coverage.md`, README features, and the `docs/next/` long-line programme record (mark C2 done and update the C3 and C5 posture).
- [ ] 9.3 Add the protected scenarios to C0's `make responsiveness-smoke` lane (open, scroll, find, inspector, tab switch, three-document restore, AT bridge, IM), and record the measured budgets.
- [ ] 9.3a Add a section for this change to `docs/next/formal-verification.md`: harnesses H1–H4x with their modules, domains and the step-induction note; the `core-protected-fold` and `core-protected-view` shards with gates and measured budgets; both proof-strength runs with their kill rates and the triage of each survivor; the density-rule counterexample (task 2.9); and the row-window assumption with its confirmation from task 1.1. Update `.agents/rules/build.md`'s placement list of Kani-checked modules.
- [ ] 9.4 Run `make check`, `make test`, `make kani KANI_SHARD=core-protected-fold`, `make kani KANI_SHARD=core-protected-view`, `make gtk-axioms` and `make check-widget-shards` on at most 16 cores and headless only, then fix every failure, including pre-existing blockers.
