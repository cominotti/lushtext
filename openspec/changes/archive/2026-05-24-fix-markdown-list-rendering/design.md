## Context

The Markdown preview stays GTK-native by streaming `pulldown-cmark` events into a read-only `GtkTextBuffer` and styling ranges with `GtkTextTag`s. That architecture is still the right fit, but list rendering currently uses top-level block spacing state for list-internal layout. In practice this lets a pending ordered marker flush before a paragraph separator, which can leave `2.` or `3.` alone on one row, and it lets a nested list begin after parent prose instead of on the next rendered row.

The existing `markdown-preview-nested-lists` spec requires readable hierarchy and marker sequencing, but the current widget tests mostly assert that marker text and depth tags exist. This change needs to verify the visible `GtkTextView` layout as well as the buffer text so the fix addresses what users see in the preview.

## Goals / Non-Goals

**Goals:**

- Render tight ordered, unordered, mixed, nested, and task lists with correct row flow in the native preview.
- Preserve intentional loose-list paragraph spacing without duplicate blank rows caused by paragraph and item end events both inserting separators.
- Keep ordered markers and item text visually attached, including offset and multi-digit markers.
- Give wrapped list items a hanging-indent layout so continuation lines align with item text rather than marker glyphs.
- Add regression coverage that presents the preview widget and checks real `GtkTextView` geometry for the failure modes shown in the screenshot.

**Non-Goals:**

- Replacing the native preview with WebKit, HTML rendering, or embedded per-list widgets.
- Attempting full browser/GitHub Markdown parity beyond supported CommonMark/GFM list constructs.
- Changing the editable Markdown source view.
- Adding dependencies, persisted settings, or a second Markdown parser.

## Decisions

### 1. Replace blunt list spacing with list-aware render state

The renderer should keep explicit list-item layout state instead of using only `needs_block_separator` for every block. At minimum, it needs to know:

- current list depth and ordered counter,
- whether the current item has emitted visible content,
- whether an item paragraph just ended,
- whether the next block is nested under an item and therefore must start on a new line,
- whether a pending marker has already been flushed for the current item.

This keeps ordinary top-level block spacing independent from list-internal spacing. A paragraph ending inside an item should not automatically cause the next item's marker to render on a separate row from the next paragraph's text.

Alternative considered: add local one-off checks around `TagEnd::Paragraph`, `TagEnd::Item`, and `Tag::List`. That is tempting, but it keeps the current hidden coupling and makes loose-list and nested-list interactions easy to break again.

### 2. Treat list boundaries as line-layout events, not paragraph separators

List starts inside a non-empty item should force exactly one newline before the first child marker. Item ends should ensure the item is terminated, but they should not blindly append a second newline after a paragraph already ended. List ends should restore the parent context without inserting extra blank rows before the next sibling item.

This should produce these shapes in the buffer and the visible preview:

```text
• Parent text
  • Child text
• Next parent

1. First
2. Second
3. Third
```

Loose-list paragraphs can still contain intentional separation inside an item, but that spacing should come from the loose paragraph boundary itself, not from duplicated item-boundary handling.

Alternative considered: normalize all list output after rendering by trimming repeated newlines. That would hide the symptom in buffer text but would be brittle around code blocks, images, tables, footnotes, and blockquotes inside list items.

### 3. Use TextTag hanging indents for actual list layout

Depth-specific list tags should continue to own indentation, but they should also express a hanging indent for wrapped items. In GTK terms, each list-depth tag can set a left margin for continuation lines and a negative first-line indent for the marker slot. The literal marker remains in the buffer, but wrapped text aligns under the item text rather than under the bullet or number.

The marker slot must be wide enough for common ordered markers such as `57.` and `100.`. If implementation needs separate marker-width classes for very large ordered counters, those tags should still be generated inside the preview widget and not leak into model or service layers.

Alternative considered: insert literal spaces before child items and continuation lines. That would be font-dependent and would fail under proportional fonts, zoom, themes, and narrow preview widths.

### 4. Keep task-list marker override on the same path

Task-list events arrive after `Tag::Item`, so the renderer still needs delayed marker insertion. The list-aware state should keep that behavior, replacing the default bullet or number with the checked/unchecked task marker without changing the surrounding item flow.

Alternative considered: special-case task lists as their own block renderer. That would duplicate list spacing logic and make task-list regressions more likely.

### 5. Verify actual rendering, not only parser output

Regression tests must include two levels:

- buffer-shape assertions for exact newline patterns around tight lists, loose lists, and nested list starts,
- presented-widget geometry assertions using the preview `GtkTextView`, such as checking marker and first item text share the same rendered line and wrapped continuation text starts to the right of the marker slot.

The existing widget harness already presents preview widgets and can query `TextView::iter_location` for rendered text offsets. Extending that style keeps the verification local and avoids adding screenshot infrastructure for this narrow renderer bug.

Alternative considered: rely on screenshot tests only. Screenshots are useful for manual confirmation, but geometry assertions are more deterministic and easier to run in the existing widget-test harness.

## Risks / Trade-offs

- [List state grows more complex] -> Keep the state private to `ui/markdown_preview` and cover edge cases with focused widget tests.
- [Loose-list handling can regress code blocks or other nested block content] -> Include nested-block test cases and keep top-level block spacing separate from list-internal state.
- [Hanging-indent values can look wrong across fonts or themes] -> Use stable pixel margins already used by the preview, measure behavior through GTK geometry, and avoid literal whitespace indentation.
- [Task-list marker delay can interact badly with new item state] -> Keep task-list marker replacement in the same delayed-prefix mechanism and add task-list layout regression coverage.
- [Tests can pass on buffer text while the UI is still wrong] -> Require presented-widget geometry checks before the task is considered complete.

## Migration Plan

No data migration is required. The change is isolated to Markdown preview rendering, preview tag styling, and tests. Rollback is low risk because source files, settings, and persisted user data are unchanged; reverting restores the previous preview behavior only.

## Open Questions

None currently. The implementation should preserve the native preview strategy and tune exact indent constants through widget-test evidence rather than introducing a new rendering surface.
