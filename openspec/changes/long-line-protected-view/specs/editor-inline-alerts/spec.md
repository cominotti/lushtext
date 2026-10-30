## ADDED Requirements

### Requirement: Protected View is announced by one bounded inline alert
A document in Protected View SHALL show one inline alert. The alert SHALL state that the document is view-only because it contains very long lines, give the folded paragraph count, and offer an Inspect Line action. When the document entered Protected View through an edit whose entry undo is available, the alert SHALL also offer Undo, which runs that bounded undo. For a recovered draft it SHALL also keep the existing Discard and Save actions of the restored-draft alert, merged into the same single alert. Its title, body and accessible text SHALL be bounded and SHALL NOT contain document text. Invoking a disabled Protected View action SHALL re-surface this alert or publish one rate-limited status message instead of stacking alerts. The alert SHALL replace C1's reduced-features alert for the same document, so that one document never shows both.

#### Scenario: Single alert across repeated blocked actions
- **WHEN** the user presses Ctrl+V ten times in a protected document
- **THEN** exactly one Protected View inline alert is visible, and at most one status message is published within the rate-limit interval

#### Scenario: Undo offered after a paste makes the document protected
- **WHEN** a paste makes a paragraph longer than `L_PROTECT_EDIT` in a document with undo history on
- **THEN** the Protected View alert offers Undo, and choosing it removes the paste and returns the document to the editable view

## MODIFIED Requirements

### Requirement: Scale-gated feature reductions never occupy the inline alert slot
The system SHALL report the features the document scale policy disables (syntax highlighting, bracket matching, the minimap, invisible characters, automatic search highlighting, and `protected_view`, which marks the document for Protected View) outside the text view's allocation path:
- one announced status-bar message, published when the document's tab is the window's selected tab, naming the reduced features and the reason class (total size or very long lines; the batch density reduces no feature);
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

