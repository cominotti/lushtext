## Context

This is C3 of the long-line programme (`docs/next/long-line-responsiveness.md`, "Proposed split"). Earlier changes in the programme:

- **C0, `probe-long-line-toolkit-costs`**: the budgets, the in-process lateness watchdog, the frame-clock timing, the synthetic fixture generator, `make responsiveness-smoke`, and the ledger axioms A23–A36. A36 is the segment merge.
- **C1, `document-scale-policy`**: the worker paragraph profile, the GTK-matching paragraph-boundary function (`\n`, `\r`, `\r\n`, U+2029), and `DocumentScale`.
- **C2, `long-line-protected-view`**: Protected View, the protected tail tag (`invisible=true, editable=false`), the pure fold placement policy `model/protected_fold.rs` (`FoldRange { paragraph, head_chars, tail_chars }`), the cross-cutting fold-at-insertion executor `ui/protected_fold.rs` that load, buffer replacement and reload call, and axioms P1–P7 (P4 extends A35, P7 pins that `insert_with_tags` of an invisible run never lays out the tail).

Today, installation slices end only at paragraph ends:

- `model/buffer_replacement.rs::next_replacement_boundary`, reached from load through the synonym `model/file_load.rs::next_install_boundary`, and from `ui/editor_page/buffer_replacement/execution.rs` and the local-history preview installer;
- the clear side is `ui/editor_page/load/policy.rs::{clear_slice_char_count, clear_slice_extends_to_paragraph_end}`, executed forward from the buffer start by `load/execution.rs` (and by `load/retirement.rs` for `ClearingCancelled`). A clear slice that stops inside a paragraph is extended to the next line start, so today a head's clear slice would run through the whole tail in one turn.

C1 already revises the requirement's delimiter set (`\n`, `\r`, `\r\n`, U+2029, through one shared paragraph-boundary function) and adds the CR-only/U+2029 and CRLF scenarios, but keeps the one-turn rule. This change's delta is written on top of C1's text.

A paragraph longer than `REPLACEMENT_INSERT_SLICE_BYTES` (256 KiB) is installed, and cleared, in one turn. That is the normative text of `main-thread-responsiveness` "Large file installation yields in bounded GTK slices". The rule is correct for *visible* paragraphs. GTK lays out a paragraph whole, so slicing one re-lays out the installed prefix on every later slice.

The maintainer decisions of 2026-10-11 set the working budgets. B_settle = 16 ms per settle dispatch. B_interact = 8 ms per interaction dispatch. Input-to-presented-frame ≤ 2 frames. C0 confirms the final values. Data integrity is absolute: file bytes never change. Fixtures are synthetic; the user's file is never copied into the repository.

GTK facts this design rests on, re-read in the GTK 4.22.5 sources under `target/huge-file-explore/src/`:

- `char_segment_cleanup_func` merges a char segment with the *immediately following* segment only when that segment is also a char segment. It builds the merged segment with `_gtk_char_segment_new_from_two_strings`, which copies both strings (`g/gtktextsegment.c:327-354`).
- `toggle_segment_cleanup_func` cancels a toggle-off with a matching toggle-on of the same tag. It looks ahead through any **zero-byte** segments, marks included (`g/gtktextsegment.c:550-590`).
- `cleanup_line` repeats a full pass over the line's segments until nothing changes (`gtk/gtk/gtktextbtree.c:4766-4797`), so its cost per mutation is O(segments on the line).

Taken together:

- appending same-tag slices one after another lets the toggles cancel, and then merges the growing tail with each new slice. That is O(M) bytes per slice and O(M²) in total, the "segment-merge cost" in the proposal;
- a zero-byte mark between the slices does **not** stop the toggles from cancelling, but it does stop the char-segment merge, because the merge only looks at the immediate neighbour.

Two more facts bound what "tagged at insertion" means:

- `gtk_text_buffer_insert_with_tags` is `gtk_text_buffer_insert` (which emits `insert-text` in full, after-handlers included) followed by `gtk_text_buffer_apply_tag` (`g/gtktextbuffer.c:1906-1942`). Text inserted at a tagged run's end is untagged (A25). So for the duration of that one call the new slice is untagged, and synchronous `insert-text` handlers see it so. It is never laid out untagged, because validation runs later from an idle (C2's P7), and never untagged across a dispatch.
- GtkTextView reports accessible changes from a `connect_after("insert-text")` handler and a `connect("delete-range")` handler (`gtk/gtk/gtktextview.c:2417-2422`, `7395-7425`). The AT-SPI context then reads the range back with `gtk_text_buffer_get_text (…, FALSE)` (`gtk_text_view_accessible_text_get_contents`, `gtk/gtk/gtktextview.c:10703-10716`; emission in `g/gtkatspicontext.c:1741-1790`, only when the bus has event listeners). The insert event therefore carries the whole slice text (not yet hidden), and the delete event of a hidden range carries no hidden characters. Both cost O(slice), never O(paragraph).

## Goals / Non-Goals

**Goals:**

- No settle dispatch of a protected install or clear exceeds B_settle at any admitted size up to `REFUSE_TO_OPEN`, with the AT bridge off or on, on the reference lane.
- Total work for a protected tail stays linear in its size. There is no O(M²) segment merge.
- Byte identity: after a completed install, `text(start, end, true)` equals the decoded document, for every slicing and every placement of cancellation points.
- The adoption decision is explicit, evidence-based, and recorded before any code is written.
- The planner is pure, GTK-free and proved with Kani. The GTK assumptions are ledger axioms pinned by probes.

**Non-Goals:**

- Slicing *visible* giant paragraphs. They cannot occur in documents that C1/C2 classify correctly, and slicing them is still quadratic.
- Editing protected paragraphs (C5).
- Changing the fold placement, N_visible, L_protect, or what Protected View offers (all C2).
- Changing file bytes, persisted formats, GSettings, automation actions or snapshot fields.
- Upstream GTK fixes. Reports may be filed, but nothing waits on them.

## Decisions

### D1. The adoption gate is the first task, and it can close the change

The gate runs C0's lane `install` case against a build with C2 applied, because C0's own `install` results predate C2 and measure today's untagged one-turn install (a lower bound on parse and copy cost, recorded alongside as context). The fixtures:

- one protected paragraph of 100, 250 and 500 MB (synthetic, from C0's generator);
- installed by C2's one-turn fold-at-insertion executor;
- with the AT bridge off, and on with a listener;
- clearing the same paragraph (eviction, close, reload).

**Adopt** if any install or clear measurement has a largest settle dispatch above B_settle, using the final value that C0 records. A case that reaches the lane's wall cap (`exceeded_cap`) counts as above B_settle. **Do not adopt** otherwise: record the measurements and the decision in the programme record `docs/next/long-line-documents.md` and in this change's tasks, and close the change without implementation. The gate is checked again whenever B_settle's recorded value or the toolkit version changes.

If only one half is over budget, adopt only that half. That means install slicing alone or clear slicing alone. The planner's two plans are independent for this reason.

Alternative considered: adopt unconditionally because 16 ms is tight. Rejected. The change adds K marks per giant paragraph and a GTK dependency (new axioms). Those costs should be paid only on evidence. The note that "the stricter budget makes adoption likely" is a prediction, not a measurement.

### D2. The exception covers the hidden tail only

A slice may end inside a paragraph **only** at or after C2's fold point for that paragraph. The head of the paragraph is everything before the fold point, at most N_visible characters. It goes in **one** visible slice, together with any whole short paragraphs that fit the budget. Tail slices are inserted with the protected tag applied by `insert_with_tags`, so tail text is untagged only inside that one synchronous call (see Context) and never across a dispatch or in a layout. That is the same guarantee as C2's single-turn executor (P7). Every synchronous `insert-text` handler LushText owns (C1's profile handler, C2's edit-growth entry) treats an install slice as installation (D6), so none of them reacts to the momentarily untagged run.

Why this does not bring back the quadratic re-layout: invisible text is left out of the paragraph's PangoLayout (A24, from C0). After every tail slice the visible text of the paragraph is the same fixed head. A new axiom pins the cost of re-validating the paragraph after a hidden append (provisionally **S2**): it is bounded by the visible text plus the segment count, not by the hidden bytes. If that probe fails, this design is not viable. The gate then closes the change and the failure is filed upstream.

One path can still read the whole paragraph, hidden text included: log attrs (A26), which cursor and word motion compute over the logical paragraph. Nothing in the install may move an iterator by cursor or word position inside a protected paragraph, so no slice pays O(paragraph). The S2 probe holds the insert mark fixed and runs no cursor motion, and the lane's per-slice timing is the evidence that no other per-slice path does.

Alternative considered: slice the head too. Rejected. The head is visible, so slicing it is exactly the quadratic case, and the head is already bounded by N_visible.

### D3. A zero-width mark separates adjacent tail slices

After each tail slice, the executor creates an anonymous **left-gravity** `GtkTextMark` at the end of the inserted run. The next slice is inserted at that iterator.

- With left gravity the mark stays to the left of the inserted text, so the segment order is `prev | mark | new`. `char_segment_cleanup_func` sees a mark after `prev` and does not merge.
- A right-gravity mark would end up after `new`. `prev` and `new` would be adjacent and merge. So the gravity is part of the contract, and a probe pins it.
- The toggles between slices cancel through the zero-byte mark (verified above). The tail ends up as one tag range over K char segments, so C2's predicate "the tail carries the protected tag" stays unchanged.

New axiom, provisionally **S1**, pinned by a probe:

- two char segments separated by a zero-byte mark segment are not merged by `cleanup_line`;
- inserting at a left-gravity mark's iterator places the text after the mark.

Segment structure is private to GTK, so, like A36, the absence of the merge is observed by cost only: the probe records the per-append time while the tail grows (flat with left-gravity marks, growing without them or with right-gravity marks). The gravity half is observed structurally, through the mark's offset after the insertion.

**Boundary lifetime.** Boundaries stay until their range is deleted or the buffer is disposed. They are never removed one by one while their neighbours are present: each removal would merge two segments and copy both, which is the quadratic cost again. After a clear has deleted every tail range (D4), the boundaries left collapsed at one position are deleted. The only possible merge then is the head with following text, which is bounded by N_visible.

**Cost of the boundaries.**

- K ≤ ⌈tail bytes / (tail slice bytes − 3)⌉ (D5). That is about 2,000 for 500 MB at 256 KiB.
- `cleanup_line` costs O(K) per slice, so O(K²) pointer steps in total, about 4·10⁶ for the worst admitted document at 256 KiB slices. At a 16 KiB slice K is about 32,000 and the total about 10⁹ steps, still O(K) ≈ 32,000 steps per slice; the lane's per-slice timing at the chosen slice size is the evidence.
- Each boundary is one GObject plus one segment. They are charged to the editor's live-memory estimate as a constant per boundary, through C1's `DocumentScale` multiplier inputs.

Alternatives considered:

- **(a) Alternate two protected tags with identical properties.** The toggles of different tags do not cancel, so the toggle segments themselves separate the char segments, and no marks are needed. Kept as the **fallback** if S1 fails. Its cost: C2's tag predicates, the fold evidence and the AT documentation must accept two tags.
- **(b) Larger slices.** This reduces K but not the per-slice O(M) copy. Rejected.
- **(c) No mitigation.** The merge copies about M²/(2·slice) bytes, roughly 500 GB for 500 MB. Rejected.

### D4. Protected clearing follows the recorded boundaries, from the end

Any clear of a buffer that holds protected tails uses a clear plan derived from the boundary ledger the install recorded. Clears happen on eviction, close, reload, replacement, or cancellation of a partial install.

- One turn deletes one recorded tail range: the range between two consecutive boundaries.
- The recorded tail ranges are deleted first, from the end of the buffer backwards. Marks survive a deletion of the text around them, so after each deletion the surviving neighbours are still separated by marks and no merge happens.
- Only then do heads and ordinary paragraphs take today's forward paragraph-aligned clear slices. Deleting the tails first matters: today's clear extends a slice that starts inside a paragraph to the next line start, so a head's slice would otherwise delete the whole tail in one turn.
- When every range is gone, the collapsed boundaries are deleted (D3).
- AT-SPI deletion payloads are bounded by the slice in the same way as insertion payloads. New axiom, provisionally **S3**, extending C0's A35: the AT-SPI `TextChanged` event emitted for an insertion or deletion is built from the inserted or deleted range only (so a slice bounds its size and its cost), and does not depend on the text that remains in the paragraph. The insert event carries the slice's text, because it is emitted before the tag is applied; the delete event of a hidden range carries no hidden characters. As for A35, there is no in-crate probe, because GtkTextView's accessible-text emission is reachable only through an AT-SPI bus with a listener, which the probe runner does not start. S3 is pinned by the lane's AT case, and its ledger row says so.

The ledger is per-buffer editor state, not install-session state: it is created by the install, outlives the install terminal (eviction, reload and close clear the buffer much later), and is emptied by the clear that deletes its ranges. It holds marks and counts only, never text. Load's `ClearingExisting` and `ClearingCancelled` phases and buffer replacement's clear turns consume it.

### D5. One GTK-free planner in `model/protected_install.rs`

`model/` is the right home because three workflows call the planner: `WFR-DOCUMENT-LOAD`, `WFR-BUFFER-REPLACEMENT`, and `WFR-LOCAL-HISTORY`'s preview installer. The existing `next_replacement_boundary` already lives in `model/` for the same reason. Forking it would let two copies drift.

The planner takes three inputs:

- the decoded text, read through `&str` slices of the retained body, with no copy;
- the fold placement from C2's policy (`FoldRange`, in characters), converted to a tail-start byte offset per protected paragraph by walking at most the head's `N_visible` characters;
- a `ProtectedInstallLimits` value: head budget, tail slice bytes, clear slice bytes.

It returns a typed `ProtectedSlice { end, kind: Visible | HiddenTail, boundary_after }` one step at a time. The production limits are derived from B_settle and C0's measured per-KiB cost by `ProtectedInstallLimits::for_settle_budget` (see "The budget is derived from B_settle" below), with comments citing that measurement. The limits are parameters so that Kani and the property tests can use small budgets.

The planner reuses C1's paragraph-boundary function. It never splits a UTF-8 scalar or a `\r\n` pair. Paragraphs without a fold keep `next_replacement_boundary` exactly.

**Chained tail ends, and why the count bound uses `B − 3`.** With `B` the per-slice tail budget (`tail_slice_bytes`), each hidden-tail slice starts at the previous slice's end `e` and ends at `min(e + B, tail_end)`, rounded down to the largest safe boundary at or below it (a char boundary not between `\r` and `\n`). The tail end is always safe: it is the end of the paragraph's content, before its delimiter, and a tail contains no delimiter, so inside a tail only the char-boundary half of the rounding ever acts. Rounding loses at most `MAX_UTF8_SCALAR_BYTES − 1 = 3` bytes, so every tail slice except the last is between `step = B − 3` and `B` bytes, and the last is at most `B`. `B ≥ MIN_TAIL_SLICE_BYTES = 4` is a precondition of `ProtectedInstallLimits`, enforced by its constructor, so `step ≥ 1` and every slice advances by at least one byte. The tail therefore takes at most `⌈tail bytes / step⌉` slices. The count bound has to use `step`, not `B`: no planner that keeps every slice within `B` and on char boundaries can meet `⌈len / B⌉ + 1`, because a tail of 4-byte scalars with `B = 5` admits one scalar per slice, so a 40-byte tail needs 10 slices against ⌈40 / 5⌉ = 8. At 256 KiB the 3-byte difference is immaterial. (An earlier draft anchored the ends to a fixed grid `t + i·step` to make the two bounds hold together; the chained ends above already satisfy both with fewer slices, and the grid, which can round two consecutive grid points to the same end when `step < 4`, needs an extra skip rule to keep progress, so it was dropped.) The rounding is one private function, `round_down_to_safe_boundary(text, floor, candidate)`, and the stepper is a private `plan_with(text, folds, limits, rounding)` that production calls only with that function; the `should_panic` harness below passes the identity (the raw candidate `e + B`) instead, through the same stepper. The stepper reads the text only through `as_bytes()` and never indexes the `&str` at a chosen end, so with the guard removed the only failing check is the harness's K3 assertion, not an unrelated slicing panic that would also satisfy `should_panic`.

**The budget is derived from B_settle, in one pure function.** `ProtectedInstallLimits::for_settle_budget(settle_budget_us, cost_ns_per_kib, margin_percent) -> Result<u64, SliceBudgetUnattainable>` derives one slice size with integer arithmetic only (saturating, no floats, as the whole-pixel policies do): the largest whole number of bytes, at most `MAX_SLICE_BYTES`, whose predicted cost (`bytes × cost_ns_per_kib / 1024` ns, rounded up) plus `margin_percent` of it fits `settle_budget_us × 1000` ns. `MAX_SLICE_BYTES` (a named constant equal to the existing 256 KiB load insertion slice) bounds the result when the cost is zero or tiny, where "the largest" would otherwise be unbounded. It is closed-form (one saturating multiply and one division, then at most one decrement to correct the round-up), with no search loop, so K10 needs no unwind bound over `u64`. It is called once with the tail cost, giving `tail_slice_bytes`, and once with the clear cost, giving `clear_slice_bytes`; `ProtectedInstallLimits::new` takes both. It returns `Err(SliceBudgetUnattainable)` exactly when the result would be below `MIN_TAIL_SLICE_BYTES`. The production limits are this function applied to C0's recorded B_settle and C0's AT-on per-KiB costs, held as named constants (`B_SETTLE_US` cited from C0, `PROTECTED_TAIL_COST_NS_PER_KIB` and `PROTECTED_CLEAR_COST_NS_PER_KIB` citing the C0 measurement), and a unit test pins the derived `PROTECTED_TAIL_SLICE_BYTES` and `PROTECTED_CLEAR_SLICE_BYTES`. So "every slice fits the budget derived from B_settle" is two proved steps joined by the constructor, not one harness: K10 proves that every `Ok` result fits B_settle and is accepted by `ProtectedInstallLimits::new` (at least `MIN_TAIL_SLICE_BYTES`), and K5 proves that every hidden-tail slice is within whatever `tail_slice_bytes` the constructor accepted (in K5's domain, 4..=7). No harness runs the `u64` derivation and the planner together, which would put 64-bit division in front of the symbolic text for no extra property. The derivation does not prove that GTK's per-KiB cost is what C0 measured; that is the lane's job (task 6).

**Kani harnesses** (`crates/lushtext-core/src/model/protected_install/kani_proofs.rs`, declared by `model/protected_install.rs` as `#[cfg(kani)] mod kani_proofs;`). They call the production planner and C1's paragraph-boundary function; folds come from C2's real `model/protected_fold.rs::fold_step`, run over the symbolic text's paragraph lengths with a symbolic `N_visible` (C2's step already takes `N_visible`, `N_dense`, `V_batch` and `R_batch` as inputs, so no refactor is needed; if C2 lands with them as constants, this change makes them parameters and production passes the constants). Nothing is copied.

Two domains, each stated in the harness doc comment and in the rustdoc of `plan_protected_install` and `ProtectedInstallLimits::for_settle_budget`:

- **Document domain (K1–K7).** Every valid UTF-8 string of at most 10 bytes over {`a` (1 byte), `é` (2), `€` (3), `𝄞` (4), `\r`, `\n`, U+2029}, so several paragraphs, CR-only, CRLF and U+2029 delimiters, and every scalar length appear; `tail_slice_bytes` in 4..=7 (`step` 1..=4); clear slice budget in 4..=7; `N_visible` in 1..=3 characters; `N_dense` in 1..=3, `R_batch` in 1..=2 and `V_batch` in `R_batch·N_dense..=9`, symbolic within C2's stated preconditions on `fold_step`, so density folding binds on some inputs.
- **Giant-paragraph domain (K8, K9, K11).** One paragraph: up to 12 bytes of non-delimiter scalars from {`a`, `é`, `€`, `𝄞`} (its content, `len` bytes), followed by no delimiter, `\n`, `\r`, `\r\n` or U+2029; `N_visible` in 1..=3; `tail_slice_bytes` in 4..=7; and density parameters for which C2's density rule does not bind on a first paragraph (`N_dense` in 1..=3, `R_batch` in 1..=2, `V_batch ≥ N_visible + (R_batch − 1)·N_dense`), so C2 shows `min(len chars, N_visible)` characters. This is "the plan for one giant paragraph" at a bounded size. The delimiter is not part of the paragraph's plan: it is visible, untagged, and starts the next `Visible` slice, which for a lone paragraph is a final one-delimiter slice that K9 does not count.

Why the bounds are enough: every decision the stepper takes is local. A slice end depends only on the at most 3 bytes below its candidate end (a scalar of 1–4 bytes, or, for an ordinary slice, a `\r\n` pair) and on whether a fold point or paragraph end falls there; the head end depends only on counting `N_visible` scalars. `B` in 4..=7 against scalar lengths 1..=4 puts a candidate end at every byte phase of every scalar length, and 10–12 bytes give at least two tail slices for some inputs of every `B`, so each local configuration (candidate end inside a scalar of each length, an ordinary end at a CRLF, the fold point, the tail end) occurs in the domain, and K1–K9 are inductive over slices: each is a per-slice property plus a sum. The `kani::cover!` calls listed below show these configurations are reached. Unwind bounds follow from the sizes: the stepper's loops run at most once per byte, so K1–K7 use `#[kani::unwind(12)]` (10 bytes plus the terminating iteration and the rounding loop's 3) and K8, K9 and K11 `#[kani::unwind(16)]` (12 content bytes plus a 2-byte delimiter plus slack); a harness whose unwinding assertion fails is a wrong bound to raise, never to silence with `--no-unwinding-checks`. Full-scale sizes (256 KiB budgets, multi-megabyte tails) are covered by the property tests of task 3.4, which check the same assertions, not by Kani.

| Id | Harness | Property |
| --- | --- | --- |
| K1 | `protected_plan_always_progresses` | Every step strictly advances while input remains, and the plan terminates. |
| K2 | `protected_plan_reproduces_the_exact_bytes` | The concatenation of the slices, in order, equals the input byte for byte: nothing dropped, duplicated or reordered. |
| K3 | `protected_plan_never_splits_a_scalar_or_crlf` | Every slice end is a char boundary of the input and never falls between `\r` and `\n`. |
| K4 | `protected_plan_slices_are_homogeneous` | A slice is wholly visible or wholly hidden; a mid-paragraph end occurs only at or after that paragraph's tail start. |
| K5 | `protected_plan_slices_fit_their_budget` | Every hidden-tail slice is at most the `tail_slice_bytes` that `ProtectedInstallLimits::new` accepted, and every hidden-tail slice but a paragraph's last is at least `tail_slice_bytes − 3`; a visible slice is at most the budget or one whole visible paragraph (at most `N_visible` characters by C2's policy). |
| K6 | `protected_plan_places_boundaries_between_tail_slices` | `boundary_after` holds exactly between two consecutive hidden-tail slices of one paragraph. |
| K7 | `protected_clear_plan_covers_the_buffer_once` | The clear ranges are the recorded tail ranges in reverse order and, with the ordinary clear slices, cover the buffer exactly once; every clear range is at most the clear budget. |
| K8 | `giant_paragraph_tag_boundary_is_at_n_visible` | For one giant paragraph longer than `N_visible` characters, C2's `FoldRange` has `head_chars == N_visible`, and the visible/hidden switch falls exactly at the byte offset of the paragraph's `N_visible`-th scalar end (`text[para_start..head_end].chars().count() == N_visible`): every byte before it is in `Visible` slices and every byte from it to the tail end that C2's `FoldRange` gives (`head_chars + tail_chars`, the content end) is in `HiddenTail` slices. For a paragraph of at most `N_visible` characters there is no fold and every byte is `Visible`. K8 proves the slice kinds; the executor's single `match` that inserts `Visible` with `insert` (untagged) and `HiddenTail` with `insert_with_tags` (the protected tag) is checked by the widget tests of task 5. |
| K9 | `giant_paragraph_slice_count_is_bounded` | The plan for one giant paragraph's content has at most `⌈len / step⌉ + 1` slices (`len` the content bytes, delimiter excluded; `step = tail_slice_bytes − 3`): one head slice plus at most `⌈tail bytes / step⌉` tail slices. |
| K10 | `settle_budget_derivation_fits_and_never_overflows` | For every `u64` input, `for_settle_budget` never panics or overflows; an `Ok(bytes)` result is in `MIN_TAIL_SLICE_BYTES..=MAX_SLICE_BYTES`, is accepted by `ProtectedInstallLimits::new`, has a predicted cost, margin included, of at most the settle budget, and is the largest such value (`bytes + 1` does not fit, or `bytes == MAX_SLICE_BYTES`); it is monotone in the settle budget and antitone in the per-KiB cost; `Err` occurs exactly when `MIN_TAIL_SLICE_BYTES` does not fit. If its measured time breaks the shard margins, the monotonicity half (two calls) moves to its own harness K10b before any input is narrowed. |
| K11 | `unrounded_tail_end_splits_a_scalar` (`#[kani::should_panic]`) | The same stepper with the rounding guard removed (`plan_with` given the identity, so a tail slice ends at the raw candidate `e + B`) produces a slice end inside `é`, `€` or `𝄞`, so the K3 assertion fails. This pins the guard from both sides: if production stops rounding, K3 fails; if the stepper stops honouring its rounding argument, so the guard can no longer be removed, K11 finds no counterexample and fails. |

`kani::cover!` on each interesting branch, so no harness is vacuous: a candidate tail end rounded down by 0, 1, 2 and 3 bytes; an ordinary slice end at a `\r\n` pair and after a U+2029 delimiter; a density fold whose head is shorter than `N_visible`; a tail of exactly one budget; a tail of at least two slices with a boundary; a document with a visible paragraph, a protected paragraph and a second protected paragraph; a head shorter than the slice budget and one longer; K9's bound reached with equality; K10 returning `Ok` below `MAX_SLICE_BYTES`, `Ok` at `MAX_SLICE_BYTES`, and `Err`; K11's counterexample inside a 2-, 3- and 4-byte scalar.

If only one half is adopted (D1), only the harnesses of that half are added: K1–K6 and K8–K11 for install, K7 and K10 for clear.

The harnesses go in one new shard, `core-protected-install`, in `scripts/kani-shards.py`, gated `pull-request` if its measured runner wall time is at most 15 minutes (a planner defect corrupts bytes, so a pull request should catch it), otherwise `scheduled`; its `ci_minutes`, `ci_peak_gib` and `measured_in` come from two dispatched runs of the Kani workflow in measurement mode, the larger of each. Every harness is listed in `ORACLES` under `crates/lushtext-core/src/model/protected_install.rs`, cheapest first, with its measured local seconds and its `ci` or `local` tier. `make proof-strength` is run for that module; its mutant and kill counts and the triage of every survivor go into `docs/next/formal-verification.md`, as does any counterexample found while writing the harnesses. A planner counterexample is a defect fixed failing-first (a unit test reproducing it before the fix), not a narrowed domain. If the shard breaks a margin, it is split before any bound is reduced, and a reduced bound is recorded with the property it weakens.

The same properties are property-tested at full scale.

### D6. The executor stays C2's cross-cutting fold-at-insertion executor (`ui/protected_fold.rs`)

No new role home is added. C2's executor becomes a sliced session that plugs into the scheduling points that already exist:

- the load session phases `Installing`, `ClearingExisting` and `ClearingCancelled`;
- the buffer replacement turns;
- the local-history preview installer.

The current envelopes are unchanged:

- weak editor ownership;
- generation and freshness checks before each slice;
- projections suspended;
- `begin_irreversible_action` around the whole install, so no undo history is recorded;
- `modified` false;
- non-editable and non-saveable until the terminal.

C1's synchronous `connect_after("insert-text")` profile handler, and C2's edit-growth entry that runs in the same emission, must treat install slices as installation, not as edits. They read the same projection-suspension guard the load already raises.

Each owning workflow's pure decisions stay in its own `policy.rs`: whether the sliced protected path applies, and which phase a slice belongs to. Those modules only *call* the planner.

### D7. Evidence, not new automation surface

`ui/editor_page/load/evidence.rs` and `ui/editor_page/buffer_replacement/evidence.rs` gain the following fields:

- protected tail slices installed;
- boundaries live;
- the largest slice in bytes;
- clear ranges remaining.

Widget tests read these. No snapshot field or readiness predicate is added: the existing load and replacement readiness already covers a sliced install. Snapshots stay content-free.

### D8. Data integrity is re-proved, not assumed

These tests use synthetic fixtures only:

- **Byte identity.** After the install, `text(start, end, true)` equals the decoded body. The checks cover CR-only text, U+2029, `\r\n` that straddles a budget, multibyte characters at every offset modulo 4, and several protected paragraphs mixed with short ones.
- **Cancellation.** At every phase, cancellation leaves no partial content that could be saved, and a re-install is again byte-identical.
- **Save As.** Save As after a sliced install writes the source bytes exactly.

The `data-safety` skill runs explicitly over the change before sign-off.

## Risks / Trade-offs

- **[S1 or S2 fails on the probe, or on a later GTK]** → The design is not viable as written. The S1 fallback is the alternating tags (D3a). If S2 fails, the gate closes the change and the upstream report is filed. A probe failure after a toolkit update reopens this design under the ledger rule.
- **[K boundary marks slow other line operations]** → Iterator offset lookups inside a protected paragraph walk O(K) segments. Protected View exposes no caret and runs no in-buffer search (C2), so only the executor, save snapshot and navigation touch them, each with bounded K. The precondition widget test asserts K ≤ ⌈tail / (slice − 3)⌉.
- **[A per-slice path reads the whole paragraph]** (log attrs, A26, or an iterator offset lookup over one giant segment) → The boundaries keep every char segment at most one slice long, the executor runs no cursor or word motion inside a protected paragraph, and the lane's per-slice timing at 500 MB is flat across the tail or the gate fails.
- **[An interaction during a long install misses the frame budget]** → Each dispatch is at most B_settle, so an input arriving mid-install waits at most one slice. Whether that meets input-to-presented-frame ≤ 2 frames is measured by the sweep's mid-install interaction, not assumed.
- **[A slice still exceeds B_settle with the AT bridge on]** → The tail slice size is a measured constant. C0's lane sweeps it until AT-on cost per slice fits with margin. Below about 16 KiB the lane records that slicing cannot meet the budget, and that becomes an Open Question for the maintainer instead of a silent pass.
- **[Partial protected state becomes observable]** → The current envelopes (non-editable, non-saveable, projections suspended, typed terminal) are kept and widget-tested at every phase. Tail text never exists untagged (D2).
- **[A planner bug corrupts bytes]** → Kani K2/K3 plus full-scale property tests plus the byte-identity widget tests. The planner never transforms text. It only chooses offsets.
- **[Boundaries leak after a clear]** → D4 deletes the collapsed marks after the ranges are gone. A widget test asserts that no anonymous boundary marks remain after eviction, reload and close.
- **[Memory]** → Slices borrow the decoded body that is already retained, and the boundaries are constant-size. There is no new transient payload, and transient-load admission is unchanged.

## Migration Plan

1. Pass the gate (D1). If it is not met, record the result and stop. There is nothing to roll back.
2. Land the axioms (S1–S3, final ids allocated after C0's A23–A36, C1's D8 axioms and C2's P1–P7) with their probes, samples and, for S3, the lane's AT case. Run `make gtk-axioms` first.
3. Land the planner, its Kani harnesses and its property tests.
4. Wire the executor and evidence. Revise the spec and docs in the same change.
5. Run the responsiveness sweep and record the results.

To roll back, revert the change. C2's one-turn executor is still the fallback path, because a document with no protected paragraph over the tail budget never takes the sliced path.

## Open Questions

- **Final slice constants.** The values of the tail slice bytes and the protected clear slice bytes are set by C0's AT-on measurement. Is the maintainer willing to accept a tail slice as small as 16 KiB, which means about 32,000 boundaries for 500 MB, if that is what B_settle with AT on requires?
- **Mark boundaries or alternating tags (D3).** The default is marks, and the S1 probe decides. Does the maintainer prefer the alternating tags regardless, to keep the buffer free of anonymous marks?
- **Half adoption.** If only clearing, or only installing, is over budget, should the change ship that half alone (as D1 proposes) or wait until both halves need it?
- **Gate evidence source.** Is the reference lane (private headless mutter, cairo, ≤ 16 cores) sufficient for the gate, or should the gate also require a consented isolated-XDG run on the maintainer's session, as PROPOSAL.md decision 8 suggests for C2?
- **Disposition if not adopted.** Archive the change with a "not adopted" note, or leave it open in `openspec/changes/` as a dormant conditional that re-evaluates on a budget or toolkit change?
- **Final ledger ids.** S1–S3 are provisional names. C0 takes A23–A36, C1's D8 axioms and C2's P1–P7 take the next ids, and `stop-expander-bind-time-child-scans` (provisionally T1) takes the next free id whenever it lands, so the final ids are fixed in application order when this change is applied.
- **Mid-install interaction budget.** With B_settle = 16 ms and a 120 Hz frame, an input that arrives just after a slice starts can wait two frames. Is B_settle per install dispatch acceptable for that, or should install slices be held to B_interact (8 ms) while the window has pending input?
