## 1. Line Inspector and whole-line copy

- [ ] 1.1 Add the paragraph index and character checkpoints to `ProtectedTextSource`, built on the worker, and charge them to the live-editor estimate.
- [ ] 1.2 Add `resources/ui/line-inspector.blp` and its generated `.ui` (`make blueprint-generate`, `make check-blueprint`).
- [ ] 1.3 In `inspector_execution.rs`, implement worker window reads with generation guards, Previous/Next Window and Go to Column, through `policy::inspector_window*` and `navigation_target`; hold a readiness blocker while a window read runs.
- [ ] 1.4 Implement the lazy whole-line `GdkContentProvider`, one chunk per GTK dispatch, its clone of the source charged as long-lived ownership.
- [ ] 1.5 Add the Inspect Line action and shortcut to the action catalogue and the command palette; a keyboard invocation defaults to the first folded paragraph in the viewport.
- [ ] 1.6 Protected find results in a hidden tail open the inspector on their window.
- [ ] 1.7 Widget tests: a window in the middle of a 1.9M-character paragraph, navigation, whole-line copy byte identity, chunked writes.

## 2. Menu, gutter and notice

- [ ] 2.1 Replace the context menu with the Protected View popover menu (Inspect Line, Copy Whole Line, Find, insensitive disabled entries).
- [ ] 2.2 Add `fold_gutter.rs` (bounded by visible rows, accessible label with the hidden-character count, activation opens the inspector).
- [ ] 2.3 Add the one bounded Protected View inline alert with Inspect Line and, while available, Undo; merge it with the restored-draft alert; replace C1's reduced-features message for the same document; re-surface it or publish one rate-limited message for pre-empted requests. Copy must not promise "the last keystroke".
- [ ] 2.4 Remaining input-gate tests: class-shortcut enumeration, touch drag scrolls, DnD drop refused, the notice appears once.

## 3. Surfaces

- [ ] 3.1 Local-history preview through the fold executor with a caret-free preview view; no capture for installed-protected documents; captures of recovered and edit-growth documents read the source.
- [ ] 3.2 Markdown preview `Simplified`/limited terminal on a giant paragraph, the file-peek `head_cap` sample bound, read-only note editors, with tests.
- [ ] 3.3 Disable bookmark toggles and line-ending normalization in Protected View; bookmark navigation through `navigation_target`.
- [ ] 3.4 Session restore: scroll through the navigation mark, keep the persisted cursor offset on later session saves, no draft for installed-protected tabs; widget test restoring three protected documents and switching tabs.
- [ ] 3.5 Workspace-search hits and local-history targets navigate through the navigation mark without moving the insert mark.
- [ ] 3.6 The round-trip exactness fact in `LoadMetadata`; Save As of an unmodified protected document whose decode does not round-trip is refused with a notice.

## 4. Accessibility, automation and docs

- [ ] 4.1 Accessible description of the protected view, inspector labels and announcements, gutter labels; the P4 fallback if the lane's AT case requires it.
- [ ] 4.2 `docs/accessibility.md`, `docs/accessibility-matrix.md`, the `docs/accessibility-orca-checklist.md` entry, and the accessibility-smoke inspector keyboard path.
- [ ] 4.3 `line_inspector_visible` in the surface snapshot, the inspector blocker, `scripts/lushtext-automation.py`; `docs/automation.md`, `docs/automation-reference.md`; `make check-automation-docs`, `make automation-client-self-test`.
- [ ] 4.4 Update the `WFR-PROTECTED-VIEW` row (new modules and called presentation surfaces), AGENTS.md, README and `docs/next/long-line-documents.md`.

## 5. Verdict and verification

- [ ] 5.1 Re-run C2's lane cases with the inspector open and navigation and Copy Whole Line added, interleaved against `main`, one run per exclusive hold.
- [ ] 5.2 `make check`, `make test`, the Protected View Kani shards, `make gtk-axioms`, the accessibility, visual and crash-recovery smokes; fix every failure.
