---
title: 'Add Print action to hamburger menu'
type: 'feature'
created: '2026-04-04'
status: 'done'
baseline_commit: 'b59b8bf'
context: []
---

<frozen-after-approval>

## Intent

**Problem:** LushText has no Print functionality — users cannot print the current document from the editor, unlike GNOME Text Editor which provides a standard Print action in its hamburger menu.

**Approach:** Add a "Print…" menu item to the hamburger menu (new section after Save/Discard, before Find and Replace) wired to a `win.print` action. Use `sourceview5::PrintCompositor::from_view()` + `gtk4::PrintOperation` to print the active editor's content with syntax highlighting and editor settings preserved.

## Boundaries & Constraints

**Always:** Use `PrintCompositor::from_view()` to inherit the source view's font, tab width, highlight settings, and wrap mode automatically. Disable the `print` action when no tabs are open (same pattern as `save`, `toggle-search`). Follow `zoom.rs` module pattern for the new helper file.

**Ask First:** Adding a keyboard shortcut for Print (Ctrl+P is already command palette).

**Never:** Custom print preview UI, page setup dialog, or print settings persistence across sessions. No new crate dependencies — `gtk4::PrintOperation` and `sourceview5::PrintCompositor` are already available.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Print active document | Tab open with content | GTK print dialog opens, prints with syntax highlighting | N/A |
| Print empty document | Tab open, empty buffer | GTK print dialog opens, prints blank page | N/A |
| No tabs open | Empty state | Menu item grayed out, action is no-op | N/A |
| Print cancelled | User clicks Cancel in dialog | Returns to editor, no side effects | N/A |
| Print error | Printer unavailable | GTK PrintOperation shows native error | Handled by GTK natively |

</frozen-after-approval>

## Code Map

- `resources/ui/window.ui` — hamburger menu definition (add Print item)
- `crates/lushtext-core/src/ui/window/mod.rs` — action registration, `update_content_stack()` disabled list
- `crates/lushtext-core/src/ui/window/print.rs` (NEW) — print logic following `zoom.rs` module pattern
- `crates/lushtext/tests/widget/window.rs` — widget tests for action state

## Tasks & Acceptance

**Execution:**
- [x] `resources/ui/window.ui` — Add `"_Print…"` menu item with `action="win.print"` in a new `<section>` after Save/Discard (section 4), before Find and Replace (section 5)
- [x] `crates/lushtext-core/src/ui/window/print.rs` (NEW) — Create `pub fn setup_print_action(window: &LushtextWindow)` that registers a `win.print` SimpleAction. On activate: get `active_editor()`, create `PrintCompositor::from_view(&source_view)`, create `PrintOperation`, connect `begin-print`/`paginate`/`draw-page` signals, call `op.run(PrintDialog, Some(&window))`
- [x] `crates/lushtext-core/src/ui/window/mod.rs` — Add `mod print;`, call `print::setup_print_action(&window)` in `new()` after existing setup calls, add `"print"` to the disabled-when-no-tabs action list in `update_content_stack()`
- [x] `crates/lushtext/tests/widget/window.rs` — Add test verifying `print` action is disabled when no tabs are open and enabled after tab creation (follow existing action-state test pattern)

**Acceptance Criteria:**
- Given a document is open, when the user clicks "Print…" in the hamburger menu, then the native GTK print dialog appears
- Given no tabs are open, when the user opens the hamburger menu, then "Print…" is grayed out
- Given the print dialog is shown and user cancels, then the editor returns to normal with no side effects

## Verification

**Commands:**
- `make check` — expected: no clippy warnings, no fmt issues
- `make test-widget` — expected: all widget tests pass including new print action test
- `make build` — expected: clean release build

## Suggested Review Order

- Core implementation: PrintCompositor + PrintOperation wiring, synchronous dialog
  [`print.rs:15`](../../crates/lushtext-core/src/ui/window/print.rs#L15)

- Menu item placement: new section after Save/Discard, before Find and Replace
  [`window.ui:166`](../../resources/ui/window.ui#L166)

- Module registration, setup call in constructor, disabled-when-no-tabs list
  [`mod.rs:11`](../../crates/lushtext-core/src/ui/window/mod.rs#L11)

- Widget tests: action disabled/enabled lifecycle across tab open/close
  [`window.rs:343`](../../crates/lushtext/tests/widget/window.rs#L343)
