## Context

`services/markdown_render.rs` plans Markdown preview GTK-free. It walks the `pulldown_cmark` event stream, accumulates one top-level block at a time, and appends complete blocks into `MarkdownEventBatch` values bounded by `MARKDOWN_EVENTS_PER_PROJECTION_SLICE` (256) and `MARKDOWN_BYTES_PER_PROJECTION_SLICE` (256 KiB). `ui/markdown_preview/mod.rs::start_render_plan` applies at most one batch per GTK main-loop turn, and `render_event_batch` rebuilds *all* of its render state as function locals on every turn. That is only sound because the planner guarantees batch boundaries occur at depth zero: the module doc comment states the invariant as "a batch boundary never loses inline/list/table state when GTK yields".

The defect is the enforcement path. In `plan_markdown_inner`, when a single top-level block exceeds either per-slice budget, the planner clears the block, records `MarkdownPlanLimit::TopLevelBlock` or `MarkdownPlanLimit::ProjectionBytes`, and `break`s out of the event loop. Everything after that block is discarded. Because a 5-column table crosses 256 events at roughly 15-20 rows, a fenced code block at roughly 250 lines, and a bullet list at roughly 40-80 items, ordinary real documents — the observed trigger was a Helm chart README values table — lose their entire tail.

Two *projection-side* widget budgets already exist below the planner's budgets and constrain what completeness can mean:

- `ui/markdown_preview/tables.rs::MAX_PREVIEW_TABLE_CELLS` = 1,000 cells. A larger table is already replaced wholesale by `build_preview_limit_fallback_widget` ("Table not rendered").
- `ui/markdown_preview/mod.rs::MAX_PREVIEW_CODE_BLOCK_BYTES` = 64 KiB. `code_blocks.rs::push_literal` stops copying text at that ceiling, and `build_code_block_widget` replaces a block over it with the fallback widget ("Code block not rendered").

These are pre-existing, intentional GTK-side ceilings. This change does not raise them, so every completeness claim below is scoped to them.

Other constraints:

- The per-slice budgets exist for main-thread responsiveness. Raising them is not a fix and enlarging one turn's work is a regression. `ordinary_blocks_are_packed_without_splitting` already asserts every batch stays within both budgets; that invariant must survive.
- Tables, code blocks, and images render as *embedded GTK widgets* created when the closing event arrives. Any scheme that closes and reopens such a block creates a second widget.
- `markdown-preview-code-blocks` already requires one code block to render as a single continuous surface that "MUST NOT split into multiple highlighted regions". Sub-slicing must be visually indistinguishable from a single-turn render.
- `WFR-MARKDOWN-PREVIEW` is `deferred` in `docs/workflow-readability-matrix.md`. This change must not partially migrate the workflow: no facade/coordination/policy/evidence role assignment, and test inspection keeps the existing `*_for_test` idiom.
- Planning stays GTK-free, deterministic, cancellable at bounded checkpoints, and fuzz-safe.
- Typed payload ownership: anything document-sized that crosses a turn must be retired off the GTK thread on generation change.
- `pulldown_cmark` is a *streaming* parser. The planner sees one event at a time and cannot know a block's total size until its closing event. Every rule below is therefore expressible as crossing-point behavior on running counters, with no lookahead and no rewriting of already-closed batches.
- Code-block event shape in the pinned `pulldown_cmark` 0.13.4 differs by kind (parse-verified): a **fenced** block's body is one coalesced `Text` event, so a fence is three events — `Start(CodeBlock)`, `Text`, `End(CodeBlock)` — regardless of line count, and its size is expressed purely in bytes. An **indented** block emits one `Text` event **per line** (80 events for an 80-line block), so the 256-event slice budget is reachable for indented blocks and the code-text-run checkpoint is a live path there.

## Goals / Non-Goals

**Goals:**

- Make tail truncation caused by one block's shape unrepresentable.
- Render an oversized table, list, code block, blockquote, or definition list completely — within the existing preview widget budgets — and as one continuous visual block, over several bounded GTK turns.
- Keep per-turn GTK work bounded by the existing event and byte slice budgets, with no over-budget batch ever emitted.
- Keep cross-turn projector state bounded by structural depth plus one in-flight embed buffer charged against its own track's ceiling (code bytes, table cells), never by document size.
- Degrade at the smallest unit that overflows, so one bad row, item, or paragraph never costs its siblings.
- Replace the misleading "preview limited" copy for this case with an honest partially-simplified terminal that counts omissions.
- Keep every global budget a terminal stop with unchanged semantics.

**Non-Goals:**

- Raising `MARKDOWN_EVENTS_PER_PROJECTION_SLICE`, `MARKDOWN_BYTES_PER_PROJECTION_SLICE`, `MAX_MARKDOWN_EVENTS`, `MAX_MARKDOWN_RETAINED_BYTES`, `MAX_MARKDOWN_SOURCE_BYTES`, `MAX_MARKDOWN_EMBED_DESCRIPTORS`, `MAX_MARKDOWN_STRUCTURE_DEPTH`, `MAX_PREVIEW_TABLE_CELLS`, or `MAX_PREVIEW_CODE_BLOCK_BYTES`.
- Migrating `WFR-MARKDOWN-PREVIEW`, adding an evidence surface, or introducing a `policy.rs` into that workflow.
- Virtualized or lazily materialized table/code widgets, incremental re-render on edit, or scroll-driven projection.
- New actions, D-Bus members, readiness predicates, automation snapshot fields, or accessibility anchors.
- Changing image admission, inline-footnote lowering budgets, or the disposal lane.

## Decisions

### 1. Cut batches wherever the open-container stack holds only block containers

The planner keeps an explicit stack of open frames instead of only a depth counter. A batch boundary is **admissible** exactly when every frame on the stack is a block container and no inline or text-flow frame is open. Depth zero (today's only boundary) becomes the trivial case of that rule.

- Block containers (a cut is allowed while these are open): `List`, `Item`, `Table`, `BlockQuote`, `CodeBlock`, `DefinitionList`, `DefinitionListDefinition`.
- Frames that forbid a cut: `Paragraph`, `Heading`, `TableHead`, `TableRow`, `TableCell`, `FootnoteDefinition`, `Emphasis`, `Strong`, `Strikethrough`, `Link`, `Image`, `Superscript`, `Subscript`, `DefinitionListTitle`, `HtmlBlock`, `MetadataBlock`.

`FootnoteDefinition` is deliberately cut-forbidding. It is structurally a block container, so a cut inside it would be inline-safe, but footnote definitions are short in practice, admitting them would add a checkpoint class needing its own taxonomy row, segment unit, spec scenario, and carried-frame descriptor, and no observed document triggers it. A footnote definition that somehow exceeds a slice therefore takes the top-level omission path. Promoting it later is a purely additive change to the classification match.

`TableHead` is deliberately cut-forbidding. `pulldown_cmark` emits header cells directly inside `TableHead` with no intervening `TableRow`, so classifying `TableHead` as a block container would permit a cut *between header cells* and require mid-header-row carry. Cutting after `End(TableHead)` is still admissible because the stack is then `[Table]`.

This single rule yields the checkpoint taxonomy the change needs:

| Block type | Admissible cut point | Why no inline state is open | Frames carried across the cut |
| --- | --- | --- | --- |
| Table | after `End(TableHead)` and after each `End(TableRow)` | a cell's inline spans close inside the cell; the cell closes inside the row | `Table { alignments }` — no header flag is carried, because a cut is admissible only outside `TableHead`, so the "in header" state is always false at a boundary and the implementation correctly omits it |
| List (any depth) | after each `End(Item)` | an item's inline content closes before `End(Item)` | one `List { ordered, next_number }` per open list, plus enclosing `Item` frames |
| Code block | between consecutive `Text` events inside a code block — **live for indented blocks** (one `Text` event per line), **unreachable for fenced blocks** under the pinned parser (body coalesced into one `Text` event) | code-block text events carry literal text and no inline state | `CodeBlock { kind, language }` |
| Blockquote | after `End(Paragraph)` directly inside a blockquote | the paragraph closes all inline spans | `BlockQuote { kind }` per open quote |
| Definition list | after `End(DefinitionListDefinition)` | a definition body closes its inline spans | `DefinitionList` |
| Everything else | none | a paragraph or heading with >256 inline events has no interior point with an empty inline stack | not applicable |

Cutting mid-block is admissible only when the *whole* stack qualifies, so a table nested inside a list item is still cuttable at a row boundary (`[List, Item, Table]` are all block containers), while a table cell containing 300 emphasis spans is not.

Rationale for putting the rule in the planner: the planner already owns budget enforcement, is GTK-free, is in mutation scope, and is the only place that can decide admissibility before GTK sees anything. The frame stack replaces the existing `depth` counter, so `MAX_MARKDOWN_STRUCTURE_DEPTH` continues to bound it.

Sub-slicing engages only when a block would otherwise exceed a slice budget. Blocks that fit keep today's exact packing, so the common path, batch counts for ordinary documents, and existing tests are unchanged.

**Alternative rejected — raise the per-slice budgets.** Any budget has a cliff; a 100-row values table would still be censored, and a larger slice makes one GTK turn slower, contradicting the requirement this change is fixing under `main-thread-responsiveness`.

**Alternative rejected — render the oversized block in one over-budget turn.** A 4 MiB single-line table would freeze GTK for seconds. The whole point of the projection slice is that no turn is unbounded.

### 2. Carry structural continuation in the projector, not synthesized close/reopen events

The chosen mechanism is one generation-owned continuation value held by the preview widget between projection turns, in new sibling modules rather than in the already-2,541-line `mod.rs`. The continuation intentionally spans **two** files, a vertical role split forced by the file-size rule rather than unplanned scope: `continuation.rs` owns the generation-carried state and batch application, and `text_flow.rs` holds the stateless text-flow primitives operating over borrowed slices. `render_event_batch`'s locals move into it: `tag_stack`, `generic_blockquote_depth`, `list_stack`, `list_item_stack`, `definition_stack`, `pending_list_prefix`, `needs_block_separator`, footnote numbering, and at most one in-flight embed buffer — the `Option<BufferedTableBuilder>` (`tables.rs`) or the `Option<ActiveCodeBlock>` wrapper that owns a `BufferedCodeBlock` plus its captured `EmbeddedBlockLayout` (`code_blocks.rs`). The purely inline locals (`active_text_links`, `active_image`) are provably empty at an admissible cut; the projector asserts that in debug builds and falls back to the defensive terminal in release rather than rendering corrupted output.

Each `MarkdownEventBatch` carries a seam value object describing the continuation it expects at its first event and the continuation it leaves open at its last (`MarkdownCarrySignature`, an ordered bounded list of `MarkdownOpenContainer` descriptors). The projector validates the signature it holds against the batch's expected signature before applying it. This satisfies the seam value-object rule: the batch's structural expectation crosses the planner/projector boundary and cannot be silently renamed into an unrelated parameter, and a mismatch is a type-checked defensive terminal rather than invisible render corruption.

**Honest scope of that defensive terminal.** The `ContinuationBreach` *transition* is unreachable without a test-actuation seam, and deliberately so: Decision 1's chaining invariant is asserted planner-side and property-tested, so a live signature mismatch requires a defect in the planner itself. What is pinned by unit test is the breach terminal's description strings and its `CarrySignature` arm; the end-to-end path from a real mismatch to a published terminal is intentionally not actuatable, because making it so would mean adding a seam whose only purpose is to forge a state the invariant forbids. Read the breach arm as a guard that converts an impossible state into an explicit terminal, not as a covered runtime path.

**Alternative rejected — synthesize balanced close/reopen events at each checkpoint so batches stay self-contained.** It looks attractive because the projector would stay stateless. It fails on the widget-building blocks that motivate the change: a synthesized `End(Table)` creates a table widget, a synthesized `End(CodeBlock)` creates a code surface, so an oversized values table becomes a stack of separate tables and an oversized fence becomes several highlighted regions — a direct violation of the `markdown-preview-code-blocks` single-surface requirement. Making the seam invisible would require the projector to keep the in-flight builder alive across turns and merge continuations, which *is* the continuation state, so close/reopen adds planner machinery without removing any projector state. It is therefore rejected as the primary mechanism. (Close/reopen synthesis remains acceptable as a purely internal planner convenience if an implementation finds it clarifies the event stream, but only if the projector still merges continuations into one widget and one visual block.)

**Alternative rejected — sub-slice only non-widget blocks (lists, blockquotes) and keep tables/code blocks atomic.** The two most common triggers are tables and code blocks, so this would leave the reported defect in place.

### 3. Bound the continuation explicitly, with one ceiling per embed track

What crosses a turn:

- The frame descriptor list: at most `MAX_MARKDOWN_STRUCTURE_DEPTH` (128) entries. Each entry holds scalars plus descriptors the plan already retains (column alignments, code language hint, list ordinal, blockquote kind), so the frame list is O(depth), not O(content).
- Scalar flow state: block-separator flag, blockquote depth, list-item and definition flow markers, footnote counter.
- Footnote label numbering, which becomes generation-scoped instead of batch-scoped. This is a strict correctness improvement — `markdown-preview-inline-footnotes` requires each reference marker to match its definition's number, and today a reference and its definition in different batches both restart at 1 — and stays bounded by the existing inline-footnote expansion budget.
- At most one in-flight embed buffer (a partially built table or code block). This is the only content-sized part of the continuation, so it is charged against a ceiling — but **the ceiling's unit must match how the projector decides that embed's presentation**, which differs by track.

**The ceiling must not change any rendered outcome.** Rendered outcomes for embedded blocks stay decided by the projection-side widget budgets alone (`MAX_PREVIEW_TABLE_CELLS`, `MAX_PREVIEW_CODE_BLOCK_BYTES`), exactly as today. Both existing budget checks are computed from *observed* totals, not from retained content: `BufferedCodeBlock::exceeds_preview_widget_budget()` is `source_bytes.max(text.len()) > MAX_PREVIEW_CODE_BLOCK_BYTES`, and the table check counts **cells**. That asymmetry is decisive. A single byte ceiling applied to both tracks is *not* outcome-neutral: an 8-cell table holding 192 KiB of cell text renders completely today, and a byte crossing at 64 KiB would newly drop its rows. So the ceiling is two-track, each threshold mirroring the widget budget that governs its own track:

| Track | Ceiling | Mirrors | Neutrality argument |
| --- | --- | --- | --- |
| Code block | `MAX_MARKDOWN_CARRIED_EMBED_BYTES` = 64 KiB of retained text | `MAX_PREVIEW_CODE_BLOCK_BYTES` (64 KiB) | Past the threshold the projector's fallback *always* fires, because the thresholds are equal. Retaining more is dead memory by construction: `push_literal` already stops copying at 64 KiB while `source_bytes` keeps counting. |
| Table | a cell-count ceiling equal to 1,000 cells | `MAX_PREVIEW_TABLE_CELLS` (1,000) | Past the threshold the projector's fallback *always* fires, because the thresholds are equal. Below it, a table of **any** byte size is retained and rendered in full, so no row is ever lost to the ceiling. |

`services/` cannot import `ui/`, so the table ceiling is a planner-side mirror constant of `MAX_PREVIEW_TABLE_CELLS`; the two values must be changed together, and the mirror's doc comment names its counterpart.

Table retention stays bounded without a byte ceiling: retained table bytes are at most 1,000 cells times per-cell markup, and per-cell markup is itself bounded because the whole document is capped at `MAX_MARKDOWN_SOURCE_BYTES` (4 MiB). A worst-case retained table is therefore a bounded fraction of an already-bounded source, not an unbounded accumulation.

Crossing-point behavior carries counts, not content (no lookahead, no rewriting closed batches):

1. The planner keeps a running charge for the currently open embed container — retained bytes for a code block, cells for a table.
2. When appending an event would push that charge over the track's ceiling, the planner stops *retaining* that container's remaining content at the current position and consumes the container's remaining events for metrics only until its closing event.
3. It emits one crossed-embed omission (decision 4) naming the track that crossed — `CarriedEmbedBytes` for code, `CarriedEmbedCells` for a table — whose payload carries the **true remaining counts** accumulated from those consumed events.
4. The projector charges those counts onto the in-flight buffer (`source_bytes` for code, cell count for tables) as if it had seen the full content, so `exceeds_preview_widget_budget()` and the table-cell check evaluate on the real totals and the existing fallback widget fires exactly as it does today.

Consequences: a code block over 64 KiB still becomes the single "Code block not rendered" fallback; a table over 1,000 cells still becomes the single "Table not rendered" fallback; and a table within 1,000 cells renders in full regardless of byte size. Because each ceiling equals the widget budget it mirrors, a crossing is *always* accompanied by the projector replacing that embed wholesale — so the crossed-embed marker is never the only thing a user sees for an embed that would otherwise have rendered.

The earlier draft's claim that the planner "computes the charge before emitting any cut" and can "drop the block's events" after sub-slices exist is withdrawn: it is not implementable against a streaming parser and would have invalidated closed batches.

**Fenced code-block size bands (worked through against the pinned parser).** This band analysis is **fenced-only**: because a fence is three events with the whole body in one `Text` event, exactly one rule can fire per size band. Indented blocks are covered below the table.

| Body size | What fires | Outcome |
| --- | --- | --- |
| 0 - 64 KiB | nothing; 3 events ≤ 256 and body ≤ 64 KiB ≤ 256 KiB | retained in full, fits one slice, **rendered whole in a single turn** — no sub-slicing, no marker |
| > 64 KiB | the code-track byte crossing, on the single `Text` event | that one event is not retained, its bytes become `unretained`, and the projector charges them onto `source_bytes`. Because the crossing threshold *equals* `MAX_PREVIEW_CODE_BLOCK_BYTES`, any body past it necessarily exceeds the widget budget too, so **the existing "Code block not rendered" fallback always fires in this band** — byte-identical to today's outcome |

Two consequences follow for fenced blocks and are load-bearing:

- The 256 KiB slice-byte band is **unreachable for a fenced block**. The crossing at 64 KiB drops the body event before `block_retained_bytes` can accumulate past 64 KiB, so a fence never reaches the slice-byte omission path, and with three events it never reaches the slice-event path either. Fenced blocks therefore produce no omission marker a user can see; they resolve entirely through the pre-existing widget budget.
- The pre-fix truncation for fenced blocks required a body over 256 KiB (which set the old `ProjectionBytes` limit and discarded the tail). That is precisely the case the crossing now absorbs, so the fix's promise holds for fences without any sub-slicing being involved.

**Indented code blocks take the sub-slicing path instead.** Per-line `Text` events mean an indented block over 256 events *does* reach the event budget, and the code-text-run checkpoint after each line is admissible, so it is **sub-sliced across turns** into one continuous code surface — the ordinary Decision 1 mechanism, with the projector's carried `ActiveCodeBlock` merging the lines. Because each retained line still charges the code-track byte ceiling, an indented block stays sub-sliceable only while its retained text is under 64 KiB; past that it crosses and resolves to the same widget fallback as a fence. So the reachable outcomes are: indented and under 64 KiB — rendered whole, sub-sliced if over 256 lines; indented or fenced and over 64 KiB — existing fallback widget.

On generation change, cancellation, or widget teardown, the continuation is dropped with the projection; when it holds an embed buffer it goes through the existing guarded retirement path (`retire_guarded_markdown` / the plain disposal lane) so document-sized text is not freed on the GTK thread.

### 4. Omit the smallest unit that overflows, and always continue

A **segment** is the run of events between two consecutive admissible boundaries. The normative source of that granularity is Decision 1's rule alone: the planner records a checkpoint after *every* event that leaves the frame stack all-block-containers. Segments are therefore finer than "one row / one item", and the implemented granularity is:

- **After `Start(Item)` and after `End(Paragraph)` inside an item.** A loose list item's body paragraph is its own segment, so an overflowing item does **not** lose its shell: the empty `Item` renders and the marker lands *inside* an otherwise-empty item at that position. Only the overflowing paragraph is replaced.
- **Between bare inline-leaf events** (`Text`, `Code`, `SoftBreak`, `HardBreak`, `Rule`, `FootnoteReference`) whenever no inline tag is open — for example inside a tight list item, which pulldown-cmark emits without `Paragraph` tags. Such an item is divisible at those leaf boundaries even though its content is inline, because no inline *span* is open there.
- **Not** inside a run bracketed by an inline container. The genuinely indivisible units are exactly the contents of `Paragraph`, `Heading`, `TableHead`/`TableRow`/`TableCell`, `FootnoteDefinition`, `DefinitionListTitle`, `HtmlBlock`, `MetadataBlock`, and the inline spans (`Emphasis`, `Strong`, `Strikethrough`, `Link`, `Image`, `Superscript`, `Subscript`).

A practical consequence worth stating because it shaped the fixtures: a *tight* list item cannot be made indivisible by piling on plain text, since every leaf is a boundary. Reproducing an indivisible overflowing item requires a **loose** item whose single paragraph carries the inline events (the task 1.4 fixture), or an item whose content sits inside one inline span.

Scope is derived, not declared: an omitted segment whose carry signature is empty is `TopLevelBlock`; otherwise it is `ContainerSegment`. Sibling preservation is unchanged and is what all of this exists for.

**Two kinds of omission: user-visible vs charge-carrier.** The two crossing reasons are not user-facing omissions at all:

- **User-visible** (`SliceEvents`, `SliceBytes`): content the preview genuinely cannot render. It gets a marker, it counts toward the terminal, and it is announced.
- **Charge-carrier** (`CarriedEmbedBytes`, `CarriedEmbedCells`): bookkeeping that moves unretained counts across the planner/projector seam. Decision 3 establishes that a crossing can only fire *past* a widget budget, so the projector's pre-existing fallback widget replaces that whole block and already names its true size in place. Rendering a marker next to it would duplicate the explanation, and counting it would make a document that publishes `Complete` today report "Preview complete; 1 block was too complex" — contradicting the promise that these blocks keep exactly today's presentation.

Therefore the projector MUST NOT render markers for the two `CarriedEmbed*` reasons and MUST NOT count them toward the terminal choice, while still charging their `unretained` counts (decision 3, step 4).

**Where the distinction lives: a planner-side accessor.** The plan exposes `user_visible_omissions()`, which counts only the slice reasons; the full marker list stays available as evidence. This keeps the visible/charge-carrier split beside the reason enum that defines it, so the projector consumes one number instead of re-deriving policy by matching on reasons at three call sites (marker rendering, terminal choice, announcement). The alternative — projector-side filtering — was rejected because it would put a policy decision in the GTK adapter and let the three consumers drift.

`MarkdownPlanLimit::TopLevelBlock` and `MarkdownPlanLimit::ProjectionBytes` are removed from the terminal limit enum. In their place, `MarkdownBlockOmission { reason: MarkdownOmissionReason, scope: MarkdownOmissionScope, unretained: UnretainedEmbedCounts }`:

- `reason` distinguishes four cases: slice events, slice bytes, `CarriedEmbedBytes` (the code-track byte ceiling), and `CarriedEmbedCells` (the table-track cell ceiling). The two crossings get two honest reasons rather than one shared variant, because deriving table-vs-code from `unretained.cells > 0` would be fragile inference over a payload field.
- `scope` is `TopLevelBlock` or `ContainerSegment`.
- `unretained` carries the source byte count and cell count of content the planner counted but did not retain, and is zero for the slice-budget reasons. Which member is meaningful follows the track that crossed (decision 3): a **code-block** crossing sets bytes only (a code block has no cells); a **table** crossing — which can only happen past 1,000 cells — sets cells, and also the bytes of those unretained cells so the projector's charge stays complete. The projector charges these onto the in-flight embed buffer so the existing widget-budget decisions see the real totals.

When a segment cannot be fitted into a slice and has no admissible interior cut, the planner drops that segment's events, appends one omission marker at that position, increments the omission count in `MarkdownPlanMetrics`, and **continues the event loop**. The `break` on this path disappears entirely. Crucially, a `ContainerSegment` omission is emitted *inside the still-open container*, so a 60-item list whose item 17 holds 400 inline events renders items 1-16 and 18-60 normally with one placeholder row where item 17 was. A container never loses its siblings because one segment overflowed.

Projection is scope-aware:

- `TopLevelBlock` omissions reuse `build_preview_limit_fallback_widget`, matching the existing table/code/image fallbacks, capped at `MAX_MARKDOWN_PLACEHOLDER_WIDGETS` (64) widgets per generation; further top-level omissions render as an accessible inline text marker. Placeholder widgets do not consume `MAX_MARKDOWN_EMBED_DESCRIPTORS`, which stays a parser-side descriptor budget.
- `ContainerSegment` omissions always render as an in-container text marker — a spanning table row, a list item, a code-block line, a quoted paragraph, a definition body — never a nested fallback widget. This keeps the container's own widget single and keeps widget count independent of segment omissions.

**Metrics rule.** An omitted segment's events, retained bytes, and embed descriptors are still charged to `MarkdownPlanMetrics` and to the global ceilings, exactly as today. The planner really did parse and transiently retain them, and charging them keeps a hostile document from bypassing `MAX_MARKDOWN_RETAINED_BYTES` by making every block indivisible. The consequence must be stated honestly: a document large enough to exhaust a *global* budget can still stop before its end. What this change eliminates is truncation caused by *where a large block sits*; cumulative-size terminals remain. In practice the reachable global terminal for an all-indivisible document is `MAX_MARKDOWN_EVENTS`; `MAX_MARKDOWN_RETAINED_BYTES` (8 MiB) is unreachable under the 4 MiB source cap, since retained bytes track source bytes and the only amplifier is inline-footnote lowering, which is capped by `MAX_INLINE_FOOTNOTE_REPLACEMENTS` (`MAX_MARKDOWN_EVENTS / 4`) and by its own retained/output-byte admission that refuses any replacement crossing `MAX_MARKDOWN_RETAINED_BYTES` — so lowering publishes its own limited terminal before it could hand the planner a source large enough to reach that ceiling. It is kept as defense-in-depth against a future source-cap or expansion change, and its charge arithmetic is pinned by test rather than by a reachable end-to-end terminal.

**Alternative rejected — placeholder for every oversized block, with no sub-slicing.** It is much smaller, and it was rejected because the oversized block is usually the document's most valuable content: a values table or API table would be replaced by an apology. Placeholder-and-continue is correct as the *last* resort, not as the mechanism.

**Alternative rejected — block rewind on segment overflow** (record the batch index where the block opened, truncate back to it, and re-emit the whole block as one placeholder). It restores "a block is all-or-nothing" symmetry, but it requires mutating already-closed batches, repairing carry signatures after the fact, and discarding correct rendered rows the user could have read. Per-segment omission keeps the plan append-only and strictly more informative.

### 5. Completeness argument

After this change, the planner's event loop has no exit other than a global budget. Every segment falls into exactly one case:

1. It fits the remaining slice budget: it is packed and projected as today.
2. It exceeds the slice budget but has an admissible interior cut: it is sub-sliced and projected completely across bounded turns, each turn within both slice budgets.
3. It exceeds the slice budget with no admissible interior cut, or its container crossed its embed-track ceiling (64 KiB of code text, or 1,000 table cells): it becomes one omission marker at its own position, and the loop continues with the next event.

No case exits the loop, and case 3 never widens beyond the overflowing segment. The `break` in `plan_markdown_inner` was the only source of truncation attributable to block shape, so shape-driven truncation becomes unrepresentable. Content beyond the projection-side widget budgets (`MAX_PREVIEW_TABLE_CELLS`, `MAX_PREVIEW_CODE_BLOCK_BYTES`) still degrades to the pre-existing fallback widget for that one block, and the following document is unaffected.

### 6. Partially-limited terminal semantics

Today `MarkdownRenderState::Limited` plus `MarkdownPlanLimit::description()` tell the user rendering stopped. That is now wrong for the omission case, where the document is complete except for named holes. Therefore:

- `MarkdownRenderState` gains `Simplified`: a complete projection containing at least one **user-visible** omission, that is `user_visible_omissions() > 0`. Like `Limited`, it is terminal and not `pending`, so readiness (`render_pending`, the `preview-animation` blocker) is unchanged and no automation snapshot field or readiness predicate changes.
- A plan whose only omissions are the `CarriedEmbed*` charge-carriers publishes **`Complete`**, not `Simplified` (decision 4). Those blocks already explain themselves through the pre-existing fallback widget, so the terminal must stay byte-identical to today's.
- `Limited` keeps its exact current meaning: a global budget stopped planning before the end of the document.
- Terminal copy for `Simplified` reports completion and a count of user-visible omissions only, e.g. "Preview complete; N blocks were too complex to render". Each rendered marker carries its own accessible description naming the omitted unit and reason.
- `MarkdownPlanLimit::description()` keeps its remaining variants verbatim so no other terminal's copy churns.

**Alternative rejected — reuse `Limited` with different copy.** It would make "the preview stopped" and "the preview is complete with holes" indistinguishable in state, which is exactly the confusion this change removes.

Accessibility posture is specified as a separate ADDED requirement in the `gtk-accessibility-spine` delta rather than as extra scenarios under the existing "Markdown preview and read-only text surfaces are accessible" requirement, because the obligation here is a new one (an omitted-content artifact must be announced and counted once) rather than a refinement of read-only preview semantics. Folding it in would also mean restating that whole requirement as MODIFIED for an unrelated addition.

### 7. Determinism, mutation, and fuzz posture

Planning stays pure, allocation-deterministic, and cancellable at the existing 64-event checkpoint cadence; the frame stack and the running embed charge add no I/O, no time source, and no randomness, so `plan_markdown_cancellable` remains fuzz-safe. Because decision 3 forbids rewriting closed batches, the plan remains append-only and these invariants hold and are assertable:

- every emitted batch stays within both per-slice budgets (today's `ordinary_blocks_are_packed_without_splitting` invariant, now required for sub-sliced batches too);
- consecutive batches' carry signatures chain (batch *n*'s open signature equals batch *n+1*'s expected signature, and the last batch closes everything);
- planning consumes the event stream to EOF unless a *global* limit fired, so `metrics.events` accounts for every parsed event as projected or omitted.

All new decision logic lives in `services/markdown_render.rs`, already inside the cargo-mutants `examine_globs`. No `policy.rs` is added to `ui/markdown_preview/**`, so the deferred `WFR-MARKDOWN-PREVIEW` row is not partially migrated and `make check-workflow-boundaries` posture is unchanged.

## Risks / Trade-offs

- **[A cut is taken where inline state is actually open, corrupting the render]** -> Admissibility is a positive whitelist of block-container frames, matched exhaustively over `pulldown_cmark::Tag` with no wildcard arm so a version bump is a compile error; the projector validates each batch's signature; debug assertions require the inline-only locals to be empty at a cut; release behavior degrades to an explicit terminal rather than corrupted text.
- **[A sub-sliced batch exceeds a slice budget after all]** -> The per-batch budget assertion is extended to cover sub-sliced batches, and segment overflow is handled by omission at the crossing point rather than by emitting an over-budget batch.
- **[The continuation drifts from the buffer after a superseded generation]** -> The continuation lives with the projection value, is dropped or retired with it, and every turn revalidates `render_session.is_current(generation)` exactly as today.
- **[A carried embed buffer retains document-sized text on the GTK side]** -> Code text is capped at `MAX_MARKDOWN_CARRIED_EMBED_BYTES` (64 KiB) at the crossing point; a table is capped at 1,000 cells, whose retained bytes are bounded by cell count times per-cell markup under the 4 MiB source cap. Retirement uses the existing off-GTK disposal lane either way.
- **[An embed ceiling silently changes a rendered embed]** -> Each ceiling mirrors the widget budget governing its own track, so a crossing always coincides with the projector replacing that embed wholesale. The crossed-embed omission carries the unretained counts, the projector charges them onto the in-flight buffer, and widget tests assert that an over-budget code block and an over-1,000-cell table still produce exactly today's single fallback widget while a ≤1,000-cell table larger than 64 KiB still renders every row.
- **[The planner-side table cell mirror drifts from `MAX_PREVIEW_TABLE_CELLS`]** -> The mirror constant's doc comment names its `ui` counterpart and states they must change together; a widget test that renders a table just under and just over the budget fails if the two diverge.
- **[Users read "complete" while widget budgets silently truncated a table or code block]** -> The pre-existing per-block fallback widget still names that degrade in place, and the tables/code-blocks requirements are scoped to those budgets with an explicit degrade scenario.
- **[More batches per document increases total projection turns and perceived latency]** -> Extra turns appear only for blocks that previously produced *no* output at all; ordinary blocks keep existing packing, and performance smoke records planning/projection counters for dense fixtures.
- **[A pathological document emits thousands of markers]** -> Container-segment markers are text, not widgets, and top-level fallback widgets are capped at `MAX_MARKDOWN_PLACEHOLDER_WIDGETS`.
- **[Generation-scoped footnote numbering changes existing rendered output]** -> It only changes documents whose references and definitions land in different batches, where the current numbering already violates the inline-footnote numbering requirement; a spec scenario and a widget test pin the corrected behavior.
- **[Widget-test and copy churn hides a behavior regression]** -> The existing dense-single-block test is rewritten in place to assert the new `Simplified` terminal, and new tests assert that an oversized table, list, code block, blockquote, and definition list each render completely with the document tail intact.

## Migration Plan

No persisted data, schema, or user setting changes. The internal `MarkdownPlanLimit` variant set, `MarkdownEventBatch` shape, `MarkdownRenderPlan` fields, and `MarkdownRenderState` variant set change within the crate; all consumers are in-tree. Rollout order is planner-first: the GTK-free planner and its unit/property/fuzz invariants land before the projector consumes continuation signatures, so the intermediate state is a planner that still emits only depth-zero boundaries. Rollback is reverting the change set; there is no forward-compatibility artifact to unwind.

## Resolved Questions

- **Marker copy: where does the unit name come from?** Resolved by the implemented field set: `MarkdownBlockOmission { reason, scope, unretained }` carries no unit descriptor, so **plan-side copy stays scope-generic** (for example "[One section was too large to render in the preview]"). Naming the unit is the *projector's* job: at the marker's position the projector holds the enclosing container in its carry signature, so it can render "table row", "list item", "quoted paragraph", or "definition" from that context and fall back to the generic wording when the signature is empty (a top-level block). No descriptor is added to the omission value.
