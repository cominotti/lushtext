## Context

The proposal's evidence is `docs/next/long-line-responsiveness.md` (sections "Why", "C1", and "Maintainer decisions recorded (2026-10-11)"). Every fixture is synthetic; the user's file is never copied into the repository. The current state:

- **Byte-only policy.** `services/file_limits.rs` exposes `FileSizeCheck::classify(bytes)` with tiers at 1 MB (large document), 10 MB (syntax off), 50 MB (undo off, local history unavailable) and 500 MB (refuse). It has 17 consumer files that name the type or module. The `EditorPage::size_check()` accessor has further callers that do not: `ui/editor_page/local_history.rs:213` (history availability), `ui/editor_page/minimap/admission.rs:57` (minimap readiness block) and `:168` (cheap availability), and `ui/window/encoding/execution.rs:99` (whether the lossy-encoding pre-check is deferred to save). `ui/editor_page/minimap/policy.rs` carries the size decision as `MinimapAvailabilityPolicy::syntax_enabled` and the `TooLarge` variant, and `ui/automation.rs:1973` maps that variant to the `minimap-too-large` absence reason. `scripts/run-mutants.sh:8` uses `services/file_limits.rs` as the default `make mutants-smoke` file. The 1 MB tier (`LARGE_FILE_TOAST`) has one consumer only: the "Large document mode is active." sentence in `ui/editor_page/accessibility.rs`; no toast is shown anywhere. Syntax is applied in `ui/editor_page/load/execution.rs:575-581` (`if size_check.syntax_enabled() { reapply_language() }`) and in `document_identity.rs::republish_document_identity`. Save rewrites `size_check` from the saved byte count (`save/execution.rs:467-470`), so a save that shrinks a document below 10 MB, followed by a rename or Save As, currently **re-enables** syntax.
- **Newline-only slicing.** `model/buffer_replacement.rs::next_replacement_boundary` cuts only after `'\n'`. `model/file_load.rs::next_install_boundary` is a named synonym of it. GtkTextBTree splits lines with `pango_find_paragraph_boundary`: `\n`, `\r`, `\r\n` and U+2029.
- **Load analysis.** `analyze_decoded_content_cancellable` (`services/editor_io.rs`) already makes one cancellable sliced pass. It runs a `LineEndingTally` with CR carry plus a per-character NBSP and zero-width count. That pass is where the profile goes.
- **Minimap.** `install_minimap` (`minimap/watch.rs:26-64`) creates the one `GtkSourceMap` at editor construction, **with its view already set**, pairs it with a `RenderHoldOverlay`, and connects a guard to the map's vadjustment. `create_native_source_map` has that single call site; nothing rebuilds a map today. Only the chunked install suspends the minimap projection (`begin_load_installation(editor, true)`); the direct install path (`install_loaded_direct`, up to the 1 MiB synchronous threshold) leaves the map attached while `set_text` inserts the body. `cheap_minimap_availability` reads `size_check().syntax_enabled()`. `current_availability` also waits for the post-attach wrapped-layout analysis cache. `run_minimap_refresh` (`projection_execution.rs:170-175`) and `detach_minimap_projection` (`retirement.rs:100`) clear `view` on a mapped, realized `GtkSourceMap`. The minimap's own `insert-text` handler (`watch.rs:75-90`) counts inserted lines by `'\n'` only, so modified-line marks are wrong for CR-only and U+2029 text. `gtk_source_map_queue_update` arms a frame-clock tick (`update_id`) that `disconnect_view` never removes (gtksourcemap.c:564-591 and 948). The tick later calls `gtk_scrollable_get_vadjustment(NULL)`, which produces the CRITICALs.
- **Feature sites.**
  - Invisibles: `ui/editor_page/invisibles.rs` (SpaceDrawer matrix).
  - Search highlighting: `ui/search_bar/execution.rs::begin_session`, which calls `context.set_highlight(true)`.
  - Bracket matching: GtkSourceBuffer's default, `highlight-matching-brackets = TRUE`.

Stakeholders: the maintainer, who records decisions in PROPOSAL.md, and C0 `probe-long-line-toolkit-costs`, which supplies the threshold values and pins the toolkit behaviour this design depends on.

## Goals / Non-Goals

**Goals:**
- Classify every document by **paragraph length, density and total size**, and do it before any expensive GTK feature sees the content.
- Use one paragraph-boundary definition that matches GTK, shared by the slicer, the profile and the edit-time watch.
- Keep one GTK-free, Kani-proved monotone policy that every feature consumer reads.
- Downgrade features synchronously on edits, in the same signal emission that creates a giant paragraph, and never re-enable them during the session.
- Decide minimap availability before attach, and never detach a map with a pending tick.
- Remove the 2 s context-engine stall and both CRITICALs.

**Non-Goals:**
- Protected View, the display fold, the Line Inspector and worker find. These are C2, which reads `DocumentScale::protected_view` and owns everything it triggers. Until C2 lands, crossing `L_protect` only adds `protected_view` to the reduced-features notice.
- Making Pango layout of a giant paragraph cheap. After C1, Pango stalls on giant paragraphs remain as a **declared degraded mode**.
- Changing `REFUSE_TO_OPEN`, the undo tier or the local-history tiers. Their values move into `DocumentScale` unchanged.
- Automation snapshot fields. C2 adds `protected_view` and the fold counts; see Open Questions.
- Window-construction stalls (C4).

## Decisions

### D0. The values this change applies (task 0.1)

C0 (`probe-long-line-toolkit-costs`) is archived. Its reference set was finalised
by the idle re-confirmation, run `rr-c0-reconfirm-20261013`
(`docs/next/long-line-documents.md`, "Idle re-confirmation (2026-10-13)"), which
supersedes the loaded run `rr-c0-reference-20261011` where they differ. The
constants live in `model/document_scale.rs`, each citing that record; the
budgets are read from `model/responsiveness_budget.rs` and never restated.

| Value | Applied | Source |
|---|---|---|
| `S_syntax` | **1 MiB** (1,048,576 bytes) | **Not derived** by C0 (a size-independent ~42 ms first-highlight stall fails every size down to the 16,000-byte floor). Open; the conservative PROPOSAL.md candidate (decision 4: keep 10 MB or about 1 MiB) is applied. `S_syntax_interact` (16,000 bytes) is informational only. |
| `L_syntax` | 2,000 characters | C0 idle: 2,000 / 3,000 |
| `L_minimap` | 22,000 | C0 idle: 22,000 / 50,000 (lower bound; `[22,000, 50,000)` not bisected) |
| `L_invisibles` | 50,000 | C0: 50,000 / 74,000 |
| `L_search_highlight` | 10,000 | C0 idle: 10,000 / 15,000 |
| `L_protect` | **500** | **Not derived** (the input-to-frame half fails at every length on the headless lane's frame floor). The stall half, `L_protect_stall`, is 500 / 710; C2's gate (`docs/next/long-line-evolution.md`) takes that stall basis, so C1 applies it. Open (see Open Question 1). |
| `N_visible` | 500,000 | C0: 500,000 / 710,000 |
| `R_batch` | 224 rows | A31's 112 rows per 2000 px batch (wrap off, default font), doubled for the 50 % minimum zoom |
| `V_batch` | 56,000 characters | dense 500-character line × 112 rows (idle: dense 500 / 710) |
| `N_dense` | 250 | `V_batch ÷ R_batch` (informational; C2's fold) |
| B_interact | 8 ms | `responsiveness_budget::B_INTERACT_US` (C0: within) |
| input-to-frame | ≤ 2 reference frames | `responsiveness_budget::INPUT_TO_FRAME_FRAMES` |
| B_settle | 16 ms | `responsiveness_budget::B_SETTLE_US` (C0 idle: indeterminate) |
| `S_minimap`, undo, history, refuse | 10 MB, 50 MB, 10/50 MB, 500 MB | unchanged byte tiers |

None of `L_minimap`, `L_invisibles` or `L_search_highlight` was left open, so the
`min(L_syntax, N_visible)` default of Open Question 1 was not needed.

**C0's ledger and D8 (task 0.2).** C0 pinned A23–A37. A27, A29, A31 and A32
already list `document-scale-policy` as a dependent design. None of D8's four
axioms was among them, so all four are new: **A42** (paragraph delimiters,
probe), **A43** (line-length read cost, verified by source: a duration is not
a probe verdict input), **A44** (same-emission language downgrade, probe) and
**A45** (unparent stops a pending tick, probe; it also covers a scrolled
`GtkSourceMap`).

### D0a. Apply-time correction: the buffer's lines depend on its edit history

A headless probe during apply (now part of ledger A42, `probe_a42`) showed that
`GtkTextBuffer` does not re-derive its lines from the whole text after an edit:

- an **insertion splits only its own text** at Pango's paragraph delimiters and
  never joins with or splits around its neighbours (a `\n` typed after a buffer
  `\r` starts a new line rather than forming a CRLF; text typed between `\r` and
  `\n` stays inside that line, which then holds an embedded `\r`);
- a **deletion joins exactly the lines whose starts it removes** (a line start
  `x` goes when `start < x <= end`): deleting the text between `\r` and `\n`
  removes no line, while deleting the `\n` of a CRLF joins two lines into one
  holding an embedded `\r`.

Text inserted whole still has Pango's lines, so the load profile (D1) and the
paragraph-aligned slicer (D2) are unaffected: slices end after a delimiter, so
every slice starts a fresh line. The edit-time watch (D5) and its Kani model
(D9 group 3) are what change:

- The watch measures the **buffer's lines** (what GtkTextView lays out). The core
  splits an inserted run **alone**; `InsertFacts` no longer carries context
  characters, and the line counts and iter lines GTK reports are the facts.
- An insertion without a delimiter never changes the line count, so typing
  between `\r` and `\n` is a fast step that measures the grown line.
- A deletion inside one line never grows a paragraph and does nothing; a
  deletion that removes a line start (deleting the `\n` of a CRLF included)
  joins lines and takes the window path. The design's first design-time finding
  (a line-span delete trigger misses a density rise when the text between `\r`
  and `\n` is deleted) **does not hold in GTK**: with history-dependent lines the
  line-span test and the line-count test agree, so the `should_panic` harness
  `watch_line_removal_without_merge_is_missed_by_a_line_span_test` was dropped
  (it would have no counterexample). The uncarried fast-path finding stands.
- The Kani model plays the buffer with a line-start flag per offset under these
  two rules (D9 group 3), not the reference rule over the whole text.
- The minimap's modified-line marks moved to an `insert-text` **after**-handler
  that reads the inserted run's first and last lines from GTK: the old
  before-handler created marks for lines that did not exist yet, so every
  multi-line insertion (`\n` included) marked only its first line. Task 1.6's
  "delimiter count" is therefore GTK's own line numbers, which A42 makes exact.

### D1. The profile is accumulated inside the existing analysis pass

**What.** A GTK-free accumulator lives in `model/paragraph_boundary.rs` and is fed slice by slice. It is the same shape as `LineEndingTally`, with carry state for a pending CR and the length of the paragraph still open at the slice end, and is named `ParagraphProfileAccumulator`. It takes `&str` slices, as `analyze_decoded_content_cancellable` already cuts them on scalar boundaries, so a U+2029 is never split across slices and needs no carry.

**Paragraph length.** A paragraph's length is its character count **excluding** its delimiter. `gtk_text_iter_get_chars_in_line` includes the delimiter (one character for `\n`, `\r` or U+2029, two for `\r\n`; gtktextiter.c:1709-1757), so the edit-time watch (D5) subtracts it. Thresholds compare this one quantity everywhere, so the load profile and the edit watch agree at `t` and `t+1`. It returns a `DocumentProfile`:

| Field | Meaning |
|---|---|
| `total_bytes` | Size of the document in bytes. |
| `longest_paragraph_chars` | Length of the longest paragraph, in characters. |
| `long_paragraph_counts` | One counter for each threshold in `ProfileThresholds`. |
| `long_paragraph_offsets` | Start offsets of long paragraphs, capped at `MAX_PROFILED_OFFSETS`, with a `truncated` flag. |
| `worst_batch_chars` | Largest character sum of any `R_batch` consecutive paragraphs. |

`analyze_decoded_content_cancellable` already walks every character for the NBSP and zero-width counts, so the accumulator adds only per-character counting and separator checks. It adds no extra pass and keeps the existing cancellation checkpoints.

**Density.** Density is computed with a ring buffer of `R_batch` paragraph lengths, so memory is bounded by `R_batch`. A document with fewer than `R_batch` paragraphs has one window, the sum of all of them.

**Two layers, so the core is checkable (D9).**
- `ParagraphLengthFold` consumes one closed paragraph length at a time and owns everything that depends only on the sequence of lengths: the longest paragraph, the per-threshold counts, the capped offset list with its `truncated` flag, and the `R_batch` ring with `worst_batch_chars`. It never sees text.
- `ParagraphProfileAccumulator` turns `&str` slices into closed lengths with D2's delimiter rule and the pending-CR and open-length carry, and feeds the fold. `finish()` closes the final paragraph (the empty last line after a trailing delimiter included, as GtkTextBuffer has one).

Both take a `ProfileConfig { thresholds: ProfileThresholds, offset_cap, r_batch, v_batch }` value; the fold ignores `v_batch`, which only the edit-time watch (D5) reads for its fast-path test. Production passes `ProfileConfig::PRODUCTION`, built from `document_scale`'s named constants (`V_batch` included) and `MAX_PROFILED_OFFSETS`; a unit test pins that the two agree. The Kani harnesses pass small symbolic configurations, which is what makes counts, the offset cap and density reachable on short strings. The edit-time core (D5) reuses the same fold to compute window sums, so the load profile and the edit watch share one definition of a window.

**Units.** Characters, never pixels, are the unit. `R_batch` is the "rows per 2000 px, wrap off" figure that C0 uses to derive `N_dense` from `V_batch` (ledger A31), taken at the smallest supported line height. Pixels are therefore never handled in the policy, the whole-pixel geometry rule does not apply, and both new modules still carry `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]`.

**Other installers.** Draft recovery, local-history restore and undo, and save-formatting mirror-back run the same accumulator on the worker that already owns their body, as part of that worker's existing traversal where one exists. An untitled tab starts from `DocumentProfile::EMPTY`.

**Alternatives.**
- A second worker pass: rejected, because it doubles the decode-side cost on 500 MB files.
- Profiling on GTK after install: rejected, because it is the stall this change removes.

### D2. One GTK-matching paragraph-boundary function

`model/paragraph_boundary.rs` owns the boundary function, the delimiter set and the accumulator. Nothing else is in the module.

**Delimiters.** The function recognises `\n`, `\r`, `\r\n` (one delimiter) and U+2029. It does **not** recognise U+2028 or U+0085, because `pango_find_paragraph_boundary` does not.

**API.**
- `next_paragraph_break(text, from) -> Option<ParagraphBreak { content_end, next_start }>`.
- `slice_end_after_paragraph(text, start, budget) -> usize`, which replaces the body of `next_replacement_boundary`. Its choice is the current one with GTK's delimiters: let `b` be `start + budget` floored to a scalar boundary and capped at the text end; if `b` is the text end, the text end; otherwise the end of the last paragraph break that ends at or before `b`, if one ends after `start`; otherwise the end of the first break after `start`; otherwise the text end. That function stays as the `WFR-BUFFER-REPLACEMENT` named entry point, and `next_install_boundary` stays its synonym.

**Guarantees.**
- A slice never ends between `\r` and `\n`.
- A slice never ends inside a UTF-8 scalar.
- A slice always advances.
- A slice ends at the text end or just after a delimiter.
- A slice is the choice stated in the API above, so a multi-paragraph remainder is never installed in one turn.

**Verification.**
- A property test compares paragraph splits against `pango::find_paragraph_boundary` (pango 0.22 `auto/functions.rs`) over generated strings drawn from a delimiter-heavy alphabet without NUL. The `pango` crate is already a `lushtext-core` dependency, and the call needs no GTK init. The test lives with the other property tests in `crates/lushtext-core/tests/properties/`, not in `model/`, so `model/` keeps no toolkit dependency even under `cfg(test)`.
- Kani proves the five guarantees above, and agreement with the delimiter rule, for every valid UTF-8 string of at most 8 bytes (the harness assumes `std::str::from_utf8` succeeds on its symbolic bytes). The harnesses, the bound and why it is enough are in D9.
- A pure helper `paragraph_chars_from_line(chars_in_line, ending) -> u64` turns `gtk_text_iter_get_chars_in_line` into a paragraph length (minus 1 for `\n`, `\r` or U+2029, minus 2 for `\r\n`, minus 0 for the last line), so D5's GTK adapter does no arithmetic of its own; it is in the D9 harness set.
- Probe: a ledger axiom (see D8) pins that GtkTextBuffer's line count agrees for each delimiter.

**Other newline-only sites.** The minimap's modified-line-mark handler (`minimap/watch.rs`) counts inserted lines with `'\n'`. It is moved to a delimiter count from this module in the same change; under the pre-existing-blockers rule it is not left behind.

**Alternatives.** Calling `pango::find_paragraph_boundary` directly from the slicer was rejected. It is FFI, it is not Kani-checkable, and it returns byte indices over a C string, which would need copies for each slice.

### D3. `DocumentScale` is one monotone pure policy that replaces `FileSizeCheck`

`model/document_scale.rs`:

```text
DocumentProfile { total_bytes: u64, longest_paragraph_chars: u64, worst_batch_chars: u64 }  // policy view; extra profile fields are evidence only
DocumentScale::for_profile(p) -> DocumentScale
DocumentScale fields (all derived, Copy):
  open_allowed, undo_enabled, estimated_buffer_multiplier: u64,
  local_history: LocalHistoryAvailability (Full > SaveOnly > Unavailable),
  syntax_enabled, bracket_matching_enabled, minimap_allowed,
  invisibles_allowed, search_highlight_allowed, protected_view,
  reduced: ReducedFeatures (bitset + reason per feature: TotalSize | LongParagraph | Density)
DocumentScale::join(self, other) -> DocumentScale   // presentation features: feature-wise minimum, protected_view OR;
                                                     // byte tiers (open, undo, multiplier, local_history): taken from `other`
```

**Two kinds of output.** The **byte tiers** (`open_allowed`, `undo_enabled`, `estimated_buffer_multiplier`, `local_history`) depend on `total_bytes` alone and keep today's semantics: they describe the latest loaded or saved bytes, so a document saved below 10 MB regains full local history, exactly as the `local-history` requirement ("for files above 10 MB…") and today's save path already do. The **presentation features** (`syntax_enabled`, `bracket_matching_enabled`, `minimap_allowed`, `invisibles_allowed`, `search_highlight_allowed`, `protected_view`) are the ones the session floor holds monotone (D4). `join` is a meet on the presentation features only.

**Which threshold gates what.** Comparisons are strict (`>`), as in `FileSizeCheck`:
- `syntax_enabled` and `bracket_matching_enabled`: off when `total_bytes > S_syntax` (reason TotalSize) or `longest_paragraph_chars > L_syntax` (reason LongParagraph);
- `minimap_allowed`: off when `total_bytes > S_minimap` or `longest_paragraph_chars > L_minimap`. `S_minimap` keeps today's 10 MB value (the minimap is currently gated by the syntax tier), so lowering `S_syntax` after C0 does not silently turn the minimap off for 1–10 MB short-line files;
- `invisibles_allowed`: off when `longest_paragraph_chars > L_invisibles`;
- `search_highlight_allowed`: off when `longest_paragraph_chars > L_search_highlight`;
- `protected_view`: on when `longest_paragraph_chars > L_protect` (reason LongParagraph) or `worst_batch_chars > V_batch` (reason Density). This is the entry rule `long-line-protected-view` (C2) consumes verbatim. Density gates nothing else in C1.

`ReducedFeatures` also lists undo and local history when the byte tiers reduce them (reason TotalSize), so the notice and the accessible description keep naming them (D7).

**Thresholds.** Each threshold is a named `pub const` whose doc comment cites the C0 measurement record in `docs/next/long-line-documents.md`. C0 derives `L_syntax`, `L_minimap`, `L_invisibles`, `L_search_highlight`, `L_protect`, `N_visible`, `V_batch`, the syntax total-size cutoff (`S_syntax`) and the rows-per-batch figure (`R_batch`) (C0 design D7, derivation table). If C0's record leaves `L_minimap`, `L_invisibles` or `L_search_highlight` open, that limit defaults to `min(L_syntax, N_visible)` — the smaller of the two C0-derived paragraph limits — and the doc comment says so (Open Question 1). The budgets B_interact, B_settle and the input-to-frame limit are not restated here: code and tests read them from C0's responsiveness-constants module, as C0's `responsiveness-measurement` capability requires.

| Constant | Gates |
|---|---|
| `S_syntax` | syntax, by total size |
| `L_syntax` | syntax, by paragraph length |
| `S_minimap` | minimap, by total size (today's 10 MB, unchanged) |
| `L_minimap` | minimap |
| `L_invisibles` | invisibles |
| `L_search_highlight` | search highlighting |
| `L_protect` | Protected View |
| `V_batch` | density |
| `R_batch` | rows per density window |

These keep their values today: `UNDO_TOTAL_BYTES` (50 MB), `HISTORY_SAVE_ONLY_BYTES` (10 MB), `HISTORY_UNAVAILABLE_BYTES` (50 MB), `S_minimap` (10 MB), `REFUSE_TO_OPEN` (500 MB). The 1 MB `LARGE_FILE_TOAST` tier is retired with its only consumer, the "Large document mode is active." accessible sentence (Open Question 7).

**Kani.** The proofs range over every `u64` triple:
1. **Monotone.** If `p ≤ q` componentwise, every boolean enable of `for_profile(q)` implies the same enable of `for_profile(p)`, `local_history(q) ≤ local_history(p)`, `protected_view(p) ⇒ protected_view(q)`, and the multiplier is non-increasing.
2. **Join.** On the presentation features `join` is the meet: commutative, associative and idempotent, and `join(a, for_profile(p))` never enables a presentation feature that `a` disables. Its byte tiers equal those of its second argument.
3. **Tier agreement.** `open_allowed`, `undo_enabled`, `local_history` and the multiplier agree with the retired `FileSizeCheck` tiers for every `total_bytes`. Because `file_limits.rs` is deleted, the harness and the parity unit tests compare against a private frozen reference (`retired_byte_tiers`, a transcription of `FileSizeCheck::classify`, its queries and `local_history_service::availability_for_size_check`, kept under `cfg(any(test, kani))`). This pins the "unchanged values" claim.
4. **Upward-closed protected view, and the profile join.** `protected_view` is upward-closed on its own harness. For every pair of profiles, the presentation features of `for_profile(p ⊔ q)` (componentwise maximum) equal those of `join(for_profile(p), for_profile(q))`. This is what lets the edit watch (D5) keep a high-water profile instead of a list of scales: the session floor of every recomputed profile is the scale of their maximum.
5. **Save without the join is pinned.** A `should_panic` harness assigns `for_total_bytes(saved)` in place of `join` and finds a presentation feature re-enabled (today's rename-after-save bug).

The harnesses go in `model/document_scale/kani_proofs.rs`; D9 lists them with their shard and oracle entries.

**Unit tests.** They cover each threshold at `t` and `t+1`.

**Alternatives.**
- Keep `FileSizeCheck` and add a parallel paragraph check: rejected, because two policies drift. The 17 consumers would keep asking a byte-only question, and the save path's re-enable bug would survive.
- A lattice of enum tiers: rejected, because features become independent once paragraph length and total size are separate axes.

### D4. Every consumer reads `DocumentScale`; the editor holds a session floor

`EditorPage` replaces `size_check: Cell<FileSizeCheck>` with these fields:
- `document_scale: Cell<DocumentScale>`, the effective policy;
- `scale_floor`, the session meet of the presentation features;
- `profile_state`, the live `ProfileWatchState` of D5: the high-water longest paragraph and batch density, plus the carried density bound.

**Who writes the scale.**
- **Load publish** (open, eviction reload, external reload) sets `document_scale = for_profile(loaded_profile)`. A load is a new document session; see Open Question 3 for eviction reload.
- **Save accept** sets `document_scale = document_scale.join(for_total_bytes(saved_bytes))`. The presentation features can never come back on, which fixes the current re-enable on save. The byte tiers follow the saved byte count, exactly as `save/execution.rs:467-470` does today, so the `local-history` and `live-editor-memory-budget` behaviour is unchanged.
- **Buffer-replacement terminals** (draft recovery, history restore and undo, formatting mirror-back) join the worker profile of the installed body.

**Readers.**
- `undo_enabled`: `load/execution.rs`.
- `syntax_enabled`: `document_identity.rs::republish_document_identity` and `reapply_language`.
- The memory policy: `estimated_buffer_multiplier`.
- `local_history_service::availability_for_scale`, called by `ui/editor_page/local_history.rs::local_history_availability` and `ui/window/local_history/journal.rs`.
- `open_allowed`: peek, the bookmark excerpt, and the editor's accessible `disabled` state and "too large to open" sentence (`accessibility.rs:50`, `:118`).
- `plain_disposal.rs`: `REFUSE_TO_OPEN`, re-exported from `document_scale`.
- `minimap_allowed`: `minimap/admission.rs` (cheap availability and the readiness block at `:57`), through a renamed `MinimapAvailabilityPolicy::scale_allows_minimap` field in `minimap/policy.rs`.
- The lossy-encoding pre-check in `ui/window/encoding/execution.rs:99` defers to save whenever `syntax_enabled` is off, which keeps today's proxy and now also covers long-paragraph documents. Deferral is safe: the save path still blocks a lossy write and asks first.

**Services that only know bytes.** Peek, bookmark excerpts, draft availability and the local-history journal call `DocumentScale::for_total_bytes(bytes)`, an explicit total-size-only constructor documented as such. They gate only open admission and history tiers, which depend on bytes alone.

### D5. Edit-time watch: synchronous, bounded and monotone

**Where it lives.** `ui/editor_page/document_scale.rs` is cross-cutting coordination with no role. It is declared in the new `WFR-DOCUMENT-SCALE` row (status `cross-cutting`). It is not a workflow with its own stage order: load, save and buffer replacement call it, and the buffer signals drive it.

**Pure core.** Every decision the handlers make lives in `model/paragraph_profile_watch.rs`, GTK-free and I/O-free, with `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]`. The GTK file only reads buffer facts, passes them in, and applies the returned downgrade. The core owns:
- `ProfileWatchState { longest_paragraph_chars, worst_batch_chars, density_bound }`. The first two are the policy view the scale reads. `density_bound` is an upper bound on the current document's worst window, carried across fast-path steps (see below). Per-threshold counts and the offset list are load-time evidence only; the watch does not maintain them.
- A two-phase step per edit: `plan_insert(state, InsertFacts) -> WatchPlan` and `plan_delete(state, DeleteFacts) -> WatchPlan`, where `WatchPlan` is either `FastPath { touched_line }` or `ReadWindow { first_line, last_line }`, followed by `complete(state, plan, line_lengths) -> ProfileWatchState`. The plan names every line the adapter must read, so the read bound is a property of the core, not of the adapter.
- Window sums through D1's `ParagraphLengthFold`, so the load profile and the watch share one window definition.

`InsertFacts` holds the run text, the line count before and after the emission (`gtk_text_buffer_get_line_count`, O(1) from the B-tree root), the lines of the run's start and end iters, and the one character before and after the run (`Cr`, `Lf`, `Other` or none). `DeleteFacts` holds the line counts before and after and the line of the deletion point.

**Handlers.** It installs three buffer handlers in `constructed()`, tracked in the editor's `SignalBag`:
- `connect_after("insert-text")`. The location iter is at the end of the inserted run.
  1. The core splits the run with D2's function, extended by the one context character on each side, so it sees exactly the delimiters GtkTextBuffer sees: a run that starts with `\n` after a buffer `\r` joins that CRLF, and a run inserted between `\r` and `\n` splits it. Lines wholly inside the run are measured from the run text. The first and last touched lines are read with `iter.chars_in_line()` through `paragraph_chars_from_line` (D2), which walks segments, not characters (a ledger axiom, D8).
  2. Update `longest_paragraph_chars`.
  3. Recompute the worst density window over the `R_batch` lines on each side of the touched range: at most `2·R_batch + 2` buffer line-length reads, because the inner touched lines come from the run text.
- `connect("delete-range")` (before) records only the line count. `connect_after("delete-range")` then acts:
  - line count unchanged: no paragraph was removed, so every paragraph only shrank or kept its length (deleting the `\n` of a CRLF leaves a lone `\r` delimiter and the same lines), and with a monotone floor there is nothing to update;
  - line count decreased: it reads the line at the deletion point and runs the window update. A merge can raise the longest paragraph, and **any** removed line can raise the density window, because later lines shift into it. Deleting the text between a `\r` and an `\n` removes a line and merges no paragraph, yet makes its neighbours adjacent.

  The earlier draft of this design tested `start.line() != end.line()`. That misses the CR/LF case above (both iters are on the removed line), so it was replaced while the D9 model was written.

**Fast path.** When the line count is unchanged by the insertion and `density_bound + inserted_chars ≤ V_batch`, the window recompute is skipped: the core reads only the touched line for `longest_paragraph_chars` and sets `density_bound += inserted_chars`. A run without delimiters is not enough on its own, because inserting text between `\r` and `\n` adds a line. Otherwise the slow path reads the window and sets `worst_batch_chars = max(worst_batch_chars, W)` and `density_bound = max(density_bound, W)`, where `W` is the largest recomputed window. The bound never under-estimates: windows away from the touched lines are unchanged, and windows through them grow by at most the inserted characters. `density_bound` never decreases within a document session (a deletion cannot lower it soundly without a full scan), so once `density_bound + inserted_chars` would exceed `V_batch`, every later insertion takes the window path. That path is still bounded by `2·R_batch + 2` O(segments) reads; the fast path is an optimisation for documents whose carried bound stays below `V_batch`, not a promise for every keystroke of a long session.

The earlier draft used "the profile's `worst_batch_chars` plus the inserted characters" as the bound without carrying it forward. That is unsound across keystrokes: with `V_batch = 10` and a worst window of 5, two 3-character insertions each pass the check (5 + 3 ≤ 10) while the real window reaches 11. Carrying `density_bound` fixes it; D9's harness and its `should_panic` twin pin both forms.

**Downgrade in the same emission.** After the update the handler computes `new = scale.join(for_profile(profile))`. If any feature turns off, it applies the downgrade before returning:
- `set_highlight_syntax(false)` and `set_language(None)`;
- `set_highlight_matching_brackets(false)`;
- invisibles off;
- search-bar highlighting off for this buffer;
- the minimap instance retired synchronously (D6), before the map's own validation idle can lay out the new paragraph;
- the notice updated (D7).

The GTK validation and highlight idles run later in the main loop (gtktextview.c:5123-5131). GtkSourceBuffer's class handler only invalidates a region and schedules the context engine's first-update idle (gtksourcebuffer.c:1092-1137, gtksourcecontextengine.c:1937-1961 and 2399-2414); bracket matching is queued on a timeout (gtksourcebuffer.c:1007-1058). So syntax analysis of the giant paragraph never starts. Clearing the language removes tags but changes no text (ledger A27). A ledger axiom pins the same-emission ordering (D8); A29 is the context-engine behaviour it avoids.

**Suspended while installing.** The handlers return immediately while `load_projection_suspended()` or a buffer-replacement session is active. Those installers deliver the profile from their worker (D1). Without this, slice installs would re-profile the document on GTK.

**Undo.** Undo is not toggled at edit time. Undo depends on total bytes, and turning a live GtkSourceBuffer's undo off mid-session is a separate data-safety decision; see Open Question 4. The policy still computes it, and the next save joins it.

**Alternatives.**
- An idle re-evaluation: rejected, because the highlight idle could run first. The reviewers flagged exactly this race.
- `connect("insert-text")` without `after`: rejected, because the iter is not yet revalidated and the text is not yet in the buffer.

### D6. The minimap decides before attach and never clears a live map's view

**Availability.**
- `cheap_minimap_availability` reads `document_scale().minimap_allowed` in place of `size_check().syntax_enabled()`.
- `current_availability` no longer waits for `wrapped_layout_too_large`.
- The wrapped-layout analysis stays, admitted by the existing conservative live-bytes estimate, but it now feeds **markers only**.
- The `TooLarge` availability variant is renamed `Reduced`, and the existing unavailable feedback says "for this document's line lengths" or "size", as appropriate.

**Before attach.** The worker profile is known before installation, so every install path — the direct path included, which today leaves the map attached during `set_text` — retires an existing map before the first byte enters the buffer whenever the incoming profile disallows the minimap, and no later refresh re-attaches one while the scale disallows it. The map `install_minimap` creates at construction is attached only to the new editor's empty buffer, so it never sees content the policy has not admitted.

**Detach.** The invariant is: *a `GtkSourceMap` whose update tick may be armed never has its `view` set to NULL.* The chosen mechanism is **retire and rebuild**:
1. To detach, hide the overlay, then unparent the `GtkSourceMap` and drop LushText's reference. Unparenting unrealizes the widget, which disconnects it from the frame clock, so an armed tick never fires; dispose later clears the view on a widget that can no longer tick. This toolkit behaviour is a ledger axiom (D8).
2. To re-attach, a new `attach_native_source_map()` factors the map half out of `install_minimap`: `create_native_source_map()` (today called only from `install_minimap`) builds the map with its view set, the overlay takes it as child, the existing `RenderHoldOverlay::set_live_child` re-points the reflow freeze at it, and the vadjustment guard handler is reconnected and tracked so the retired map's handler is released with it.

**Rejected alternatives.**
- "Hide, then clear the view after the next frame": a value change in the same frame can re-arm the tick after GtkSourceMap's tick has run, and LushText cannot read `update_id`.
- Keeping the view attached while hidden: a hidden but realized map still lays out every paragraph.

**Failing first.** The fix lands after a widget test reproduces the CRITICAL under the harness's critical-as-failure handling. The test scrolls to arm the tick, then runs the current detach path, and must fail before the fix. C0's defect axiom A32 (`set_view(NULL)` with a pending tick) pins the toolkit side.

**Upstream.** An upstream GtkSourceView report is filed. The remedy does not wait for it.

### D9. Kani harnesses

Every harness follows `formal-verification-kani` and the "Formal Verification (Kani)" section of `.agents/rules/build.md`: a child `kani_proofs.rs` declared `#[cfg(kani)] mod kani_proofs;` by the checked module, over the production pure core and never a copy; `kani::cover!` on each interesting branch; the restricted domain stated in the harness and in the checked function's rustdoc (and in the spec scenarios); every harness in exactly one shard and in the oracle table. The three modules are all under `model/`, so no `ui/**/policy.rs` mutation-scope file gains `cfg(kani)` code.

**Production cores checked.**

| Core | Module | Harness file |
|---|---|---|
| `next_paragraph_break`, `slice_end_after_paragraph`, `paragraph_chars_from_line` | `model/paragraph_boundary.rs` | `model/paragraph_boundary/kani_proofs.rs` |
| `ParagraphProfileAccumulator` and `ParagraphLengthFold` | `model/paragraph_boundary.rs` | same file, `profile_` harnesses |
| `ProfileWatchState`, `plan_insert`, `plan_delete`, `complete` | `model/paragraph_profile_watch.rs` | `model/paragraph_profile_watch/kani_proofs.rs` |
| `DocumentScale::for_profile`, `for_total_bytes`, `join` | `model/document_scale.rs` | `model/document_scale/kani_proofs.rs` |

**Refactors that make the cores checkable.** The accumulator and the watch take a `ProfileConfig` value instead of reading constants (D1); the accumulator is split into the text layer and the length fold (D1); the watch's decisions move out of `ui/editor_page/document_scale.rs` into `model/paragraph_profile_watch.rs` with a plan/complete API whose facts are the GTK reads (D5); the line-length subtraction moves into `paragraph_chars_from_line` (D2). None changes behaviour; each is a precondition for harnesses over production code.

**Reference rule.** The boundary and profile harnesses compare against a specification written in the harness file as a per-position predicate: position `i` ends a paragraph at `\n`, at U+2029, or at `\r` not followed by `\n`; `\r\n` is one delimiter ending after `\n`; U+2028 and U+0085 never end one. It is deliberately not a scan like the production function, so it is a specification rather than a copy. Agreement with Pango itself stays with the property test (D2), which samples far longer strings.

**Harnesses.**

1. *Boundary* (`boundary_` prefix), over symbolic bytes of length ≤ 8 with `from_utf8` assumed to succeed, a symbolic start on a scalar boundary and a symbolic budget:
   - `boundary_break_is_a_scalar_boundary_after_a_delimiter`: `next_paragraph_break` returns scalar boundaries, `content_end < next_start`, and the bytes between them are exactly one delimiter;
   - `boundary_slice_end_is_safe_and_advances`: the first four D2 guarantees for `slice_end_after_paragraph`;
   - `boundary_slice_end_is_the_reference_choice`: `slice_end_after_paragraph` returns the D2 choice computed from the reference rule's breaks. The four safety guarantees alone admit `return text.len()`, which is exactly the one-turn install this change removes, so the choice needs its own harness;
   - `boundary_agrees_with_the_reference_rule`: the sequence of breaks equals the reference rule's, including CR-only, U+2029, CRLF and the non-delimiters U+2028 and U+0085;
   - `boundary_line_length_excludes_its_delimiter`: `paragraph_chars_from_line` equals the reference paragraph length for every ending and never underflows;
   - `boundary_newline_only_slicer_disagrees` (`should_panic`): a transcription of the retired `'\n'`-only `next_replacement_boundary` body, kept in the harness file with its source revision and with its `REPLACEMENT_INSERT_SLICE_BYTES` constant replaced by the harness's symbolic budget, violates the reference choice that `boundary_slice_end_is_the_reference_choice` asserts (Kani finds a CR-only or U+2029 text whose retired slice is the text end). With the 256 KiB constant kept, every string of at most 8 bytes would fit the budget and the retired body would agree vacuously. The retired body satisfies the four safety guarantees, so this harness pins the blocker through the choice property, not through them.

   `kani::cover!` marks a break at each delimiter kind, a budget ending between `\r` and `\n`, a budget ending inside a multibyte scalar, a text with no delimiter, a slice ending at the last break within the budget, and a slice ending at the first break after it.

   Every group-1 harness carries `#[kani::unwind(10)]` (at most 8 bytes, plus the exit iteration and one of lookahead), and Kani's unwinding assertions stay on, so a bound too small fails the harness instead of truncating the proof.

2. *Slice independence* (`profile_` prefix), over strings of ≤ 6 bytes (fitted at apply time from the 8 designed: (the design's 8 bytes did not fit: a `String`-built text and growing `Vec`s exhausted the 16 GiB lane cap, and with fixed buffers 6 bytes peak at 7 GiB, so 8 would break the shard's 12 GiB budget)) drawn from the alphabet {`a`, `é`, `\r`, `\n`, U+2028, U+2029, U+0085} (symbol indices mapped to their UTF-8, which avoids a symbolic `from_utf8` on this larger harness), cut into at most 3 slices at any scalar boundaries, empty slices included, with a symbolic `ProfileConfig` whose thresholds are ≤ 8, `offset_cap ≤ 2` and `1 ≤ r_batch ≤ 3`:
   - `profile_is_independent_of_slicing`: the profile fed slice by slice equals the whole-text profile field by field: `total_bytes`, `longest_paragraph_chars`, every per-threshold count, the offset list and `truncated`, and `worst_batch_chars`. The carry at a slice end (pending CR, open length, the ring, the counts and offsets) is the slice's summary, and continuing from it is the exact merge;
   - `profile_matches_the_specification`: the whole-text profile equals a profile built from the reference rule's paragraph lengths: the longest, each count, the first `offset_cap` long-paragraph offsets with `truncated` set exactly when more exist, and the maximum window of `min(r_batch, n)` paragraphs;
   - `profile_slices_without_carry_disagree` (`should_panic`): profiling each slice with a fresh accumulator and combining componentwise (maximum of longest and worst, sum of counts) is wrong; Kani finds a CRLF pair or a paragraph split across a cut.

   `kani::cover!` marks a CRLF split across a cut, a cut immediately before and after U+2029, an empty slice, offset truncation, and a document shorter than `r_batch`.

   The group-2 harnesses carry `#[kani::unwind(10)]` (at most 8 symbols or bytes per loop; the ring and threshold loops are shorter), with unwinding assertions on.

3. *Edit-time watch* (`watch_` prefix), in the style of the editor-memory `ResidencyTotals` ledger proof. The harness holds a model document of symbols {`a`, `\r`, `\n`, U+2029}, ≤ 4 symbols at the start, and applies up to 3 edits, each an insertion of ≤ 2 symbols at any offset or a deletion of any range of ≤ 2 symbols, with `1 ≤ r_batch ≤ 2` and `V_batch ≤ 8`. It plays GtkTextBuffer: it computes line counts, iter lines, context characters and `chars_in_line` from the model with the reference rule (trusted under the D8 paragraph-delimiter axiom, as the editor-memory proof trusts its `BTreeMap`), feeds them to the production `plan_*` and `complete`, and after every edit recomputes the profile of the whole model from scratch: the reference rule's paragraph lengths fed to the production `ParagraphLengthFold`. It does not run the production text accumulator over the model text. That would be no stronger, because `profile_matches_the_specification` already ties the accumulator to exactly this fold over the reference lengths, and it would be far costlier: the model reaches 10 symbols and 30 bytes, beyond group 2's 8-byte domain, and every per-byte loop would need an unwind of 31 on each of four recomputations. The production D2 splitter still runs inside the watch core, on the inserted run and its two context characters only (at most 4 symbols). It proves:
   - `watch_longest_paragraph_matches_recomputation`: `longest_paragraph_chars` equals the maximum over the start document and every intermediate document of the recomputed longest paragraph, exactly;
   - `watch_density_matches_recomputation`: `density_bound` is never below the current document's recomputed worst window; `worst_batch_chars` is never above the high-water of recomputed worst windows and equals it whenever no step took the fast path (a deletion that leaves the line count unchanged does nothing and only shrinks windows, so it does not break the equality); and `worst_batch_chars > V_batch` exactly when that high-water exceeds `V_batch`, so the decision the scale reads is exact;
   - `watch_reads_are_bounded`: every plan names at most `2·r_batch + 2` lines, and a fast-path plan names one.

   With D3 harness 4, these give the scale-level statement: the effective presentation features after any edit sequence equal the session meet of `for_profile` over every recomputed profile. Two `should_panic` harnesses pin the corrections D5 records:
   - `watch_line_removal_without_merge_is_missed_by_a_line_span_test`: deciding the slow path by `start.line() != end.line()` misses the density rise when the text between `\r` and `\n` is deleted;
   - `watch_fast_path_without_a_carried_bound_is_unsound`: resetting `density_bound` to `worst_batch_chars` before each step lets two fast-path steps cross `V_batch` unseen.

   Both drive the production core, not a copy of it. The retired decision is expressed through its inputs: the first reports the line count after a deletion as unchanged whenever the deleted range's start and end lines are equal (so the production `plan_delete` takes the slow path exactly when the retired line-span test did); the second assigns `density_bound = worst_batch_chars` before each step (the harness is a child module and reaches the private field), which turns the production fast-path test into the draft's `worst_batch_chars + inserted ≤ V_batch`. Each asserts the exact-decision property of `watch_density_matches_recomputation` and is expected to fail it.

   The group-3 harnesses carry `#[kani::unwind(12)]` (at most 10 model symbols, plus the exit iteration and one context character), with unwinding assertions on. The model document is a fixed-capacity array of symbol indices with a symbolic length, never a `String`, so model loops are per symbol. This is the costliest group; it is expected to take minutes per harness, which is why D9 gives it the fit order's first split.

   **Apply-time revision (group 3).** The bounded replay did not fit: one replayed edit from a 4-symbol start already took 14 minutes and 9 GiB. Following the fit order (split, solver, then bounds), the group was rewritten as an **inductive step**: `watch_step_preserves_the_invariant` takes any buffer of at most 2 symbols with any line structure the A42 rule can reach (and more), any watch state and high-waters (at most 16) satisfying the invariant the two `watch_*_matches_recomputation` harnesses stated, and one edit (insert ≤ 2 symbols or delete ≤ 2), and checks the invariant again; `watch_reads_are_bounded` checks the read bound for the same step, and `watch_fast_path_without_a_carried_bound_is_unsound` is unchanged in intent. The invariant holds after a load, so the proof covers every edit sequence, at the cost of a smaller per-edit buffer. `watch_line_removal_without_merge_is_missed_by_a_line_span_test` was dropped: with the buffer's history-dependent lines (A42) the line-span and line-count tests agree, so it has no counterexample (see D5's note). Harnesses use `unwind(8)` and CaDiCaL. Deleting the text between `\r` and `\n` needs three symbols, so its `kani::cover!` was unsatisfiable in the 2-symbol step; `watch_delete_between_cr_and_lf_preserves_the_invariant` proves the step, the read bound included, for that one deletion over the buffer `\r`, any symbol, `\n` with any line structure after its delimiters and any state satisfying the invariant (388 s locally). The general harnesses' covers live in a helper the fixed-shape harness does not call, so each harness's covers are all satisfiable. The 38 local minutes of the four watch harnesses together broke the 25-minute shard margin, so each general watch harness has its own shard (`core-profile-watch-step`, `-reads`, `-unsound`) and the deletion harness joins `core-paragraph-profile`.

   `kani::cover!` marks: a fast-path step, a window-path step, two consecutive fast-path steps, an insertion joining a CRLF, an insertion splitting a CRLF, deleting the `\n` of a CRLF, deleting the text between `\r` and `\n`, a merge raising only the density window, and a density crossing.

4. *Document scale* (`scale_` prefix), over every `u64` component, unbounded: `scale_is_monotone`, `scale_protected_view_is_upward_closed`, `scale_join_is_a_meet_on_presentation_features`, `scale_of_a_profile_join_is_the_join_of_scales`, `scale_byte_tiers_match_the_retired_tiers`, and the `should_panic` `scale_save_without_join_reenables_a_feature` (D3). `kani::cover!` marks each feature on and off, each reason class, and each local-history tier. The scale code has no loops, so these harnesses need no unwind bound and are expected to verify in seconds.

**Why the bounds are enough.**
- *Boundary.* The function's decision at any position depends on at most one scalar of lookahead (`\r` then `\n`), and a scalar is at most 4 bytes. The slice choice adds only the comparison of a break's end with the budget, and a symbolic budget over 8 bytes places that edge before, inside and after every break and scalar. Eight bytes hold every ordered pair of delimiters, a 4-byte scalar straddling a budget edge, and a CRLF beside a U+2029 (`a\r\n` + U+2029 + `a` is 7 bytes). A longer string only repeats these local situations, which the property test then samples against Pango.
- *Slice independence.* The carry state is finite in kind: a pending-CR bit, one open length, a ring of at most `r_batch` lengths, and counts and offsets. Three slices give two cuts, which is enough for a cut inside a CRLF and a second cut before or after U+2029 in the same string. Symbolic thresholds ≤ 8 reach `t` and `t+1` for every length the strings can hold, and `offset_cap ≤ 2` with threshold 0 reaches truncation with three one-character paragraphs (5 bytes). The production configuration differs only in constant values, which the fold compares with the same code; the property test runs it on long strings.
- *Watch.* Two edits already exhibit the unsound fast path (two fast steps); the third lets a fast step be followed by a merge or a CRLF split, and with ≤ 2 symbols per edit and `r_batch ≤ 2` the model reaches every interaction listed under `kani::cover!`, each of which needs at most three lines on each side. Paragraph lengths are abstracted by the fold's arithmetic, so short paragraphs exercise the same comparisons as long ones once `V_batch` is symbolic.
- *Scale.* Unbounded over `u64`; no restriction to state.

Nothing here claims an unbounded result for the text-level harnesses, so the "unbounded claims are attempted in Kani first" requirement does not apply. If a later change needs one, a compositional argument over the fold is the first attempt.

**Shards and oracle.** Two new `lushtext-core` shards in `scripts/kani-shards.py`, both gated `scheduled` (a shard may move to `pull-request` later only with a recorded runner wall time of at most 15 minutes):

| Shard | Harness prefixes |
|---|---|
| `core-document-scale` | `model::document_scale::kani_proofs::`, `model::paragraph_boundary::kani_proofs::boundary_` |
| `core-paragraph-profile` | `model::paragraph_boundary::kani_proofs::profile_`, `model::paragraph_profile_watch::kani_proofs::` |

Each records `ci_minutes`, `ci_peak_gib` and `measured_in` from two dispatched `kani.yml` runs (the larger of each), within 25 minutes and 12 GiB. If either breaks a margin, the fit order applies: split the shard (the watch harnesses first), then try another solver if the verdict is unchanged, and only then reduce a bound, recording the weakened statement in the harness doc and the programme record.

`ORACLES` gains `model/paragraph_boundary.rs`, `model/document_scale.rs` and `model/paragraph_profile_watch.rs`, each with its harnesses cheapest first, measured local seconds from `proof-strength.py baseline`, and a `ci` or `local` tier. The `should_panic` harnesses that drive production code (`profile_slices_without_carry_disagree`, both `watch_` ones, `scale_save_without_join_reenables_a_feature`) are oracle entries like `skipping_the_temp_sync_tears_the_destination`: a mutant that stops them panicking is killed. `boundary_newline_only_slicer_disagrees` calls no production function (it checks a transcription against the reference rule), so it goes in `NOT_ORACLE_HARNESSES` with the reason "pins the retired '\n'-only slicer, a transcription inside the harness file, not a property of the shipped boundary function", and the harness file imports nothing from `model/buffer_replacement.rs`. The watch harness imports `paragraph_boundary`, which is itself an oracle module. `make proof-strength PROOF_STRENGTH_MODULE=<module>` is run for each of the three modules, and the mutant counts, kills under each arm, source revision and tool versions are recorded in `docs/next/formal-verification.md` with every Kani survivor triaged into one class. The frozen references (`retired_byte_tiers`, the transcribed newline-only slicer) are not production behaviour: a mutant inside `retired_byte_tiers` is triaged as an equivalent mutant of the module's behaviour, or the reference is excluded in `.cargo/mutants.toml` with that reason.

**Programme record.** `docs/next/formal-verification.md` gains a section for this change: the four harness groups, their domains, the two design-time findings from writing the watch model (the line-span delete trigger and the uncarried fast-path bound, both corrected in D5 before implementation), the shard budgets, the proof-strength results, and the triage of any counterexample the lane finds.

### D7. Reduced-features notice

**The surfaces (revised 2026-10-30).** The notice was first an inline alert above the editor. The interleaved 10.4 lane showed that every show and hide of it reallocated the text view, and GTK then laid out each visible giant paragraph again: click 317 ms and typing at the document start 765 ms on the 1.9M-character fixture, against 3.6 and 6.9 ms on main, and 3.5 and 8.2 ms with the notice disabled. It is now reported outside the text view's allocation path:
- one announced status-bar message, warning style, naming the reduced features and the reason class, bounded with `a11y::bounded_announcement_text`, and published when the tab is the window's selected one (a background tab's notice waits for its selection);
- a persistent "Editor Features" row in the document properties with the same text, since the status message expires;
- the editor's accessible description, as before.

It never takes the inline alert slot, so recovery and error alerts keep it without any priority rule.

**When it is announced.**
- Once per document session.
- Again only when an edit-time downgrade reduces an *additional* feature.

**What it lists.** The reduced presentation features, plus undo and local history when the byte tiers reduce them (reason total size), so the undo statement today's description makes for documents above 50 MB is not lost. Only a reduced presentation feature raises the notice; undo and history reductions are named in it, never raise it on their own.

**Accessible description.** The editor's description (`accessibility.rs`) replaces the old size-tier sentences with the same reduced-features list. Two statements are not size tiers of features and stay: the `disabled` state and the "This document is too large to open." sentence, now driven by `!open_allowed`. The "Large document mode is active." sentence for the retired 1 MB tier is removed (Open Question 7). Accessibility matrix row `A11Y-EDITOR-LARGE-READONLY`, which cites `services/file_limits`, is updated.

**Docs.** `docs/accessibility.md` and the matrix rows are updated, and `docs/accessibility-orca-checklist.md` gains the notice and the reduced-features description if its manual expectations change.

### D8. Toolkit axioms this change relies on

C1 cites C0's entries A27 (tag removal changes no text), A29 (the context engine's atomic line analysis), A31 (2000 px validation batches, the source of `R_batch`) and A32 (the `set_view(NULL)` defect), and adds `document-scale-policy` to the dependent-designs column of each, as C0's ledger requirement asks.

Each new entry below goes into the ledger with a `gtk-lush-axioms` probe and a sample, unless C0 has already pinned it. Ids are the next free ones after C0's entries and are assigned at apply time.

- **Paragraph delimiters.** GtkTextBuffer line boundaries are exactly Pango's paragraph delimiters (`\n`, `\r`, `\r\n`, U+2029).
- **Line length cost.** `gtk_text_iter_get_chars_in_line` costs time in proportion to the line's segment count, not its character count. The probe compares a 1-character and a 2M-character single-segment line against a generous ratio bound and asserts no growth with length. If timing proves unstable under the harness, the entry is declared "verified by source", with the reason recorded, under the ledger's unpinnable rule.
- **Same-emission downgrade.** Setting the language to none inside the `insert-text` after-handler leaves no context-class tags on the inserted text after the main loop drains. This is the observable proof that the context engine never analysed the paragraph.
- **Unparent stops a pending tick.** A widget unparented (and so unrealized) while a tick callback is armed does not run that callback in any later frame, and its later disposal emits no warning. The minimap's retire-and-rebuild remedy (D6) rests on this; A32 only records the defect.

`make gtk-axioms` runs first, as the ledger growth rule requires.

## Risks / Trade-offs

- [C0 values are not yet measured] → Every threshold is a named constant. The tasks block implementation until C0 is archived with recorded values. If C0 leaves one open, it is listed in Open Questions and the implementer uses the conservative (lower) candidate from PROPOSAL.md.
- [The edit-time handler adds work to every keystroke] → A fast path for insertions that leave the line count unchanged while the carried density bound stays within `V_batch`, plus at most `2·R_batch + 2` O(segments) reads otherwise. C0's watchdog lane must show that typing in paragraphs below every threshold stays within B_interact, with the watch's own share measured. A deterministic widget test asserts the read count, never wall time. Typing **inside** a giant paragraph still pays GTK's whole-paragraph buffer costs (A26 log attrs, about 95 ms per keystroke on the 1.9M-character line); that is part of the declared degraded mode that C2 removes by making such documents view-only, and C1 does not claim it.
- [Monotone downgrades leave features off after the user deletes the giant line] → This is accepted and documented: reopening the file re-profiles it. The notice says so.
- [Retire-and-rebuild of the minimap loses transient map state (adjustment, slider CSS, the reflow freeze pairing)] → `create_native_source_map` already holds the whole map configuration, `RenderHoldOverlay::set_live_child` re-points the freeze, and the vadjustment guard is reconnected per instance. Rebuild is a new path, so the geometry visual proofs (`native-minimap-*-anchors`) and the minimap widget tests must stay green, and a rebuild test asserts geometry equality with a constructed map.
- [The paragraph-boundary change alters slice boundaries for CR-only or U+2029 documents] → This is intended, and it fixes a blocker. Byte-identity round-trip tests cover CR, CRLF, U+2029 and mixed documents through load, draft recovery and save.
- [The reduced-features notice competes with recovery alerts] → It never uses the inline alert slot; it is a status-bar message plus a document-properties row (D7, revised after the 10.4 lane).
- [Data safety] → This change never mutates buffer text. It only toggles projections. `data-safety` is still run explicitly over the load, save and replacement call-site edits.

## Migration Plan

1. C0 lands and records the thresholds and budgets.
2. Land `paragraph_boundary.rs` with the property and Kani tests (D9), plus the CR-only and U+2029 slicing tests, which fail first.
3. Land `document_scale.rs` with Kani, plus `for_total_bytes` parity tests against `FileSizeCheck`.
4. Migrate all 17 consumers in one commit series and delete `file_limits.rs`.
5. Add the load-time gating, then the edit-time watch (its pure core `model/paragraph_profile_watch.rs` and its Kani harnesses first), then the minimap work: availability, then retirement after the failing-first test.
6. Add the notice and the docs.

No persisted data changes, so rollback is a revert.

## Open Questions

1. **Threshold values** — partly resolved at apply time (D0). Still open:
   `S_syntax` (C0 could not derive it; 1 MiB applied, the conservative
   candidate) and `L_protect` (not derived; the stall basis 500 applied, as C2's
   gate does). With `L_protect` at 500, any document with a paragraph over 500
   characters (ordinary Markdown prose written one paragraph per line) is marked
   for Protected View, and C1's notice says so; the maintainer may prefer a
   higher C1 notice threshold until C2's user-session validation. Earlier text:
   **Threshold values.** C0 must record `S_syntax` (maintainer decision 2: decided by the C0 measurement; candidates 10 MB or about 1 MiB), `L_syntax`, `L_minimap`, `L_invisibles`, `L_search_highlight`, `L_protect` (PROPOSAL.md decision 2: about 20k versus 100k–300k), `N_visible`, `R_batch` and `V_batch`. It must also confirm or revise B_interact = 8 ms, input-to-frame ≤ 2 frames and B_settle = 16 ms. If C0's record leaves `L_minimap`, `L_invisibles` or `L_search_highlight` open, does the maintainer accept the default `min(L_syntax, N_visible)`?
2. **Should `L_protect` be a GSettings key** — resolved 2026-10-11: a fixed policy constant, not a GSettings key. A setting that lets the app block would contradict the always-responsive requirement; revisit only together with `explore-editing-long-paragraphs`.
3. **Eviction reload and the session floor.** Should an eviction reload keep the session floor? It reloads byte-identical clean content, so a fresh profile is correct. But a user who pasted, undid and saved would see features come back after eviction. Default in this design: an eviction reload keeps the floor, and only open and external reload reset it.
4. **Edit-time undo downgrade.** Should a session that grows past the undo tier through edits turn undo off with `begin_irreversible_action`? It does not today, and C1 keeps that behaviour.
5. **Automation exposure.** Should C1 add a bounded `reduced_features` snapshot field and a readiness predicate, or leave every snapshot change to C2?
6. **Notice wording and style.** Warning or informational styling, and whether it offers an action ("Learn more", or "Open in Protected View" once C2 lands).
7. **The 1 MB tier.** `LARGE_FILE_TOAST` drives only the accessible sentence "Large document mode is active."; no toast exists, although AGENTS.md says ">1MB toast". This design retires both. Keep a size-only "large document" sentence instead?
8. **Proposal ordering.** Resolved: C0's `responsiveness-measurement` requirement now blocks *implementation*, not proposal, of the later changes until the programme record holds derived values, which matches this change's task 0.1.
