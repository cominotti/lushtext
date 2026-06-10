## ADDED Requirements

### Requirement: Automation exposes bounded native minimap render diagnostics
The Automation1 visual geometry snapshot SHALL expose bounded diagnostics for the native minimap rendering path when the minimap is visible. The diagnostics MUST be sufficient to compare app-estimated native slider geometry with rendered screenshot anchors, while preserving the automation privacy boundary.

#### Scenario: Snapshot reports native minimap diagnostic fields
- **WHEN** a snapshot is requested while the active editor has a visible minimap
- **THEN** the visual geometry payload includes bounded native minimap diagnostic fields such as source-map allocation, editor visible-rect summary, source-map visible-rect or adjustment summary, estimated native slider rect, first-content-row rect, and projection source classification
- **AND** the payload does not include document text, note bodies, draft bodies, local-history contents, complete search result text, or private persistence identifiers

#### Scenario: Hidden or unavailable minimap reports explicit absence
- **WHEN** the active editor has no visible native minimap
- **THEN** the visual geometry payload reports stable absent native-minimap diagnostic rows with a bounded absence reason
- **AND** visual tooling can distinguish unavailable state from a missing snapshot schema

### Requirement: Visual geometry readiness includes native minimap frame work
Automation readiness for visual geometry SHALL account for native minimap refresh, source-map allocation, and post-frame invalidation work that can affect rendered minimap anchors. The `visual-geometry-settled` predicate MUST NOT return ready for native minimap rendered-effect scenarios while known app-owned minimap work or required post-frame native-map sampling is still pending.

#### Scenario: Minimap frame work blocks readiness
- **WHEN** a sidebar or editor width transition schedules minimap projection refresh, source-map redraw or resize, dynamic overscroll refresh, or final native minimap frame sampling
- **THEN** `visual-geometry-settled` reports a bounded minimap-related blocker until that work has settled
- **AND** the application remains responsive while readiness is pending

#### Scenario: Readiness still uses screenshots for rendered truth
- **WHEN** `visual-geometry-settled` reports ready for a native minimap scenario
- **THEN** visual tooling may capture screenshots
- **AND** the final pass/fail result still depends on screenshot-derived pixel anchors rather than the readiness predicate alone
