## ADDED Requirements

### Requirement: Palette "More" rows and category views are accessible
A command palette "More" row SHALL expose an accessible name that states its count and category ("Show 1,234 more results in All Workspaces"), a description saying it opens every result in that category, and the same selected, position and set-size metadata as the other result rows. A category view's result list SHALL keep the role `list`, SHALL be labelled "Command palette results in {category}", and SHALL report its set size from its model's length without materializing every row. Its back button SHALL be labelled "Back to all results". Its title and any "Showing X of Y results" or note-source notice SHALL carry their visible text. Entering a category view SHALL raise one polite announcement naming the category and how to return; a budget notice SHALL be announced once per change. Focus SHALL stay in the query entry in both views, and Up, Down, Page Up, Page Down, Enter, Escape and Backspace on an empty query SHALL work from it. The accessibility matrix, the accessibility guide and the Orca checklist SHALL list these expectations and their proofs.

#### Scenario: Orca reads a More row
- **WHEN** keyboard navigation selects the workspace group's More row while 39,936 files are not shown
- **THEN** the screen reader can report "Show 39,936 more results in All Workspaces"

#### Scenario: Entering a category is announced once
- **WHEN** the user opens a category view and then types three queries
- **THEN** exactly one entry announcement has been raised

#### Scenario: A large category view does not build every row
- **WHEN** a category view lists 131,072 rows and the selection moves
- **THEN** the list's accessible value and the rows' set size are correct and the model has created row objects only for the rows the toolkit requested

#### Scenario: Existing palette anchors still resolve
- **WHEN** the accessibility smoke opens the palette in its existing scenarios
- **THEN** the `Command palette query` entry, the `Command palette results` list, the mode selector and the group labels are found by their existing roles and names
