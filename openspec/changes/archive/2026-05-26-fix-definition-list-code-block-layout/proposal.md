## Why

Definition-list support exposed a remaining Markdown preview code-block layout bug: short code lines can still show a horizontal scrollbar when the code block is embedded inside a definition body. The existing top-level code-block width fix sizes anchored code widgets to the whole preview text column, but nested block contexts have their own effective text column and margins.

## What Changes

- Track layout context for embedded Markdown code-block widgets instead of treating every code block as top-level.
- Size nested code blocks to the effective available text column after definition/list/blockquote/alert/footnote context margins are accounted for.
- Preserve the existing behavior that horizontal scrolling appears only for genuinely long code lines.
- Add geometry-focused regression tests for definition-list code blocks, including the screenshot-style sample that currently exposes the scrollbar.
- Keep the fix in the GTK-native Markdown preview renderer with no parser changes and no new runtime dependencies.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `markdown-preview-code-blocks`: Clarifies that code-block width calculation must respect nested Markdown block layout context, not only the root preview text column.

## Impact

- Affected renderer code: `crates/lushtext-core/src/ui/markdown_preview/mod.rs` and, if needed, `crates/lushtext-core/src/ui/markdown_preview/imp.rs`.
- Affected tests: Markdown preview widget tests under `crates/lushtext/tests/widget/markdown_preview.rs`.
- Related active change: `support-markdown-definition-lists`, whose nested-code-block scenario depends on this more precise code-block layout contract.
- No new settings, persisted data, file formats, parser behavior, or runtime dependencies are expected.
