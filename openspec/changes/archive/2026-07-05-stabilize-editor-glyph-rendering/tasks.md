## 1. Re-evaluate Previous Attempt

- [x] 1.1 Remove the stale broad plan for Automation1, minimap, visual-geometry policy, repaint repair, and style-scheme text-color changes.
- [x] 1.2 Keep only the resource rebuild correction that was independently useful for `make run` freshness.

## 2. Fix Active-Line Editor Metrics

- [x] 2.1 Disable `GtkTextView:monospace` for the main editor template.
- [x] 2.2 Apply the selected editor font directly to `tab-content-editor-surface` so the editor remains monospaced.
- [x] 2.3 Add an editor-only `line-height: 1.18` top-ink guard and keep other `.monospace` surfaces on their existing metrics.

## 3. Add Live Regression Coverage

- [x] 3.1 Add `scripts/editor-glyph-live-smoke.py` to type real `[` key events into an isolated fresh document and analyze screenshot pixels before Enter.
- [x] 3.2 Add `make editor-glyph-live-smoke` and include it in `end-user-smoke`.
- [x] 3.3 Add the GitHub Actions end-user smoke lane and workflow drift checker entry.

## 4. Verification

- [x] 4.1 Run the smoke against the pre-fix shape and confirm it fails on missing active-line top ink.
- [x] 4.2 Run the smoke after the metric fix and confirm the before-Enter active line passes.
- [x] 4.3 Run `make check-blueprint`, `cargo fmt`, `openspec validate stabilize-editor-glyph-rendering --type change --strict`, `make check-end-user-smoke-workflow`, `make editor-glyph-live-smoke`, and `git diff --check`.
