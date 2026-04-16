## 1. Contract And Documentation

- [x] 1.1 Update `docs/next/session-time-travel.md` so empty local-history snapshots are described as valid empty historical states, not broken previews.
- [x] 1.2 Keep the local-history browser documentation and product copy aligned with the distinction between no snapshots, empty snapshots, and preview failures.
- [x] 1.3 Extend the change docs/specs so draft-restored files do not surface fresh stale-disk baseline entries as normal local-history rows.

## 2. Empty Snapshot Browser UX

- [x] 2.1 Add a dedicated empty-snapshot preview state in `crates/lushtext-core/src/ui/window/local_history.rs` so selecting an empty snapshot shows explanatory copy instead of a blank preview pane.
- [x] 2.2 Update local-history row and selected-snapshot metadata so empty snapshots are described semantically instead of only as `0 B`.
- [x] 2.3 Keep restore available for empty snapshots while adjusting copy affordance and preview-state behavior to match the lack of text content.
- [x] 2.4 Adjust the draft-restore/local-history interaction so reopening a file with restored unsaved work does not mint a fresh baseline row for stale on-disk contents.

## 3. Verification

- [x] 3.1 Add widget-test coverage for empty snapshot metadata, explanatory preview rendering, and action availability in the local-history browser.
- [x] 3.2 Re-run the targeted local-history widget coverage so the empty-snapshot UX, existing no-history empty state, and restore flow still behave correctly together.
- [x] 3.3 Add coverage for the draft-restored local-history timeline so stale-disk baseline entries do not reappear as fresh rows.
- [x] 3.4 Re-run the focused local-history and draft-restore widget coverage after the timeline behavior changes.
