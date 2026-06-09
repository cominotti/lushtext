## ADDED Requirements

### Requirement: Automation snapshots expose bounded visual geometry state
Automation1 SHALL expose bounded visual geometry state for smoke helpers and agents. The state MUST identify named surfaces, visibility, rectangles, allocation sizes, scroll anchors, scale factor, and visual readiness details without exposing document text or private persistence identifiers.

#### Scenario: Snapshot includes safe visual anchors
- **WHEN** an automation client requests a snapshot after a visual scenario step settles
- **THEN** the snapshot includes a documented visual geometry object or equivalent fields for named surfaces such as header bar, tab strip, editor viewport, source view, minimap, status bar, workspace sidebar, document properties, preview, and active transient surface when present
- **AND** each entry is bounded to safe geometry and state metadata

#### Scenario: Snapshot omits user content
- **WHEN** visual geometry state includes editor, minimap, preview, notes, bookmarks, or search surfaces
- **THEN** it does not include document text, minimap-rendered text, note bodies, draft bodies, local-history contents, complete search result text, or private persistence identifiers
- **AND** any path-like values follow the existing automation snapshot privacy policy

#### Scenario: Absent surfaces are explicit
- **WHEN** a named visual surface is not present because it is hidden, compact-suppressed, unsupported, or unavailable for the active document
- **THEN** the visual geometry state records the absence reason
- **AND** clients can distinguish intentional absence from a missing or stale snapshot field

### Requirement: Automation readiness includes visual geometry settlement
Automation1 SHALL provide a readiness predicate for visual geometry settlement. The predicate MUST wait for known UI blockers that affect screenshot correctness, including GTK idle layout work, shell split-view synchronization, minimap refresh/debounce, relevant animations, workspace refresh, and active visual scenario setup.

#### Scenario: Visual geometry wait succeeds after layout settles
- **WHEN** a smoke helper toggles a shell surface and waits for visual geometry readiness
- **THEN** Automation1 reports ready only after the affected layout, minimap, and visual anchors have settled
- **AND** the final snapshot matches the requested visible state

#### Scenario: Visual geometry wait reports blocker on timeout
- **WHEN** visual geometry readiness does not settle before the timeout
- **THEN** Automation1 returns a timeout with a bounded blocker such as `workspace-refresh`, `split-view-layout`, `minimap-refresh`, `animation`, `search`, or `unknown-visual-blocker`
- **AND** the helper preserves state and screenshot artifacts produced before failure

### Requirement: Visual geometry automation stays documented and versioned
Visual geometry snapshot fields, readiness predicates, helper flags, and scenario manifest fields SHALL be part of the documented automation contract and guarded by existing documentation drift checks.

#### Scenario: New geometry field requires docs
- **WHEN** a visual geometry snapshot field or readiness predicate is added, renamed, or removed
- **THEN** automation documentation and reference checks fail until the field, meaning, type, privacy boundary, and coverage lane are documented

#### Scenario: Helper flag drift is caught
- **WHEN** a visual capture helper flag related to geometry, masks, paired captures, or comparison artifacts changes
- **THEN** the automation reference drift check fails until the helper flag documentation is synchronized
