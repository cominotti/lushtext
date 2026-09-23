## ADDED Requirements

### Requirement: The app-data layout has one owner
Every file and directory LushText keeps under its app data home SHALL be named
once, in a single GTK-free layout owner, with a relative path, a kind (file,
directory, or nested family), whether durable writes create temp files there,
and whether the format-upgrade inventory scans it (or the recorded reason it
does not). Services SHALL resolve their app-data paths through that owner
rather than joining their own literals. The startup leftover sweep and the
format-upgrade inventory SHALL derive the directories and files they visit from
the owner's entries, so that adding an app-data location makes it swept and
inventoried, or explicitly exempt, without editing either consumer's list.
Changing the owner MUST NOT change any resolved path of an existing location
unless a format-upgrade converter migrates it.

#### Scenario: A new durable-write directory is swept without editing the sweep
- **WHEN** a contributor adds a layout entry for a new app-data directory that holds durable writes
- **THEN** the startup leftover sweep visits it under the same provably-ours, stale-only, bounded rules
- **AND** no hand-written directory list outside the layout owner needed to change

#### Scenario: Existing paths are unchanged
- **WHEN** the layout owner resolves every existing location for a given data home
- **THEN** each resolved path is byte-identical to the path the pre-owner services resolved

#### Scenario: A location outside the upgrade inventory is explicit
- **WHEN** a layout entry is not scanned by the format-upgrade inventory
- **THEN** the entry records why, and a unit test fails if an entry is neither scanned nor exempted
