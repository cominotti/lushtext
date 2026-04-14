# minimap-navigation-parity Specification

## Purpose
Keep end-of-file minimap interaction stable by pairing GNOME-style editor overscroll tail room with matching minimap geometry so clicks and drags near the bottom of the document remain predictable.
## Requirements
### Requirement: The editor keeps overscroll tail room after the last line
The system SHALL provide dynamic end-of-document overscroll on supported editor pages so the last visible lines can travel upward with extra blank tail space below them. That overscroll SHALL remain large enough to preserve usable minimap travel near the end of the file.

#### Scenario: Editor allocation creates extra blank tail room near EOF
- **WHEN** the active editor page is mapped and allocated with a visible document viewport
- **THEN** the editor computes bottom overscroll from the current visible height instead of keeping only a small fixed bottom margin
- **AND** the user can continue scrolling past the last line into extra blank space

### Requirement: Minimap clicks target the region the user selected
The system SHALL keep enough shared end-of-document tail room between the editor and minimap that clicks near the bottom of the minimap still target the intended document region instead of collapsing prematurely into the end of the file.

#### Scenario: Clicking outside the current viewport indicator jumps to the targeted region
- **WHEN** the user clicks a visible minimap position outside the current viewport indicator
- **THEN** the active editor viewport moves so that clicked minimap position falls within the resulting viewport indicator
- **AND** the editor remains the primary editing surface after the jump

### Requirement: Dragging the viewport indicator preserves the original grab anchor
The system SHALL keep enough shared end-of-document tail room between the editor and minimap that dragging the viewport indicator near EOF preserves the original relative grab position instead of collapsing the indicator against the bottom boundary too early.

#### Scenario: Dragging from inside the viewport indicator keeps the indicator attached to the pointer
- **WHEN** the user presses inside the current minimap viewport indicator and drags upward or downward
- **THEN** the active editor viewport follows the drag continuously
- **AND** the viewport indicator keeps the original relative grab position instead of drifting away from the pointer over repeated drag updates
