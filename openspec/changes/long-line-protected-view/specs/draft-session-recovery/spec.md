## ADDED Requirements

### Requirement: Recovery of giant-paragraph drafts does not lay out the paragraph
Draft recovery and set-aside open SHALL install a body whose worker profile marks it `protected_view` through the fold-at-insertion executor, so that no part of a folded tail is ever laid out. The recovered editor SHALL enter Protected View, stay modified and draft-restored, and retain the installed body as its `ProtectedTextSource`. Because its buffer is immutable and equals its persisted draft body, autosave SHALL NOT snapshot it or rewrite its body; Save, Save As and close-with-changes SHALL write the retained source. The crash-recovery smoke's lazy-aggregate case (two accepted drafts grown to 32 MiB each without a paragraph delimiter) SHALL reach `recovery-restore-complete` within the smoke's existing 60 s limit, which SHALL NOT be raised, and SHALL record the time to that predicate and each recovered tab's `protected_view` in its artifacts.

#### Scenario: Two 32 MiB single-line drafts recover without a layout stall
- **WHEN** the application restarts after a crash with two accepted drafts of 32 MiB each whose last paragraph holds more than 33 million characters
- **THEN** both editors are in Protected View with their tails hidden from the first inserted byte, no paragraph of more than `head_cap` visible characters is laid out, and `recovery-restore-complete` settles within the existing 60 s smoke limit

#### Scenario: A recovered protected draft is not re-snapshotted
- **WHEN** autosave ticks while a recovered protected editor stays open
- **THEN** no buffer snapshot runs and no draft body is written for it, and its draft entry and body stay unchanged

#### Scenario: Saving a recovered protected draft
- **WHEN** the user saves a recovered protected draft with Save As
- **THEN** the written bytes are the recovered body encoded with the document's encoding, written from the worker without reading the buffer on the GTK thread, and the draft is retired through the existing journal order
