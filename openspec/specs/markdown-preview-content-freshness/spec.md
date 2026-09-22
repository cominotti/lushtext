# markdown-preview-content-freshness Specification

## Purpose
Keep the Markdown preview in step with the selected document: republish it at every load, buffer-replacement, and identity terminal, name the installing interval with a placeholder, and keep preview mode window-level so it follows the selected tab.
## Requirements
### Requirement: Markdown preview republishes at every install terminal
When Markdown preview is active (preview-only or side-by-side) and the selected editor reaches an install terminal, the system SHALL refresh the preview from the buffer as it then stands, without user action. Install terminals are: document-load success, document-load failure, and every whole-buffer replacement terminal that restores the projection guard.

#### Scenario: Opening a Markdown file while preview-only is active
- **WHEN** preview-only mode is active and the user opens a `.md` file that is not already open
- **THEN** the new tab is selected, the window stays in preview-only mode, and once the load reaches `Loaded` the preview shows the rendered content of that file

#### Scenario: Opening a Markdown file while side-by-side preview is active
- **WHEN** the side-by-side preview pane is visible and the user opens a `.md` file
- **THEN** the pane stays visible and shows the rendered content of the new file once its load reaches `Loaded`

#### Scenario: Load-path reload completes on the selected tab
- **WHEN** preview is active and the selected Markdown tab completes a reload driven by memory eviction, an external change, Replace All, or reopen with encoding
- **THEN** the preview shows the rendered content of the reloaded buffer

#### Scenario: Buffer replacement completes on the selected tab
- **WHEN** preview is active and a whole-buffer replacement (draft restore, local-history restore or undo, save mirror-back, or `evict()`) reaches its `Complete` terminal on the selected Markdown tab
- **THEN** the preview shows the rendered content of the replaced buffer

#### Scenario: Buffer replacement is cancelled or superseded on the selected tab
- **WHEN** preview is active and a whole-buffer replacement on the selected Markdown tab ends with a cancellation reason other than `Disposed`
- **THEN** the projection guard is restored and the preview re-renders the buffer as it then stands instead of keeping the preparing placeholder

#### Scenario: Load fails on the selected tab
- **WHEN** preview is active and the selected tab's document load reaches its failure terminal without a partially installed buffer
- **THEN** the preview leaves the preparing placeholder and renders the current buffer and language, so the user is not left looking at "Preparing Markdown preview…"

#### Scenario: Install terminal on a background tab
- **WHEN** preview is active and an install terminal fires for a tab that is not the selected page
- **THEN** the preview is not re-rendered, its projection dispatch count does not advance, and it continues to show the selected tab's content

### Requirement: Markdown preview follows document identity changes
When Markdown preview is active and the selected editor's path or language identity changes without a load (Save As, sidebar rename, reopen with encoding), the system SHALL refresh the preview so the rendered state matches the new identity.

#### Scenario: Save As turns an untitled buffer into a Markdown document
- **WHEN** preview is active on an untitled tab showing "Not a Markdown file" and the user saves it as `notes.md`
- **THEN** the preview renders the buffer as Markdown without a preview toggle

#### Scenario: Rename moves a Markdown document to a non-Markdown extension
- **WHEN** preview is active on a `.md` tab and the file is renamed to `.txt` from the sidebar
- **THEN** the preview shows "Not a Markdown file"

### Requirement: Markdown preview names the installing interval
While the selected editor is a Markdown document whose content is not yet trustworthy (load state `Loading`, a projection-suspending buffer replacement in flight, or an incomplete load installation), the system SHALL show the "Preparing Markdown preview…" content placeholder instead of rendering the empty or partial buffer.

#### Scenario: Tab selected before its load completes
- **WHEN** preview is active and a newly opened `.md` tab is selected while its load is still in flight
- **THEN** the preview's text buffer reads "Preparing Markdown preview…" until the install terminal republishes the real render

#### Scenario: Failed load over a partially installed buffer
- **WHEN** preview is active and the selected `.md` tab's load fails after an earlier installation was aborted partway
- **THEN** the preview keeps the preparing placeholder rather than rendering the partial buffer, until a retry installs one exact payload

#### Scenario: Non-Markdown document still loading
- **WHEN** preview is active and a newly opened non-Markdown file is selected while its load is still in flight
- **THEN** the preview shows "Not a Markdown file" immediately, because the language is known from the path

### Requirement: Preview mode is window-level and follows the selected tab
The system SHALL keep preview-only and side-by-side state as window-level state. Opening an existing document MUST NOT change the current preview mode. New untitled tabs exiting preview-only mode is owned by `new-document-flow` and is unchanged by this capability.

#### Scenario: Opening an existing document keeps preview mode
- **WHEN** preview-only mode is active and the user opens an existing file from the sidebar, palette, recent list, or CLI
- **THEN** the `toggle-preview-mode` action state remains true and the preview follows the newly selected tab

