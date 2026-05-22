## 1. Focus Handoff

- [x] 1.1 Add a selected-page-aware editor focus helper near the existing window focus restoration code, with a brief retry path for GTK settling.
- [x] 1.2 Wire the user-facing new document action so newly created untitled documents request focus on their source editor after tab selection.
- [x] 1.3 Ensure startup session restore does not get its active-tab selection overridden by queued new-document focus attempts.
- [x] 1.4 Keep focus handoff from targeting stale tabs when selection changes before the delayed focus attempt runs.

## 2. Ctrl+N Clean Break

- [x] 2.1 Replace the new document runtime shortcut with `Ctrl+N` and remove the `Ctrl+T` binding entirely.
- [x] 2.2 Update command palette metadata so the new document command advertises `Ctrl+N`.
- [x] 2.3 Update `resources/ui/shortcuts.ui`, primary menu/header wording, and README shortcut documentation to advertise `Ctrl+N` and stop advertising `Ctrl+T`.
- [x] 2.4 Search the repository for stale `Ctrl+T`, `New Tab`, or new-document shortcut references and remove or reword any user-facing leftovers.

## 3. Tests and Verification

- [x] 3.1 Add widget coverage showing the new document action focuses the newly created editor when activated from the window action path.
- [x] 3.2 Add widget coverage showing `Ctrl+N` creates and focuses a new document while `Ctrl+T` no longer creates or selects one.
- [x] 3.3 Add command palette coverage for the new document command's displayed shortcut and focus behavior after palette cleanup.
- [x] 3.4 Run `openspec validate fix-new-tab-focus-and-ctrl-n --strict`.
- [x] 3.5 Do not run widget tests on this machine per user instruction; compile the widget test target with `cargo check --workspace --all-targets` instead.
- [x] 3.6 Run `cargo clippy --workspace --all-targets -- -D warnings`.
