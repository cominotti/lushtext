## 1. Empty Notes Dialog Sizing

- [x] 1.1 Update the empty Notes browser dialog so it honors its intended compact content size instead of following the status page's natural size.
- [x] 1.2 Ensure the empty Notes content box and status page expand inside the dialog so the title and description receive a readable line length.
- [x] 1.3 Preserve the existing empty-state copy, close button, Escape dismissal, and no-fake-sidebar/no-created-data behavior.
- [x] 1.4 Inspect the Local History empty-dialog sizing pattern and either align confirmed affected behavior or leave it unchanged with a brief rationale in code or tests.
- [x] 1.5 Size the empty Notes browser so the status-page content fits without a vertical scrollbar.

## 2. Widget Coverage

- [x] 2.1 Add or update empty Notes browser coverage to assert the dialog does not follow child content size.
- [x] 2.2 Add or update empty Notes browser coverage to assert the settled rendered allocation is wide and tall enough for a readable compact browser state.
- [x] 2.3 Keep coverage proving the no-workspace empty state shows `No notes yet` and does not materialize an `AdwSidebar` or fake rows.
- [x] 2.4 Keep close-button and Escape dismissal coverage passing for the empty Notes browser.
- [x] 2.5 Add coverage proving the empty Notes browser does not introduce vertical scroll overflow.

## 3. Validation

- [x] 3.1 Run `cargo fmt --all`.
- [x] 3.2 Run focused widget tests for empty Notes browser and Notes browser states.
- [x] 3.3 Run `openspec validate fix-empty-notes-browser-sizing --strict`.
- [x] 3.4 Run `git diff --check`.
