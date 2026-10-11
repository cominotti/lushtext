## ADDED Requirements

### Requirement: Sidebar rendered rows are proved against screenshot anchors

The visual invariant `workspace-tree-rendered-row-anchors` SHALL be defined by two manifests under `scripts/visual-geometry-scenarios/`: `workspace-tree-reveal.json`, covering fixture kinds `beyond-cap`, `nested-beyond-cap`, and `two-workspaces`, and `workspace-tree-reveal-at-rest.json`, covering `already-visible`. Both SHALL run each case in one same-session pair of captures across at least a desktop size, a short-wide size, and both forced color schemes. Both SHALL protect the header bar, the status bar, and the editor viewport with exact equality. The at-rest manifest SHALL additionally protect the revealed section's header with exact equality. Each case SHALL carry a pixel anchor whose detector finds the revealed row's drawn content (its icon and label ink) inside a crop equal to the snapshot's `workspace-revealed-row` rectangle, and the detected screen row (the lowest row of content ink) SHALL equal that rectangle's top edge plus the manifest's fixed `reference_offset_y` with a delta of zero logical pixels. The sidebar deliberately draws no selection fill (`listview.workspace-file-tree row:selected` is transparent), so the row's own content is the pixel feature every row carries; its lowest ink row sits a fixed distance below a whole-pixel row's top edge in both colour schemes. Detector evidence SHALL be recorded in the case's rendered-anchor report.

#### Scenario: The drawn row matches the reported row
- **WHEN** a `beyond-cap` case reveals the last of 400 files
- **THEN** the row content detected in the screenshot ends at the snapshot's revealed-row top edge plus the declared offset, and the case passes

#### Scenario: An offset drawing fails the anchor
- **WHEN** a replayed screenshot draws the revealed row two pixels below its reported rectangle, as the v0.8.1 content-box defect did
- **THEN** the pixel anchor fails with a non-zero screen delta, and the case fails

#### Scenario: Revealing a visible row leaves the header pixel-identical
- **WHEN** an `already-visible` case reveals a row already on screen
- **THEN** the section header crop is pixel-identical before and after, and the outer scroll position is unchanged

### Requirement: Visual proof policy requires sidebar row proof for slice-sensitive changes

The Rust-authoritative visual proof policy SHALL require a passing, fingerprint-matched `workspace-tree-rendered-row-anchors` proof for local changes to `crates/gtk-lush/widgets/src/viewport_slice_bin/`, `crates/gtk-lush/widgets/src/slice_geometry.rs`, `crates/gtk-lush/widgets/src/scroll_request.rs`, the workspace section's list host (`ui/sidebar/workspace_section/imp.rs`, `row_factory.rs`, `folder_execution.rs`), the reveal coordination module, `resources/ui/workspace-section.blp` and its generated `.ui`, and the two reveal manifests. `scripts/check-visual-proof-policy.py` SHALL mirror the same path set and invariant ID, and both self-tests SHALL cover a required and a not-required path.

#### Scenario: A slice-bin change without sidebar proof is rejected
- **WHEN** a local change edits `crates/gtk-lush/widgets/src/scroll_request.rs` and no matching sidebar proof artifact exists
- **THEN** `make check-visual-proof-policy` fails and names `workspace-tree-rendered-row-anchors`

#### Scenario: An unrelated change is not burdened
- **WHEN** a local change edits only `crates/lushtext-core/src/services/draft_service.rs`
- **THEN** the policy does not require the sidebar invariant
