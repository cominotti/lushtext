## ADDED Requirements

### Requirement: Replace All crash windows are exercised by the crash smoke
The crash smoke SHALL drive Replace All over a multi-file fixture through automation and kill the process with `SIGKILL` at each of `replace-body-before-entry`, `replace-entry-before-target-rename`, `replace-target-renamed-before-finish` and `durable-renamed-before-dirsync` (Replace All tag). After relaunch it SHALL assert every target is original, or replaced with Undo offered, and that Undo restores it. No orphan body or temp may remain, and the artifacts SHALL record the journal state.

#### Scenario: Killed with the entry armed
- **WHEN** the process is killed at `replace-entry-before-target-rename` and relaunched
- **THEN** that target holds its original bytes and its temp file is removed, and the files written before it are offered for Undo and restored by it
