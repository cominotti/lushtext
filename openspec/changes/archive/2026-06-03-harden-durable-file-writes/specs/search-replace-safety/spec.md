## ADDED Requirements

### Requirement: Replace All preserves each replaced file's identity metadata
The system SHALL preserve each replaced file's on-disk identity metadata when
Replace All rewrites it through the shared atomic write path. A file rewritten by
Replace All MUST keep its prior permission (mode) bits and SHALL keep ownership,
POSIX ACLs, and extended attributes on a best-effort basis, matching the
guarantee for in-editor saves and undo restores.

#### Scenario: Replacing inside a restrictive file keeps its permissions
- **WHEN** Replace All rewrites a `0600` file in the workspace
- **THEN** the rewritten file on disk is still `0600`

#### Scenario: Undo restore keeps the file's permissions
- **WHEN** the user undoes a Replace All that rewrote an executable file
- **THEN** the restored file on disk is still marked executable
