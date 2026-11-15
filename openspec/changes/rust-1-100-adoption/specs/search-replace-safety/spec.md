## ADDED Requirements

### Requirement: In-tab Replace All uses the safe GtkSourceView binding
The in-tab search bar's Replace All SHALL call GtkSourceView through the safe `sourceview5` binding, which returns the number of matches it replaced. A Replace All that matches nothing SHALL be a no-op that reports zero replacements and neither panics nor logs an error. A `GError` from GtkSourceView SHALL be logged without document text, and the match counter SHALL be refreshed in every case. The search bar SHALL contain no `unsafe` code for this call.

#### Scenario: Replace All with no match is a no-op
- **WHEN** the user runs Replace All in the in-tab search bar and the query matches nothing in the document
- **THEN** the document is unchanged and the application does not panic
- **AND** the match counter still reflects zero matches

#### Scenario: Replace All replaces every match
- **WHEN** the user runs Replace All and the query matches several places
- **THEN** every match is replaced and the counter shows no remaining matches
