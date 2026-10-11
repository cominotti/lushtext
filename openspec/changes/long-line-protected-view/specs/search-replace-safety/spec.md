## ADDED Requirements

### Requirement: In-tab Replace is unavailable in Protected View
In Protected View the in-tab search bar SHALL hide or disable its replace row and the replace actions. `win.begin-replace` (Ctrl+H) SHALL open find only and publish the Protected View notice. No `GtkSourceSearchContext` SHALL be attached to the protected buffer. Workspace Replace All SHALL keep its existing skip rules for open documents: a modified open document, including one that entered Protected View through an edit, is skipped. When Replace All rewrites the file of an open, unmodified protected document, the existing automatic reload of that tab SHALL install the new content through the fold-at-insertion executor and re-derive the Protected View decision.

#### Scenario: Ctrl+H on a protected document
- **WHEN** the user presses Ctrl+H with a protected document active
- **THEN** the find bar opens without a replace row, no search context exists on the buffer, and the notice is shown
