## 1. Heading Renderer Contract

- [x] 1.1 Replace the loose heading smoke tests with ATX H1-H6 coverage that verifies each rendered heading text has its matching `heading1` through `heading6` tag.
- [x] 1.2 Add Setext H1 and H2 widget coverage that verifies `===` maps to `heading1` and `---` maps to `heading2`.
- [x] 1.3 Assert rendered heading buffers do not include raw ATX marker prefixes or Setext underline marker lines as document text.
- [x] 1.4 Add flow coverage proving paragraphs before, between, and after headings remain in source order.
- [x] 1.5 Strengthen rendered-preview heading styles so the visual hierarchy is materially visible, not only tag-detectable.
- [x] 1.6 Scale the bundled source-editor heading style so Markdown heading lines stand out while raw syntax remains editable.

## 2. Preview Discoverability

- [x] 2.1 Add a visible primary-menu item labeled `Markdown Preview` bound to the existing `win.toggle-preview-mode` action.
- [x] 2.2 Preserve the existing `Alt+P` shortcut and preview-only state behavior while adding the menu entry.
- [x] 2.3 Add widget coverage that verifies the primary menu exposes the Markdown Preview action.
- [x] 2.4 Add window-level coverage that activating the visible preview action renders the active Markdown buffer and keeps non-Markdown files on the placeholder path.

## 3. Documentation And Spec Hygiene

- [x] 3.1 Update Markdown preview docs or follow-up notes if they mention hidden-only preview activation or omit mandatory heading support.
- [x] 3.2 Keep source-editor heading support to syntax styling only: raw markers stay visible and rendered preview remains explicit.

## 4. Verification

- [x] 4.1 Run `openspec validate require-markdown-heading-preview --strict`.
- [x] 4.2 Run focused Markdown preview widget tests.
- [x] 4.3 Run focused window/widget tests for the primary-menu preview action.
- [x] 4.4 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 4.5 Run `./scripts/run-widget-tests.sh --auto`.
