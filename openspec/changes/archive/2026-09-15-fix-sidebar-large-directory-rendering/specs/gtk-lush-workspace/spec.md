## ADDED Requirements

### Requirement: Viewport slice container is a governed widgets-crate API
`gtk-lush-widgets` SHALL own the viewport slice container as a stable in-tree platform API consumed by LushText through the workspace path dependency. Its addition MUST update the crate README, CHANGELOG, public-API snapshot, a proof-harness example, and the adoption-lab matrix in the same change, and MUST NOT introduce a new family crate.

#### Scenario: Slice container lands inside the widgets crate
- **WHEN** the viewport slice container is added
- **THEN** it lives under `crates/gtk-lush/widgets/src/`, is re-exported from the crate root, and appears in the public-API snapshot
- **AND** `make check-gtk-lush-policy` and `make check-gtk-lush-adoption` pass

#### Scenario: Adoption lab demonstrates the pattern
- **WHEN** the adoption lab is run
- **THEN** it hosts a `GtkListView` with more than 200 rows inside the slice container under an outer scroller and proves the last row renders
