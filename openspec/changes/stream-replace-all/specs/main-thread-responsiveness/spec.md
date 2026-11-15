## ADDED Requirements

### Requirement: Replace All presentation stays within the interaction budgets
The Replace All completion terminal, opening the Skipped Files dialog with 16,384 rows, and the dialog's first frame SHALL each finish within `B_SETTLE_US`, with no exception recorded. Each progress heartbeat and each Cancel Replace All activation SHALL finish within `B_INTERACT_US`. No GTK dispatch SHALL acquire the journal guard or a target write guard; a run holding them delays only other workers.

#### Scenario: Completion of a run that skipped 16,384 files
- **WHEN** a Replace All run completes with 16,384 skipped files
- **THEN** its completion dispatch and the dispatch that opens the Skipped Files dialog each stay within `B_SETTLE_US`

#### Scenario: Saving a file while Replace All rewrites it
- **WHEN** the user saves an unrelated file, and then the file being rewritten, while Replace All streams a 512 MiB file
- **THEN** typing and both saves' GTK dispatches stay within their budgets
- **AND** the unrelated save completes at once, and only the save of the file being rewritten waits, on its worker, for that file's guard
