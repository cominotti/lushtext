## ADDED Requirements

### Requirement: Workspace list cuts and loads are stated and announced once
A workspace list that a retained-byte budget cuts, or that is still loading incrementally, SHALL say so in visible text and in its accessible names, and SHALL NOT be silently incomplete. Workspace search SHALL raise one polite announcement when its budget is reached, stating the matches shown, and SHALL announce the final shown and found counts at completion through the existing bounded completion announcement. It SHALL NOT announce each running-count update. The sidebar's budget row and loading row SHALL carry stable accessible names and descriptions stating the shown and total counts and how to reach the rest. Loading progress SHALL NOT be announced. The accessibility matrix rows for dense workspace search, Replace All, and the dense workspace tree SHALL list these states and their proofs.

#### Scenario: Search budget reached
- **WHEN** a search reaches its retained-byte budget
- **THEN** a single polite announcement states the matches shown
- **AND** the count label's accessible description carries the shown and found counts

#### Scenario: Budget row in the tree
- **WHEN** a directory past the budget is expanded and its last row is reached by keyboard
- **THEN** the screen reader receives the row's name "Showing X of Y entries" and a description of how to reach the rest

#### Scenario: Loading row is silent
- **WHEN** a large directory loads in batches
- **THEN** no announcement is raised per batch, and the loading row's name reflects progress when focused
