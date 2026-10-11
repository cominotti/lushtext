## ADDED Requirements

### Requirement: Protected View source ownership is charged and evictable
The immutable Protected View text source, captured for find, inspector windows and whole-line copy, SHALL be charged to the editor's live memory estimate while it is retained. It SHALL be released through off-GTK plain disposal when the editor is closed, evicted or reloaded, or when a whole-content install replaces the buffer. An unmodified protected editor SHALL remain eligible for eviction under the existing rules. Its reload SHALL re-fold at insertion and SHALL NOT recapture the source until find, the inspector or copy needs it again.

#### Scenario: Eviction and reselection of a protected tab
- **WHEN** an unmodified protected tab with a captured source is evicted and then reselected
- **THEN** the source charge is released at eviction, the reload is folded at insertion, and the editor stays in Protected View
