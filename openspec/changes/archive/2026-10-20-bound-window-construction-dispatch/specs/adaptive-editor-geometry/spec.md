## ADDED Requirements

### Requirement: Split-view settings migration and restore are a pure plan
The decision that migrates legacy split-view settings and restores the
workspace and document-properties split views SHALL be a pure, whole-pixel
function in the shell-geometry policy that takes float-free stored facts
(migration flag, legacy sidebar visibility, the current window width in whole
sp, the stored workspace preset, snapped by the adapter, and the requested
visibilities) and returns the effective visibility intents, the workspace
preset, and the workspace and properties pane shares. The adapter SHALL apply
the plan to the widgets during construction and hand the values the
pre-change code wrote (the migration keys on a fresh profile, both width
fractions always, each with the value the pre-change code left persisted last)
to the coalescing settings writer instead of writing settings; the writer's
change-only filter drops each value the store already holds. Kani SHALL prove,
for every `i32` width and every flag combination, that the effective intents,
preset and shares equal the pre-change behaviour's pure transcription, that a
fresh profile takes the default preset, and that document properties are
hidden after migration.

#### Scenario: Steady-state restore persists nothing
- **WHEN** the window is constructed on a migrated profile whose stored preset and properties share match the restored width
- **THEN** the settings writer drops every value the restore hands it and no backend write happens
- **AND** the construction-time reset of the side-by-side preview visibility requests no write when the stored value is already hidden

#### Scenario: First-launch migration is equivalent
- **WHEN** the window is constructed on a fresh profile with or without a legacy `sidebar-visible` value
- **THEN** the widgets receive the same sidebar widths, visibility intents, and breakpoint as before this change
- **AND** the values persisted after the first frame equal those the previous migration wrote

#### Scenario: The plan is proved
- **WHEN** `make kani` runs the shell-geometry policy shard
- **THEN** the split-view restore plan harness proves its properties for every `i32` width and flag combination
