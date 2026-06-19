## ADDED Requirements

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
