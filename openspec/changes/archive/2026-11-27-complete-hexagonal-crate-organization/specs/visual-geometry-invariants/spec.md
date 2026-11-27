## MODIFIED Requirements

### Requirement: Visual proof policy requires sidebar row proof for slice-sensitive changes
The Rust-authoritative visual proof policy SHALL require a passing `workspace-tree-rendered-row-anchors` proof for changes to `ViewportSliceBin`/`slice_geometry`/`scroll_request`, the workspace section list host, reveal coordination, `workspace-section.blp`/`.ui`, and reveal manifests. `scripts/check-visual-proof-policy.py` SHALL mirror it, with both self-tests covering required and non-required paths.

#### Scenario: A slice-bin change without sidebar proof is rejected
- **WHEN** a local change edits `crates/gtk-lush/widgets/src/scroll_request.rs` and no matching sidebar proof artifact exists
- **THEN** `make check-visual-proof-policy` fails and names `workspace-tree-rendered-row-anchors`

#### Scenario: An unrelated change is not burdened
- **WHEN** a local change edits only `crates/lushtext-services/src/` (such as `draft_service`)
- **THEN** the policy does not require the sidebar invariant
