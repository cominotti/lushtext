## ADDED Requirements

### Requirement: Visual geometry SHALL protect accessibility-sensitive regions
Visual geometry invariant scenarios SHALL treat focus indicators, persistent controls, close/back buttons, primary actions, visible labels, and item-region scroll bounds as accessibility-sensitive regions when a scenario claims accessibility visual coverage.

#### Scenario: Focus ring is protected
- **WHEN** a visual geometry scenario compares states after keyboard focus is moved to a named target
- **THEN** the manifest identifies the focused target and the expected visible focus-indicator region
- **AND** missing, clipped, hidden, or displaced focus indication fails the accessibility-sensitive invariant

#### Scenario: Primary actions remain allocated
- **WHEN** a scenario captures constrained geometry for dialogs, bottom sheets, search bars, workspace search, notes, local history, Open popover, command palette, or properties
- **THEN** close, back, save, cancel, destructive, primary, and persistent command regions retain positive visible allocation unless intentionally hidden by the tested mode
- **AND** clipped or overlapped action regions fail the scenario

#### Scenario: Item region owns scrolling
- **WHEN** a scenario captures dense or awkward rows in file tree, search results, command palette, notes/bookmarks, local history, preferences, or Open popover
- **THEN** the manifest identifies the item scrolling region
- **AND** headers, search controls, close/back controls, and primary actions remain outside unintended scrolling or clipping regions

### Requirement: Accessibility visual invariants SHALL cover state extremes
Accessibility-sensitive visual geometry coverage SHALL include no-context, representative populated, dense or awkward, and constrained states for each surface it claims to cover.

#### Scenario: No-context states have readable empty regions
- **WHEN** a visual geometry scenario claims accessibility coverage for a no-document, no-workspace, no-results, no-notes, no-bookmarks, no-history, or missing-context state
- **THEN** the empty-state region has positive visible allocation and readable text bounds
- **AND** persistent actions remain reachable without fake rows

#### Scenario: Representative states prove normal operation
- **WHEN** a visual geometry scenario captures a representative populated state
- **THEN** it proves the normal item label, secondary metadata, action control, focus target, and scroll region geometry
- **AND** the scenario records enough bounded identity to diagnose which item was selected or focused

#### Scenario: Dense and awkward states do not break semantics
- **WHEN** a visual geometry scenario captures many rows, long names, deep indentation, long paths, large result counts, or long translated labels
- **THEN** it verifies that text ellipsizes or wraps according to the surface contract
- **AND** row actions, focus targets, and primary controls remain reachable and non-overlapping

### Requirement: Visual proof policy SHALL require accessibility evidence for accessibility-sensitive UI changes
The visual proof policy SHALL require current accessibility visual evidence when changes modify focus styling, accessible-visible controls, row factories, transient surfaces, visual accessibility smoke tooling, or CSS/geometry that can affect keyboard and low-vision users.

#### Scenario: Accessibility-sensitive diff requires current evidence
- **WHEN** a change modifies UI Rust, Blueprint/UI templates, CSS, visual smoke tooling, AT-SPI capture tooling, or widget rows in a way classified as accessibility-sensitive
- **THEN** proof policy requires a passing, unfiltered visual accessibility or visual geometry summary matching the current worktree fingerprint
- **AND** skipped or stale artifacts do not satisfy the requirement

#### Scenario: Policy explains missing accessibility proof
- **WHEN** proof policy rejects a change for missing accessibility visual evidence
- **THEN** the failure names the required scenario or invariant class
- **AND** it points to the Make target or smoke command that can generate the artifact

#### Scenario: Decorative-only changes can be exempted narrowly
- **WHEN** a UI visual change is intentionally decorative and cannot affect focus, readable text, semantic state, primary controls, or accessible navigation
- **THEN** the exemption is narrow, documented, and reviewed by the policy rule
- **AND** broad UI changes cannot bypass accessibility visual evidence by marking an entire file decorative

### Requirement: Accessibility visual geometry artifacts SHALL stay bounded and private
Accessibility-sensitive geometry artifacts SHALL preserve diagnostic geometry, crop, and environment data without exposing unbounded user document content.

#### Scenario: Geometry artifacts include accessibility context
- **WHEN** an accessibility-sensitive visual geometry scenario completes
- **THEN** artifacts include scenario identity, focused surface, accessible anchor names, bounded rectangles, crop paths, comparison reports, environment metadata, and warning-scan status
- **AND** they avoid unbounded document text, note bodies, complete search results, and private persistence identifiers

#### Scenario: Failure artifacts identify the broken contract
- **WHEN** a focus, clipping, scrolling, overlap, or primary-action invariant fails
- **THEN** the failure report names the broken accessibility contract, affected surface, expected region, observed region, and relevant screenshot/crop paths
- **AND** it does not require maintainers to inspect the entire screenshot manually before seeing the likely cause
