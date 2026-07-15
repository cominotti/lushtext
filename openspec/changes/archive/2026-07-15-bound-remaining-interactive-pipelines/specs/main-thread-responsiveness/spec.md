## ADDED Requirements

### Requirement: Markdown preview rendering is bounded across planning and projection
The system SHALL render Markdown preview through a generation-owned GTK-free plan and bounded GTK projection slices. Automatic rendering MUST enforce deterministic source, event/node, embed, and per-slice work budgets, and MUST expose a limited or paused terminal state when an input exceeds those budgets.

#### Scenario: Dense Markdown stays below the byte pause threshold
- **WHEN** a document is small enough for automatic preview but expands into more render events or GTK nodes than the configured render budget
- **THEN** planning terminates at the deterministic budget with an explicit limited preview state
- **AND** GTK does not build the unbounded remainder in one callback

#### Scenario: Accepted plan needs many GTK nodes
- **WHEN** a current render plan contains more nodes than one projection slice permits
- **THEN** GTK applies it over bounded main-loop turns
- **AND** input, repaint, and other completions can run between slices

#### Scenario: Preview generation changes during projection
- **WHEN** the document, preview mode, or page lifetime changes while a plan or projection session is active
- **THEN** all later work owned by the stale generation is discarded
- **AND** it cannot insert widgets, tags, placeholders, or terminal state into the newer preview

### Requirement: Markdown image work has bounded generation ownership
Local-image preview work SHALL be admitted by current render generation under explicit count and byte limits. Excess, oversized, stale, or failed image work MUST resolve to an accessible placeholder or stale discard without allowing queued image payloads to grow with embed count.

#### Scenario: Preview contains many local images
- **WHEN** one accepted render contains more local-image descriptors than its image admission budget
- **THEN** only the bounded admitted subset may own active or queued image payloads
- **AND** remaining embeds render deterministic accessible placeholders

#### Scenario: Stale image completes after rerender
- **WHEN** an image decode or load completes after a newer preview generation owns the surface
- **THEN** the completion releases its payload ownership and is ignored
- **AND** the newer preview remains unchanged

### Requirement: Large search projection transitions yield to GTK
Search-result replacement, retirement, cache cleanup, and Replace Preview handoff SHALL avoid whole-result cloning or teardown in one GTK turn. Retired generations MUST be detached immediately and disposed in bounded slices that cannot mutate a newer generation.

#### Scenario: Ten thousand results are replaced
- **WHEN** a new search generation supersedes a result model at the configured maximum result count
- **THEN** the old generation is no longer current immediately
- **AND** its rows and auxiliary caches are retired over bounded main-loop turns

#### Scenario: Replace Preview starts from accepted results
- **WHEN** the user enters Replace Preview for a large accepted result generation
- **THEN** worker construction receives a shared immutable result snapshot without cloning every match on GTK
- **AND** preview identity and checked-row semantics still refer to that exact generation

#### Scenario: New results arrive during old-generation disposal
- **WHEN** a disposal slice runs after a newer generation has populated visible results or caches
- **THEN** the slice removes only state owned by the retired generation
- **AND** current rows, match identities, and readiness remain intact
