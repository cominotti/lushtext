## ADDED Requirements

### Requirement: Replace All crash windows are exercised by the crash smoke
The crash smoke SHALL drive Replace All over a multi-file fixture through automation and stop the process at each Replace All kill point (`replace-body-before-entry`, `replace-entry-before-target-rename`, `replace-target-renamed-before-finish`, `durable-renamed-before-dirsync@replace`, and `replace-undo-before-target-rename` in a later Undo). After relaunch each target SHALL be original, or replaced with an Undo that restores it, with no orphan body or temp left.

#### Scenario: Killed with the entry armed
- **WHEN** the process is killed at `replace-entry-before-target-rename` and relaunched
- **THEN** that target holds its original bytes and its temp file is removed, and the files written before it are offered for Undo and restored by it
- **AND** the smoke's artifacts record the journal state before and after the relaunch

### Requirement: Replace All kill points land inside their windows
A kill point SHALL accept an occurrence ordinal, so that a kill fires on a later file of the fixture, after the earlier files were replaced. The `replace-entry-before-target-rename` and `replace-undo-before-target-rename` points SHALL sit after the target's temp file holds its bytes, so that recovery has a temp file to remove.

#### Scenario: Killed on the third of five files
- **WHEN** the smoke sets `LUSHTEXT_KILL_AT=replace-entry-before-target-rename@replace#3` and confirms a Replace All over five files
- **THEN** the process stops with the first two files replaced and a written temp beside the third, which keeps its original bytes
