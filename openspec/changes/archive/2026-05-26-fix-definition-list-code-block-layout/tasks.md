## 1. Regression Coverage

- [x] 1.1 Add a Markdown preview widget regression test using the screenshot-style definition-list sample with `{ some code, part of Definition 2 }`, asserting no false horizontal overflow when the line fits the effective definition body column.
- [x] 1.2 Strengthen the nested definition-list code-block test to assert actual allocated geometry, including code-block/scroller bounds, instead of relying only on `width_request()` and source-buffer text.
- [x] 1.3 Add a late-allocation regression test where a definition-list code block is rendered before the preview receives width allocation, then assert the nested block recomputes to the effective context width.
- [x] 1.4 Add a nested long-line regression test proving horizontal scrolling still appears when a definition-list code line is genuinely wider than the effective definition body column.

## 2. Renderer Layout Fix

- [x] 2.1 Replace the preview's bare embedded-widget registry with metadata that stores each embedded widget and its captured block layout context.
- [x] 2.2 Add a helper for deriving embedded block horizontal margins from active Markdown render state, including definition bodies and existing nested contexts such as lists, blockquotes, alert bodies, and footnote definitions where applicable.
- [x] 2.3 Update code-block insertion so nested code-block containers receive the captured visual context offset while their width request is computed from `preview_text_column_width - context_start - context_end`.
- [x] 2.4 Update code-block width refresh so every embedded code block recomputes from its own captured layout context on render, map, allocation, and readable-column changes.
- [x] 2.5 Preserve rerender cleanup behavior so embedded-widget metadata is cleared together with the anchored GTK widgets.

## 3. Verification

- [x] 3.1 Run the focused Markdown preview widget tests for definition-list code-block layout.
- [x] 3.2 Run the full Markdown preview widget test module with `./scripts/run-widget-tests.sh -- markdown_preview::`.
- [x] 3.3 Run `cargo fmt --all -- --check`.
- [x] 3.4 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 3.5 Run `openspec validate fix-definition-list-code-block-layout --strict`.
