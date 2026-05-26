## Why

The completed definition-list code-block layout change still fails in the real app: the screenshot-style sample renders the nested code block as a tiny natural-width surface with horizontal scrolling after opening Markdown Preview. The previous spec and tests covered a standalone preview widget, but missed the live `LushtextWindow` preview shell where the preview starts hidden, animates through `GtkPaned`, and receives allocation after rendering.

## What Changes

- Treat the existing `fix-definition-list-code-block-layout` result as incomplete for live app behavior.
- Require embedded Markdown code blocks to settle to their effective text column after preview-only and side-by-side preview transitions from hidden state.
- Strengthen regression coverage from standalone `LushtextMarkdownPreview` tests to window-level tests that exercise the real `LushtextWindow` preview lifecycle.
- Verify the screenshot-style definition-list sample in the actual preview shell, including allocation bounds and horizontal-scroll adjustment state.
- Preserve the intended behavior that horizontal scrolling appears only for genuinely long code lines.
- Keep the fix in the GTK-native Markdown preview and window preview integration, with no parser changes and no new runtime dependencies.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `markdown-preview-code-blocks`: Embedded code-block sizing must remain correct after the real preview shell transitions from hidden to visible, including code blocks nested in pulldown-cmark definition-list definitions.

## Impact

- Affected renderer code: `crates/lushtext-core/src/ui/markdown_preview/mod.rs` and `crates/lushtext-core/src/ui/markdown_preview/imp.rs`.
- Affected preview-shell code: `crates/lushtext-core/src/ui/window/preview.rs` and related window preview wiring if the final fix needs a shell-level refresh hook.
- Affected tests: Markdown preview widget tests and window widget tests under `crates/lushtext/tests/widget/`.
- Related active changes: `support-markdown-definition-lists` and `fix-definition-list-code-block-layout` remain useful context but their completed task lists do not prove the live-shell behavior.
- No settings, persisted data, file formats, parser behavior, or runtime dependencies are expected to change.
