## ADDED Requirements

### Requirement: Automation reports when an opened document has settled
The automation interface SHALL provide a `document-settled` readiness predicate for the selected editor. It holds when all of the following hold:
- `file-open-complete` holds and the language attach stage has run;
- no post-open slice, publish or debounce is pending;
- the editor's vertical adjustment `upper` has not changed for the settle-quiet period;
- no `highlight-updated` has been seen for the settle-quiet period.

The tab snapshot SHALL report a bounded settle state: the current post-open stage and whether settle is complete. The snapshot SHALL be projected from the load workflow's evidence surface. Reading it SHALL NOT mutate state or make the toolkit do work. The predicate and the fields SHALL be documented in `docs/automation.md` and `docs/automation-reference.md`.

#### Scenario: A smoke helper waits for an opened document to settle
- **WHEN** a smoke helper calls `WaitForReady` with `document-settled` after opening a document
- **THEN** LushText reports readiness only once the document's post-open stages have completed and highlighting and validation have been quiet for the settle-quiet period

#### Scenario: No editor is selected
- **WHEN** `document-settled` is requested while no editor tab exists
- **THEN** the predicate holds immediately, as `file-open-complete` does
