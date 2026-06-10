## ADDED Requirements

### Requirement: Superseded minimap direction does not replace the native effect
This change SHALL NOT be used to replace the visible native `GtkSourceMap` minimap viewport highlight. Implementation work for the minimap highlight stability bug SHALL follow `stabilize-native-minimap-highlight-anchors`.

#### Scenario: Older change points to native-highlight work
- **WHEN** an agent inspects this change before implementation
- **THEN** it sees that the replacement-effect direction is superseded
- **AND** it applies `stabilize-native-minimap-highlight-anchors` for the actual minimap fix
