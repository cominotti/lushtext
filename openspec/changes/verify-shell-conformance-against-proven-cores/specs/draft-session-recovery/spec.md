## ADDED Requirements

### Requirement: The draft service and drafts coordination conform to the verified journal model
The system SHALL have a generated conformance test in which the real
`draft_service` operates on a temporary data directory. The model's operations
SHALL map onto the service calls that the drafts coordination makes:

- Register onto `register_draft_entries`;
- WriteBody onto `write_draft`;
- Commit onto `update_manifest`;
- DeletionStep onto `resolve_draft_restore`, `preserve_stale_draft_body`,
  `delete_draft_file`, and `remove_manifest_entry`;
- Inspect and ExecCleanup onto `inspect_orphan_cleanup_from` and
  `execute_orphan_cleanup`;
- Startup onto `load_restore_state_cancellable`;
- Save and ExternalMtime onto changes to the backing file;
- Crash onto dropping every in-memory value.

Each step SHALL be driven in lockstep with the journal model, with faults
injected before and after effects. After every step, the test SHALL check that
the abstraction of the real state equals the model's. That abstraction covers
manifest entries and their backing versions, body contents, set-aside and
local-history preservation, the registration token, and the manifest
authority. The test SHALL run with one window driver and, over one service
instance, with two in-process window drivers. A bounded set of scripted
multi-window and single-window widget scenarios SHALL make the same comparison
for the GTK drafts coordination, using `DraftEvidence` and the on-disk
abstraction after every settled step. Every scenario SHALL be derived from a
recorded model trace.

#### Scenario: Service state tracks the model after every step
- **WHEN** a generated sequence of journal operations, faults, crashes, and restarts runs against the real draft service
- **THEN** after each step the manifest entries, bodies, backing versions, preserved copies, and authority observed on disk and returned by the service equal the journal model's state

#### Scenario: A reported-failed but persisted write is handled as the model decides
- **WHEN** a registration, body write, or manifest commit fails after its rename has landed
- **THEN** the service's subsequent behaviour, and the state the next startup reconstructs, match the journal model's after-effect fault step

#### Scenario: Two windows share one process's service
- **WHEN** two window drivers interleave operations over one draft service instance and one data directory
- **THEN** the real state tracks the two-window journal model, including the process-wide manifest lock and target guard

#### Scenario: The GTK coordination follows a model trace
- **WHEN** a scripted widget scenario replays a recorded model trace through real windows
- **THEN** after every settled step `DraftEvidence` and the on-disk abstraction equal the model's state for that trace
