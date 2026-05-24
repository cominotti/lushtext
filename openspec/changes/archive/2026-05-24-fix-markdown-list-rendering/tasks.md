## 1. Reproduce And Lock Visible Failures

- [x] 1.1 Add focused Markdown preview buffer-shape tests for tight ordered lists, tight unordered lists, nested lists after parent prose, loose list paragraphs, and task lists so newline regressions are visible before implementation.
- [x] 1.2 Add presented-widget geometry helpers for Markdown preview tests that can compare rendered line `y` positions and wrapped-line `x` positions using `GtkTextView::iter_location`.
- [x] 1.3 Add geometry regression tests proving ordered markers such as `2.`, `3.`, and `57.` share the same rendered row as their first item text at normal preview widths.
- [x] 1.4 Add geometry regression tests proving long ordered and nested unordered items wrap with continuation text aligned under item text rather than under the marker column.

## 2. List Render State

- [x] 2.1 Replace the current list use of global `needs_block_separator` with list-aware render state that tracks depth, item content, paragraph endings, pending marker state, and ordered counters.
- [x] 2.2 Ensure nested list starts inside an item force exactly one new rendered row before the child marker without adding extra blank rows.
- [x] 2.3 Ensure item endings terminate rows without duplicating newlines already emitted by paragraph endings.
- [x] 2.4 Preserve loose-list paragraph breaks inside a list item while preventing duplicated empty rows before the next item.
- [x] 2.5 Keep task-list marker replacement on the delayed-prefix path so checked and unchecked markers follow the same row-flow rules as ordinary list items.

## 3. GTK Layout Styling

- [x] 3.1 Update depth-specific list `TextTag` creation to support hanging indents for wrapped list items, using GTK margin and indent properties rather than literal whitespace.
- [x] 3.2 Tune marker-slot and depth-step constants so common ordered markers, including offset and multi-digit markers, remain visually attached to item text.
- [x] 3.3 Verify the preview `GtkTextView` wrap mode and margins still produce readable list wrapping in normal preview-only and side-by-side widths.

## 4. Regression And Integration Coverage

- [x] 4.1 Extend Markdown preview widget tests to assert exact rendered text ordering and absence of unintended `\n\n` patterns for tight lists.
- [x] 4.2 Extend Markdown preview widget tests to assert rendered geometry for marker/text attachment and hanging-indent continuation alignment.
- [x] 4.3 Add or update sample Markdown content under `samples/` if the canonical preview showcase includes list examples affected by this fix.
- [x] 4.4 Review `AGENTS.md`, nested guidance, and docs snippets for any Markdown preview list-rendering wording that needs to mention the tightened behavior.

## 5. Verification

- [x] 5.1 Run `openspec validate fix-markdown-list-rendering --strict`.
- [x] 5.2 Run the targeted Markdown preview widget tests through `scripts/run-widget-tests.sh` using the harness filter for `markdown_preview`.
- [x] 5.3 Run `cargo test --workspace --lib --test integration`.
- [x] 5.4 Run `cargo fmt --check`.
- [x] 5.5 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 5.6 Manually inspect the rendered preview with the screenshot's list sample in a real app window or equivalent GTK session and confirm no marker-alone rows, unintended blank rows, or inline child-list starts remain.
