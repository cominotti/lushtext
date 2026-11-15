## ADDED Requirements

### Requirement: In-tab Replace All uses the safe GtkSourceView binding
The in-tab search bar's Replace All SHALL call GtkSourceView through the safe `sourceview5` binding, which returns the number of matches it replaced. A match-free Replace All SHALL be a successful no-op. A `GError` SHALL be logged, never unwrapped, under the search-diagnostics requirement. The match counter SHALL be refreshed in every case, and the call SHALL use no `unsafe` code.

#### Scenario: Replace All with no match is a no-op
- **WHEN** the user runs Replace All in the in-tab search bar and the query matches nothing in the document
- **THEN** the document is unchanged and the application does not panic
- **AND** the match counter still reflects zero matches

#### Scenario: Replace All replaces every match
- **WHEN** the user runs Replace All and the query matches several places
- **THEN** every match is replaced and the counter shows no remaining matches

#### Scenario: A GtkSourceView error leaves the document and counter consistent
- **WHEN** GtkSourceView reports a `GError` for the Replace All, for example for a malformed regular-expression replacement
- **THEN** the document is unchanged and the application does not panic
- **AND** the match counter is refreshed
