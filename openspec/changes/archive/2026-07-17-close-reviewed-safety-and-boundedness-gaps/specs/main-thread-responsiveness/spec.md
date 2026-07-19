## ADDED Requirements

### Requirement: Session restore bounds GTK work and load planning
The window SHALL restore persisted tabs through a generation-owned bounded coordinator. The coordinator MUST create no more than the configured number of pages in one GTK turn, MUST admit no more than the configured number of file-backed load-planning operations concurrently, and MUST retain remaining tabs as compact restore descriptors. Tab-derived palette, sidebar, recent-open, status, and related projections MUST remain deferred until one terminal accepted rebuild.

#### Scenario: Large session requires several GTK turns
- **WHEN** a valid session contains more tabs than one restore-turn budget
- **THEN** the window creates pages over multiple scheduled GTK turns
- **AND** input and unrelated main-loop sources can run between restore batches
- **AND** tab order remains equal to the persisted session order

#### Scenario: File-backed restore exceeds planning capacity
- **WHEN** more file-backed tabs await restore than the configured in-flight planning limit
- **THEN** only the admitted subset owns load-planning work
- **AND** later descriptors remain compact until success, failure, cancellation, or editor teardown releases capacity

#### Scenario: Derived projections publish once
- **WHEN** session restoration creates file-backed and untitled tabs across several batches
- **THEN** per-tab open paths do not rebuild aggregate palette, sidebar, recent-open, or status projections
- **AND** one terminal current-generation rebuild publishes the complete accepted tab state

#### Scenario: Restore intent survives batching
- **WHEN** the persisted session selects a later tab or command-line activation targets an opened document while restore is in progress
- **THEN** active-tab priority, unavailable-file retry state, cursor and scroll restoration, and lazy draft markers preserve their existing semantics
- **AND** stale restore callbacks cannot override the accepted current selection

#### Scenario: Window closes during restore
- **WHEN** the window lifetime ends with queued descriptors or admitted plans remaining
- **THEN** the restore generation is cancelled and all permits and projection deferral state are released exactly once
- **AND** no later completion creates a page or publishes projections into the closed window

### Requirement: Plain-data disposal admission never blocks GTK
Document-sized plain-Rust payload retirement SHALL reserve non-blocking worker-destruction capacity before the value is transferred onto GTK, with explicit job-count and retained-byte bounds where payload weight is knowable. Callers with a conservative upper bound SHALL reserve before construction; data-dependent worker results SHALL remain worker-owned until their measured reservation succeeds. The reservation MUST remain attached through replaceable UI ownership so final destruction performs a guaranteed non-blocking worker handoff. When capacity is unavailable, each producer MUST retain at most one compact latest request and one retry or capacity-wakeup source, never an unreserved document-sized payload on GTK. GTK-owned objects MUST remain on GTK and retire through their existing bounded GTK paths.

#### Scenario: Disposal lane is saturated from GTK
- **WHEN** all disposal workers and reserved capacity are occupied and a GTK callback requests another document-sized plain-data operation
- **THEN** reservation fails immediately before that value crosses onto GTK
- **AND** only the compact latest request or worker-held result remains eligible for retry
- **AND** the GTK callback does not wait for a worker or blocking channel send

#### Scenario: Producer is superseded while disposal is full
- **WHEN** one producer already owns a compact pending request and a newer request replaces it before capacity returns
- **THEN** the producer retains at most one latest compact request and one wakeup source
- **AND** any previously published document-sized value keeps its reservation until accepted transfer or final worker destruction
- **AND** superseded data is not restored to current generation state

#### Scenario: Aggregate weighted pressure drains
- **WHEN** several preview, palette, notes, history, search, and undo producers submit weighted plain-data destruction concurrently
- **THEN** admitted job and byte high-water marks stay within the documented lane policy, with exclusive progress for an overweight job when applicable
- **AND** pre-admitted nested final destructors run off GTK
- **AND** every accepted owner or compact pending request is eventually released or cancelled on owner teardown

#### Scenario: Pure-drop workflows leave the shared completion lane
- **WHEN** a `Send` payload requires only final destruction and no GTK result handling
- **THEN** it uses the proven plain-disposal contract rather than holding a generic worker slot for a no-op GTK completion
- **AND** lifecycle-specific blocking destructors remain on their owning worker path
