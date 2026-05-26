## Why

LushText's native Markdown preview currently mis-renders common list documents: nested list blocks can continue on the parent item's line, loose-list paragraph handling can introduce extra blank rows, and ordered list markers can become visually separated from their item text. This needs a spec-backed fix because the current tests prove that list markers exist, but not that the GTK preview actually lays lists out like readable rendered Markdown.

## What Changes

- Tighten Markdown preview list rendering so ordered, unordered, nested, mixed, and task-list items keep correct line flow in the actual rendered GTK preview.
- Ensure nested lists always begin on their own rendered line beneath the parent item instead of being appended after parent prose.
- Ensure tight lists render with one item per row and no unintended blank rows between items.
- Ensure loose lists preserve intentional paragraph separation inside items without adding duplicate empty rows around item boundaries.
- Ensure ordered-list markers, including multi-digit and offset markers such as `57.`, remain visually attached to their item text and wrap with a hanging-indent layout rather than leaving a number alone on one row.
- Add regression coverage that inspects both rendered buffer text and real widget layout behavior so the implementation cannot pass while the visible preview remains wrong.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `markdown-preview-nested-lists`: Clarify and extend the list preview contract to cover actual GTK rendered line flow, spacing, ordered marker attachment, nested-list starts, and wrap alignment for CommonMark and task-list cases.

## Impact

- Affected renderer code: `crates/lushtext-core/src/ui/markdown_preview/mod.rs` and `crates/lushtext-core/src/ui/markdown_preview/imp.rs`.
- Affected UI resource: `resources/ui/markdown-preview.ui` if the final layout needs TextView wrapping or margin adjustments.
- Affected tests: Markdown preview widget tests in `crates/lushtext/tests/widget/markdown_preview.rs`, including layout-level checks that present the widget at constrained widths.
- No new runtime dependencies, data migration, public API changes, or persisted setting changes are expected.
