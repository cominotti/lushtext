## Why

Markdown preview planning packs only *complete* top-level blocks into projection batches, and when one block exceeds a per-slice budget the planner clears that block and `break`s out of the planning loop. One oversized block therefore censors the entire remainder of the document. A Helm chart README stops dead at its "Values" heading because the values table is a single top-level block over the 256-event slice budget; a 5-column table crosses that budget at roughly 15-20 rows, and a bullet list at roughly 40-80 items. Fenced code blocks trip the *byte* budget instead: the pinned `pulldown_cmark` 0.13.4 coalesces a fence body into one `Text` event, so a fence is three events regardless of line count and only a body over 256 KiB truncated the tail. The censored block is usually the content the reader opened the document for, and everything after it disappears with it. Document shape must never decide how much of a document is previewed.

## What Changes

- Let the planner cut a projection batch at any point where the open-container stack holds only block containers, so no batch boundary can split inline state. This generalizes table-row, list-item, code-block-text, blockquote-paragraph, and definition-body boundaries into one checkable invariant and lets a single oversized block be projected over several bounded GTK turns. The code-block-text checkpoint is live for indented blocks (one `Text` event per line) and unreachable for fenced blocks under the pinned parser, which coalesces a fence body into one event; a fence therefore resolves entirely through the existing widget budget (see design Decision 3).
- Give the GTK projector one generation-owned structural continuation carried across turns (open-container frames, list numbering, blockquote depth, block-separator and pending-marker state, generation-scoped footnote numbering, at most one in-flight table or code-block buffer) so a sub-sliced table, list, code block, blockquote, or definition list renders as one continuous visual block rather than visibly split fragments.
- Degrade at the smallest overflowing unit instead of the whole block: a table row, list item, code-block text run, blockquote paragraph, or definition body that cannot be fitted becomes one marker *inside its still-open container*, so its siblings still render. Only a top-level block with no interior checkpoint is replaced wholesale.
- Remove the `break`: the planner emits an accessible omission marker and **keeps planning**. No block shape can truncate the document tail. Cumulative *global* budgets remain terminal, so a document large enough to exhaust one can still stop before its end.
- Retire `MarkdownPlanLimit::TopLevelBlock` and `MarkdownPlanLimit::ProjectionBytes` as terminal plan states and re-express them as per-omission reasons carrying a top-level-block or container-segment scope, with new named retention ceilings — 64 KiB of carried code text (matching the existing code-block widget budget) and a mirrored 1,000-cell ceiling for tables (matching the existing table-cell widget budget) — plus a cap on top-level placeholder widgets.
- Introduce a partially-limited terminal: a document that rendered completely except for N omissions reports "preview complete, N blocks simplified" instead of today's copy implying rendering stopped. **BREAKING** for the internal `MarkdownPlanLimit` variant set, the plan/batch shape, and the widget-test copy assertions; no user action, D-Bus member, persisted format, or accessibility anchor changes.
- Fix batch-local footnote numbering, which today restarts at 1 when a reference and its definition land in different batches, violating the existing mixed-footnote numbering requirement.
- Keep every global budget terminal: source bytes (4 MiB), events (50,000), retained bytes (8 MiB), embed descriptors (256), structural depth (128), and inline-footnote expansion still stop planning. This change removes only the per-slice block-atomicity cliff.
- Keep the existing projection-side widget budgets (`MAX_PREVIEW_TABLE_CELLS` = 1,000 cells, `MAX_PREVIEW_CODE_BLOCK_BYTES` = 64 KiB) exactly as they are, so completeness claims are scoped to them and a block beyond them keeps its current in-place fallback widget.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: require bounded Markdown projection to continue past a block that exceeds one slice, sub-slicing at inline-safe checkpoints with bounded cross-turn continuation state, and reserve terminal limited state for the global budgets. The reproduced requirement narrows "exceeds those budgets" to "exceeds a global budget" and rewords the retained dense-Markdown scenario to name the global event/node budget, because the per-slice case is no longer terminal; both edits are intentional, not an unfaithful reproduction.
- `markdown-preview-tables`: require a table larger than one projection slice to render as one complete continuous table within the table-cell widget budget.
- `markdown-preview-code-blocks`: require a code block within the code-block widget budget to render whole, and a larger one to resolve to the existing in-place fallback with its true size, with the document tail preserved either way.
- `markdown-preview-nested-lists`: require a list larger than one projection slice to render every item with correct hierarchy and numbering, with an overflowing item degrading to a single placeholder item.
- `markdown-preview-blockquotes`: require a blockquote larger than one projection slice to render every paragraph with correct rail depth.
- `markdown-preview-definition-lists`: require a definition list larger than one projection slice to render every title and definition.
- `markdown-preview-inline-footnotes`: require reference and definition numbering to agree when they are projected in different turns.
- `gtk-accessibility-spine`: require omitted-unit markers and the partially-limited terminal to be named and counted for assistive technology instead of appearing as silently missing content.

## Impact

- `crates/lushtext-core/src/services/markdown_render.rs`: checkpoint admissibility, sub-slicing, omission emission and scope, batch continuation signatures, limit/omission types, new named ceilings, unit tests. In mutation scope.
- `crates/lushtext-core/src/ui/markdown_preview/`: new `continuation.rs` sibling for the generation-owned projection continuation; `mod.rs`, `tables.rs`, `code_blocks.rs`, `imp.rs` consume it; scope-aware marker projection; terminal copy and accessible description; carry retirement on generation change.
- `crates/lushtext/tests/widget/markdown_preview.rs`: existing dense-block limited-terminal assertions change; new oversized table/list/code/blockquote/definition-list continuity, segment-omission, and footnote-numbering cases.
- `fuzz/` and the property lane: planner determinism plus new plan invariants (per-batch budgets hold for sub-sliced batches, batch continuations chain, tail is never dropped without a global limit).
- Docs: `docs/workflow-readability-matrix.md` (WFR-MARKDOWN-PREVIEW census; the row stays `deferred`), `docs/accessibility-matrix.md`, `docs/accessibility.md`, `docs/benchmarks/bounded-interactive-pipelines.md` (the "256 complete-block events" description), `README.md` preview limits wording.
- No new crate, dependency, GTK widget type, application action, D-Bus member, readiness predicate, automation snapshot field, accessibility anchor, or persisted format.
