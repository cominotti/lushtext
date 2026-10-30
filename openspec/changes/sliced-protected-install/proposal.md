## Why

`long-line-protected-view` (C2) folds each giant paragraph at insertion: the head stays visible and the tail goes in already carrying the hidden, non-editable protected tag. The install still obeys the current rule in `main-thread-responsiveness` ("Large file installation yields in bounded GTK slices"): a paragraph longer than the slice budget goes in **one** GTK turn. That rule exists because GTK lays out a whole paragraph at a time, so slicing a *visible* paragraph turns into quadratic re-layout. A hidden tail is not laid out (ledger A24), so that argument does not hold for it. One turn that inserts a 100–500 MB tail still costs work in proportion to the tail: the copy into the B-tree, `insert-text` emission, and an AT-SPI `TextChanged` payload as large as the tail when the accessibility bridge is on. The stricter settle budget recorded on 2026-10-11 (B_settle = 16 ms per settle dispatch, working value confirmed by C0) makes it likely that this one turn breaks the budget. "Instantly responsive however large the file" then fails at open, reload, eviction reload, and draft recovery of exactly the documents Protected View exists for.

This change is **conditional**. It is adopted only if the responsiveness lane of `probe-long-line-toolkit-costs` (C0), run against a build with `long-line-protected-view` (C2) applied, measures that installing or clearing one giant protected paragraph takes a dispatch longer than B_settle at 100, 250 or 500 MB, or with the AT bridge and a listener on. C0's own `install` case runs before C2 exists, so it measures today's untagged one-turn install; it is a lower bound and a hint, not the gate. If every admitted size is within B_settle, this change is closed without implementation and the one-turn rule stays.

## What Changes

- **Adoption gate (first task).** Run C0's lane `install` case against the C2 build (C2's one-turn protected install and clear at 100/250/500 MB, with the AT bridge off and on), together with C2's **32 MiB draft-recovery case** (two recovered drafts whose last paragraph holds more than 33 million characters, the crash-recovery smoke's lazy-aggregate fixture, AT bridge off and on; C2 design D13 records its one-turn hidden-tail inserts as this gate's input) and record it with C0's own pre-C2 `install` results. Adopt the change only if some dispatch is over B_settle; a case that hits the lane's wall cap counts as over budget. Otherwise record the decision and stop.
- **The new exception.** In a document under Protected View, a slice MAY end inside a paragraph, but only inside that paragraph's hidden tail. The head, up to C2's fold point, is never split. It is inserted visible in one slice of bounded size (≤ N_visible characters). Tail slices are inserted with C2's protected tag applied by `insert_with_tags` in the same call, so tail text is untagged only inside that synchronous call (synchronous `insert-text` handlers, and the AT insert event, see it before the tag is applied) and is never laid out or left untagged across a dispatch. So no visible text ever grows across slices, and the quadratic re-layout cannot happen.
- **Segment-merge mitigation.** GTK merges adjacent character segments by copying both strings (`char_segment_cleanup_func` → `_gtk_char_segment_new_from_two_strings`, gtktextsegment.c:327-354). Same-tag toggle pairs cancel through zero-size segments (`toggle_segment_cleanup_func`, gtktextsegment.c:550-590). Appending tagged slices one after another would therefore copy the growing tail on every slice, which is O(M) per slice and O(M²) in total. The install keeps a **segment boundary** between slices, so adjacent char segments never touch. The default boundary is a zero-width anonymous mark per slice. The alternative is to alternate two identical protected tags. The boundary stays for the buffer's lifetime: removing boundaries one by one would bring the quadratic merge back.
- **Sliced clearing of protected tails.** Clearing (eviction, reload, cancellation, replacement) also deletes a protected tail in slices that follow the recorded boundaries, instead of in one paragraph-sized turn.
- **One GTK-free planner** in `model/protected_install.rs`. It decides the slice sequence from the text, the paragraph profile, and C2's fold placement: the visible head slice, the hidden-tail slices, the boundary placement, and the clear plan. It never splits UTF-8 or `\r\n`. Each tail slice ends at most `tail_slice_bytes` after the previous one, rounded down to a safe boundary, so both the per-slice bound and the slice count (in steps of `tail_slice_bytes − 3`) hold. Its slice budgets are derived from B_settle and C0's measured per-KiB cost by one pure integer function. Kani (design D5, K1–K11, shard `core-protected-install`, in the proof-strength oracle table) proves, over a stated bounded domain: progress; that the slices concatenate to the exact original bytes; that no boundary splits a UTF-8 scalar or a `\r\n` pair; slice homogeneity; that every slice fits the budget derived from B_settle; boundary placement; clear-plan coverage; that for one giant paragraph the tag boundary falls exactly at `N_visible` (head visible, every later tail byte hidden; the executor's tagging of the two kinds is widget-tested) and the plan has at most ⌈len / step⌉ + 1 slices, `step = tail_slice_bytes − 3`; and, as a `should_panic` harness, that removing the rounding guard splits a scalar.
- **Callers.** The executor is C2's fold-at-insertion executor `ui/protected_fold.rs` (cross-cutting coordination, no role), which this change makes sliced. It is used by the load workflow (`ui/editor_page/load/`), whole-buffer replacement (`ui/editor_page/buffer_replacement/`), and the local-history preview installer. Their evidence surfaces report slice and boundary counts.
- **Axioms.** New ledger entries (provisionally S1–S3; final ids are allocated after C0's A23–A36, C1's D8 axioms and C2's P1–P7 when this change is applied): S1, char segments separated by a zero-size left-gravity mark segment are not merged (gravity pinned structurally, the absent merge pinned by per-append cost, as C0 pins A36); S2, appending invisible text re-validates a paragraph at a cost bounded by its visible text plus its segment count, not by its hidden bytes; S1 and S2 each get a `gtk-lush-axioms` probe and a sample. S3 extends C0's A35: AT-SPI `TextChanged` for an insertion or deletion is bounded by the inserted or deleted run. Like A35, it has no in-crate probe (the probe runner starts no AT-SPI bus) and is pinned by the lane's AT case. The segment-merge axiom itself (A36) comes from C0.
- **Spec revision.** `main-thread-responsiveness` "Large file installation yields in bounded GTK slices" is revised on top of C1's revision (C1 only changes which delimiters end a slice and keeps the one-turn rule). Of the whole stream, only this change revises the one-turn giant-paragraph rule; the delta carries C1's text and scenarios forward unchanged.
- **Data integrity is unchanged and re-proved.** The installed buffer's `text(start, end, true)` equals the decoded document byte for byte, whatever the slicing and cancellation points. Tests prove it on synthetic fixtures only.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`:
  - revise "Large file installation yields in bounded GTK slices": the one-turn giant-paragraph rule becomes a sliced hidden-tail rule for Protected View, and stays only for visible paragraphs, which the C1/C2 policy bounds;
  - add requirements for segment-boundary preservation, sliced protected clearing, and byte identity under slicing.
- `gtk-axiom-ledger`: add the segment-boundary, hidden-append re-validation, and AT insert-payload axioms that the sliced install depends on.
- `performance-regression-coverage`: add the 100/250/500 MB protected-install sweep, with the AT bridge off and on and with an interaction issued mid-install, to the responsiveness lane, plus the deterministic per-slice precondition gates.

## Impact

- **Depends on** `probe-long-line-toolkit-costs` (C0: budgets, watchdog, lane, fixture generator, A24/A36 probes) and `long-line-protected-view` (C2: the protected tag, the fold placement policy, the fold-at-insertion executor, and Protected View entry). It also depends on `document-scale-policy` (C1), through C2, for the GTK-matching paragraph-boundary function and the profile.
- **Code:**
  - `model/protected_install.rs` (new, GTK-free) with `model/protected_install/kani_proofs.rs`;
  - `model/buffer_replacement.rs` and `model/file_load.rs` (boundary dispatch);
  - `ui/editor_page/load/{policy,execution,retirement,evidence}.rs` (`retirement.rs` owns the `ClearingCancelled` phase);
  - `ui/editor_page/buffer_replacement/{policy,execution,evidence}.rs`;
  - C2's fold-at-insertion executor `ui/protected_fold.rs`;
  - `ui/window/local_history/preview_execution.rs`.
- **Verification:**
  - Kani shard `core-protected-install` with a CI-runner-measured budget, `ORACLES` entries with measured local seconds, and a proof-strength run (`scripts/kani-shards.py`, `make check-kani-shards`, `make proof-strength`);
  - new `gtk-lush-axioms` probes and samples (`make gtk-axioms`, `make check-gtk-axioms`);
  - widget tests for byte identity, cancellation and the precondition gates;
  - `make responsiveness-smoke` sweep entries.
- **Docs:**
  - `AGENTS.md` (the large-file handling and bounded transient file loads decisions);
  - `.agents/rules/ui.md` (large-buffer guardrails);
  - the ledger in the gtk4-libadwaita-internals references;
  - `docs/workflow-readability-matrix.md` (re-derived cells of WFR-DOCUMENT-LOAD and buffer replacement);
  - `docs/accessibility.md` (the bounded AT insert payload);
  - the long-line programme record `docs/next/long-line-documents.md`, and `docs/next/formal-verification.md` (the new Kani harness and axioms).
- **No change** to file bytes, persisted formats, GSettings, automation actions, or snapshot fields. Non-protected documents keep today's paragraph-aligned slicing.
