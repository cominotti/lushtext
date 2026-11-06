## MODIFIED Requirements

### Requirement: Large search projection transitions yield to GTK
Search-result replacement, retirement, cache cleanup, and Replace Preview handoff SHALL avoid whole-result cloning or teardown in one GTK turn, at any result size up to the search's retained-byte budget (`SEARCH_RESULTS_RETAINED_BYTES`). Retired generations MUST be detached immediately and disposed in bounded slices that cannot mutate a newer generation.

#### Scenario: Ten thousand results are replaced
- **WHEN** a new search generation supersedes a result model holding at least 16,384 matches, up to a full retained-byte budget
- **THEN** the old generation is no longer current immediately
- **AND** its rows and auxiliary caches are retired over bounded main-loop turns, none longer than `B_SETTLE_US`

#### Scenario: Replace Preview starts from accepted results
- **WHEN** the user enters Replace Preview for a large accepted result generation
- **THEN** worker construction receives a shared immutable result snapshot without cloning every match on GTK
- **AND** preview identity and checked-row semantics still refer to that exact generation
- **AND** the preview rows are installed in time-sliced turns, none longer than `B_SETTLE_US`

#### Scenario: New results arrive during old-generation disposal
- **WHEN** a disposal slice runs after a newer generation has populated visible results or caches
- **THEN** the slice removes only state owned by the retired generation
- **AND** current rows, match identities, and readiness remain intact

### Requirement: Workspace-search event turns count every received event
The workspace-search GTK consumer SHALL receive and dispatch at most 256 channel events per scheduled turn (`MAX_SEARCH_EVENTS_PER_TICK`, an internal per-turn cap), and SHALL stop receiving once the turn's elapsed time reaches `LIST_TURN_SLICE_US` after at least one event, as decided by the shared list budget's turn continuation. Every successfully received `Match`, `Progress`, `BudgetReached`, `BeyondBudget`, `Error`, and `Done` event MUST consume one unit before variant-specific handling; channel disconnection MAY terminate the turn without consuming an event unit. The matches received in one turn MUST be applied as at most one splice per touched file's child store plus one splice of new file rows on the root store, not one model emission per match.

#### Scenario: Progress burst contains no matches
- **WHEN** more than 256 progress events are ready before one consumer turn
- **THEN** that turn receives at most 256 events
- **AND** remaining events wait for a later scheduled turn

#### Scenario: Mixed event burst reaches the cap
- **WHEN** match, progress, budget, and error events are interleaved in a ready channel
- **THEN** their combined received count is at most 256 for the turn, and the turn stops early once its elapsed time reaches the slice
- **AND** visible result and diagnostic semantics remain unchanged
- **AND** the turn's matches reach the model as one splice per touched file plus one root splice

#### Scenario: Terminal event is received within budget
- **WHEN** `Done` is the next event before the turn reaches 256 received events or the time slice
- **THEN** it consumes one event unit and terminates the active search normally
- **AND** the latest pending search may proceed through the existing flight contract

## ADDED Requirements

### Requirement: Workspace search results are bounded by retained bytes, not a match count
Workspace search SHALL have no match-count cap. The worker SHALL charge every match against `SEARCH_RESULTS_RETAINED_BYTES` through the shared list budget: the match's line bytes plus `SEARCH_MATCH_FIXED_CHARGE`, plus the file's path bytes once per file. It SHALL retain matches while the budget admits them. Admission and the send of an admitted match SHALL be one serialized step across the parallel walkers, so that the channel delivers matches in admission order. At the first refusal it SHALL send one `BudgetReached` event, queued behind every retained match, and retain nothing further. It SHALL keep walking and counting matches until the traversal ends, is cancelled, or terminates as incomplete, and SHALL report the running total at most every 250 ms and the final total with `Done`. The budget SHALL be a power of two at or above `main`'s worst-case retention under the former 16,384-match cap, and a compile-time assertion SHALL hold that inequality. Each file's path SHALL be retained once, shared by its file row, its match rows, and the navigation index.

#### Scenario: A search past 16,384 matches is not cut
- **WHEN** a search finds 40,000 matches of ordinary lines (under 256 bytes) in 2,000 files
- **THEN** all 40,000 matches are shown and navigable with F4 and Shift+F4
- **AND** no limited state is shown
- **AND** Replace Preview lists the first 16,384 of them (its own unchanged row budget, `MAX_REPLACE_PREVIEW_ROWS`) and states the omitted count, as it does today when its budget cuts

#### Scenario: Nothing main showed is lost at main's worst case
- **WHEN** a search finds 16,384 matches, each a 4,096-byte line in its own file with a path of 4,096 bytes
- **THEN** every one of the 16,384 matches is retained and shown

#### Scenario: The budget cuts a pathological search
- **WHEN** retaining the next match would exceed the retained-byte budget
- **THEN** the retained matches are exactly the prefix delivered before it, with no gaps
- **AND** the panel states how many matches are shown and how many more were found, updating that count until the search ends
- **AND** the process's peak resident growth for the results stays at most 5/4 of the budget

#### Scenario: Cancelling a counting search
- **WHEN** the user cancels or closes the panel while the worker is counting matches past the budget
- **THEN** counting stops through the existing cancellation token, and no later count reaches the panel

### Requirement: Workspace list dispatches fit the settle budget
Every GTK dispatch that delivers workspace-search results, reveals the first results, installs Replace Preview rows, applies a sidebar directory's rows, or publishes a sidebar directory's terminal SHALL complete within `B_SETTLE_US`, at every size up to the respective retained-byte budget. This SHALL be measured on the optimized widget build with the surface presented, at host load below 4, interleaved with `main`. The turn that first reveals the search results SHALL apply at most one batch. A sidebar directory's path caches SHALL be maintained per applied batch, so that its terminal dispatch does not grow with the directory size. A dispatch that cannot fit because of a toolkit cost SHALL be recorded as a measured toolkit-floor exception, citing a GTK axiom with a probe and a sample, and SHALL NOT be left unexplained.

#### Scenario: First results of a large search
- **WHEN** a search delivering at least 16,384 matches reveals its first results
- **THEN** the worst dispatch from the first match to `Done` is within `B_SETTLE_US`, or is a recorded toolkit-floor exception

#### Scenario: Entering Replace Preview over a large result
- **WHEN** the user enters Replace Preview over 16,384 accepted matches
- **THEN** no dispatch until the preview terminal exceeds `B_SETTLE_US`

#### Scenario: Expanding a directory of 131,072 entries
- **WHEN** a directory of 131,072 entries is expanded in the sidebar
- **THEN** no dispatch from the expansion to the directory's terminal exceeds `B_SETTLE_US`
- **AND** the terminal dispatch's duration does not grow with the entry count, compared at 10,000 and 131,072 entries

#### Scenario: Main's slow dispatches are measured first
- **WHEN** the change starts implementation
- **THEN** `main`'s first-results reveal, Replace Preview entry, and 10,000-entry expansion are re-measured with the recorded measurement code and attributed to their costs before any fix is written

### Requirement: List budgets share one proved policy
Retained-byte admission and per-turn continuation for workspace lists SHALL come from one GTK-free, float-free module (`model/list_budget.rs`) used by both workspace search and the workspace sidebar. Admission SHALL saturate, SHALL never let the charged total exceed the limit, and SHALL latch on its first refusal, so that what is retained is a prefix of delivery order. Turn continuation SHALL let every turn make at least one unit of progress and SHALL stop at the first unit started at or after `LIST_TURN_SLICE_US` (`B_INTERACT_US / 2`). Kani SHALL prove these properties in the shard `core-list-budget`, together with the no-loss statement that a limit of at least `n × w` retains every sequence of at most `n` charges of at most `w`.

#### Scenario: Admission never exceeds its limit
- **WHEN** any sequence of charges is admitted against any `u64` limit
- **THEN** the charged total never exceeds the limit

#### Scenario: A refusal latches
- **WHEN** a charge is refused
- **THEN** every later charge is refused, even one that would fit

#### Scenario: The budget keeps what a count cap kept
- **WHEN** the limit is at least `n × w`
- **THEN** any sequence of at most `n` charges, each at most `w`, is retained in full
- **AND** a limit one below `n × w` refuses the `n`-th worst-case charge (a counter-example harness)

#### Scenario: Every turn makes progress
- **WHEN** a turn starts with its elapsed time already past the slice
- **THEN** it still processes one unit and then yields
