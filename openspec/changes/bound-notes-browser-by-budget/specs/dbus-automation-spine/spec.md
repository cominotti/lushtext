## ADDED Requirements

### Requirement: Automation snapshots report the Notes browser's listed and found rows
The automation snapshot SHALL report, under `notes`, whether a Notes browser is open (`browser_open`), how many rows it lists (`browser_row_count`), how many rows its source construction found (`browser_rows_found`, saturating at `u32::MAX`), and whether a byte limit cut its source (`browser_limited`). The values SHALL be projected from the notes workflow's evidence surface. They SHALL carry no note text, bookmark label, or path. When no browser is open, the counts SHALL be zero and `browser_limited` false. The fields SHALL be documented in `docs/automation.md` and `docs/automation-reference.md`.

#### Scenario: A limited browser is open
- **WHEN** a Notes browser whose source was cut by a byte limit is open
- **THEN** the snapshot reports `browser_open` true, `browser_limited` true, and `browser_row_count` less than `browser_rows_found`

#### Scenario: No browser is open
- **WHEN** no Notes browser is open
- **THEN** the snapshot reports `browser_open` false, both counts zero, and `browser_limited` false
