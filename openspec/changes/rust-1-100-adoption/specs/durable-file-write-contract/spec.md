## ADDED Requirements

### Requirement: A no-replace rename falls back only when the rename step is unsupported
The durable no-replace rename SHALL classify its failures as before-rename (the rename did not take effect) or after-rename (it took effect, and a parent-directory sync failed). The best-effort fallback (an existence check plus a durable rename) SHALL run only when the rename step itself reported that the platform lacks `RENAME_NOREPLACE`. The fallback's rename SHALL be classified the same way.

#### Scenario: A directory sync that reports Unsupported after the rename is not a name collision
- **WHEN** the no-replace rename succeeds and the following parent-directory sync fails with an error of kind `Unsupported`
- **THEN** the call returns an after-rename failure carrying that error
- **AND** it does not return `AlreadyExists`, and the file is at the destination with the source name gone

#### Scenario: An unsupported rename flag still falls back
- **WHEN** the rename step fails because the kernel or filesystem does not implement `RENAME_NOREPLACE`
- **THEN** the call falls back to the existence check plus a durable rename
- **AND** an existing destination is refused with `AlreadyExists` and left untouched

#### Scenario: The fallback's post-rename sync failure is classified
- **WHEN** the fallback's durable rename succeeds and its parent-directory sync fails
- **THEN** the call returns an after-rename failure

### Requirement: Callers treat an after-rename rename failure as a completed rename
Every caller of the durable no-replace rename SHALL treat an after-rename failure as a rename that happened and SHALL report that its durability is unconfirmed. The workspace sidebar SHALL project it exactly like a successful rename and add a warning chosen by pure sidebar policy. Draft set-aside SHALL count the body as placed and log the unconfirmed durability.

#### Scenario: A sidebar rename whose directory sync fails updates open tabs
- **WHEN** a sidebar rename moves a file that is open in a tab, and the parent-directory sync after the rename fails with any error
- **THEN** the row, its cached item, the watch targets and the tab show the new path, and the rename callback runs
- **AND** a warning says the rename could not be confirmed on disk, and no "already exists" warning appears

#### Scenario: A new file's first name survives an unconfirmed sync
- **WHEN** the first-name rename of a newly created file succeeds and its directory sync fails
- **THEN** the item keeps its new name in the tree and is opened through the create callback
- **AND** the row is not removed and no cleanup is scheduled for its old temporary name

#### Scenario: Draft set-aside keeps a moved body
- **WHEN** setting a draft body aside moves it into the set-aside area and the directory sync then fails
- **THEN** set-aside reports the body as placed under that name
- **AND** it does not try a further name with the source already gone

### Requirement: The rename classification is proved in the verified core
The I/O-free durable rename protocol SHALL decide the before-rename/after-rename classification itself. The shell SHALL only map the core's terminal class and the last error to the returned error, one-to-one. A Kani harness SHALL prove that classification sound over every outcome sequence, for same-directory and cross-directory renames.

#### Scenario: Rename classification soundness is proved on the production core
- **WHEN** the Kani harness explores every outcome sequence of the rename protocol
- **THEN** a before-rename class implies the rename did not take effect, an after-rename class that it did and a directory sync failed, and success that it did and every mutated directory synced
- **AND** each of the three classes is covered by a reachable sequence
