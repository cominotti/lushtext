## ADDED Requirements

### Requirement: The Notes browser's sections and limited state are accessible
The Notes browser's result list SHALL keep its accessible role `list` and name "Notes results". Each row SHALL keep its visible title as its accessible name. A screen reader SHALL be able to learn each row's section, either from a section header exposed as a heading with the section title, or from the row's accessible description when the toolkit cannot expose the header as a heading. The limited-state label SHALL carry the same text as its visible label and SHALL be announced once per limited source generation, with polite priority. The browser's existing stable AT-SPI anchors SHALL keep their roles and names. The accessibility matrix rows for the empty and populated Notes browser and the Orca checklist SHALL list the section and limited-state expectations and their proofs.

#### Scenario: Orca reads a row in a new section
- **WHEN** keyboard navigation moves the selection from the last bookmark to the first folder note
- **THEN** the screen reader can report that the folder note belongs to the `Folder Notes` section

#### Scenario: Limited source announced once
- **WHEN** a limited source is published and the user then types three queries
- **THEN** exactly one limited-state announcement has been raised

#### Scenario: Existing anchors still resolve
- **WHEN** the accessibility smoke opens the populated Notes browser
- **THEN** the `Notes results` list, the `Search notes` entry, the `Open` button, and the seeded `Bookmark · Line 1` row label are found by their existing roles and names
