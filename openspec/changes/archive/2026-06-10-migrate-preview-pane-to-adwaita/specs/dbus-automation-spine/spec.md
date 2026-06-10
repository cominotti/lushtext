## ADDED Requirements

### Requirement: Preview automation remains stable across shell migration
The automation spine SHALL preserve the documented preview actions, snapshot
fields, and readiness behavior while the preview shell moves from `GtkPaned`
animation to Adwaita-native presentation. Automation consumers MUST be able to
drive the same target states and observe the same bounded preview state without
mutating private widgets or depending on implementation-specific pane geometry.

#### Scenario: Preview target-state actions still converge
- **WHEN** an automation client activates `win.set-preview-pane-visible` or `win.set-preview-mode` with a boolean parameter
- **THEN** the action routes through the normal window preview workflow
- **AND** repeated calls with the same parameter converge on the same visible preview state
- **AND** side-by-side preview and preview-only mode remain mutually exclusive

#### Scenario: Snapshot fields keep their meaning
- **WHEN** a snapshot is requested after preview layout settles
- **THEN** `surfaces.preview_pane_visible` reports whether side-by-side preview is requested and visible according to the shell's explicit preview state
- **AND** `surfaces.preview_mode` reports whether preview-only mode is the active content presentation
- **AND** the snapshot does not expose private widget identities, preview document text, or implementation-specific layout-node paths

#### Scenario: Readiness tracks preview presentation work
- **WHEN** a preview target-state action starts a shell transition, layout-view switch, or embedded preview layout repair
- **THEN** `visual-geometry-settled` and `idle` readiness do not report ready until the preview presentation work has settled
- **AND** any renamed or newly exposed preview readiness blocker is documented in the automation guide, developer reference, action catalog checks, and automation client self-test before it is treated as stable
