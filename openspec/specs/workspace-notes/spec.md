# workspace-notes Specification

## Purpose
Define folder-note behavior for workspace folders, plus the unified workspace-scoped notes browser that combines folder notes, document notes, bookmarks, and eligible saved open-tab rows.

## Requirements

### Requirement: Users can create and manage one folder note for each workspace folder
The system SHALL allow users to create, edit, view, and clear one folder note for a concrete workspace folder without requiring an active document. A direct folder-note workflow MUST target one folder at a time and MUST NOT guess a target when the current shared scope is `All workspaces`, when the selected workspace has zero folders, or when the selected workspace has multiple folders.

#### Scenario: Open or create a folder note from a folder row
- **WHEN** the user invokes `Open Folder Note...` from a top-level workspace folder row
- **THEN** the system opens that folder's folder-note surface
- **AND** the system creates the folder note lazily if it did not already exist

#### Scenario: Open or create a folder note for the only folder in the selected workspace
- **WHEN** a concrete workspace with exactly one folder is the current shared scope
- **AND** the user invokes the header/menu `Open Folder Note...` action
- **THEN** the system opens that workspace folder's folder-note surface
- **AND** the system does not require an active document tab

#### Scenario: Multi-folder workspace requires an explicit note target
- **WHEN** a concrete workspace with two or more folders is the current shared scope
- **AND** the user invokes the header/menu `Open Folder Note...` action
- **THEN** the system presents a clear folder choice or opens `Browse Notes...` focused to folder notes
- **AND** it does not choose a folder merely because it is first in workspace order

#### Scenario: Zero-folder workspace cannot open a folder note
- **WHEN** a concrete workspace with zero folders is the current shared scope
- **AND** the user invokes the header/menu folder-note action
- **THEN** the system does not create a folder note
- **AND** the user receives recoverable feedback or an insensitive action state explaining that the workspace has no folders

#### Scenario: Clear an existing folder note
- **WHEN** the user clears the folder note attached to a workspace folder
- **THEN** the persisted folder note for that folder identity is removed
- **AND** reopening that folder note starts without an empty folder-note payload

### Requirement: Folder notes support edit and rendered markdown reading modes
The system SHALL let users switch a folder note between editable text mode and a read-only rendered mode based on the stored note text. Switching modes MUST NOT discard in-progress note text. When a folder note opens with non-empty, non-whitespace loaded text, the dialog MUST select Render initially. When the loaded folder note is missing, empty, or whitespace-only, the dialog MUST select Edit initially. The Save action MUST remain visible and MUST be enabled only when the normalized current note text is non-empty and differs from the normalized loaded note text.

#### Scenario: Open an existing folder note for reading
- **WHEN** the user opens a folder note that contains non-whitespace text
- **THEN** the folder-note dialog opens with Render selected
- **AND** the rendered note view is read-only
- **AND** Save is visible but disabled

#### Scenario: Open a missing or empty folder note for writing
- **WHEN** the user opens a folder-note workflow for a folder with no meaningful saved folder-note text
- **THEN** the folder-note dialog opens with Edit selected
- **AND** Save is visible but disabled until the user enters meaningful note text

#### Scenario: Render a folder note as markdown
- **WHEN** the user opens a folder note containing markdown syntax and switches to render mode
- **THEN** the system shows a read-only rendered markdown view of the current note text
- **AND** the rendered view does not permit direct editing

#### Scenario: Return from render mode to edit mode
- **WHEN** the user switches a folder note from edit mode to render mode and back again
- **THEN** the editable note text remains the same
- **AND** the note returns to an editable text surface without losing content

#### Scenario: Enable Save after a meaningful folder-note edit
- **WHEN** a folder-note dialog is open
- **AND** the user changes the note text so the normalized current text is non-empty and differs from the loaded text
- **THEN** Save becomes enabled
- **AND** Save remains enabled if the user switches to Render before saving

#### Scenario: Disable Save after reverting a folder-note edit
- **WHEN** a folder-note dialog has unsaved edits
- **AND** the user changes the note text back to the normalized loaded text
- **THEN** Save becomes disabled

#### Scenario: Keep Save disabled for whitespace-only folder-note text
- **WHEN** a folder-note dialog is open
- **AND** the current note text contains only whitespace
- **THEN** Save is disabled

#### Scenario: Save folder-note edits after reviewing Render
- **WHEN** the user edits a folder note, switches to Render, and activates Save
- **THEN** the system persists the current note text
- **AND** the folder's source files remain unchanged

### Requirement: Folder-note persistence follows canonical folder identity
The system SHALL persist folder notes under app data using a stable identity derived from the workspace folder's canonical path. Renaming a workspace label MUST keep the same folder note. Reordering folders or removing a folder from a workspace MUST NOT delete that folder note. Renaming the folder through LushText's in-app rename workflow MUST migrate the folder note to the renamed folder identity. Re-adding the same canonical folder to any workspace MUST restore the same folder note.

#### Scenario: Renaming a workspace label keeps the same folder note
- **WHEN** the user renames a workspace label without changing any folder path
- **THEN** existing folder notes for that workspace's folders remain attached to their canonical folder identities
- **AND** note content does not reset

#### Scenario: Reordering folders keeps their notes
- **WHEN** the user reorders folders inside a workspace
- **THEN** each folder note remains attached to the same canonical folder
- **AND** no folder-note sidecar is created, deleted, or renamed merely because of ordering

#### Scenario: In-app folder rename preserves a folder note
- **WHEN** the user renames a top-level workspace folder through LushText's in-app rename workflow
- **THEN** the persisted folder note is migrated to the renamed folder identity
- **AND** reopening that renamed folder note restores the same note body

#### Scenario: Remove and re-add the same folder restores the same folder note
- **WHEN** the user removes a workspace folder that has a folder note and later adds the same canonical folder again
- **THEN** the system restores the same folder note for that folder identity
- **AND** the note does not depend on the old workspace slot or folder-list position

### Requirement: Folder-note sidecars use recovery-aware app-owned JSON
The system SHALL persist folder-note sidecars as supported app-owned JSON envelopes under app data and MUST write newly saved sidecars with the folder-note sidecar kind. Runtime loading MUST require the folder-note sidecar kind or the explicitly supported legacy workspace-note sidecar kind before reading the note payload. Legacy workspace-note sidecar names MUST remain isolated to compatibility constants, compatibility fixtures, and migration aliases; code, UI strings, comments, tests, and documentation MUST otherwise use folder-note terminology for the domain concept.

#### Scenario: Save folder note as supported JSON
- **WHEN** a workspace folder's note is persisted
- **THEN** the folder-note sidecar is written as a pretty JSON envelope
- **AND** the envelope kind is the supported folder-note sidecar kind
- **AND** the payload stores the canonical folder identity and rich note body
- **AND** newly written domain-facing code does not name that note with legacy root-scoped note terminology

#### Scenario: Legacy workspace-note sidecar can migrate or load compatibly
- **WHEN** a valid existing workspace-note sidecar is found for a folder's canonical identity
- **THEN** the system loads it through an explicit compatibility path
- **AND** saving that note rewrites the sidecar with the folder-note sidecar kind
- **AND** the note body remains available as that folder's folder note

#### Scenario: Unsupported folder-note sidecar is isolated
- **WHEN** a folder-note sidecar is bare pre-public JSON, wrong-kind JSON, unsupported-version JSON, or malformed JSON
- **THEN** that sidecar is preserved through recovery diagnostics before replacement is allowed
- **AND** unrelated valid folder notes continue to load

### Requirement: Users can browse notes within the current workspace scope
The system SHALL provide a `Browse Notes...` surface that lists workspace-scoped bookmarks, folder notes, and document notes that fall inside the current shared workspace scope, plus a clearly separated `Open Tabs` section for saved open files that have bookmarks or document notes but fall outside that current scope. In a concrete workspace scope, normal workspace sections MUST be limited to that workspace's folder set. In `All workspaces`, normal workspace sections MUST aggregate bookmarks and notes across restored workspace folders. Document-level rows MUST be de-duplicated by canonical saved-file identity when overlapping folders cover the same file. Supplemental open-tab rows MUST preserve their open-tab source explicitly and MUST NOT be represented as belonging to a fake workspace.

#### Scenario: Browse notes inside one workspace
- **WHEN** a concrete workspace is the current shared scope and the user opens `Browse Notes...`
- **THEN** the browser lists that workspace's folder notes together with document notes and bookmarks that belong to files inside that workspace's folder set
- **AND** bookmarks and notes from files outside that workspace are excluded from the workspace sections
- **AND** bookmarks or document notes attached to saved open tabs outside that workspace appear only in a dedicated `Open Tabs` section

#### Scenario: Browse notes across all workspaces
- **WHEN** the current shared scope is `All workspaces` and the user opens `Browse Notes...`
- **THEN** the browser aggregates bookmarks, folder notes, and document notes from every restored workspace folder
- **AND** each workspace row preserves enough scope metadata for the user to tell which workspace and primary folder it belongs to
- **AND** bookmarks or document notes attached to saved open tabs outside every restored workspace folder appear only in the `Open Tabs` section

#### Scenario: Overlapping folder coverage does not duplicate document-level rows
- **WHEN** one saved file is covered by two folders in the current workspace scope
- **AND** that file has a document note or bookmark
- **THEN** `Browse Notes...` shows the document-level entry only once in its normal workspace section
- **AND** the displayed folder context uses folder order to pick the primary covering folder

#### Scenario: Browse open-tab notes with no restored workspace folders
- **WHEN** no workspace folders are restored
- **AND** at least one saved open tab has a bookmark or an existing document note
- **AND** the user opens `Browse Notes...`
- **THEN** the browser opens successfully
- **AND** it lists the eligible rows in the `Open Tabs` section without requiring the user to add a workspace folder first

#### Scenario: No notes remain explicit when there are no folders or open-tab rows
- **WHEN** no workspace folders are restored
- **AND** no saved open tab has a bookmark or an existing document note
- **AND** the user opens `Browse Notes...`
- **THEN** the system reports that there are no browsable notes or bookmarks
- **AND** it does not create workspace, folder-note, bookmark, or document-note data implicitly

#### Scenario: Open a folder note from the notes browser
- **WHEN** the user activates a folder-note row in `Browse Notes...`
- **THEN** the system opens that folder note's surface
- **AND** the system does not require an active document tab for that folder

#### Scenario: Open a bookmark from the notes browser
- **WHEN** the user activates a bookmark row in `Browse Notes...`
- **THEN** the system opens or focuses the associated saved file tab
- **AND** the editor moves the cursor to the bookmarked line

### Requirement: Folder-note browser entries use the native Adwaita sidebar rail
The system SHALL present folder-note entries in the workspace-scoped `Browse Notes...` surface through an `AdwSidebar` section rather than a hand-built `GtkListBox` rail. The sidebar section MUST preserve workspace-scope filtering, folder-note preview, preview-only pointer selection, and explicit Open behavior.

#### Scenario: Browse folder notes in the Adwaita sidebar rail
- **WHEN** the current shared workspace scope contains one or more folder notes and the user opens `Browse Notes...`
- **THEN** the Notes browser shows those folder notes in a dedicated `AdwSidebar` section
- **AND** each folder-note item identifies the workspace folder it belongs to

#### Scenario: Preview a folder note from the sidebar rail
- **WHEN** the user selects a folder-note item in the Notes browser sidebar rail
- **THEN** the browser updates the preview pane with that folder note's rendered Markdown content or explicit empty-note state
- **AND** the Open action targets the selected folder note

#### Scenario: Click a folder-note item without opening the editor
- **WHEN** the user clicks a folder-note item in the Notes browser sidebar rail
- **THEN** the browser updates the selected item and preview pane only
- **AND** the folder-note editing surface is not opened

#### Scenario: Open a folder note explicitly from the browser
- **WHEN** the user selects a folder-note item and invokes the browser's Open action
- **THEN** the system opens that folder note's editing surface
- **AND** the Open action does not require an active document tab

#### Scenario: Filtered folder-note state remains explicit
- **WHEN** the Notes browser search text filters out every folder-note item
- **THEN** the folder-note sidebar section no longer shows matching items
- **AND** if no notes of any kind match, the browser shows an explicit empty filtered state

### Requirement: Notes browser loads bookmark excerpts lazily and safely
The system SHALL load bookmark source excerpts in `Browse Notes...` only when a bookmark row is selected. Closed-file excerpt loading MUST run off the GTK main thread, MUST be bounded by explicit size, line, and scan budgets, and MUST ignore stale completions after selection changes. Loading, unavailable, and rendered states MUST preserve the existing notes-browser layout, search field, sidebar selection, preview-only row activation behavior, and Open action semantics.

#### Scenario: Selecting a bookmark starts a lazy preview load
- **WHEN** `Browse Notes...` is open with bookmark rows
- **AND** the user selects a bookmark whose source file is not already open
- **THEN** the preview pane shows a bookmark-specific loading state
- **AND** the notes sidebar remains interactive while the excerpt is loaded
- **AND** the browser does not pre-load excerpts for unselected bookmark rows

#### Scenario: Stale bookmark preview completion is ignored
- **WHEN** a closed-file bookmark excerpt load is in progress
- **AND** the user selects a different notes-browser row before that load completes
- **THEN** the earlier load completion does not replace the currently selected row's preview
- **AND** the currently selected row's Open action target is not changed by the stale completion

#### Scenario: Bookmark excerpt preview keeps dialog geometry stable
- **WHEN** the user changes selection between bookmark rows, folder-note rows, and document-note rows
- **THEN** the notes-browser dialog keeps its settled outer allocation stable
- **AND** the preview pane uses internal scrolling or clipping rather than resizing the dialog around the excerpt

#### Scenario: Bookmark excerpt text does not drive browser search
- **WHEN** the user searches in `Browse Notes...`
- **THEN** bookmark filtering continues to use bookmark label, saved file metadata, source metadata, and line number
- **AND** the browser does not read closed-file excerpt text merely to decide whether a bookmark row matches the search

#### Scenario: Markdown and raw bookmark previews coexist with note previews
- **WHEN** the user selects Markdown bookmark rows, raw text bookmark rows, folder-note rows, and document-note rows in one `Browse Notes...` session
- **THEN** each selection renders through the correct preview mode for that row
- **AND** switching preview modes does not leave stale Markdown, raw text, loading, or unavailable content visible for the next selected row

#### Scenario: Open-tab bookmark rows use the same preview behavior
- **WHEN** a bookmark row appears in the `Open Tabs` section
- **AND** the user selects that bookmark
- **THEN** the preview pane uses the live open-editor excerpt behavior for that row
- **AND** the row remains labeled as an open-tab source rather than a fake workspace row

### Requirement: Folder-note editor mode switching is layout-stable
The system SHALL keep the shared folder-note editor popup visually stable when switching between Edit and Render. The edit and rendered note surfaces MUST keep matching text-origin padding so the same plain note content does not shift horizontally or vertically when changing modes. The popup MUST keep the same outer size with no visible shrink or expansion, including when the note starts empty and the user types before the first Render switch.

#### Scenario: Switch a folder note from Edit to Render
- **WHEN** the user opens a folder-note editing popup and switches from Edit to Render
- **THEN** the popup keeps the same outer size
- **AND** the rendered text starts at the same visual origin as the editable text

#### Scenario: Switch a newly typed folder note from Edit to Render
- **WHEN** the user opens an initially empty folder-note editing popup
- **AND** the user types note text in Edit mode
- **AND** the user switches to Render for the first time
- **THEN** the popup keeps the same outer size with no visible shrink or expansion
- **AND** the rendered text starts at the same visual origin as the editable text

### Requirement: Folder-note sidecar corruption is isolated and diagnostic
The system SHALL isolate malformed folder-note sidecars from valid folder-note state. A malformed folder-note sidecar MUST be preserved when possible, reported through recovery diagnostics, and excluded from normal note restoration until repaired or replaced.

#### Scenario: Malformed folder note does not block unrelated notes
- **WHEN** one folder-note sidecar cannot be parsed during notes browser listing
- **THEN** valid folder notes and document notes continue to load and appear in the notes browser
- **AND** the malformed folder note is reported as a recovery diagnostic

#### Scenario: Opening a folder with corrupt note keeps workspace usable
- **WHEN** a workspace folder has a malformed folder-note sidecar
- **THEN** the workspace loads and remains selectable
- **AND** the folder-note workflow reports that the saved note could not be loaded

#### Scenario: Replacement preserves corrupt folder-note evidence
- **WHEN** the user saves a new folder note for a folder whose previous note sidecar was malformed
- **THEN** the malformed sidecar is quarantined or otherwise preserved before replacement

### Requirement: Folder-note migrations are retryable
The system SHALL record pending folder-note migrations before or as part of the post-rename folder migration workflow. If migration or cleanup fails, the pending state MUST survive restart and be retried during startup reconciliation.

#### Scenario: Pending folder-note migration survives restart
- **WHEN** an in-app folder rename succeeds but folder-note migration fails before completion
- **THEN** a pending migration record remains in app data
- **AND** restarting LushText retries the folder-note migration

#### Scenario: Completed folder-note migration clears pending state
- **WHEN** folder-note migration succeeds and obsolete sidecars are cleaned up or safely reconciled
- **THEN** the pending folder-note migration record is removed durably

#### Scenario: Migration failure warns without losing folder note text
- **WHEN** folder-note migration fails after the folder rename succeeded
- **THEN** the user receives warning feedback
- **AND** the existing folder-note sidecar remains preserved for retry or inspection

### Requirement: Folder-note reconciliation preserves folder-note content
The system SHALL reconcile duplicate old and new folder-note sidecars conservatively. It MUST preserve folder-note text when deterministic identity or timestamp evidence makes a safe merge possible, and MUST preserve evidence instead of guessing when the conflict is ambiguous.

#### Scenario: Duplicate folder notes choose deterministic newest body
- **WHEN** old and new folder-note sidecars both exist and one can be identified as the newer durable save
- **THEN** the newer note body is kept for the migrated folder identity
- **AND** the older copy is removed only after the target note is durably written

#### Scenario: Ambiguous folder-note conflict is preserved
- **WHEN** duplicate folder notes conflict and the newest body cannot be determined safely
- **THEN** the system does not discard either note body silently
- **AND** it reports that automatic folder-note reconciliation was incomplete

#### Scenario: Aggregate notes browser reports partial folder-note recovery
- **WHEN** the notes browser omits or quarantines a malformed folder note in `All workspaces`
- **THEN** it still displays valid notes from other workspace folders
- **AND** it exposes a warning that some folder-note data could not be loaded

### Requirement: Folder-note reliability has layered automated coverage
The project SHALL add deterministic service, integration, and widget coverage for folder-note corruption, folder-rename retry state, duplicate reconciliation, partial notes-browser behavior, and terminology cleanup.

#### Scenario: Service tests cover corrupt folder-note sidecars
- **WHEN** service tests load malformed folder-note sidecar bytes
- **THEN** the result preserves or quarantines the sidecar and returns recovery diagnostics
- **AND** unrelated valid folder notes still load

#### Scenario: Migration tests cover folder-note retry state
- **WHEN** tests simulate a folder rename whose folder-note migration fails after the folder rename
- **THEN** a pending migration record survives restart
- **AND** a later successful retry removes the record durably

#### Scenario: Widget tests cover partial folder-note browsing
- **WHEN** the notes browser sees one corrupt folder note and at least one valid note
- **THEN** the valid notes remain browsable
- **AND** visible partial-recovery feedback is shown

#### Scenario: Naming tests cover old folder-note compatibility names
- **WHEN** the implementation is complete
- **THEN** tests, fixtures, UI resources, and developer documentation no longer present folder-note workflows with legacy root-scoped note terminology
- **AND** any remaining `workspace-note` compatibility symbols are isolated to migration or sidecar compatibility code with clear comments

### Requirement: Notes browser source construction and querying remain bounded
The system SHALL construct the `Browse Notes...` source with explicit aggregate entry, searchable-text byte, sidecar-scan, open-editor snapshot, and recovery-diagnostic limits. The browser SHALL retain one bounded immutable source, SHALL execute note-body matching outside GTK, and SHALL retain at most one active query plus one latest compact superseding query. Source or result truncation MUST be explicit without changing workspace-scope, section ordering, canonical de-duplication, preview, or Open semantics for admitted rows.

#### Scenario: Aggregate Notes source exceeds admission limits
- **WHEN** bookmarks, folder notes, document notes, and eligible open-tab rows exceed an aggregate browser admission limit
- **THEN** source construction stops at a deterministic boundary and retains no more than the configured entry and text budgets
- **AND** the browser reports that later source material was omitted instead of presenting the admitted source as complete

#### Scenario: Open editors contain many bookmark rows
- **WHEN** GTK snapshots open-editor note and bookmark metadata before opening the browser
- **THEN** collection stops at the browser-owned open-editor snapshot bound
- **AND** no `usize::MAX` or equivalent unbounded collection bypasses worker-side admission

#### Scenario: Queries change faster than matching completes
- **WHEN** the user types several Notes queries while an earlier full-source match is active
- **THEN** the active query is cancelled cooperatively and only the latest compact pending query is retained
- **AND** stale matches never rebuild the sidebar or change the selected preview

#### Scenario: Current query exceeds the render limit
- **WHEN** the current background match finds more rows than the existing browser render cap
- **THEN** it retains and publishes only the capped ordered result indexes
- **AND** the browser preserves its existing visible refinement message and grouped row behavior

#### Scenario: Notes browser closes during source or query work
- **WHEN** the dialog is disposed while bounded source construction or query matching is active
- **THEN** current work is cancelled or discarded without a later GTK callback
- **AND** retained source, pending query, and result payloads are released

### Requirement: Bookmark-only browsing reuses the bounded Notes pipeline
The dedicated bookmark browser SHALL be a generation-scoped bookmark-only mode of the unified Browse Notes inventory, query, projection, and disposal workflow. It MUST preserve the existing Show Bookmarks action and bookmark-specific activation, scope, empty, truncation, recovery, keyboard, and accessibility behavior without retaining a separate uncapped loader or synchronous widget-rebuild path.

#### Scenario: Show Bookmarks opens
- **WHEN** the user activates the existing bookmark-browser action
- **THEN** the unified Notes workflow starts with a bookmark-only source filter and current workspace scope
- **AND** document notes and folder notes cannot appear in the result inventory

#### Scenario: Bookmark inventory is large
- **WHEN** bookmark sidecars contain more rows than one admitted inventory, query, or projection slice permits
- **THEN** source loading, matching, and GTK projection obey the same item, byte, active-plus-latest, and disposal bounds as Browse Notes
- **AND** GTK does not synchronously scan the full source or rebuild hundreds of row widget trees in one callback

#### Scenario: Bookmark query has no matches
- **WHEN** a bookmark-only query yields no accepted rows
- **THEN** the browser reaches the bookmark-specific empty state through bounded query completion
- **AND** the main loop remains responsive while prior rows retire

#### Scenario: A bookmark row is activated
- **WHEN** the user activates a current bookmark-only result
- **THEN** the existing bookmark navigation semantics open or focus the file and line
- **AND** a stale generation cannot activate a replaced row

#### Scenario: Bookmark metadata is malformed or truncated
- **WHEN** bounded inventory loading encounters recovery diagnostics or the configured source cap
- **THEN** the browser exposes the existing accessible recovery or truncation state
- **AND** valid admitted bookmarks remain usable

#### Scenario: Production bookmark inventory is requested
- **WHEN** an interactive caller constructs the bookmark-only inventory
- **THEN** it must supply the unified Notes source limit, byte budget, generation, and cancellation policy
- **AND** no unrestricted aggregate bookmark-vector API remains available to production UI code

### Requirement: Notes source construction scratch is byte-bounded
The unified Notes source loader SHALL enforce explicit conservative byte ceilings for concurrently retained construction scratch and sidecar traversal paths in addition to existing entry, searchable-text, final retained-source, sidecar-count, open-editor, and diagnostic limits. Accounting MUST use saturating arithmetic and MUST include the current recovery-aware sidecar input, retained path batch, canonical identity copies, diagnostic storage, temporary category/capacity ownership, and other construction allocations that overlap the final source. Reaching a construction or path ceiling MUST stop at a deterministic complete boundary and publish a distinct typed truncation reason with compact current/peak metrics.

#### Scenario: Sidecar directory contains long Unicode paths
- **WHEN** fewer than the sidecar entry cap would nevertheless exceed the traversal path-byte ceiling
- **THEN** the byte-bounded scanner retains only complete entries within both limits
- **AND** source feedback distinguishes path-byte truncation from sidecar-count truncation

#### Scenario: One near-limit sidecar overlaps admitted rows
- **WHEN** recovery-aware loading holds a sidecar input near its metadata byte limit while final rows and construction scratch already exist
- **THEN** measured peak construction ownership remains within the documented scratch ceiling
- **AND** the loader stops before another complete allocation would exceed that ceiling

#### Scenario: Diagnostics and canonicalization consume scratch
- **WHEN** malformed sidecars and many folder identities produce bounded recovery diagnostics and canonical path copies
- **THEN** those allocations contribute to construction metrics rather than bypassing admission
- **AND** valid rows admitted before the deterministic boundary remain ordered, browsable, and activatable

#### Scenario: Construction is cancelled
- **WHEN** source generation is superseded or the Notes browser closes during sidecar traversal or parsing
- **THEN** cancellation releases path, sidecar, diagnostic, and category scratch on the worker
- **AND** no large construction allocation crosses to GTK in the cancelled outcome

#### Scenario: Final source reaches GTK
- **WHEN** bounded construction completes with admitted rows
- **THEN** only the final measured retained source and compact metrics cross to GTK under the existing progress reservation
- **AND** construction scratch has already been released and is not hidden inside diagnostic payloads

### Requirement: Closed-file bookmark previews are one-active and one-latest
Each open Notes browser SHALL own at most one active closed-file bookmark excerpt load and one latest pending compact request. Selecting a newer preview MUST cancel the active request and replace the pending request without launching another worker until the active request reaches a terminal outcome. Excerpt loading MUST check cancellation during bounded ingestion and line scanning, and only the current browser lifetime, preview generation, and selected bookmark identity may publish preview state.

#### Scenario: Rapid selection outpaces a slow closed-file load
- **WHEN** the user selects several closed-file bookmarks before the first excerpt load terminates
- **THEN** the browser retains one active load and at most the latest compact pending request
- **AND** intermediate selections do not accumulate worker jobs or excerpt payloads

#### Scenario: Active excerpt observes cancellation
- **WHEN** a newer selection cancels a closed-file excerpt during bounded read or line scanning
- **THEN** obsolete work stops at a bounded cancellation checkpoint
- **AND** the latest pending request becomes eligible only after the active terminal is observed

#### Scenario: Stale terminal reaches the browser
- **WHEN** an obsolete excerpt load returns after the selection or browser lifetime changed
- **THEN** it cannot replace preview content, loading state, or the Open action target
- **AND** only the still-current latest request may publish

#### Scenario: Notes browser closes under preview pressure
- **WHEN** the dialog closes with active and pending closed-file preview work
- **THEN** active work is cancelled and pending work is discarded
- **AND** no later completion retains or mutates the closed browser

#### Scenario: Bookmark source is already open
- **WHEN** the selected bookmark can be previewed from its live editor
- **THEN** the browser uses the existing live excerpt path without starting a closed-file worker
- **AND** obsolete closed-file work is cancelled or discarded
