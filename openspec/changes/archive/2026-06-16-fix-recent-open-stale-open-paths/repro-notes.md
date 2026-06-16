# Repro Notes

## Live Session

- Date: 2026-06-16
- Process: `/var/home/danilo/Workspace/github/cominotti/lushtext/target/debug/lushtext` pid `3130336`
- Data file: `~/.local/share/lushtext/recent-documents.json`

Observed with `scripts/lushtext-automation.py snapshot`, `scripts/lushtext-automation.py action win.open-recent`, and `scripts/lushtext-automation.py wait visual-geometry-settled`:

- `tab_count` was `0`.
- `surfaces.open_popover_visible` was `true` after opening the popover.
- Persisted recent-document data contained three existing file rows.
- The Open popover reported the recent-list surface with no visible allocation and the empty-state surface visible.

That state proves the failure is not missing persisted data. The UI was filtering eligible rows after all tabs had already been closed in the same running application.

## Fixed-Code Proof

- `open_popover::test_open_popover_startup_loaded_recents_visible_with_no_tabs` seeds persisted recents and proves a fresh no-tab window shows rows.
- `open_popover::test_open_popover_same_session_file_chooser_close_reveals_recent` opens through the production chooser path, closes the tab, and proves the row reappears without restart.
- `open_popover::test_open_popover_ignores_stale_display_and_canonical_open_identities` directly covers stale display and canonical identities.
- `app::test_open_activation_close_updates_recent_popover_automation_snapshot` drives application activation and validates the automation snapshot shows the list instead of the empty state after close.
