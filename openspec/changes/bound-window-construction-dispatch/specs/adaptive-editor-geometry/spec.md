## ADDED Requirements

### Requirement: Split-view settings migration and restore are a pure plan
The decision that migrates legacy split-view settings and restores the
workspace and document-properties split views SHALL be a pure, whole-pixel
function in the shell-geometry policy that takes float-free stored facts
(migration flag, legacy sidebar visibility, the restored and current window
widths in whole sp, the stored workspace preset, whether the stored workspace
and properties fractions already equal the values the plan implies, and the
requested visibilities; the adapter reduces each stored-fraction comparison to
a boolean) and returns
the effective visibility intents, the workspace preset, the workspace and
properties pane shares, and the set of keys whose persisted value must change.
The adapter SHALL apply the plan to the widgets during construction and hand
the persist set to the coalescing settings writer instead of writing settings.
Kani SHALL prove, for every `i32` width and every flag combination, that an
already-migrated profile whose stored fractions match yields an empty persist
set, that a fresh profile yields exactly the five migration keys with each
width-fraction key carrying the value the pre-change code left persisted last
(the restore's, which overwrote the migration's), that the effective intents and shares equal the
pre-change behaviour's pure transcription, and that document properties are
hidden after migration.

#### Scenario: Steady-state restore persists nothing
- **WHEN** the window is constructed on a migrated profile whose stored preset and properties share match the restored width
- **THEN** the plan's persist set is empty and no settings write is requested
- **AND** the construction-time reset of the side-by-side preview visibility requests no write when the stored value is already hidden

#### Scenario: First-launch migration is equivalent
- **WHEN** the window is constructed on a fresh profile with or without a legacy `sidebar-visible` value
- **THEN** the widgets receive the same sidebar widths, visibility intents, and breakpoint as before this change
- **AND** the values persisted after the first frame equal those the previous migration wrote

#### Scenario: The plan is proved
- **WHEN** `make kani` runs the shell-geometry policy shard
- **THEN** the split-view restore plan harness proves its four properties for every `i32` width and flag combination
