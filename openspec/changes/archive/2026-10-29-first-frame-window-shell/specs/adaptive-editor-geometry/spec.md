## ADDED Requirements

### Requirement: Secondary-surface geometry is final from the first frame
The system SHALL construct the window's split views, layout views, bottom sheet, and their slot
hosts, restore them through the split-view restore plan, and install the window's breakpoints
before the window is realized, so the first presented frame draws the workspace rail and
document-properties pane edges of the restored layout with the composed widgets. No placeholder
SHALL stand in for a split view.

Revealing staged content in a slot SHALL NOT change the allocation of the rail, the pane, or the
bottom-sheet slot. A widget test SHALL prove this for every workspace width preset, at text scales
1, 1.25, 1.5 and 2, at a wide, a medium, and a compact window width. If a staged surface's minimum
width would exceed its slot's allocation, the slot host SHALL carry a width floor derived from the
same restore plan, converted from sp to px in the GTK adapter.

#### Scenario: Revealing the sidebar keeps the rail edge
- **WHEN** a window is restored at 1,200 sp with the workspace sidebar requested and the `Comfy` preset, and the sidebar's content is revealed after the first frame
- **THEN** the workspace rail's allocated width is the same before and after the reveal

#### Scenario: Compact restore shows no rail
- **WHEN** the restored window width is at or below the breakpoint that collapses the workspace sidebar
- **THEN** the first frame shows no docked workspace rail, and the composed window renders none either

#### Scenario: Breakpoints apply in the first frame
- **WHEN** the restored window width matches the Open-button breakpoint
- **THEN** the first presented frame already shows the Open button's compact form and the collapsed workspace, and no later frame of composition changes either
