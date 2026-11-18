## ADDED Requirements

### Requirement: Automation drives and observes long Replace All runs without document text
The catalog SHALL export `win.cancel-search-panel-replacements` (cancel the run in flight) and `win.replace-in-editor(s)` (Open and Replace in Editor for a skipped path). The `content_search` snapshot section SHALL report whether a run is in flight, its progress, whether cancellation was requested, and the last run's skip counts per reason and unlisted count. It SHALL never report document text, replacement text or journal bodies. `search-complete` SHALL stay pending through a run's rollback.

#### Scenario: A smoke cancels a run
- **WHEN** an automation client confirms a Replace All over a 512 MiB fixture and activates `win.cancel-search-panel-replacements` while progress shows bytes done
- **THEN** `search-complete` becomes ready after the rollback, the snapshot shows zero replaced files, and the fixture's bytes are unchanged

#### Scenario: A smoke reads the skip counts
- **WHEN** a run skips one file with unsaved changes and one non-UTF-8 file
- **THEN** the snapshot reports one `UnsavedChanges` and one `NotUtf8` skip without exposing either file's content
