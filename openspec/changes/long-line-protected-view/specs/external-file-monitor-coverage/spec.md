## ADDED Requirements

### Requirement: External reload of a protected document re-folds at insertion
When an external change is reloaded into a protected document, the reload SHALL re-profile the new content. It SHALL install the new content with the fold applied at insertion and SHALL discard the stale immutable source. The document's Protected View decision SHALL be taken from the new profile. Reload of an unmodified protected document SHALL NOT require a discard decision, because it has no unsaved work.

#### Scenario: External growth of a giant line
- **WHEN** another program appends 500,000 characters to the giant paragraph of an open protected document and the user reloads
- **THEN** the reloaded buffer is folded at every turn boundary and the snapshot's hidden-character count reflects the new length
