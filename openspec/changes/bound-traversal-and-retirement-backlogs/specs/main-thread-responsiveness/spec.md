## ADDED Requirements

### Requirement: Document-sized rejected payloads retire outside GTK dispatch
When Replace Preview or Markdown work becomes stale, rejected, superseded, unchecked, or otherwise unprojected, the system SHALL detach it from current generation state immediately. The final destruction of document-sized plain-Rust outcomes, plans, queued sources, and projection tails MUST run away from GTK dispatch through bounded worker ownership. GTK-owned buffers, widgets, models, tags, and links MUST remain on the GTK thread and MUST retire through bounded main-loop slices that cannot mutate the current generation.

#### Scenario: Stale Replace Preview worker returns a near-limit outcome
- **WHEN** a preview outcome becomes stale before its worker completion reaches GTK
- **THEN** GTK rejects it without synchronously destroying its row and text payload
- **AND** bounded worker retirement releases the final plain-data owner

#### Scenario: Markdown projection is superseded with batches remaining
- **WHEN** a newer render generation invalidates a plan with unprojected event batches
- **THEN** the idle callback stops before applying another stale batch
- **AND** the remaining plain-Rust batch tail is destroyed away from GTK dispatch

#### Scenario: Detached Markdown GTK state drains
- **WHEN** an old rendered buffer, embeds, or link targets are detached from the visible preview
- **THEN** they remain GTK-owned and are released within the configured per-turn character and item budgets
- **AND** no retirement slice changes the current rendered generation

### Requirement: Markdown detached generations apply explicit backpressure
Ordinary Markdown rerendering SHALL retain at most two detached GTK render generations and at most one latest pending render request behind retirement. When the detached-generation cap is reached, the system MUST replace older pending requests with the latest generation, MUST retire the superseded pending plain data away from GTK, and MUST defer new ordinary detachment and projection until retirement falls below the cap. Preview readiness MUST remain pending while current planning, projection, admitted image work, detached retirement, or the latest pending render request exists.

#### Scenario: Rapid edits outpace Markdown retirement
- **WHEN** repeated edits request more renders than bounded GTK retirement can drain
- **THEN** detached render ownership never exceeds two ordinary generations
- **AND** at most the latest pending render request is retained
- **AND** the newest current request resumes after retirement creates capacity

#### Scenario: Pending render is superseded repeatedly
- **WHEN** several newer render requests arrive while detached state is at the cap
- **THEN** intermediate pending sources are replaced rather than queued
- **AND** their final plain-data destruction does not occur in the GTK callback

#### Scenario: Preview closes under retirement pressure
- **WHEN** the preview closes with planning, pending, projection, image, and detached retirement work present
- **THEN** every obsolete generation is invalidated and releases ownership through its applicable bounded path
- **AND** no later completion reopens or mutates the closed preview

### Requirement: Workspace-search event turns count every received event
The workspace-search GTK consumer SHALL receive and dispatch at most 250 channel events per scheduled turn. Every successfully received `Match`, `Progress`, `ResultCap`, `Error`, and `Done` event MUST consume one unit before variant-specific handling; channel disconnection MAY terminate the turn without consuming an event unit.

#### Scenario: Progress burst contains no matches
- **WHEN** more than 250 progress events are ready before one consumer turn
- **THEN** that turn receives at most 250 events
- **AND** remaining events wait for a later scheduled turn

#### Scenario: Mixed event burst reaches the cap
- **WHEN** match, progress, result-cap, and error events are interleaved in a ready channel
- **THEN** their combined received count is at most 250 for the turn
- **AND** visible result and diagnostic semantics remain unchanged

#### Scenario: Terminal event is received within budget
- **WHEN** `Done` is the next event before the turn reaches 250 received events
- **THEN** it consumes one event unit and terminates the active search normally
- **AND** the latest pending search may proceed through the existing flight contract

### Requirement: Sliced buffer mutation is reentrancy-safe before GTK calls
For every non-empty sliced delete or insert, the editor-owned replacement session SHALL establish mutation-started state before invoking a signal-emitting GTK mutation API. The session MUST NOT remain mutably borrowed across that GTK call, and continuation MUST revalidate session identity afterward. Synchronous reentrant cancellation or supersession MUST clean partial state exactly once, publish no successful terminal state for the invalidated generation, and leave only the accepted current content editable and saveable.

#### Scenario: First changed signal supersedes replacement synchronously
- **WHEN** the first non-empty sliced mutation emits `changed` and a synchronous handler starts a newer replacement
- **THEN** the older session is already marked as having mutated
- **AND** its cancellation path performs exact partial cleanup without a borrow conflict
- **AND** final buffer content equals only the newer accepted source

#### Scenario: Mutation returns after reentrant cancellation
- **WHEN** control returns from a GTK delete or insert after the session was cancelled reentrantly
- **THEN** the stale continuation does not record progress or schedule another slice
- **AND** projection suppression, editability, saveability, retained text, and terminal ownership are released exactly once
