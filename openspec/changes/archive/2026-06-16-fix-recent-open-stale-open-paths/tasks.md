## 1. Reproduce And Isolate

- [x] 1.1 Add a failing widget regression that seeds persisted recents, starts a window with no tabs, and proves the Open popover shows rows from disk.
- [x] 1.2 Add a same-session regression that opens a file through the production workflow, closes it, and proves the row reappears without restart.
- [x] 1.3 Add a stale-open-identity regression seam/test proving stale display and canonical identities cannot hide rows after tab close.
- [x] 1.4 Capture live or automation repro notes with `scripts/lushtext-automation.py` for zero tabs plus persisted recents.

## 2. Fix Recent Visibility Source Of Truth

- [x] 2.1 Add a helper that derives current open document identities from mounted `AdwTabView` editor pages, ignoring failed, cancelled, cleared, or detached pages.
- [x] 2.2 Use tab-derived identities when rebuilding Open popover visible rows instead of trusting long-lived `open_paths`.
- [x] 2.3 Reconcile or scrub `open_paths` after close/detach, Save As, rename/delete, failed/cancelled load, session restore, and canonical refresh paths so duplicate detection stays healthy.
- [x] 2.4 Preserve existing duplicate-safe open behavior and the rule that session-restore opens do not create recent-document entries.

## 3. Regression Matrix

- [x] 3.1 Add or extend service tests for visible-row filtering with stale display paths, stale canonical paths, symlink/canonical duplicates, missing files, and mixed open/closed row sets.
- [x] 3.2 Add or extend window/widget tests for startup persisted recents, same-session open/close, close while popover visible, open while popover visible, bulk close, close-tab-for-path, Save As, sidebar rename/delete, failed load, cancelled load, session restore, and canonical refresh after close.
- [x] 3.3 Add or extend keyboard tests for `Ctrl+K`, `Enter`, `Up`/`Down`, `Escape`, search reset after stale-state repair, and chooser reachability.
- [x] 3.4 Add or extend accessibility assertions for the Open button, search box, chooser row, recent list, row remove buttons, and empty state across empty, populated, and constrained states.
- [x] 3.5 Add or extend an automation/D-Bus regression using `scripts/lushtext-automation.py` or exported actions to prove real action paths update recents and snapshots show the list instead of the empty state.
- [x] 3.6 Add or extend visual geometry proof for no eligible rows, one row, representative rows, dense rows, awkward labels, all rows open, all rows closed, and 720p constrained geometry.

## 4. Verification

- [x] 4.1 Run targeted unit/service tests for `recent_documents`.
- [x] 4.2 Run focused widget tests for Open popover and affected window close/path workflows.
- [x] 4.3 Run `make test-widget-headless`.
- [x] 4.4 Run focused `cargo-gtk-proof` Open popover visual proof and `make visual-geometry-smoke` if proof artifacts or geometry expectations change.
- [x] 4.5 Run `make check-automation-docs` if action, snapshot, or automation-client contracts change.
- [x] 4.6 Run `openspec validate fix-recent-open-stale-open-paths --strict`, `openspec validate --specs --strict`, `git diff --check`, and `make pre-commit`.
