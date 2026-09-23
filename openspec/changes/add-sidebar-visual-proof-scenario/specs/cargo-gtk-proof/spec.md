## ADDED Requirements

### Requirement: Tool supports the workspace-tree-reveal scenario type

`cargo-gtk-proof` SHALL accept scenario manifests with `scenario_type` `workspace-tree-reveal`. Schema validation SHALL require a non-empty `matrix.fixture_kinds` drawn from `beyond-cap`, `nested-beyond-cap`, `two-workspaces`, and `already-visible`, and SHALL reject any other kind. The live runner SHALL prepare each case in its isolated data directory by generating the fixture folder tree (at least 300 entries in the revealed directory for every kind except `already-visible`, so the case crosses GTK's 200-row realized cap) and writing `workspaces.json` in the versioned workspace-state envelope, wait for `workspace-refresh-complete` before the before-capture, use `reveal-workspace-path` with the fixture's target path as the primary action, and wait for `workspace-path-revealed` and `visual-geometry-settled` before the after-capture. Fixture file names SHALL be synthetic and SHALL NOT appear in any artifact beyond the case's own fixture directory.

The runner SHALL evaluate these relationship assertions from the after-snapshot: `revealed-row-inside-sidebar-viewport`, `neighbour-rows-contiguous` (each neighbour row starts where the previous ends, within zero logical pixels), `revealed-row-at-rest` (two snapshots taken after readiness report identical reveal surfaces), and, for `already-visible`, `outer-scroll-unchanged` and `section-header-unchanged` between before and after snapshots. An unsupported fixture kind, a missing reveal surface, or a reveal status other than `revealed` SHALL fail the case with a named reason.

#### Scenario: A valid reveal manifest expands into cases
- **WHEN** `cargo gtk-proof schema` validates, and the scenario loader expands, a `workspace-tree-reveal` manifest with two sizes, two color schemes, and three fixture kinds
- **THEN** it expands twelve cases with stable case IDs and reports the manifest valid

#### Scenario: An unknown fixture kind is rejected
- **WHEN** the manifest lists fixture kind `huge`
- **THEN** validation fails and names the unsupported fixture kind

#### Scenario: A failed reveal fails the case
- **WHEN** the application reports reveal status `unsettled` for a case
- **THEN** the case fails with a reason naming the reveal status, and its artifacts are still written

#### Scenario: Gapped neighbour rows fail the relationship
- **WHEN** a replayed after-snapshot places a neighbour row one pixel below the end of the previous row
- **THEN** `neighbour-rows-contiguous` fails and names both rectangles
