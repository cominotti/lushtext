## ADDED Requirements

### Requirement: Protected View source ownership is charged and evictable
The immutable Protected View text source SHALL be charged to the editor's live memory estimate while it is retained. For a whole-content install it SHALL be the installed body, moved out of the install's transient admission into long-lived ownership at the install's `Complete` terminal, with a guaranteed handoff to off-GTK plain disposal; it SHALL NOT be a second copy. It SHALL be released through plain disposal when the editor is closed, evicted or reloaded, or when a whole-content install replaces the buffer. An unmodified protected editor SHALL remain eligible for eviction under the existing rules; its reload SHALL re-fold at insertion and retain the newly installed body as its source. A Copy Whole Line clipboard provider's clone of the source SHALL be charged as long-lived ownership, not to the shared transient admission.

#### Scenario: Eviction and reselection of a protected tab
- **WHEN** an unmodified protected tab is evicted and then reselected
- **THEN** the source charge is released at eviction, the reload is folded at insertion, the new body becomes the source without a GTK snapshot, and the editor stays in Protected View
