## 1. Preconditions and decisions

- [x] 1.1 Read C1's (`document-scale-policy`, archived) names for `DocumentScale`, the reduced-features reasons and the edit-time watch, and adopt them. C2 (`long-line-protected-view`) is not merged: `L_PROTECT` on `main` is still C1's 500; C5 is independent of C2's code and reuses only its measurement patch
- [x] 1.2 Record the maintainer's decisions (2026-10-31): adopt the retired-map fix failing-first with an axiom; per-paragraph wrap gate if measurement supports it; measure whether GNOME's input path requests surrounding text on every key (design "Resolved Questions")

## 2. The keystroke lane group (design D7)

- [x] 2.1 Land the `keystroke` fixture kind in `scripts/generate-long-line-fixtures.py` (start/middle/end markers, a short paragraph after the long one, `prose` and `css` shapes) with self-test cases for marker placement and exact paragraph length
- [x] 2.2 Land the `keystroke` group, the `place-*`/`key-*`/`verify` phases and the verified-placement check in `scripts/responsiveness-smoke-driver.py`; a position whose burst is not found next to its marker is reported as not verified and excluded
- [x] 2.3 Add per-keystroke worst and summed stalls and the "Keystrokes" report table, with a driver self-test in which two sub-budget dispatches per key exceed B_interact in sum
- [x] 2.4 Run `make check-responsiveness-lane`

## 3. Toolkit axioms (design D5)

- [x] 3.1 A52 probe, sample, catalogue entry and ledger row (an unparented, alive text view keeps laying out its buffer; finalizing it detaches it)
- [x] 3.2 A53 probe, sample, catalogue entry and ledger row (a paragraph wraps by its first character's tags)
- [x] 3.3 A54 probe, sample, catalogue entry and ledger row (the view answers surrounding text from the paragraph start; its handler is matched by signal and data)
- [ ] 3.4 Run `make gtk-axioms` and `make check-gtk-axioms`; update the `gtk-lush-axioms` CHANGELOG

## 4. Release the retired minimap (design D1, pre-existing defect)

- [x] 4.1 Failing-first widget tests: a map retired at construction and one retired by an edit are finalized (weak reference cleared); confirmed failing with a temporary strong reference
- [x] 4.2 Find the reference: `RenderHoldOverlay`'s strong `live_child`; make it weak (`live_child()` returns `Option`), with a unit test, CHANGELOG and README
- [x] 4.3 Extend the minimap evidence with `retired_maps_alive` (expected 0); update the `WFR-MINIMAP` row and run `make check-workflow-boundaries`

## 5. The per-paragraph wrap gate (design D3)

- [x] 5.1 `model/paragraph_wrap.rs`: the per-paragraph decision with `REWRAP_MARGIN` hysteresis and the long interior paragraphs of an inserted run, with unit tests
- [x] 5.2 `wrap_allowed`, `L_WRAP_EDIT` and `ScaleFeature::WordWrap` in `model/document_scale.rs`; reduced features, `describe()` and the reason classes
- [x] 5.3 Extend the `core-document-scale` Kani harnesses (monotonicity, meet laws, never re-enabling) to cover `wrap_allowed`
- [x] 5.4 Run `make kani KANI_SHARD=core-document-scale` and `make check-kani-shards`
- [x] 5.5 `ui/editor_page/paragraph_wrap.rs`: tag long paragraphs (the first character decides, A53; the tag on the whole paragraph keeps the hysteresis state) from the `insert-text`/`delete-range` after-handlers, during installation too; never write the preference
- [x] 5.6 Widget tests: only the long paragraph is unwrapped; typing before the first character keeps it unwrapped; hysteresis around the threshold; a split and a join; the preference stays enabled; the reduction is reported
- [ ] 5.7 Accessibility docs (`docs/accessibility.md`, `docs/accessibility-matrix.md`) name the new reduced feature; run `make check-accessibility-policy`

## 6. Bounded surrounding text (design D6)

- [x] 6.1 `model/surrounding_text.rs`: the bounded window, with unit tests
- [x] 6.2 `ui/editor_page/surrounding_text.rs`: block the view's handler on its key controller's input-method context and answer with the window; warn and keep GTK's behaviour when not found
- [x] 6.3 Widget test: the answer is a bounded window around the cursor within its paragraph

## 7. Verification and records

- [x] 7.1 Verdict run of this change's build under `scripts/measure.sh` (prose, wrap preference on; 5k, 8k, 10k, 12k, 20k, 50k, 100k; plain and `wayland`; start, middle, end, short; three repetitions) against the pre-change baseline; record per-key worst and summed stalls, p90 and load; confirm or adjust `L_WRAP_EDIT`
- [x] 7.2 Update `docs/next/long-line-documents.md` (C5 section and row, the profile, the candidate table, the lengths that meet B_interact per input path, the remaining toolkit-floor exception, the deferred above-`L_protect` candidates and re-open triggers, the axioms table) and `docs/next/long-line-evolution.md`
- [x] 7.3 Update AGENTS.md (document scale policy, minimap retirement, long-paragraph keystrokes, module layout, recent changes), README and the `WFR-DOCUMENT-SCALE` matrix row
- [ ] 7.4 Run `make check`, the widget suite, the rustdoc gate and `openspec validate --strict`
