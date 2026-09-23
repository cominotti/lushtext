## ADDED Requirements

### Requirement: Durable-write crash leftovers are swept only when provably LushText's and stale
The system SHALL remove temp files left behind by an interrupted durable write
only when every one of these conditions holds:

- the file name matches the exact pattern that the durable-write temp-name
  builder emits;
- the embedded process id and the launch nonce carried in the sequence field
  do not identify the current launch (a process id alone repeats across
  launches inside a pid namespace);
- the file is a regular file older than a stale-age threshold.

Sweeping SHALL be bounded per pass and run off the GTK thread. It SHALL cover
the LushText app-data directories at startup. In a user workspace directory it
SHALL run only for leftovers that name the exact target LushText is writing at
that moment, in that target's own directory. The system MUST NOT remove any file
that fails a condition, and MUST NOT traverse user workspace directories to look
for leftovers.

#### Scenario: A leftover from a killed process is swept from app data
- **WHEN** startup finds `.manifest.json.<tag>.<other-pid>.<seq>.tmp` in the drafts directory, older than the stale-age threshold
- **THEN** the leftover is removed
- **AND** no other file in the directory is touched

#### Scenario: A current-process or fresh temp file is never swept
- **WHEN** a matching temp file carries the current launch's process id and nonce, or is younger than the stale-age threshold
- **THEN** the sweep leaves it in place

#### Scenario: A user file resembling the pattern is never swept
- **WHEN** a user workspace directory contains a hidden file whose name resembles the pattern but whose target name is not the file LushText is writing
- **THEN** the file is left in place

#### Scenario: Saving a document clears its own stale leftovers
- **WHEN** LushText durably saves `notes.md` and the same directory holds a stale `.notes.md.<tag>.<other-pid>.<seq>.tmp` older than the threshold
- **THEN** that leftover is removed after the save succeeds

### Requirement: A bare relative destination syncs the current directory
The system SHALL treat the parent directory of a destination path with no directory component as the current directory (`.`) for parent-directory sync and ancestor creation. A write to such a path MUST NOT report an after-rename durability failure merely because an empty parent path cannot be opened.

#### Scenario: Durable write to a bare file name
- **WHEN** a durable write targets a relative path consisting only of a file name
- **THEN** the parent-directory sync opens `.` and succeeds
- **AND** the write reports success
