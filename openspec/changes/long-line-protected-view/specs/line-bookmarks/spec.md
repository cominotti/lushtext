## ADDED Requirements

### Requirement: Bookmarks in Protected View navigate without a caret
In Protected View, existing bookmarks SHALL render in the gutter. Activating one SHALL scroll to its line through the navigation mark without moving the insert mark. `win.toggle-bookmark` and `win.notes-toggle-bookmark` SHALL be disabled, because there is no caret line, and SHALL publish the Protected View notice. Bookmark sidecars SHALL NOT be changed by opening or viewing a protected document.

#### Scenario: Toggling a bookmark in a protected document
- **WHEN** the user presses the toggle-bookmark shortcut in a protected document
- **THEN** no bookmark is added or removed and the notice is shown
