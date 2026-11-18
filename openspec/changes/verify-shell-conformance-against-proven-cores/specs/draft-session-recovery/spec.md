## ADDED Requirements

### Requirement: The draft service conforms to the verified journal model
A generated conformance test SHALL drive the real draft service over a
temporary data directory, in lockstep with the journal model, through the
service calls the drafts coordination makes for each model action. Crashes
SHALL fall between service calls. After every step, the real manifest
entries, bodies, backing versions, token, and authority SHALL equal the
model's. The real preserved copies SHALL include every copy the model
preserved.

#### Scenario: Service state tracks the model after every step
- **WHEN** a generated sequence of journal operations, faults, crashes, and restarts runs against the real draft service
- **THEN** after each step the manifest entries, bodies, backing versions, token, and authority observed on disk and returned by the service equal the journal model's state
- **AND** every content the model preserved is held by a set-aside copy or a local-history snapshot

#### Scenario: A reported-failed but live write is handled as the model decides
- **WHEN** a registration, body write, manifest commit, or entry removal fails after its rename has landed
- **THEN** the service's later behaviour, and the state the next startup reconstructs, match the journal model's after-effect step

#### Scenario: A fault inside a multi-write step matches a model outcome
- **WHEN** one backend call inside a service step that makes several durable writes fails
- **THEN** the resulting state matches the model's before-effect or after-effect outcome for that step, or the finding is recorded as a step-atomicity environment finding and resolved

#### Scenario: Restarts reload from disk alone
- **WHEN** the script crashes and then starts up
- **THEN** every in-memory value the driver held is dropped, and the next state comes only from loading the data directory
