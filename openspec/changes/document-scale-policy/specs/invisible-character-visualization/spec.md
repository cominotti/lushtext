## ADDED Requirements

### Requirement: Invisible-character drawing respects the document scale policy
The system SHALL draw invisible-character hints only when the document scale policy allows invisibles for the active document. When the policy disallows them, at load or through an edit-time downgrade, the system MUST switch the source view's space drawer off. It MUST present the `Whitespace Only` and `All` modes as unavailable for that document, with feedback that names the reason. It MUST keep the user's chosen mode for other documents. It MUST NOT change the document contents, its saved bytes or its modified state.

#### Scenario: Long-line document disables invisibles
- **WHEN** the user selects `All` mode on a document whose policy disallows invisibles
- **THEN** no invisible-character hints are drawn
- **AND** the user is told that invisible characters are unavailable because of very long lines

#### Scenario: Edit downgrades invisibles in the same emission
- **WHEN** invisibles are visible and an edit creates a paragraph longer than the invisibles threshold
- **THEN** the space drawer is switched off before the edit's signal emission returns
- **AND** the mode is not re-applied for the rest of the document session

#### Scenario: Other documents keep their mode
- **WHEN** one document has invisibles gated off and the user switches to a document whose policy allows them
- **THEN** that document draws hints according to its own selected mode
