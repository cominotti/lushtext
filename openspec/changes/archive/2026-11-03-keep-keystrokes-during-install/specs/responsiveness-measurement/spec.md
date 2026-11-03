## ADDED Requirements

### Requirement: The lane gates keys typed while the editor holds input
The measurement build's probe SHALL class a key that arrives while its editor holds typed input (a workflow owns the buffer, a load is in flight, or held text waits for its insertion) as headed to a held editor, distinct from an editable and a read-only editor. Its insertion mark SHALL be recorded once per inserted character, so one insertion of held text counts every character it inserts. The lane SHALL count keys into a held editor as keys into an editor: a typing phase whose insertions are fewer than its keys into an editable or a held editor SHALL fail the run and be listed under `misdirected_typing`. Keys into a held editor SHALL be reported apart from editable typing, with their count and latency, and SHALL NOT enter its input-to-frame figures, because what they insert is drawn only when the held text is inserted.

#### Scenario: Keys typed during an install are all inserted
- **WHEN** a `type-during-settle` phase types keys while the document's install holds the buffer
- **THEN** the probe classes those keys as headed to a held editor, the phase reports them apart, and the phase passes only when the insertions it recorded equal every key into an editor

#### Scenario: Held keys that are never inserted fail the run
- **WHEN** keys into a held editor are not followed by their insertion before the phase ends
- **THEN** the phase is listed under `misdirected_typing` and the run fails
