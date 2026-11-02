## 1. Line Inspector and whole-line copy

- [x] 1.1 Add the paragraph index and character checkpoints to `ProtectedTextSource`, built on the worker, and charge them to the live-editor estimate.
- [x] 1.2 Add `resources/ui/line-inspector.blp` and its generated `.ui` (`make blueprint-generate`, `make check-blueprint`).
- [x] 1.3 In `inspector_execution.rs`, implement worker window reads with generation guards, Previous/Next Window and Go to Column, through `policy::inspector_window*` and `navigation_target`; hold a readiness blocker while a window read runs.
- [x] 1.4 Implement the lazy whole-line `GdkContentProvider`, one chunk per GTK dispatch, its clone of the source charged as long-lived ownership.
- [x] 1.5 Add the Inspect Line action and shortcut to the action catalogue and the command palette; a keyboard invocation defaults to the first folded paragraph in the viewport.
- [x] 1.6 Protected find results in a hidden tail open the inspector on their window.
- [x] 1.7 Widget tests: a window in the middle of a 1.9M-character paragraph, navigation, whole-line copy byte identity, chunked writes.

## 2. Menu, gutter and notice

- [x] 2.1 Replace the context menu with the Protected View popover menu (Inspect Line, Copy Whole Line, Find, insensitive disabled entries).
- [x] 2.2 Add `fold_gutter.rs` (bounded by visible rows, accessible label with the hidden-character count, activation opens the inspector).
- [x] 2.3 Add the one bounded Protected View inline alert with Inspect Line and, while available, Undo; merge it with the restored-draft alert; replace C1's reduced-features message for the same document; re-surface it or publish one rate-limited message for pre-empted requests. Copy must not promise "the last keystroke".
- [x] 2.4 Remaining input-gate tests: class-shortcut enumeration, touch drag scrolls, DnD drop refused, the notice appears once. *(Applied: the enumeration (found `move-focus` and `text.clear` unclassified, both now classified), no drop target while protected, and the notice published once for ten pastes. A touch drag is pinned by `press_disposition`'s unit tests (a touchscreen drag off a marker stays unclaimed); the widget harness cannot synthesize a touchscreen sequence.)*

## 3. Surfaces

- [x] 3.1 Local-history preview through the fold executor with a caret-free preview view; no capture for installed-protected documents; captures of recovered and edit-growth documents read the source.
- [x] 3.2 Markdown preview `Simplified`/limited terminal on a giant paragraph, the file-peek `head_cap` sample bound, read-only note editors, with tests. *(Applied: read-only folded note editors with a notice; the file-peek sample cuts any line at `HEAD_CAP` (failing first). The Markdown preview's giant-paragraph `Simplified`/limited terminal is the existing behaviour its widget suite already covers (`markdown_preview::test_oversized_*`); no change was needed.)*
- [x] 3.3 Disable bookmark toggles and line-ending normalization in Protected View; bookmark navigation through `navigation_target`.
- [x] 3.4 Session restore: scroll through the navigation mark, keep the persisted cursor offset on later session saves, no draft for installed-protected tabs; widget test restoring three protected documents and switching tabs.
- [x] 3.5 Workspace-search hits and local-history targets navigate through the navigation mark without moving the insert mark.
- [x] 3.6 The round-trip exactness fact in `LoadMetadata`; Save As of an unmodified protected document whose decode does not round-trip is refused with a notice.
- [x] 3.6a *(review S2, S3)* Compute the round-trip fact only for protected profiles, chunked; Save As of an unmodified protected document is a byte-exact copy of its file; the refusal is kept only for an edited document whose decode does not round-trip. (Pre-existing blocker fixed on the way: UTF-16 saves went through encoding_rs, whose output encoding for UTF-16 is UTF-8.)
- [x] 3.7 *(review M1, M3, S1, S1b, S4, S5, S6)* Folded preview and note copies served from their text (Ctrl+C, menu, primary; drag of a folded selection not started); note bodies profiled on a worker and installed in bounded turns; Copy Whole Line copies only the paragraph's bytes and announces the copy; single-line entries cap one insertion at 16 KiB with a notice; the fold gutter keeps a fixed width; the inspector steps between folded lines and states each line's hidden count; notices rate-limited per reason; the accessibility smoke drives Ctrl+Alt+I and Shift+F10.

## 4. Accessibility, automation and docs

- [x] 4.1 Accessible description of the protected view, inspector labels and announcements, gutter labels; the P4 fallback if the lane's AT case requires it.
- [x] 4.2 `docs/accessibility.md`, `docs/accessibility-matrix.md`, the `docs/accessibility-orca-checklist.md` entry, and the accessibility-smoke inspector keyboard path.
- [x] 4.3 `line_inspector_visible` in the surface snapshot, the inspector blocker, `scripts/lushtext-automation.py`; `docs/automation.md`, `docs/automation-reference.md`; `make check-automation-docs`, `make automation-client-self-test`.
- [x] 4.4 Update the `WFR-PROTECTED-VIEW` row (new modules and called presentation surfaces), AGENTS.md, README and `docs/next/long-line-documents.md`.

## 5. Verdict and verification

- [x] 5.1 Re-run C2's lane cases with the inspector open and navigation and Copy Whole Line added, interleaved against `main`, one run per exclusive hold. *(Applied 2026-11-01: `docs/next/long-line-documents.md`, "C2b … Verdict": paging ≤ 7.8 ms, Copy Whole Line ≤ 6.3 ms, C2's cases on par with `main`; the AdwDialog open/close transition under the cairo renderer, 24–32 ms per frame, is a recorded toolkit-floor exception with its perf stack.)*
- [x] 5.1a Add the lane cases C2 could not (its task 9.3): Save As of a protected document (needs a measurement-build hook for the file chooser) and a live word-wrap toggle (needs a runtime settings write in the lane), with driver self-tests. *(Deferred to C3 `sliced-protected-install`, recorded in the programme record: both need a measurement-build hook the lane lacks; Save As's worker write from the source is pinned by C2's widget tests.)*
- [ ] 5.2 `make check`, `make test`, the Protected View Kani shards, `make gtk-axioms`, the accessibility, visual and crash-recovery smokes; fix every failure.
