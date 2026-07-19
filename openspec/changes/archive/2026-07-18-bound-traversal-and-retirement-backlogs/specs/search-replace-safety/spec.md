## ADDED Requirements

### Requirement: Replace Preview confirmation and retirement stay payload-bounded
Replace Preview SHALL keep current checked-match identity incrementally and SHALL NOT scan, filter, or synchronously destroy a near-limit preview outcome in the GTK confirmation path. Confirmation MUST detach the current immutable outcome and checked identity set, partition selected replacements away from GTK, retire unchecked or rejected payloads away from GTK, and invoke Replace All only with rows selected from the still-current preview and search generation. Replaced, invalidated, stale, and exited preview state MUST use the applicable bounded retirement path rather than final document-sized destruction on GTK.

#### Scenario: User confirms a near-limit checked subset
- **WHEN** the current preview contains near-limit replacement data and only a subset of stable match identities remains checked
- **THEN** the GTK action captures current identity without filtering the full outcome
- **AND** worker processing returns only the checked generated replacements to the normal Replace All callback
- **AND** unchecked replacement payloads are destroyed away from GTK

#### Scenario: Preview changes during confirmation selection
- **WHEN** query, replacement, search result, preview, or panel generation changes while worker-side selection is active
- **THEN** the selected stale rows are not passed to Replace All
- **AND** their payload is retired without changing the newer preview

#### Scenario: Entering a new preview replaces a visible outcome
- **WHEN** a new preview request starts while a prior near-limit preview outcome is visible
- **THEN** the prior outcome and checked identity detach from current state immediately
- **AND** their GTK-owned projection and plain-data payload follow their bounded retirement paths

#### Scenario: All generated rows are unchecked
- **WHEN** every generated preview row is unchecked before confirmation
- **THEN** no replacement enters the apply callback
- **AND** the full rejected outcome is retired away from GTK
