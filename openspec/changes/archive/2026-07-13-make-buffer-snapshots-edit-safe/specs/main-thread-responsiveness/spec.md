## ADDED Requirements

### Requirement: Chunked buffer snapshot positions remain valid across yields
The system MUST NOT retain `GtkTextIter` positions across main-loop turns in which the source buffer can change. Chunked snapshots SHALL use GTK-stable position ownership, SHALL detect source-buffer mutation, and SHALL remove every temporary mark, signal handler, timer source, and callback on each terminal outcome.

#### Scenario: Buffer changes between snapshot slices
- **WHEN** an insertion or deletion changes the source buffer after one chunk and before the next scheduled slice
- **THEN** the snapshot terminates as cancelled before reusing an invalid position
- **AND** no mixed-generation or partial text reaches its consumer

#### Scenario: Snapshot completes without mutation
- **WHEN** every chunk is captured from one unchanged buffer generation
- **THEN** the consumer receives the complete text exactly once
- **AND** the snapshot's temporary marks, handlers, and sources are released

#### Scenario: Editor closes during capture
- **WHEN** the source widget or owning workflow is disposed while a snapshot is pending
- **THEN** later sources perform no consumer callback or GTK mutation
- **AND** temporary snapshot state does not retain the editor indefinitely

### Requirement: Snapshot consumers handle cancellation explicitly
Every chunked snapshot consumer SHALL choose an explicit cancellation policy appropriate to its workflow. Persistence workflows MUST remain retryable, stale analysis and previews MUST be discarded or superseded, and save workflows that freeze editing MUST restore interactivity on every terminal outcome.

#### Scenario: Draft capture is cancelled by an edit
- **WHEN** draft autosave snapshotting is cancelled because the source buffer changed
- **THEN** the editor remains draft-dirty
- **AND** one coalesced later autosave remains eligible to capture the latest generation

#### Scenario: Analysis or preview is superseded
- **WHEN** encoding analysis, note preview, or local-history preparation is cancelled by newer content
- **THEN** the stale consumer result is not rendered or applied
- **AND** the owning debounce or generation policy may schedule the latest request

#### Scenario: Save snapshot reaches a terminal outcome
- **WHEN** a save freezes the editor while chunked snapshotting runs
- **THEN** success proceeds with one coherent snapshot
- **AND** cancellation or failure restores the prior editable and cursor-visible state without clearing modified content
