## ADDED Requirements

### Requirement: A reduced-features notice explains scale-gated features
The system SHALL show one inline alert per document session when the document scale policy disables any of these features:
- syntax highlighting;
- bracket matching;
- the minimap;
- invisible characters;
- automatic search highlighting;
- `protected_view`, which marks the document for Protected View.

The alert MUST name the reduced features and the reason class: total size, very long lines, or dense long lines. When it is shown, it MUST also name undo history and local history if the total-size tiers reduce them; those reductions alone do not raise the alert. Its title, body and accessible announcement MUST be bounded by the existing accessible-text limits and MUST NOT include document content. The notice MUST rank below every recovery and error alert, MUST wait while one of those is active, and MUST NOT be dropped. It MUST be shown again only when an edit-time downgrade reduces an additional feature. Dismissing it MUST NOT re-enable any feature.

#### Scenario: Long-line document opens
- **WHEN** a document whose longest paragraph disables syntax highlighting and the minimap is opened
- **THEN** an inline alert states that syntax highlighting and the minimap are off because of very long lines

#### Scenario: Recovery alert has priority
- **WHEN** a restored-draft alert is active on a document that also has reduced features
- **THEN** the restored-draft alert is shown first
- **AND** the reduced-features notice is shown after that alert is resolved or dismissed

#### Scenario: Edit reduces another feature
- **WHEN** the notice was dismissed and an edit later disables a feature that was still enabled
- **THEN** the notice is shown again, naming the newly reduced feature

#### Scenario: Notice text is bounded
- **WHEN** the reduced-features notice is announced to assistive technologies
- **THEN** its announcement fits the existing bounded announcement limit and contains no document text
