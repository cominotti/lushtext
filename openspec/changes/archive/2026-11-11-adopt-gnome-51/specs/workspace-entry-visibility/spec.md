## ADDED Requirements

### Requirement: The excluded-names editor is built from rows its container accepts
The `workspace-excluded-names` editor in Preferences SHALL add every row of its `AdwExpanderRow`, including the add-name `AdwEntryRow`, the way the supported Libadwaita versions accept, so that Libadwaita 1.9 and 1.10 log no warning such as `AdwEntryRow … placed into AdwExpanderRow …, can only be placed into GtkListBox`. Adding, removing, and resetting names SHALL keep working, and so SHALL the row-removal contract recorded in `ui/preferences/excluded_names.rs`.

#### Scenario: Opening Preferences on Libadwaita 1.10
- **WHEN** the preferences dialog is constructed on Libadwaita 1.10
- **THEN** no Adwaita parent-placement warning is logged
- **AND** the excluded-names editor still adds, removes, and resets names and re-projects the sidebar
