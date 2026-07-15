# local-history Specification

## Purpose
TBD - created by archiving change session-time-travel. Update Purpose after archive.
## Requirements
### Requirement: The system captures local-history snapshots for saved documents automatically
The system SHALL capture local-history snapshots for saved, file-backed documents without blocking the GTK main thread. The system MUST record a baseline snapshot when a clean saved document first becomes modified, MUST record additional snapshots no more than once every five minutes while that document remains modified, and MUST record a snapshot after each successful save. The system MUST skip writing a new snapshot when the candidate content is identical to the most recent snapshot already stored for that document.

#### Scenario: Baseline snapshot on first dirty transition
- **WHEN** a saved document with no unsaved changes becomes modified for the first time in an editing cycle
- **THEN** the system records a local-history snapshot of the document state that existed immediately before those unsaved edits

#### Scenario: Periodic snapshot during a long unsaved edit session
- **WHEN** a saved document remains modified and at least five minutes have elapsed since its last local-history snapshot
- **THEN** the system records a new local-history snapshot in the background

#### Scenario: Deduplicated snapshot candidate
- **WHEN** the system reaches a local-history capture boundary but the candidate content is identical to the newest stored snapshot for that document
- **THEN** the system does not create a duplicate snapshot

#### Scenario: Post-save snapshot
- **WHEN** the user successfully saves a saved document
- **THEN** the system records a local-history snapshot representing the saved content

### Requirement: Users can browse local history for the active saved document
The system SHALL provide a deliberate browse action for local history on the
active saved document. Opening local history MUST present an adaptive
GTK-native browser that shows snapshots in newest-first order together with a
read-only preview of the currently selected snapshot. On windows wide enough to
show both areas side by side, the browser MUST open as a large, viewer-first
dialog that occupies most of the parent window while remaining smaller than the
parent window, and the preview MUST receive the majority of the side-by-side
width. The browser MUST distinguish between "no snapshots yet", "empty
historical snapshot", and "preview could not be loaded" states. When the
selected snapshot contains no text, the browser MUST show an explicit
empty-snapshot explanation instead of an ambiguous blank preview, and the
snapshot metadata MUST describe the state semantically rather than relying only
on a raw `0 B` size. Empty snapshots remain valid history and MUST still be
restorable. The browser MUST avoid surfacing fresh baseline entries that exist
only because a file-backed draft was restored over stale on-disk content. The
browser MAY hide legacy empty baseline rows from older history data when they
match the known stale-disk draft-restore noise pattern, but it MUST preserve
the underlying stored history on disk. The MVP MUST NOT require or expose
diff-only controls in order to browse history. The browser MUST be reachable
from a keyboard shortcut and from native context menus on eligible saved files
in both the sidebar and the active editor content surface.

#### Scenario: Open local history for a saved file
- **WHEN** the active editor is a saved document and the user invokes the local-history action
- **THEN** the system opens a local-history browser for that document
- **AND** the browser lists available snapshots from newest to oldest
- **AND** the selected snapshot is shown in a read-only preview

#### Scenario: Wide-window local history opens as a large viewer
- **WHEN** the local-history browser is opened in a window width that can
  comfortably show the snapshot list and preview side by side
- **THEN** the browser opens as a large dialog that occupies most of the parent
  window without exceeding it
- **AND** the preview receives the majority of the side-by-side width
- **AND** the snapshot list remains available as a narrower browse rail

#### Scenario: Empty historical snapshot explains itself
- **WHEN** the user selects a stored snapshot whose text body is empty
- **THEN** the preview shows an explicit empty-snapshot explanation instead of
  a blank content area
- **AND** the explanation makes clear that the snapshot itself contained no
  text when captured
- **AND** the browser does not present the state as a preview failure

#### Scenario: Empty snapshot metadata is semantic
- **WHEN** the browser lists or focuses a snapshot whose text body is empty
- **THEN** the snapshot metadata indicates that the snapshot is empty
- **AND** the browser does not rely only on `0 B` to communicate that state

#### Scenario: Empty snapshot keeps restore available
- **WHEN** the user selects a stored snapshot whose text body is empty
- **THEN** the restore action remains available
- **AND** secondary copy behavior reflects that there is no text content to copy

#### Scenario: Legacy stale-disk empty baselines are hidden from view
- **WHEN** a stored history timeline contains older empty baseline rows that
  match the known stale-disk draft-restore noise pattern
- **THEN** the browser omits those rows from the visible snapshot list
- **AND** the remaining visible rows still preserve correct preview and action
  behavior

#### Scenario: Hidden legacy rows remain stored
- **WHEN** the browser suppresses a legacy stale-disk empty baseline row from
  view
- **THEN** the underlying stored local-history data remains unchanged on disk

#### Scenario: Open local history from the keyboard
- **WHEN** the active editor is an eligible saved document and the user presses the local-history shortcut
- **THEN** the system opens the local-history browser for that document

#### Scenario: Open local history from the sidebar context menu
- **WHEN** the user right-clicks an eligible saved file row in the sidebar and chooses `Local History`
- **THEN** the system opens the local-history browser for that file

#### Scenario: Open local history from the editor context menu
- **WHEN** the active editor is an eligible saved document and the user chooses `Local History` from the editor content context menu
- **THEN** the system opens the local-history browser for that document

#### Scenario: Narrow-window local history browsing
- **WHEN** the local-history browser is opened in a window width that cannot comfortably show the snapshot list and preview side by side
- **THEN** the system adapts the browser into a navigation flow that still allows the user to reach both the snapshot list and the selected snapshot preview

#### Scenario: No snapshots available
- **WHEN** the user opens local history for a saved document that has no stored snapshots
- **THEN** the browser shows an empty state instead of a broken or blank list

#### Scenario: Preview text keeps deliberate inner spacing
- **WHEN** the browser shows a read-only snapshot preview
- **THEN** the preview text is padded inside its scrollable surface instead of rendering flush against the frame edge

### Requirement: Local-history restore is safe and reversible
The system SHALL restore historical snapshots into the active editor buffer without writing directly to disk. Before replacing buffer content, the system MUST store the current eligible buffer state as a fresh local-history snapshot. After a complete current restore, the system MUST mark the editor modified and MUST provide an immediate undo path. Large restore and undo bodies MUST use the bounded GTK replacement contract, and a partial replacement MUST remain non-editable and non-saveable until exact finalization. The system SHALL also provide a non-destructive copy action for the selected snapshot.

#### Scenario: Restore a historical snapshot
- **WHEN** the user chooses Restore for a selected snapshot in the local-history browser
- **THEN** the system stores the current eligible buffer content as a fresh local-history snapshot before applying the selected snapshot
- **AND** the editor buffer is replaced with the selected snapshot content
- **AND** the editor is marked modified only after complete installation

#### Scenario: Restore a large historical snapshot
- **WHEN** the selected or current body exceeds the synchronous replacement threshold
- **THEN** history preparation and buffer replacement retain bounded full-body ownership and yield between GTK slices
- **AND** no partial snapshot can be edited, saved, or reported as restored

#### Scenario: Undo a restore
- **WHEN** the user restores a snapshot and then invokes the immediate undo affordance for that restore
- **THEN** the system returns the editor buffer to the content that was active immediately before the restore
- **AND** a large undo body observes the same bounded installation and freshness rules

#### Scenario: Restore becomes stale
- **WHEN** editor lifetime, path identity, or history generation changes while replacement is pending
- **THEN** remaining work is cancelled without publishing successful restore state
- **AND** retained source and undo bodies are released exactly once

#### Scenario: Copy snapshot content
- **WHEN** the user chooses Copy for a selected snapshot in the local-history browser
- **THEN** the system copies that snapshot content without modifying the active editor buffer

### Requirement: Local-history identity follows in-app renames and resets on Save As
The system SHALL key local history by a stable saved-document identity derived from the document’s canonical path. When a saved document or its parent path is renamed through LushText’s in-app rename workflow, the system MUST migrate the existing local-history lineage to the new path identity. When a document is saved through Save As, the system MUST start a new local-history lineage for the new path instead of merging histories.

#### Scenario: In-app rename preserves history lineage
- **WHEN** the user renames a saved document or one of its ancestor directories through LushText’s in-app rename workflow
- **THEN** the system keeps that document’s existing local-history snapshots associated with the renamed path

#### Scenario: Save As starts a new history lineage
- **WHEN** the user saves a document to a new path through Save As
- **THEN** the new path starts with its own local-history lineage
- **AND** the previous path’s local-history snapshots are not merged into the new path automatically

### Requirement: Local history respects large-file safety policy
The system SHALL apply LushText’s existing large-file safety thresholds to local history. For files above 10 MB and at or below 50 MB, the system MUST limit history capture to save-boundary snapshots. For files above 50 MB, the system MUST make local history unavailable and MUST not capture or preview historical snapshots for that document.

#### Scenario: Reduced history capture for very large but still openable files
- **WHEN** the active saved document is larger than 10 MB and not larger than 50 MB
- **THEN** the system limits local-history capture to save-boundary snapshots

#### Scenario: Local history unavailable for huge files
- **WHEN** the active saved document is larger than 50 MB
- **THEN** the system does not offer local-history browsing for that document
- **AND** the system does not create new local-history snapshots for that document

### Requirement: Local history is stored as app-data lineages keyed by saved-document identity
The system SHALL persist local history under `$XDG_DATA_HOME/lushtext/local-history/` using one lineage per saved-document identity derived from the document's canonical path. Snapshot metadata and snapshot text MUST live under that lineage rather than inside the source-file tree, so history remains separate from user documents and version-controlled project files.

#### Scenario: First snapshot creates an app-data lineage for the document
- **WHEN** the system captures the first local-history snapshot for a saved document
- **THEN** the snapshot is stored under the app data directory in that document's local-history lineage
- **AND** the source document's own directory is not used as the history store

### Requirement: Local-history indexes use the public v1 JSON envelope
The system SHALL persist each local-history lineage `index.json` as a supported v1 app-owned JSON envelope. Snapshot body files MUST remain plain UTF-8 text files outside the JSON envelope.

#### Scenario: Save local-history index as v1
- **WHEN** local-history snapshot metadata is persisted for a document lineage
- **THEN** that lineage's `index.json` is written as a pretty JSON envelope with the local-history index document kind
- **AND** snapshot text bodies remain stored as separate `.txt` files

#### Scenario: Load supported local-history index
- **WHEN** local-history browsing loads a supported v1 lineage index
- **THEN** it reads snapshot metadata from the envelope payload
- **AND** it loads the selected snapshot body from the separate text file

### Requirement: Local-history retention stays bounded across documents
The system SHALL keep local-history retention bounded by trimming the oldest stored snapshots after newer ones are recorded. The shipped retention policy MUST keep at most 48 snapshots for one document lineage and at most 240 snapshots across the whole app-data history store.

#### Scenario: One document lineage trims its oldest snapshots after the per-document cap
- **WHEN** a document's local-history lineage grows beyond 48 stored snapshots
- **THEN** the oldest snapshots in that lineage are removed
- **AND** the newest snapshots remain available for browsing and restore

#### Scenario: Global retention trims the oldest snapshots across all lineages
- **WHEN** the total number of stored local-history snapshots across the app exceeds 240
- **THEN** the oldest stored snapshots across all lineages are trimmed
- **AND** newer snapshots remain available across the retained lineages

### Requirement: Local-history snapshots use the native Adwaita sidebar rail
The system SHALL present local-history snapshot entries through an `AdwSidebar` rail rather than a hand-built `GtkListBox` rail. The sidebar rail MUST preserve newest-first ordering, async preview loading, compact navigation handoff, Copy, Restore, empty snapshot handling, preview-error handling, safety snapshots, and large-file availability gating.

#### Scenario: Browse snapshots in the Adwaita sidebar rail
- **WHEN** the user opens Local History for an eligible saved document with stored snapshots
- **THEN** the browser shows snapshot entries in an `AdwSidebar` rail
- **AND** the entries remain ordered newest-first
- **AND** each entry preserves the semantic snapshot metadata currently shown in the browser

#### Scenario: Select a snapshot from the sidebar rail
- **WHEN** the user selects a snapshot item in the Local History sidebar rail
- **THEN** the browser starts loading that snapshot preview asynchronously
- **AND** stale preview loads from earlier selections cannot update the preview after the selection changes

#### Scenario: Activate a snapshot from a compact sidebar rail
- **WHEN** the Local History browser is collapsed and the user activates a snapshot item in the sidebar rail
- **THEN** the browser navigates to the preview page for that selected snapshot
- **AND** the back affordance returns to the snapshot rail

#### Scenario: Copy and restore stay bound to the selected sidebar item
- **WHEN** the user selects a snapshot item in the Local History sidebar rail and the preview loads successfully
- **THEN** Copy copies that selected snapshot's text when text exists
- **AND** Restore captures the current buffer as a safety snapshot before applying that selected snapshot

#### Scenario: Empty and error states remain explicit
- **WHEN** the selected snapshot item represents an empty historical snapshot
- **THEN** the preview shows the explicit empty-snapshot explanation instead of a blank content area
- **AND** the restore action remains available

#### Scenario: Huge files still cannot open local-history browsing
- **WHEN** the active saved document is larger than 50 MB
- **THEN** the system does not offer the Local History browser
- **AND** the Adwaita sidebar rail is not shown as a bypass around the large-file policy

### Requirement: Unsupported local-history indexes preserve snapshot bodies
The system SHALL treat unsupported old-shape, wrong-kind, unsupported-version, malformed, unreadable, or oversized local-history indexes as recovery metadata problems without deleting snapshot body files.

#### Scenario: Unsupported index does not delete snapshots
- **WHEN** a local-history lineage index cannot be loaded as supported v1 metadata
- **THEN** the original index is preserved or left untouched when preservation fails
- **AND** snapshot `.txt` files under that lineage remain on disk

#### Scenario: Replacement is safe only after index preservation
- **WHEN** a local-history index is unsupported and the system can safely preserve it
- **THEN** the system may write an empty or repaired v1 index only after preservation succeeds
- **AND** ambiguous snapshot body evidence remains available for manual inspection or future tooling

#### Scenario: Malformed local-history index does not delete snapshot text
- **WHEN** a local-history lineage index cannot be parsed during startup or history browsing
- **THEN** the original index is preserved through quarantine or left untouched when quarantine fails
- **AND** snapshot text files under that lineage remain on disk

#### Scenario: Repairable lineage index is rebuilt conservatively
- **WHEN** snapshot files contain enough deterministic metadata to rebuild the lineage index
- **THEN** the system writes a repaired index through the durable JSON path
- **AND** it records a recovery diagnostic describing the repair

#### Scenario: Ambiguous lineage repair is skipped
- **WHEN** snapshot files cannot be mapped into a deterministic newest-first lineage
- **THEN** the system does not invent snapshot ordering or identities
- **AND** it preserves the lineage data with a diagnostic that manual inspection may be required

### Requirement: Local-history lineage migrations are retryable and merge-safe
The system SHALL record pending local-history lineage migrations before or as part of the post-rename lineage migration workflow. If migration, merge, or cleanup fails, the pending state MUST survive restart and be retried during startup reconciliation.

#### Scenario: Pending local-history migration survives restart
- **WHEN** an in-app file or directory rename succeeds but local-history lineage migration fails before completion
- **THEN** a pending migration record remains in app data
- **AND** restarting LushText retries the local-history migration

#### Scenario: Target lineage is written before source cleanup
- **WHEN** local-history migration moves snapshots from an old identity to a new identity
- **THEN** the target lineage index and snapshot bodies are durably written before the old lineage is removed
- **AND** cleanup failure leaves retryable diagnostic state

#### Scenario: Save As never consumes pending rename lineage
- **WHEN** a document has pending local-history rename migration state and the user later uses Save As to a different path
- **THEN** the Save As path starts a separate lineage
- **AND** the pending rename migration remains tied only to the original in-app rename

### Requirement: Local-history reconciliation is bounded and conservative
The system SHALL reconcile duplicate or orphaned local-history lineages conservatively during startup and browsing. Reconciliation MUST be bounded in time and data volume, MUST preserve the newest durable snapshots when deterministic, and MUST preserve evidence instead of deleting ambiguous non-empty lineage data.

#### Scenario: Duplicate lineages merge deterministically
- **WHEN** old and new local-history lineages both exist and snapshot identifiers are deterministic
- **THEN** the system merges the lineages while preserving newest-first order and retention caps
- **AND** it removes the obsolete lineage only after the merged target is durably written

#### Scenario: Corrupt duplicate lineage is quarantined
- **WHEN** one duplicate lineage is malformed and the other is valid
- **THEN** the malformed lineage is quarantined or preserved with diagnostics
- **AND** the valid lineage remains browsable

#### Scenario: Reconciliation work is capped
- **WHEN** startup sees many local-history lineages or very large snapshot stores
- **THEN** reconciliation applies explicit scan and time budgets
- **AND** unfinished work is recorded for later retry instead of blocking startup indefinitely

### Requirement: Local-history reliability has layered automated coverage
The project SHALL add deterministic service, integration, widget, property or fuzz-adjacent, and performance coverage for local-history index corruption, repair, retryable migration, duplicate reconciliation, and bounded startup behavior.

#### Scenario: Service tests cover corrupt indexes and intact snapshots
- **WHEN** service tests load a malformed lineage index with intact snapshot text files
- **THEN** the result preserves snapshot bodies and returns recovery diagnostics
- **AND** repair occurs only when deterministic evidence is present

#### Scenario: Integration tests cover migration retry
- **WHEN** tests simulate an in-app rename whose local-history migration fails after source rename
- **THEN** a pending migration record survives restart
- **AND** a later successful retry preserves all expected snapshots

#### Scenario: Widget tests cover partial history browsing
- **WHEN** local-history browsing opens with one corrupt lineage and one valid lineage
- **THEN** the valid lineage remains browsable
- **AND** visible partial-recovery feedback is shown

#### Scenario: Generated duplicate sets never drop the last copy
- **WHEN** generated local-history duplicate and orphan states are reconciled
- **THEN** the reconciler never deletes the last non-empty snapshot body before a durable merged target exists

#### Scenario: Performance tests cover bounded reconciliation
- **WHEN** the performance lane runs recovery fixtures with many lineages
- **THEN** it records reconciliation timing and confirms startup remains within the documented budget

### Requirement: Periodic local-history capture uses live buffer policy
The system SHALL classify periodic local-history capture from the current editor buffer rather than only the backing file's last known size. Current buffers conservatively above 10 MiB MUST use save-boundary-only history, buffers above 50 MiB MUST remain unavailable for history, and eligible smaller buffers MUST use the shared direct-or-chunked snapshot policy.

#### Scenario: Originally small document grows above 10 MiB
- **WHEN** a file loaded below the full-history threshold grows conservatively above 10 MiB while modified
- **THEN** the periodic timer skips full-buffer capture
- **AND** successful save-boundary capture remains the next eligible automatic history point

#### Scenario: Live buffer grows above 50 MiB
- **WHEN** a modified file-backed editor's current buffer grows conservatively above 50 MiB
- **THEN** local-history capture becomes unavailable for that live state
- **AND** the periodic callback does not copy or persist the buffer

#### Scenario: Eligible buffer requires chunking
- **WHEN** an eligible periodic capture is above the synchronous snapshot threshold but within the full-history policy
- **THEN** text is captured in bounded main-loop slices
- **AND** persistence starts only after path and edit generations still match

### Requirement: Periodic history rejects stale snapshots
The system MUST discard a periodic snapshot if the editor closes, changes file identity, changes periodic generation, becomes ineligible, or is edited during chunked capture. A stale snapshot MUST NOT be written into either the old or new document lineage.

#### Scenario: File identity changes during capture
- **WHEN** Save As or in-app rename changes the editor's file identity while a periodic snapshot is in progress
- **THEN** the stale completion is rejected before persistence
- **AND** it is not attributed to the wrong local-history lineage

#### Scenario: Editor closes during capture
- **WHEN** an editor is destroyed before its periodic chunked snapshot completes
- **THEN** the weak editor completion performs no persistence
- **AND** no callback retains the closed editor indefinitely

### Requirement: Periodic local-history scheduling is superseding and disposable
Each editor SHALL own at most one scheduled periodic local-history timer and at most one active chunked periodic snapshot. Rescheduling, save, path change, ineligibility, or disposal MUST remove or supersede older sources without retaining them until their original deadlines.

#### Scenario: Repeated clean and dirty transitions occur within five minutes
- **WHEN** an editor repeatedly starts and ends modified cycles before the periodic interval expires
- **THEN** only the latest eligible timer remains scheduled
- **AND** obsolete timer callbacks do not accumulate for the old cycles

#### Scenario: Periodic snapshot is superseded by an edit
- **WHEN** the source buffer changes during an active periodic snapshot
- **THEN** the snapshot is cancelled and releases its admission permit and GTK resources
- **AND** at most one later periodic schedule represents the current cycle

#### Scenario: Editor is disposed with history work pending
- **WHEN** an editor closes while its timer or snapshot is pending
- **THEN** both sources are removed or rendered inert immediately
- **AND** no later callback retains or mutates the disposed editor

### Requirement: Failed baseline capture remains safely retryable
The system MUST retain or recover the clean pre-edit baseline when baseline persistence fails. A retry MUST occur only while the same editor lifetime, saved-file identity, and editing cycle still own that baseline, and MUST NOT overwrite a newer clean baseline.

#### Scenario: Initial baseline write fails transiently
- **WHEN** the first baseline persistence attempt fails while the document remains modified on the same path
- **THEN** the pre-edit text remains available for bounded retry
- **AND** a later successful retry records the original pre-edit state

#### Scenario: Path changes before failed baseline returns
- **WHEN** a baseline write fails after Save As, rename, reload, or editor disposal changes its ownership facts
- **THEN** the old text is not restored into the new lineage
- **AND** no retry writes it under the newer path identity

#### Scenario: New clean baseline supersedes a failed attempt
- **WHEN** a later successful save establishes a newer clean baseline before the older failure completes
- **THEN** the older baseline cannot replace the newer baseline candidate
- **AND** future editing cycles use the latest clean state

### Requirement: Local-history preview loading and installation are bounded and superseding
The system SHALL retain at most one active local-history preview load and one latest compact selection request. Snapshot reading MUST remain size-gated and cooperatively cancellable, and accepted text above the synchronous threshold MUST be installed into the read-only preview buffer in bounded UTF-8-safe GTK slices. Copy and Restore MUST stay bound to one completely installed current snapshot.

#### Scenario: User rapidly selects large snapshots
- **WHEN** a large snapshot load is active and the user selects one or more different snapshots
- **THEN** the active load is cancelled cooperatively and only the latest pending selection is retained
- **AND** no stale text, title, metadata, Copy target, or Restore target is published

#### Scenario: Accepted preview requires several slices
- **WHEN** the current snapshot text exceeds the synchronous preview-install threshold
- **THEN** the preview buffer is cleared and populated through bounded UTF-8-safe main-loop slices
- **AND** repaint, input, and current asynchronous completions can run between slices

#### Scenario: Preview installation is superseded
- **WHEN** selection changes or the browser closes between preview-install slices
- **THEN** remaining slices stop without enabling Copy or Restore for the stale snapshot
- **AND** temporary sources and retained stale payloads are released

#### Scenario: Small, empty, missing, and failed snapshots terminate directly
- **WHEN** the selected snapshot is below the synchronous threshold, empty, missing, or unreadable
- **THEN** the browser reaches the corresponding existing content, empty, missing, or error state without scheduling unnecessary slices
- **AND** action sensitivity remains consistent with that terminal state
