## ADDED Requirements

### Requirement: Scale-gated feature reductions never occupy the inline alert slot
The system SHALL report the features the document scale policy disables (syntax highlighting, bracket matching, the minimap, invisible characters, automatic search highlighting, and `protected_view`, which marks the document for Protected View) outside the text view's allocation path:
- one announced status-bar message, published when the document's tab is the window's selected tab, naming the reduced features and the reason class (total size, very long lines, or dense long lines);
- a persistent "Editor Features" row in the document properties carrying the same text;
- the editor's accessible description.

When the message is published, it MUST also name undo history and local history if the total-size tiers reduce them; those reductions alone do not raise it. The message MUST be bounded by the existing accessible-text limits and MUST NOT include document content. It MUST be published again only when an edit-time downgrade reduces an additional feature. The report MUST NOT use the editor's inline alert slot and MUST NOT change the text view's allocated width or height, because a reallocation makes GTK lay out every visible paragraph again, which costs hundreds of milliseconds for a paragraph of hundreds of thousands of characters.

#### Scenario: Long-line document opens
- **WHEN** a document whose longest paragraph disables syntax highlighting and the minimap is opened in the selected tab
- **THEN** a status-bar message states that syntax highlighting and the minimap are off because of very long lines
- **AND** the document properties' Editor Features row states the same

#### Scenario: The text view keeps its allocation
- **WHEN** an edit makes a paragraph long enough to disable syntax highlighting
- **THEN** the text view's allocated width and height do not change and no inline alert is shown

#### Scenario: Recovery alert keeps the inline slot
- **WHEN** a restored-draft alert is active on a document whose scale then reduces a feature
- **THEN** the restored-draft alert stays on the inline bar and the reduction is reported in the status bar

#### Scenario: Background tab
- **WHEN** a document's scale is reduced while its tab is not selected
- **THEN** the message is published the first time its tab is selected

#### Scenario: Edit reduces another feature
- **WHEN** an edit later disables a feature that was still enabled
- **THEN** the message is published again, naming the newly reduced feature

#### Scenario: Notice text is bounded
- **WHEN** the reduced-features message is announced to assistive technologies
- **THEN** its announcement fits the existing bounded announcement limit and contains no document text
