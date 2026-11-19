## ADDED Requirements

### Requirement: The draft service conforms to the verified journal model
A generated conformance test SHALL drive the real draft service over a
temporary data directory, in lockstep with the journal model, through the
service calls the drafts coordination makes for each model action. Crashes
SHALL fall between model steps. After every step, the real manifest
entries, bodies, backing versions, token, and authority SHALL equal the
model's, and the real set-aside copies SHALL include every content the
model preserved.

#### Scenario: Service state tracks the model after every step
- **WHEN** a generated sequence of journal operations, faults, crashes, and restarts runs against the real draft service
- **THEN** after each step the manifest entries, bodies, backing versions, token, and authority observed on disk and returned by the service equal the journal model's state
- **AND** every content the model preserved is held by a set-aside copy; a local-history snapshot does not count, because the service does not count it as preservation

#### Scenario: A reported-failed but live write is handled as the model decides
- **WHEN** a body write or any manifest commit (a registration, an autosave commit, an entry removal, orphan cleanup, or startup reconciliation) fails after its rename has landed
- **THEN** the service's later behaviour, and the state the next startup reconstructs, match the journal model's after-effect step

#### Scenario: A fault inside a multi-write step matches a model outcome
- **WHEN** one backend call inside a service step that makes several durable writes fails
- **THEN** the resulting state matches the model's before-effect or after-effect outcome for that step, or the finding is recorded as a step-atomicity environment finding and resolved

#### Scenario: Environment-decided choices come from the real service
- **WHEN** a model step takes a choice the real environment decides, such as a restore's service-determined ending or a crash's keep-or-revert of an unconfirmed effect
- **THEN** the driver hands the model the outcome the real service produced, and a real outcome the model does not admit is a divergence

#### Scenario: Restarts reload from disk alone
- **WHEN** the script crashes and then starts up
- **THEN** every in-memory value the driver held is dropped, and the next state comes only from loading the data directory

### Requirement: A set-aside copy is kept only once it is durable
When the system copies a draft body into the set-aside location, the copy
SHALL count as kept only once the set-aside directory has been synced after
the copy landed. A copy whose directory sync fails SHALL be reported as not
kept, so the caller keeps the journal body and its entry, and a later retry
that finds the same bytes under that name SHALL count them as kept only after
syncing the set-aside directory. A body moved (not copied) into the set-aside
location MAY count as placed when only its directory sync failed, because a
reverted rename returns the body to the journal directory and never loses it.

#### Scenario: A copy whose directory sync fails preserves nothing
- **WHEN** a stale draft body is copied into the set-aside location and the set-aside directory sync fails
- **THEN** preservation reports a failure, the body and its manifest entry stay in the journal, and nothing is deleted

#### Scenario: A retry confirms the copy it finds
- **WHEN** a later preservation finds a byte-identical copy under the same set-aside name
- **THEN** it syncs the set-aside directory first, and reports the body kept only when that sync succeeds, without writing a second copy
