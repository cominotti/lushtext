# gtk-accessibility-spine Specification

## Purpose
Define LushText's GTK accessibility spine so visible surfaces expose intentional semantics, keyboard parity, bounded announcements, visual accessibility proof, and privacy-preserving accessibility evidence.

## Requirements
### Requirement: App-wide accessibility semantics are intentional and reviewable
LushText SHALL maintain an app-wide accessibility contract for visible user-facing surfaces. The contract MUST identify the semantic role, accessible name, optional description, meaningful relations, meaningful states, keyboard path, announcement behavior, state extremes, and proof lane for each major surface.

#### Scenario: Surface inventory covers major workflows
- **WHEN** maintainers inspect the accessibility inventory for this change
- **THEN** it lists shell/header/status/tab controls, the editor, Markdown preview, workspace sidebar and file tree, Open popover, command palette, in-tab search, workspace search, document properties, notes/bookmarks, local history, preferences, save/close dialogs, context menus, focus mode, preview mode, minimap, and compact layout variants
- **AND** each listed surface names the cheapest proof lane that can honestly verify its accessibility contract

#### Scenario: State extremes are part of the semantic contract
- **WHEN** a surface can appear with no required context, representative data, dense or awkward data, or constrained geometry
- **THEN** the inventory records the applicable state extremes
- **AND** the contract names the expected accessible region, primary actions, item-region scrolling behavior, empty-state meaning, and dismissal path for each applicable extreme

#### Scenario: Accessibility metadata is product language
- **WHEN** stable accessible names and descriptions are added for automation or smoke helpers
- **THEN** the wording remains meaningful for assistive technology users
- **AND** helper-only identifiers, private widget names, and brittle tree-depth assumptions are not exposed as user-facing accessibility metadata

### Requirement: Interactive controls expose complete GTK accessibility metadata
Every user-facing interactive control SHALL expose a correct GTK accessible role and a meaningful accessible name. Controls whose purpose is not fully conveyed by visible text SHALL expose descriptions or relations, and controls with meaningful dynamic state SHALL update accessible state when that state changes.

#### Scenario: Icon-only and compact controls have explicit names
- **WHEN** a button, toggle, menu button, row action, or status control is icon-only or label-constrained
- **THEN** it exposes a stable accessible label that describes the action in user language
- **AND** its visible tooltip, if any, does not substitute for the accessible label

#### Scenario: Related labels and descriptions are connected
- **WHEN** an entry, search box, spin row, dropdown, result list, file tree, or detail panel relies on nearby text for context
- **THEN** the widget exposes a label, description, or accessible relation that connects the control to that context
- **AND** assistive technology can identify the control without relying only on widget order

#### Scenario: Dynamic states reflect visible behavior
- **WHEN** a control becomes expanded, collapsed, selected, pressed, checked, busy, invalid, readonly, disabled, or current in a way that matters to the user
- **THEN** its accessible state reflects the visible behavior after the UI settles
- **AND** stale states are cleared when recycled rows, mode switches, or hidden surfaces reuse widgets

### Requirement: Custom rows and transient surfaces preserve accessible identity
Custom list rows, factory-bound rows, overlays, popovers, dialogs, and transient surfaces SHALL refresh all item-specific accessibility metadata during binding and clear stale metadata during unbinding or dismissal.

#### Scenario: Recycled rows do not leak stale metadata
- **WHEN** GTK recycles a row in the Open popover, command palette, workspace search, notes/bookmarks, local history, workspace file tree, or preferences-like list
- **THEN** the row's accessible label, description, state, tooltip, and action-specific metadata describe the newly bound item
- **AND** metadata from the previous item is not exposed after filtering, scrolling, or model replacement

#### Scenario: Empty states expose a semantic group
- **WHEN** a no-context or no-results state is visible
- **THEN** it exposes a named accessible region or group that describes the empty state
- **AND** persistent actions such as close, create, open, retry, or settings remain reachable without fake rows or unrelated data

#### Scenario: Transient dismissal preserves focus semantics
- **WHEN** a command palette, search bar, Open popover, dialog, file peek, context menu, or bottom sheet closes
- **THEN** focus returns to the documented target or a documented fallback
- **AND** the accessibility tree no longer exposes hidden controls as visible or focusable after dismissal settles

### Requirement: Main editor accessibility is explicitly proven
The editor surface SHALL keep GtkSourceView as the editing engine while adding LushText-owned metadata and proof for document identity, editability, focus, text exposure, caret behavior, selection behavior, and read-only states.

#### Scenario: Editor exposes a meaningful editing region
- **WHEN** a document tab is active
- **THEN** the source editor exposes a meaningful accessible name or labelled region for the active document editing surface
- **AND** the metadata identifies the document by bounded display identity without exposing unbounded document contents

#### Scenario: Editor editability state tracks workflow policy
- **WHEN** the editor becomes temporarily readonly during save, load, large-file policy, preview-only mode, or a failure placeholder
- **THEN** the accessible state or surrounding accessible metadata communicates that editing is unavailable
- **AND** the state returns to editable only when the user can edit through the normal GtkSourceView path

#### Scenario: Editor text path is verified through accessibility tooling
- **WHEN** the accessibility smoke lane opens a representative text fixture
- **THEN** it verifies, through AT-SPI or the documented host accessibility API, that the active editor text surface, focus path, caret or insertion context, and selection behavior are available when supported by GTK/GtkSourceView
- **AND** any unsupported platform limitation is documented as a caveat with a manual verification fallback rather than silently treated as passed

### Requirement: Markdown preview and read-only text surfaces are accessible
Markdown preview, local-history preview, note render views, read-only file peek, and embedded preview widgets SHALL expose read-only semantics, meaningful structure, and reachable actions without pretending they are editable source buffers.

#### Scenario: Preview mode exposes read-only content state
- **WHEN** Markdown preview-only or side-by-side preview is visible
- **THEN** the preview surface exposes a named read-only region
- **AND** embedded tables, code blocks, links, images, fallback image states, and alert callouts expose useful names or descriptions when they are visible

#### Scenario: Preview and editor mode switches are announced when user-initiated
- **WHEN** the user switches between editor-only, side-by-side preview, and preview-only mode
- **THEN** the mode change is visible to assistive technology through state, focus, or a bounded announcement
- **AND** the previous hidden surface does not remain reachable as a visible focus target

#### Scenario: Read-only history and peek content avoids destructive ambiguity
- **WHEN** local history, file peek, or a read-only note/render preview is focused
- **THEN** assistive technology can distinguish browsing or copying content from editing the live document
- **AND** restore, open, copy, close, and back actions expose stable labels and states

### Requirement: Workflow announcements are useful, bounded, and non-noisy
LushText SHALL announce user-meaningful dynamic workflow outcomes through GTK accessibility announcements or equivalent platform mechanisms, while avoiding high-frequency or redundant screen-reader noise.

#### Scenario: Alerts and errors announce with appropriate priority
- **WHEN** an inline alert, blocking error, recovery warning, durability warning, failed load, invalid search, or destructive confirmation appears
- **THEN** assistive technology receives an announcement or alert role with the title and bounded body
- **AND** high-priority announcements are reserved for urgent or blocking states

#### Scenario: Search and replace announce debounced outcomes
- **WHEN** in-document search, workspace search, replace preview, Replace All, or undo replacement completes after user input
- **THEN** the result count, no-result state, error state, completion state, or undo availability is exposed through a bounded announcement or accessible state
- **AND** normal typing in the search entry does not announce every intermediate keystroke as a separate workflow result

#### Scenario: Long-running operations announce state changes
- **WHEN** user-initiated load, save, workspace refresh, indexing, content search, migration scan, format upgrade, or recovery restore begins and finishes
- **THEN** the busy/completed/error state is exposed through accessible state or a bounded announcement
- **AND** repeated progress heartbeats are throttled so assistive technology users receive useful milestones rather than noise

### Requirement: Keyboard parity covers every non-decorative interaction
Every non-decorative user operation SHALL be reachable by keyboard, menu, command palette, or an equivalent accessible command path. Pointer-only and hover-only affordances MUST have a keyboard or menu fallback.

#### Scenario: Hover-only row actions have keyboard alternatives
- **WHEN** a row action appears only on hover, overlay, pointer movement, or DnD context
- **THEN** the same operation is available through keyboard focus, context menu, command palette, or another documented accessible path
- **AND** visual focus styling identifies the currently reachable target

#### Scenario: Context and destructive actions are keyboard-operable
- **WHEN** file tree, workspace, note, bookmark, local-history, editor, search-result, or tab context actions are available
- **THEN** a keyboard-only user can reach the relevant menu or command and complete or cancel the operation
- **AND** destructive confirmation controls expose stable names, roles, and states

#### Scenario: Focus restoration is deterministic
- **WHEN** a transient surface closes after keyboard activation, pointer dismissal, Escape, success, cancellation, or workflow completion
- **THEN** focus returns to the surface promised by that workflow
- **AND** hidden or destroyed widgets are not used as final focus targets

### Requirement: Visual accessibility remains usable across themes, scale, motion, and geometry
LushText SHALL preserve visible accessibility affordances across supported visual environments, including focus indication, high contrast, large text, reduced motion where supported, color-not-only state communication, opacity/readability, and constrained geometry.

#### Scenario: Focus indication remains visible
- **WHEN** keyboard focus moves through shell controls, editor, rows, dialogs, popovers, bottom sheets, status controls, and context menus
- **THEN** the focused target has a visible focus indication in normal, dark, high-contrast, compact, and constrained states where those states are supported
- **AND** focus indication is not hidden behind overlays, clipped row actions, transparent backgrounds, or animation frames

#### Scenario: State is not communicated by color alone
- **WHEN** LushText communicates warning, error, success, selection, search match, modified tab, disabled action, file health, local-history restore, bookmark, or destructive state
- **THEN** the state is also communicated through text, iconography, role/state metadata, shape, or position
- **AND** the visual treatment remains distinguishable in high-contrast and dark style variants

#### Scenario: Text scale and constrained geometry keep commands reachable
- **WHEN** the app runs with large text, dense rows, long names, narrow width, compact layout, or short height
- **THEN** primary actions, close/back controls, persistent chrome, and item-region scrolling remain reachable
- **AND** no unintended horizontal scrollbar, clipped primary control, or overlapping text is accepted as passing coverage

### Requirement: Accessibility documentation and developer guardrails stay synchronized
LushText SHALL document its accessibility contract and keep developer guidance, stable anchors, smoke scenarios, and policy checks synchronized with implementation.

#### Scenario: User-facing accessibility guide exists
- **WHEN** users or maintainers read LushText documentation
- **THEN** it explains keyboard operation, major accessibility features, screen-reader expectations, known platform caveats, smoke-test coverage, and how to report accessibility bugs
- **AND** it avoids claiming unsupported certification or unverified behavior

#### Scenario: Developer rules cover new accessible UI work
- **WHEN** developers add or change icon-only controls, custom rows, transient surfaces, hover actions, dialogs, preview widgets, search results, or accessibility smoke anchors
- **THEN** repo guidance tells them which metadata, keyboard parity, tests, docs, and smoke artifacts are required
- **AND** documentation drift checks fail when stable anchors or helper flags change without matching docs

#### Scenario: Policy checks catch common regressions
- **WHEN** a change adds a new icon-only button, custom list row, transient surface, or hover-only affordance without accessible metadata or keyboard parity evidence
- **THEN** a focused policy check, widget test, or smoke documentation check fails with a clear remediation path
- **AND** intentionally decorative widgets can be exempted only with an explicit local rationale

### Requirement: Accessibility proof preserves privacy and data safety
Accessibility metadata, smoke artifacts, logs, screenshots, and automation state SHALL remain bounded and SHALL NOT expose unbounded user document contents, note bodies, draft bodies, local-history contents, complete search results, or private persistence identifiers.

#### Scenario: Smoke artifacts use bounded fixture data
- **WHEN** accessibility, automation, or visual smoke artifacts include text-like values
- **THEN** they use committed fixtures or bounded summaries such as display names, counts, selected identity classes, roles, states, and short diagnostic strings
- **AND** user document content is not dumped into accessibility trees or manifests beyond explicit fixture content created for the smoke run

#### Scenario: Accessibility actions do not bypass save or destructive safety
- **WHEN** an accessibility scenario activates save, close, discard, replace, delete, rename, restore, migration, or format-upgrade workflows
- **THEN** the normal durable-write, modified-document, confirmation, undo, recovery, and error-handling behavior applies
- **AND** accessibility-only shortcuts or automation paths do not mark unsafe operations successful
