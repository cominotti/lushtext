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

### Requirement: Accessibility completion matrix is authoritative
LushText SHALL maintain a complete accessibility acceptance matrix that maps every major user-facing surface and state extreme to its semantic contract, keyboard path, announcement behavior, visual accessibility expectation, proof lane, and manual verification expectation.

#### Scenario: Matrix covers every named accessibility surface
- **WHEN** maintainers inspect the accessibility completion matrix
- **THEN** it lists shell/header/status/tab controls, editor, Markdown preview, workspace sidebar and file tree, Open popover, command palette, in-tab search, workspace search, document properties, notes/bookmarks, local history, preferences, save/close/destructive dialogs, context menus, focus mode, preview mode, minimap, compact and bottom-sheet layouts, recovery surfaces, and error surfaces
- **AND** every row identifies the surface owner, expected accessible role or region, accessible name policy, optional description or relation, meaningful dynamic states, keyboard path, dismissal path when applicable, announcement lane when applicable, visual accessibility expectation, and proof owner

#### Scenario: Matrix records state extremes
- **WHEN** a matrix surface can appear with no required context, representative populated data, dense or awkward data, constrained or compact geometry, hidden or dismissed state, loading or busy state, error state, recovery state, or destructive confirmation state
- **THEN** the matrix records each applicable state extreme
- **AND** it states whether commands remain reachable, empty states remain readable, scrolling is constrained to the item region, persistent chrome remains visible, focus remains visible, and fake rows or unrelated context are forbidden

#### Scenario: Matrix and proof artifacts reconcile
- **WHEN** the full accessibility validation set is reviewed for release
- **THEN** every required matrix row is covered by at least one honest proof lane or an explicit documented platform caveat
- **AND** skipped AT-SPI, compositor, visual, or manual screen-reader checks are not counted as verified coverage

### Requirement: Existing UI metadata uses the shared accessibility boundary
All app-owned GTK accessibility metadata SHALL flow through `crate::ui::accessibility` helpers unless a local documented GTK contract requires a direct GTK call.

#### Scenario: Direct GTK accessibility calls are normalized or justified
- **WHEN** policy checks inspect UI code in the current tree
- **THEN** direct uses of `set_accessible_role`, `gtk4::accessible::Property::*`, `update_state`, `update_relation`, or `announce` outside `ui::accessibility` are either absent or listed in a narrow allowlist with a local rationale
- **AND** allowlisted exceptions identify why the helper cannot express the GTK contract

#### Scenario: Recycled rows use shared row metadata
- **WHEN** GTK recycles rows in the Open popover, command palette, workspace search, notes/bookmarks, local history, workspace file tree, preferences-like lists, or future factory-bound lists
- **THEN** bind paths apply bounded row accessibility metadata through shared helpers
- **AND** unbind or model replacement paths clear labels, descriptions, selected state, position metadata, and item-specific controls before reuse

#### Scenario: Helper API covers required metadata shapes
- **WHEN** a surface needs label, description, labelled-by, described-by, controls, key-shortcuts, has-popup, value text, readonly, multiline, hidden, busy, invalid, disabled, expanded, selected, pressed, or bounded announcement metadata
- **THEN** `ui::accessibility` exposes a helper or typed wrapper for that shape
- **AND** widget tests prove both setting and clearing behavior where stale state could affect users

### Requirement: Accessibility smoke covers the complete surface matrix
`make accessibility-smoke` SHALL prove AT-SPI-visible accessibility behavior for the complete matrix wherever the host exposes AT-SPI support.

#### Scenario: Smoke cases cover every surface family
- **WHEN** `scripts/run-accessibility-smoke.sh --list-cases` is run
- **THEN** the listed cases include shell, editor, Markdown preview, workspace sidebar and file tree, file peek, Open popover, command palette, in-tab search, workspace search, document properties, notes/bookmarks, local history, preferences, save and close dialogs, destructive confirmations, context menus, focus mode, preview mode, minimap, compact layouts, recovery surfaces, and representative error states
- **AND** each case maps to one or more accessibility matrix rows

#### Scenario: AT-SPI anchors are product-facing and stable
- **WHEN** an accessibility smoke case asserts a role/name anchor
- **THEN** the expected name is meaningful user-facing product language rather than a private widget id, helper id, or brittle tree-position assumption
- **AND** stable anchors are synchronized with `docs/automation-reference.md` through drift checks

#### Scenario: Text-interface proof covers editable and read-only text surfaces
- **WHEN** accessibility smoke opens representative editor, Markdown preview, note preview, bookmark preview, local-history preview, file peek, and search-result preview fixtures
- **THEN** it records AT-SPI text-interface evidence for supported text surfaces, including bounded text length, focus or visible fallback, caret metadata where supported, and selection metadata where supported
- **AND** unsupported platform limitations are recorded as caveats instead of passing silently

#### Scenario: Focus proof covers opening and dismissal
- **WHEN** a transient surface opens or closes through keyboard activation, pointer dismissal, Escape, success, cancellation, or workflow completion
- **THEN** accessibility smoke or widget tests verify the expected focus target or documented fallback
- **AND** hidden or dismissed controls are not exposed as visible focus targets after accessibility readiness settles

### Requirement: Dynamic states and announcements are fully proven
LushText SHALL expose user-meaningful dynamic workflow states through GTK accessible state, alert roles, or bounded announcements, and SHALL prove that repeated updates do not become screen-reader noise.

#### Scenario: Workflow states update accessible metadata
- **WHEN** an editor, search surface, preview surface, sidebar, dialog, preference row, or status control becomes readonly, busy, invalid, hidden, disabled, expanded, collapsed, selected, current, pressed, checked, or unavailable
- **THEN** the accessible metadata reflects the visible behavior after the UI settles
- **AND** the metadata is cleared when the state no longer applies

#### Scenario: Result and replace workflows announce bounded outcomes
- **WHEN** in-tab search, workspace search, replace preview, Replace All, undo replacement, command palette filtering, Open popover filtering, notes filtering, or local-history filtering completes after user input
- **THEN** assistive technology receives a bounded result count, no-result state, error state, completion state, or undo-availability announcement or state update
- **AND** normal typing does not announce every intermediate keystroke

#### Scenario: Alerts and destructive confirmations use appropriate priority
- **WHEN** a failed save, durability warning, failed load, recovery warning, invalid operation, destructive confirmation, unsaved close dialog, delete confirmation, restore confirmation, or migration/format-upgrade warning appears
- **THEN** the alert title and bounded body are exposed through an alert role or high-priority bounded announcement
- **AND** destructive and suggested actions expose stable names, roles, and keyboard-operable response paths

#### Scenario: Progress milestones are throttled
- **WHEN** load, save, workspace refresh, indexing, content search, replace preview, migration scan, format upgrade, recovery restore, or long-running preview rendering emits repeated progress
- **THEN** only useful milestones are announced
- **AND** repeated heartbeats or status repaints are throttled through shared announcement lanes

### Requirement: Keyboard parity includes context and pointer-convenience workflows
Every non-decorative operation SHALL be reachable without pointer hover or drag-only interaction, and the accessible path SHALL be verified.

#### Scenario: Context menus are keyboard-operable
- **WHEN** file tree, workspace header, note, bookmark, local-history, editor, search-result, tab, or preview context actions are available
- **THEN** a keyboard-only user can reach the same operation through the context-menu key, `Shift+F10`, menu action, command palette, or an equivalent documented keyboard path
- **AND** accessibility proof records the menu or fallback path without relying on pointer coordinates

#### Scenario: Hover and overlay actions have accessible fallbacks
- **WHEN** a row action, overlay action, drag handle, drop target, focus-folder button, remove button, or pointer convenience affordance appears only on hover or pointer movement
- **THEN** the same user operation is available through keyboard focus, context menu, command palette, or visible persistent control
- **AND** focus indication identifies the currently reachable target

#### Scenario: Destructive workflows keep safety behavior
- **WHEN** an accessibility or keyboard path triggers delete, discard, close, restore, Replace All, undo, migration, format upgrade, or Save As behavior
- **THEN** the normal confirmation, durable-write, modified-document, undo, recovery, and error-handling behavior applies
- **AND** accessibility-only or automation-only paths cannot mark unsafe operations successful

### Requirement: Visual accessibility proof covers variants and geometry extremes
LushText SHALL preserve visible accessibility affordances across supported visual variants, text scale, motion settings, opacity settings, and constrained geometry, with proof in the visual lanes where semantics alone cannot prove usability.

#### Scenario: Visual smoke covers variant states
- **WHEN** visual accessibility evidence is reviewed
- **THEN** it includes normal, dark, high-contrast where supported, large text, reduced-motion where supported, transparency/readability, compact layout, short height, narrow width, dense rows, long names, and destructive/error states
- **AND** unsupported host variants skip explicitly and do not count as passing evidence

#### Scenario: Focus indication remains visible
- **WHEN** keyboard focus moves through shell controls, editor, file tree rows, search rows, command palette rows, Open popover rows, dialogs, context menus, bottom sheets, preferences, and preview surfaces
- **THEN** a visible focus indication is present in supported visual variants and constrained geometry
- **AND** focus indication is not hidden behind overlays, clipped row actions, transparent content backgrounds, or animation frames

#### Scenario: State is not color-only
- **WHEN** LushText communicates warning, error, success, selection, current row, search match, modified tab, disabled action, file health, local-history restore state, bookmark state, destructive action, or replacement preview state
- **THEN** the state is also communicated through text, iconography, role/state metadata, shape, position, or another non-color-only cue
- **AND** the cue remains distinguishable in dark and high-contrast variants where supported

#### Scenario: Geometry preserves commands and readable regions
- **WHEN** the app runs with dense data, long labels, compact layout, bottom-sheet properties, narrow width, short height, or large text
- **THEN** primary actions, close/back controls, persistent status/header chrome, and item-region scrolling remain reachable
- **AND** unintended horizontal scrollbars, clipped primary controls, overlapping text, or fake rows are not accepted as passing coverage

### Requirement: Manual Orca validation is a release-grade artifact
LushText SHALL complement automated GTK and AT-SPI proof with repeatable manual Orca validation in a normal GNOME session before claiming release-grade accessibility for changed workflows.

#### Scenario: Manual checklist records environment and workflows
- **WHEN** a release or accessibility-sensitive change is validated manually
- **THEN** the manual Orca checklist records LushText build, install mode, operating system, GNOME session details, display backend, theme or visual variant, text scale, screen reader version, workflows checked, outcome, and caveats
- **AND** the checklist references the corresponding automated accessibility, visual, and widget artifacts when they exist

#### Scenario: Manual validation covers user-facing speech behavior
- **WHEN** manual Orca validation is performed
- **THEN** it covers shell navigation, editor focus, typing, caret feedback where available, selection feedback where available, in-tab search, command palette, Open popover, workspace search, workspace sidebar/file tree, document properties, preferences, Markdown preview, notes/bookmarks, local history, destructive/close dialogs, context menus, and changed workflows
- **AND** any behavior that differs from automated AT-SPI evidence is recorded as a caveat or follow-up finding

#### Scenario: Manual checks cannot be replaced by skipped automation
- **WHEN** automated AT-SPI, visual, compositor, or text-interface coverage skips for host reasons
- **THEN** the same behavior remains unverified until another runner or a manual Orca environment covers it
- **AND** release notes or validation artifacts identify the exact coverage source

### Requirement: Accessibility guardrails prove current tree and current evidence
LushText SHALL provide guardrails that detect accessibility drift in the current codebase, documentation, smoke matrix, stable anchors, and release evidence.

#### Scenario: Current-tree policy detects helper and row regressions
- **WHEN** accessibility policy checks run in strict mode
- **THEN** they inspect the current UI tree for direct helper bypasses, unallowlisted direct GTK accessibility calls, list factories without row apply/clear logic, icon-only controls without names, hover-only affordances without accessible fallback evidence, and transient surfaces without matrix coverage
- **AND** failures include clear file/line remediation guidance

#### Scenario: Matrix, docs, and smoke anchors stay synchronized
- **WHEN** stable AT-SPI anchors, smoke helper flags, accessibility smoke cases, accessibility matrix rows, or user-facing accessibility expectations change
- **THEN** documentation drift checks fail unless `docs/accessibility.md`, `docs/accessibility-matrix.md` or equivalent, `docs/automation.md`, `docs/automation-reference.md`, and relevant agent rules are synchronized
- **AND** stale fixture-only anchors are clearly marked as fixture-only rather than public product anchors

#### Scenario: Release proof uses fresh unfiltered summaries
- **WHEN** a release-grade accessibility claim is made for a commit
- **THEN** the accessibility summary is passed, unfiltered unless a scoped release note explains the filter, not skipped, has no unexpected warnings, includes required matrix coverage, and is fresh for the relevant current tree or exact release commit
- **AND** visual and manual evidence required by the matrix is reviewed with the same freshness rule

### Requirement: Accessibility proof remains privacy-preserving
Accessibility metadata, smoke artifacts, manual validation notes, screenshots, and logs SHALL remain bounded and SHALL NOT expose private user content beyond explicit committed smoke fixtures.

#### Scenario: Artifacts use bounded fixture data
- **WHEN** accessibility, automation, visual, or manual proof artifacts include text-like values
- **THEN** they use committed fixtures or bounded summaries such as roles, names, counts, fixture display names, selected state, short status strings, and capped diagnostic text
- **AND** they do not dump private document contents, note bodies, draft bodies, complete search result text, local-history contents, or private sidecar identifiers

#### Scenario: Announcements do not export content
- **WHEN** app-owned accessibility announcements are emitted
- **THEN** announcement text is capped and describes workflow state rather than exporting arbitrary document or note content
- **AND** tests cover UTF-8 safe truncation and throttling for repeated announcements

#### Scenario: Smoke fixtures remain synthetic
- **WHEN** smoke cases need editor text, search matches, recent documents, notes, bookmarks, local history, or recovery data
- **THEN** the data is created from synthetic fixture files or isolated smoke app data
- **AND** artifacts identify fixture paths and counts without depending on the user's real workspace

### Requirement: Omitted Markdown preview content is announced rather than silently missing
When Markdown preview replaces content it cannot render with a marker, the marker SHALL be an accessible object with a name or description that identifies it as omitted preview content and states why. This requirement covers user-visible omissions only. An embedded block that the preview replaces with its own in-place fallback presentation MUST be announced through that fallback alone, and MUST NOT also produce an omission marker or contribute to an omission count. A marker replacing a whole top-level block and a marker replacing one unit inside a still-rendered container (a table row, list item, code-block run, quoted paragraph, or definition body) MUST both be reachable and self-describing at their own position. The preview's terminal state SHALL distinguish a complete preview containing omissions from a preview whose rendering stopped at a global budget, and MUST report the number of omissions once rather than announcing each marker as it is projected.

#### Scenario: Marker replaces a whole block the preview cannot render
- **WHEN** Markdown preview omits a top-level block that exceeds its per-slice budgets and has no inline-safe checkpoint
- **THEN** assistive technology reaches a named marker at that position in the document
- **AND** its description states that the block was omitted and why

#### Scenario: Marker replaces one unit inside a rendered container
- **WHEN** Markdown preview omits one row, item, quoted paragraph, or definition body while rendering that container's other units
- **THEN** assistive technology reaches the marker at that unit's position inside the container, as a named object where that container's units are themselves accessible objects and through the preview's text interface where they are rendered as buffer text
- **AND** the container's surrounding units remain readable and correctly ordered
- **AND** the marker names the omitted unit rather than implying the whole container was dropped

#### Scenario: Embedded block resolves to its own in-place fallback
- **WHEN** a table or code block is replaced by the preview's own fallback presentation because it exceeds that block type's widget budget
- **THEN** assistive technology reaches only that fallback, which names the block and its true size
- **AND** no additional omission marker is present for the same block
- **AND** the preview's terminal description still reports a complete preview

#### Scenario: Complete preview containing omissions
- **WHEN** a preview generation finishes with one or more omissions
- **THEN** the preview surface's accessible description reports a complete preview with the number of omissions
- **AND** it does not claim that rendering stopped before the end of the document

#### Scenario: Preview stops at a global budget
- **WHEN** a preview generation stops because a global source, event, retained-byte, embed-descriptor, depth, or inline-footnote budget was exceeded
- **THEN** the accessible description names that stopped state and its reason
- **AND** it is distinguishable from the complete-with-omissions state
