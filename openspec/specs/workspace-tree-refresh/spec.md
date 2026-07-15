# workspace-tree-refresh Specification

## Purpose
Keep each workspace section's visible folder trees aligned with on-disk changes while preserving the section's current browsing context and avoiding visually disruptive rebuilds.

## Requirements

### Requirement: Workspace sections refresh automatically for external filesystem changes
The system SHALL keep each workspace section's visible folder trees aligned with files and directories inside the sidebar's currently materialized scope when those paths are created, removed, renamed, or moved outside the LushText sidebar workflow. Automatic watching MUST prefer the visible top-level workspace folder rows and expanded directories needed to keep the rendered tree current, rather than recursively watching every descendant under every broad configured folder at startup.

#### Scenario: External file creation appears in the tree
- **WHEN** a new file is created on disk under a workspace folder that is currently visible in the sidebar
- **THEN** the corresponding workspace section shows the new file without requiring the user to remove and re-add the folder or reopen the workspace

#### Scenario: External removal clears stale rows
- **WHEN** a file or directory that is currently shown in a workspace section is removed on disk outside LushText
- **THEN** the workspace section removes the stale row after refresh processing settles
- **AND** the tree no longer exposes actions for the removed path

#### Scenario: External rename updates the visible tree
- **WHEN** a visible file or directory inside a workspace folder is renamed outside LushText
- **THEN** the workspace section stops showing the old path
- **AND** the workspace section shows the renamed path in the correct sorted position

#### Scenario: Broad folder with unreadable deep descendants does not block startup
- **WHEN** a workspace folder points at a broad directory such as the user's home folder and some deep descendant paths are unreadable to the watcher backend
- **THEN** the workspace section still renders its visible tree without waiting for a recursive watch across every descendant
- **AND** automatic refresh covers the currently materialized folder rows and expanded directories
- **AND** the user can still use the manual `Refresh` control for broader reloads

#### Scenario: Zero-folder workspace starts no folder watchers
- **WHEN** a workspace section contains zero folders
- **THEN** automatic refresh does not attempt to watch a fake folder
- **AND** the workspace section remains usable for adding folders

### Requirement: Workspace sections expose a manual refresh control
The system SHALL show a `Refresh` button in each workspace-section header as the rightmost header-control button, and invoking it MUST refresh that workspace section using the same tree-reload behavior as automatic refresh for each configured workspace folder. The refresh control MUST remain available without any adjacent replace-root control.

#### Scenario: Refresh button placement in the header
- **WHEN** a workspace section header is rendered
- **THEN** it shows a `Refresh` control in the rightmost header-control position
- **AND** no replace-root control appears to the right of it

#### Scenario: Manual refresh reloads stale content across folders
- **WHEN** the user activates the `Refresh` control for a workspace section whose folder trees are stale
- **THEN** that workspace section reloads visible tree data for its configured folders
- **AND** newly added, removed, or renamed paths appear in the refreshed result

#### Scenario: Manual refresh of an empty workspace is harmless
- **WHEN** the user activates `Refresh` for a workspace with zero folders
- **THEN** no filesystem tree reload is attempted
- **AND** the section remains visible with its add-folder action available

### Requirement: Manual refresh remains visually stable
The system SHALL keep manual refresh visually stable. Triggering the `Refresh` control MUST NOT blank, flash, collapse, or reconstruct unchanged visible rows in the workspace tree when the currently materialized folder trees can be reconciled in place.

#### Scenario: Manual refresh keeps unchanged folder rows mounted
- **WHEN** the user triggers manual refresh for a workspace whose visible top-level folder rows still represent the same paths after the reload
- **THEN** the workspace section keeps the existing tree models mounted where possible
- **AND** unchanged visible rows remain visually stable while refreshed data is applied

#### Scenario: Expanded workspace refresh avoids subtree blanking
- **WHEN** the user triggers manual refresh while a workspace folder or nested directory is expanded
- **THEN** refreshed child rows appear without first blanking the existing subtree contents
- **AND** the refresh preserves expansion and selection for unchanged paths

#### Scenario: Reordered folders keep stable refreshed content
- **WHEN** folders were reordered before a manual refresh
- **THEN** refresh preserves the persisted folder order
- **AND** it does not restore the old order from stale tree state

### Requirement: Refresh preserves section context when possible
The system SHALL preserve the current drill-down scope, expanded rows, selected row, and top-level folder order across a refresh whenever the corresponding paths still exist after the refreshed tree is applied.

#### Scenario: Expanded rows stay expanded after refresh
- **WHEN** a workspace section refreshes and an expanded directory still exists afterward
- **THEN** that directory remains expanded in the refreshed tree

#### Scenario: Selection is restored when the path still exists
- **WHEN** the selected file or directory still exists after a workspace-section refresh
- **THEN** the refreshed tree restores selection to that same path

#### Scenario: Removed selection is cleared safely
- **WHEN** the selected path no longer exists after a workspace-section refresh
- **THEN** the refresh completes without leaving a broken selection pointing at a missing path

#### Scenario: Folder order survives refresh
- **WHEN** a workspace has folders A, B, and C in persisted order
- **AND** the workspace section refreshes
- **THEN** the top-level folder trees remain ordered A, B, C

### Requirement: Automatic refresh remains visually stable
The system SHALL keep automatic workspace refresh visually stable. Refreshing visible rows MUST NOT blank, flash, collapse, or otherwise visibly re-render unchanged portions of any folder tree because of watcher noise or subtree reload mechanics.

#### Scenario: Access-only watcher noise is ignored
- **WHEN** the watcher backend emits access or open events that do not change the visible tree shape
- **THEN** the workspace section does not trigger a tree refresh from those events

#### Scenario: Real subtree refresh keeps unchanged rows mounted
- **WHEN** a visible directory refreshes because a child was created, removed, or renamed
- **THEN** unchanged rows in that directory remain visually stable
- **AND** the workspace section does not blank the directory contents before showing the updated result

#### Scenario: Overlapping folders refresh independently
- **WHEN** a file under `/repo/src` is visible through both `/repo` and `/repo/src` workspace folder trees
- **AND** that file is changed outside LushText
- **THEN** automatic refresh may update both visible tree locations
- **AND** it does not collapse either folder tree merely because both rows point at the same canonical file

### Requirement: Refresh failures surface recoverable feedback
The system SHALL surface lightweight user-visible feedback when automatic watching cannot keep a workspace section current or when a manual refresh fails to reload the latest tree state for one or more workspace folders.

#### Scenario: Watcher startup or runtime failure
- **WHEN** automatic workspace refresh cannot start or later stops because the watcher backend fails for a workspace folder
- **THEN** the user receives feedback that automatic refresh is unavailable for that workspace section or folder
- **AND** the manual `Refresh` control remains available

#### Scenario: Manual refresh cannot complete
- **WHEN** the user triggers a manual refresh and the workspace section cannot reload the latest tree state
- **THEN** the user receives feedback that the refresh failed
- **AND** the previously rendered tree remains in a usable state

### Requirement: Materialized watch targets update incrementally
The system SHALL maintain the deduplicated workspace watch-target set as flattened tree rows and expansion states change. Updating the set MUST do work proportional to affected rows rather than rewalking the complete flattened tree, and overlapping visible rows MUST retain correct target reference counts.

#### Scenario: Expand one nested directory
- **WHEN** the user expands one directory in a large workspace tree
- **THEN** that directory's non-recursive target is added through the affected row update
- **AND** the section does not rescan every flattened row to derive the new set

#### Scenario: Collapse a branch with expanded descendants
- **WHEN** a branch containing several expanded descendant directories is collapsed
- **THEN** targets contributed only by the removed flattened descendants are released
- **AND** targets still contributed by another overlapping visible row remain active

#### Scenario: Effective target set does not change
- **WHEN** tree signals update row presentation without changing the deduplicated materialized target set
- **THEN** the watcher generation does not restart
- **AND** the current backend watcher remains installed

#### Scenario: Zero-folder workspace has no targets
- **WHEN** a workspace contains no configured folders and no materialized rows
- **THEN** the incremental target set is empty
- **AND** no fake or fallback filesystem target is created

### Requirement: Watcher lifecycle work stays off the GTK main thread
The system SHALL perform watcher creation, target registration, replacement teardown, and stale-handle disposal outside GTK callbacks. GTK MUST receive only an owned watcher result or typed failure and MUST keep the current rendered tree and controls usable while replacement is in progress.

#### Scenario: Slow watcher startup
- **WHEN** the watcher backend takes noticeable time to create or register many materialized targets
- **THEN** startup work runs on a background worker
- **AND** sidebar input, repaint, scrolling, and manual Refresh remain schedulable

#### Scenario: Slow watcher teardown
- **WHEN** dropping the old backend watcher blocks while its resources shut down
- **THEN** teardown occurs off the GTK main thread
- **AND** replacing or hiding the workspace section does not synchronously stall the UI

#### Scenario: Empty target set retires a watcher
- **WHEN** the latest materialized target set becomes empty
- **THEN** the old watcher is retired outside the GTK callback
- **AND** no poll source remains installed for that section

### Requirement: Watcher replacement is generation-safe
The system SHALL associate each effective target snapshot with a monotonically advancing generation. A startup success or failure MUST affect the section only if its generation and section lifetime remain current; stale watcher handles MUST be disposed off-thread.

#### Scenario: Scope changes during watcher startup
- **WHEN** a watcher is starting and the workspace filter, focus folder, folders, or expansion state produces a newer target generation
- **THEN** the older watcher is never installed
- **AND** the newest generation remains authoritative

#### Scenario: Stale startup succeeds
- **WHEN** an obsolete watcher startup returns successfully after a newer generation exists
- **THEN** the obsolete handle is disposed outside the GTK callback
- **AND** it does not replace or clear the current watcher error state

#### Scenario: Stale startup fails
- **WHEN** an obsolete watcher startup reports an error after a newer generation exists
- **THEN** the old error is ignored
- **AND** the current section does not show feedback for obsolete targets

#### Scenario: Section is destroyed during startup
- **WHEN** a workspace section is disposed before its background watcher startup completes
- **THEN** no poll source or watcher is installed on the destroyed section
- **AND** any returned watcher is disposed off-thread

### Requirement: Responsive watcher replacement preserves refresh semantics
The system SHALL retain materialized-scope non-recursive watching, access-noise filtering, overlapping-folder updates, stable tree reconciliation, recoverable warnings, and manual Refresh while watcher lifecycle changes are pending or failed.

#### Scenario: Current-generation startup fails
- **WHEN** the watcher backend cannot register a current materialized path
- **THEN** the existing rendered tree remains mounted and usable
- **AND** the section exposes one recoverable automatic-refresh warning
- **AND** manual Refresh remains reachable

#### Scenario: Overlapping folder target remains valid
- **WHEN** the same canonical directory is materialized through overlapping workspace folders
- **THEN** one deduplicated backend target represents all current row contributions
- **AND** removing one contribution does not stop watching while another remains

#### Scenario: Constrained sidebar during restart
- **WHEN** watcher replacement occurs while a workspace with long paths is shown in a narrow sidebar
- **THEN** header controls and Refresh remain visible
- **AND** only the file-tree item region scrolls
- **AND** no horizontal scrollbar or transient fake row is introduced

### Requirement: Workspace watcher responsiveness has layered coverage
The project SHALL add pure target-set tests, service integration tests, GTK widget tests, accessibility/geometry checks, and performance fixtures for empty, representative, many-target, overlapping, unreadable, slow-backend, stale-completion, reorder, expansion, collapse, and constrained-sidebar states.

#### Scenario: Incremental state matches full oracle
- **WHEN** generated sequences of folder, row, expansion, collapse, refresh, and reorder events are applied
- **THEN** the incremental deduplicated set matches a test-only full derivation oracle after every step
- **AND** reference counts never underflow or retain removed-only targets

#### Scenario: Many expanded rows avoid GTK full scans
- **WHEN** a performance fixture changes one target in a tree with many expanded rows
- **THEN** target bookkeeping touches only the affected splice or row state
- **AND** watcher construction and disposal time is excluded from the GTK main-thread interval

### Requirement: Watcher event delivery uses a bounded coalescing mailbox
The watcher backend SHALL normalize each raw tree-changing event outside GTK and merge it directly into one bounded pending notice without first retaining an app-unbounded debouncer queue or event vector. The notice SHALL contain either a capped unique path set or a conservative full-refresh marker; exceeding any retained-path bound, observing an ambiguous rename shape, or encountering producer-side lock contention MUST promote through constant-space state to full refresh rather than silently losing visible changes or creating backlog.

#### Scenario: Event burst stays below the path cap
- **WHEN** raw create, remove, and rename events produce a unique changed-path set within the configured cap
- **THEN** the mailbox retains one deduplicated bounded path notice
- **AND** GTK receives no access-only or duplicate paths

#### Scenario: Event burst exceeds the path cap
- **WHEN** unique tree-changing paths exceed the configured cap before GTK consumes them
- **THEN** the pending notice becomes a full-refresh marker
- **AND** additional raw events do not grow retained memory

#### Scenario: Producer outruns GTK polling
- **WHEN** raw backend callbacks arrive faster than the next GTK poll can consume them
- **THEN** they merge into the same bounded notice or constant-space full-refresh latch
- **AND** no backend debouncer vector or application channel backlog grows with event count

#### Scenario: Producer cannot acquire mailbox state
- **WHEN** a raw callback overlaps mailbox consumption and cannot immediately merge its event
- **THEN** it records a conservative full-refresh need in constant space without blocking GTK
- **AND** a later poll observes that refresh need

#### Scenario: Error and disconnect arrive with pending changes
- **WHEN** bounded changes, backend errors, or disconnection overlap
- **THEN** the mailbox preserves a bounded current error/disconnect state and conservative refresh need
- **AND** repeated identical errors do not grow retained state

### Requirement: GTK consumes bounded watcher work per turn
Each workspace-section poll callback SHALL take at most one bounded watcher notice and SHALL keep refresh-side pending paths under the same cap. A full-refresh marker MUST dominate accumulated targeted paths, while manual Refresh and current tree interaction remain available.

#### Scenario: GTK consumes a path notice
- **WHEN** the poll callback receives a bounded changed-path notice
- **THEN** it performs work proportional to at most the configured cap
- **AND** it returns control to the main loop without draining an unbounded producer queue

#### Scenario: Targeted refresh accumulation crosses the cap
- **WHEN** refresh debouncing accumulates more unique paths than the targeted-refresh cap
- **THEN** the pending plan becomes one full refresh
- **AND** the path set is released instead of continuing to grow

#### Scenario: Full refresh is already pending
- **WHEN** later targeted watcher notices arrive while a full refresh is pending
- **THEN** those paths are not accumulated separately
- **AND** the one pending full refresh remains authoritative

### Requirement: Accepted workspace child caches rebuild in linear time
After bounded child-store reconciliation accepts a terminal mirror, the system SHALL rebuild sibling paths, item locations, and visible-path occurrence evidence in O(n) work for that mirror. The bulk rebuild MUST preserve duplicate-path accounting and lookup behavior without invoking per-row index shifting across already cached rows.

#### Scenario: Broad child store reaches the scan cap
- **WHEN** reconciliation accepts a directory mirror near the configured 10,000-row cap
- **THEN** cache rebuilding visits accepted and replaced cache entries only a bounded number of times
- **AND** it does not perform one cached-location scan for each inserted row

#### Scenario: Mirror contains duplicate and reordered identities
- **WHEN** accepted rows include duplicate paths, removals, insertions, and reordering
- **THEN** the bulk cache result matches the test-only full derivation oracle
- **AND** visible-path reference counts neither underflow nor retain removed-only occurrences

#### Scenario: Reconciliation is superseded before terminal acceptance
- **WHEN** a newer refresh invalidates the current reconciliation before its mirror is accepted
- **THEN** the stale mirror does not replace current cache state
- **AND** no partial bulk-cache commit remains visible

### Requirement: Large tree reconciliation applies in bounded GTK batches
An accepted workspace-directory refresh whose reconciliation exceeds the calibrated synchronous threshold SHALL apply model changes through generation-guarded GTK batches. Reconciliation planning MUST use plain row state outside repeated GObject scans where practical, and expansion, selection, row caches, watcher targets, and readiness MUST finalize only for the complete current plan. A stale or replaced plan MUST stop without announcing refresh completion.

#### Scenario: Broad expanded directory changes near the start
- **WHEN** refresh changes a large prefix or middle range of an expanded directory containing thousands of visible rows
- **THEN** GTK constructs and splices only a bounded row batch per main-loop turn
- **AND** input, drawing, and manual Refresh remain schedulable between batches

#### Scenario: Refresh is superseded between batches
- **WHEN** a newer scan generation or section lifetime replaces an active reconciliation plan
- **THEN** remaining batches from the stale plan stop
- **AND** stale cache, expansion, selection, watcher-target, and readiness finalization do not overwrite the newer plan

#### Scenario: Small reconciliation remains direct
- **WHEN** the changed range is below the calibrated synchronous threshold
- **THEN** the section MAY reconcile it in one GTK callback
- **AND** it observes the same generation and terminal-finalization contract

#### Scenario: Batched reconciliation completes
- **WHEN** the final accepted batch has been applied
- **THEN** row caches and surviving expansion and selection state are reconciled once against the completed model
- **AND** workspace refresh readiness becomes complete exactly once
