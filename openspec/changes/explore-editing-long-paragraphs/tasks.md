## 1. Preconditions and decisions

- [ ] 1.1 Confirm C2 (`long-line-protected-view`) is applied or archived with the hybrid `L_protect`, and read its final `L_protect`, its keystroke sweep and its recorded toolkit-floor exception; adopt C1's and C2's actual names for `DocumentScale`, the reduced-features reasons and the edit-time watch
- [ ] 1.2 Ask the maintainer design Open Questions 1–3 (adopt the wrap gate; view-wide or per-paragraph; never build a map while the preference is off) and record the answers in `docs/next/long-line-documents.md`; if the gate is declined, drop group 5 and the wrap requirement from the specs before continuing, and keep the exception for wrapped paragraphs up to `L_protect`

## 2. The keystroke lane group (design D6)

- [ ] 2.1 Land the `keystroke` fixture kind in `scripts/generate-long-line-fixtures.py` (start/middle/end markers, `prose` and `css` shapes) with self-test cases for marker placement and exact paragraph length
- [ ] 2.2 Land the `keystroke` group, the `place-*`/`key-*`/`verify` phases and the verified-placement check in `scripts/responsiveness-smoke-driver.py`; a case whose burst is not found next to its marker is reported as not verified and excluded from thresholds
- [ ] 2.3 Add the summed stall per keystroke beside the worst stall to the report, with a failing-first driver self-test in which two sub-budget dispatches per key exceed B_interact in sum
- [ ] 2.4 Register the group in `scripts/responsiveness-reference-sweep.py` so `L_wrap_edit` is bisected (wrap on, worst of plain, AT and probe IM), and document the group in `.agents/rules/build.md` and `docs/next/long-line-documents.md`
- [ ] 2.5 Run `make check-responsiveness-lane`

## 3. Toolkit axioms (design D5)

- [ ] 3.1 Probe first, then ledger: "one commit-handler layout per typed character, reused by validation and snapshot" (count `create_display` work through a PangoLayout counter or display-cache misses); confirm it fails on an inverted expectation
- [ ] 3.2 Probe first, then ledger: "an unparented, alive text view attached to a buffer re-validates edited paragraphs; clearing its buffer after unrealize stops it without a warning"; pin the exact call D1 uses (`set_buffer(None)` on the map), and record whether `GtkSourceMap` itself keeps a reference after unparent
- [ ] 3.3 Add samples, register both in the adoption lab's `axiom_probes`, update the ledger reference, the `gtk-lush-axioms` README/CHANGELOG and `docs/next/formal-verification.md`; run `make gtk-axioms` and `make check-gtk-axioms`

## 4. Release the retired minimap (design D1, pre-existing defect)

- [ ] 4.1 Failing-first widget test: with the minimap preference off, after construction and after an edit-time retirement, a weak reference to the retired map is cleared, or the map's buffer is not the document buffer; and typing into a long paragraph queues no map validation (count through the minimap evidence surface)
- [ ] 4.2 Find the reference that keeps the retired map alive; remove it if LushText owns it, otherwise report it upstream and keep the buffer detach
- [ ] 4.3 In `retire_native_source_map`, after unparenting, detach the map from the document buffer by the call 3.2 pinned; never clear a live map's `view` (A32)
- [ ] 4.4 Extend the minimap evidence with the count of retired maps still bound to a buffer (expected 0); update the `WFR-*` minimap row in `docs/workflow-readability-matrix.md` and run `make check-workflow-boundaries`
- [ ] 4.5 Re-run the keystroke group at 10k, 20k, 50k and 100k, wrap on and off, under `scripts/measure.sh`; confirm one paragraph layout per keystroke and record the summed-stall change against this change's baseline

## 5. The long-paragraph wrap gate (design D3; only if 1.2 adopted it)

- [ ] 5.1 Derive `L_wrap_edit` with the keystroke group (2.4) on the reference lane, after group 4 has landed; record the value, the bracketing runs and their load in the programme record
- [ ] 5.2 Add `wrap_allowed` and `L_wrap_edit` to `model/document_scale.rs`; extend `ReducedFeatures`, `describe()` and the reason classes
- [ ] 5.3 Extend the `core-document-scale` Kani harnesses so monotonicity, the meet laws and the componentwise-maximum agreement cover `wrap_allowed`; add a `should_panic` harness pinning that a policy which re-enables wrapping on a smaller save is caught; re-measure the shard's budget if it moved (`make check-kani-shards`)
- [ ] 5.4 Apply the effective wrap mode (`preference ∧ wrap_allowed`) in `ui/editor_page` on every install path before content reaches the view, and in the edit-time watch's downgrade inside the `insert-text`/`delete-range` after-handler; never write the preference
- [ ] 5.5 Widget tests: opening over the threshold shows the view unwrapped with the preference still on and another tab wrapped; typing past the threshold unwraps before the emission returns and stays unwrapped after the paragraph shrinks; toggling the preference while the gate holds leaves the view unwrapped
- [ ] 5.6 Reduced-features notice, document-properties row and accessible description name word wrap; update `docs/accessibility.md` and `docs/accessibility-matrix.md`; run `make check-accessibility-policy`

## 6. Verification and records

- [ ] 6.1 Run the keystroke group (10k, 20k, 50k, 100k; prose and css; wrap on and off; plain, AT and probe IM; three repetitions) under `scripts/measure.sh` on the reference lane; record per-key worst and summed stalls, p90, over-budget counts and load
- [ ] 6.2 Update `docs/next/long-line-documents.md`: the C5 row, the profile, the candidate table (measured and rejected), the paragraph lengths that meet B_interact with and without the gate and per IM variant, the remaining toolkit-floor exception, the deferred above-`L_protect` editing candidates and every re-open trigger; update `docs/next/long-line-evolution.md`
- [ ] 6.3 File the upstream GTK report (synchronous whole-paragraph layout in the commit handler; no partial paragraph layout) and, if 4.2 found a toolkit-held reference, the `GtkSourceMap` one; link both from the record
- [ ] 6.4 Update AGENTS.md (document scale policy, minimap retirement) and README (reduced features); run `make check`, `make test`, `make gtk-axioms`, the rustdoc gate, and `make kani KANI_SHARD=core-document-scale`
