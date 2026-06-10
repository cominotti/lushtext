## ADDED Requirements

### Requirement: Rendered-effect anchors use independent screenshot oracles
The visual invariant system SHALL verify rendered-only effects with screenshot-derived pixel anchors when app-computed geometry could share the same bug as the UI. Automation geometry MAY bound safe crops, readiness, scale factor, and diagnostic reports, but it MUST NOT be the only source of truth for the rendered anchor location.

#### Scenario: Native minimap highlight detector rejects the reported bad state
- **WHEN** the visual PNG detector is run against the reported good minimap screenshot and the reported bad minimap screenshot
- **THEN** the good screenshot passes the native viewport highlight/content-row anchor relationship check
- **AND** the bad screenshot fails because the native viewport top edge is missing or shifted relative to the first minimap content row

#### Scenario: Live visual scenario compares screenshot-derived anchors
- **WHEN** a same-session visual scenario captures the minimap with the workspace sidebar shown and hidden
- **THEN** the scenario detects the first minimap content row from screenshot pixels
- **AND** it detects the native viewport highlight top edge from screenshot pixels
- **AND** it compares the vertical delta between those anchors across captures instead of accepting app-computed viewport geometry as proof

#### Scenario: Geometry-only evidence cannot satisfy rendered-effect coverage
- **WHEN** a visual-sensitive change touches minimap viewport rendering, minimap CSS, visual-geometry detector logic, or native-highlight scenario manifests
- **THEN** proof policy requires a root visual-geometry summary that lists the native minimap highlight invariant as verified by pixel-anchor assertions
- **AND** a run that reports only bounded rectangles, allocation relationships, or nonblank crops fails the proof-policy check for that invariant

#### Scenario: Anchor failures preserve bounded evidence
- **WHEN** a rendered-effect pixel anchor is missing, shifted beyond tolerance, or confused with fill/background pixels
- **THEN** the visual-geometry runner writes bounded before/after crops, detector reports, anchor coordinates, relative-delta results, and failure reasons
- **AND** the artifacts do not expose document text, note bodies, draft bodies, local-history contents, or private persistence identifiers
