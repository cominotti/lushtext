## ADDED Requirements

### Requirement: Annotation identity follows in-app renames and resets on Save As
The system SHALL key persisted annotation sidecars by a saved-document identity derived from the document's canonical path under `$XDG_DATA_HOME/lushtext/annotations/`. When a saved document or its parent path is renamed through LushText's in-app rename workflow, the system MUST migrate the existing annotation sidecar to the renamed identity. When a document is saved through Save As, the new path MUST start with a fresh annotation identity instead of inheriting the original annotation set automatically.

#### Scenario: In-app rename preserves annotation sidecars
- **WHEN** the user renames a saved annotated document or one of its ancestor directories through the LushText sidebar workflow
- **THEN** the persisted annotation sidecar is migrated to the renamed identity
- **AND** reopening the renamed file restores the same annotations

#### Scenario: Save As starts a new annotation identity
- **WHEN** the user saves an annotated document to a new path through Save As
- **THEN** the new saved document starts without copied annotations by default
- **AND** the original document keeps its existing annotation sidecar

### Requirement: Empty annotation state removes its sidecar file
The system SHALL remove an annotation sidecar file when a document no longer has any persisted annotations, instead of leaving an empty annotation sidecar behind indefinitely.

#### Scenario: Removing the final annotation deletes the annotation sidecar
- **WHEN** the user removes the last remaining annotation for a saved document
- **THEN** the persisted annotation sidecar for that document is deleted from the app data directory
- **AND** reopening the document no longer restores an empty annotation sidecar payload
