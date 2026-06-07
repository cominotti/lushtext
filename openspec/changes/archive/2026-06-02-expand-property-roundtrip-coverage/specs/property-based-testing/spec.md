## ADDED Requirements

### Requirement: Round-Trip Property Coverage
The project SHALL extend the property-test lane with bounded round-trip
properties for deterministic save-formatting, session persistence models, draft
persistence models, and Replace All undo behavior.

#### Scenario: EditorConfig save formatting is idempotent
- **WHEN** generated bounded text is formatted with generated
  `trim_trailing_whitespace` and `insert_final_newline` overrides
- **THEN** applying the same save-formatting overrides a second time produces
  byte-identical text to the first formatted result

#### Scenario: Session models serialize and deserialize stably
- **WHEN** generated bounded `SessionData` and `SessionTab` values are serialized
  to JSON and deserialized
- **THEN** the deserialized values preserve tab paths, draft IDs, cursor and
  scroll positions, pinned state, and active-tab index values

#### Scenario: Draft models serialize and deserialize stably
- **WHEN** generated bounded `DraftManifest` and `DraftEntry` values are
  serialized to JSON and deserialized
- **THEN** the deserialized values preserve draft IDs, optional original paths,
  original mtimes, saved timestamps, and entry ordering

#### Scenario: Replace All undo restores original bytes
- **WHEN** generated bounded file contents receive generated Replace All
  replacements and the resulting undo backup is immediately applied
- **THEN** every non-diverged file restored by undo matches its original bytes
  exactly
- **AND** the undo outcome leaves no remaining backup entries for successfully
  restored files

### Requirement: Deterministic Property Scope
The property-test lane SHALL permit tiny deterministic tempdir-backed service
properties while continuing to exclude GTK widgets, live sessions, and
environment-dependent workflows.

#### Scenario: Tempdir-backed service property is deterministic
- **WHEN** a property needs real files to exercise production service behavior
- **THEN** the property uses bounded temporary directories, tiny files, and
  deterministic inputs
- **AND** it does not start GTK, watchers, file choosers, D-Bus, portals, or a
  compositor

#### Scenario: Broad workflow remains out of scope
- **WHEN** a proposed generated-input test depends on widget construction,
  watcher timing, file chooser behavior, portal state, or live session restore
- **THEN** the behavior remains covered by widget/integration/fuzzing tests or is
  first extracted into deterministic helper logic

### Requirement: Expanded Property Documentation
The project SHALL document the expanded round-trip property coverage and the
deterministic tempdir service-property boundary.

#### Scenario: Developer reviews property scope
- **WHEN** a developer reads property-testing documentation or agent build rules
- **THEN** the documentation lists the added round-trip coverage areas and
  explains that bounded tempdir-backed service properties are allowed only for
  deterministic non-GTK workflows
