## ADDED Requirements

### Requirement: Superseded visual-geometry direction defers to native-highlight anchors
This change SHALL defer rendered minimap anchor requirements to `stabilize-native-minimap-highlight-anchors`. The useful invariant from this older change is that rendered pixels need independent screenshot-derived anchors when app geometry can share the bug.

#### Scenario: Older visual-geometry change is not authoritative
- **WHEN** visual-geometry work is applied for the minimap native-highlight bug
- **THEN** the requirements in `stabilize-native-minimap-highlight-anchors` are authoritative
- **AND** this older change does not claim completion for the native-highlight bug
