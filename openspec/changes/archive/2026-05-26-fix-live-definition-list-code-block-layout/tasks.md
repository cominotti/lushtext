## 1. Live-Shell Regression Coverage

- [x] 1.1 Add a window-level widget test that opens Markdown Preview in preview-only mode for the screenshot-style pulldown-cmark definition-list sample and reproduces the tiny clipped code-block allocation before the fix.
- [x] 1.2 In that preview-only regression, assert final allocated code-block geometry after the preview animation settles, including block bounds, scroller bounds, source-view visibility, and horizontal adjustment overflow.
- [x] 1.3 Add a side-by-side preview-pane regression for a definition-list code block rendered from hidden preview state, asserting the block settles to the pane's effective definition-body code column without false horizontal overflow.
- [x] 1.4 Keep or strengthen standalone `LushtextMarkdownPreview` tests only as primitive coverage, and ensure they no longer substitute for the live-shell acceptance tests.
- [x] 1.5 Add or reuse test helpers that compute expected nested code-block width from the real preview `GtkTextView` allocation, text margins, captured nested block margins, and code-block padding, not from width request alone.

## 2. Embedded Code-Block Layout Repair

- [x] 2.1 Trace the live preview-only failure enough to identify whether stale geometry lives on the child anchor, outer code-block container, inner scroller, source view, or missed shell refresh timing.
- [x] 2.2 Update `LushtextMarkdownPreview` so embedded code-block layout refresh recomputes from the current visible text column and captured Markdown block context after late allocations.
- [x] 2.3 If width request alone is insufficient, queue the necessary GTK relayout on the text view or embedded widgets and propagate the computed viewport width to the inner scroller/source-view layer.
- [x] 2.4 Expose a narrow internal refresh hook if needed so `LushtextWindow` preview transitions can ask the preview to refresh embedded layouts without rerendering Markdown.
- [x] 2.5 Wire preview-only and side-by-side animation completion, preview visibility changes, or paned-position changes to the embedded layout refresh path so hidden-to-visible transitions cannot leave stale natural-width code blocks.
- [x] 2.6 Preserve true horizontal scrolling for genuinely long top-level and nested code lines.

## 3. Spec and Documentation Alignment

- [x] 3.1 Update any affected comments or developer guidance that still imply standalone preview widget tests prove live preview-shell layout correctness.
- [x] 3.2 Ensure the older active `fix-definition-list-code-block-layout` artifacts are not treated as implementation acceptance evidence for this live-shell bug.

## 4. Verification

- [x] 4.1 Run the new focused window-level preview-only and side-by-side regression tests.
- [x] 4.2 Run the focused Markdown preview widget tests for code blocks and definition-list code blocks.
- [x] 4.3 Run the full Markdown preview widget module with `./scripts/run-widget-tests.sh -- markdown_preview::`.
- [x] 4.4 Run relevant window widget tests for Markdown preview actions and preview animations.
- [x] 4.5 Run `cargo fmt --all -- --check`.
- [x] 4.6 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 4.7 Run `openspec validate fix-live-definition-list-code-block-layout --strict`.
- [x] 4.8 Run `openspec validate --all --strict`.
- [x] 4.9 Verify with a fresh `make run` that the screenshot-style definition-list sample no longer shows a tiny clipped code block or false horizontal scrollbar in the live app.
