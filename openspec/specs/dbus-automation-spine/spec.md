# dbus-automation-spine Specification

## Purpose
Define LushText's bounded automation spine so agents, smoke tests, and developer
tools can drive user-level actions and inspect settled app state without unsafe
widget mutation or unbounded user-content exposure.

## Requirements
### Requirement: LushText exposes a bounded automation contract
The system SHALL expose a deliberate automation contract that combines normal
GTK/GIO actions for user-level commands with a narrow app-owned read-only D-Bus
inspection surface for state, readiness, and events. The contract MUST be
versioned, documented, and safe for use by automated agents, smoke tests, and
developer tools without exposing arbitrary widget mutation.

#### Scenario: Automation contract is introspectable
- **WHEN** LushText is running in a session bus environment
- **THEN** automation clients can discover the supported automation interface
  version, object path, methods, properties, and signals through D-Bus
  introspection or the documented reference
- **AND** unsupported or disabled automation surfaces fail with a documented
  error instead of silently doing nothing

#### Scenario: Automation surface is bounded
- **WHEN** an automation client requests state
- **THEN** LushText returns only bounded diagnostic fields such as active
  document identity, tab metadata, visible surfaces, search state, workflow
  readiness, and recent notification summaries
- **AND** it does not expose arbitrary widget mutation or unbounded document
  contents

#### Scenario: Automation contract works in isolated smoke sessions
- **WHEN** the headless Mutter smoke helpers launch LushText with isolated XDG
  state
- **THEN** the automation contract is available on the private session bus
- **AND** state assertions can run without touching the user's normal app data

### Requirement: User-level operations remain GTK/GIO actions
The system SHALL expose externally drivable mutations as GTK/GIO actions when
those operations correspond to user-visible commands. These actions MUST share
behavior with menus, shortcuts, command palette entries, toolbar buttons, and
in-app controls rather than creating a separate automation-only command path.

#### Scenario: Parameterized search action sets query through normal workflow
- **WHEN** an automation client activates the documented search action with a
  text parameter
- **THEN** the in-tab search UI opens or updates through the normal search
  workflow
- **AND** match highlighting, minimap markers, result counts, focus behavior,
  and close behavior match user typing in the visible search entry

#### Scenario: Visible surface actions update stateful actions
- **WHEN** an automation client toggles workspace sidebar, document properties,
  preview mode, focus mode, minimap, or search panel actions
- **THEN** the visible UI state changes through the same path as the user-facing
  control
- **AND** any corresponding stateful action reports the settled state after
  layout has applied

#### Scenario: Unsupported action parameters fail clearly
- **WHEN** an automation client activates a parameterized action with the wrong
  type or unsupported value
- **THEN** LushText rejects the activation without changing unrelated state
- **AND** the failure is observable through the action call result, automation
  state, logs, or documented smoke artifact

### Requirement: Automation snapshots report settled app state
The system SHALL provide a read-only automation snapshot that reports enough
state for agents and smoke tests to verify outcomes without relying only on
screenshots, fixed sleeps, or coordinate guesses.

#### Scenario: Snapshot identifies active document and tabs
- **WHEN** a snapshot is requested with open tabs
- **THEN** it reports the active tab, tab count, per-tab identity class,
  modified state, path or display identity when available, pinned state when
  supported, failed-load status when present, and save/load activity state
- **AND** path-like values are omitted or redacted only according to the
  documented privacy policy

#### Scenario: Snapshot identifies visible shell surfaces
- **WHEN** a snapshot is requested after UI actions settle
- **THEN** it reports visible or requested state for workspace sidebar, document
  properties, search bar, search panel, command palette, preview mode, focus
  mode, minimap, status bar, and active transient surfaces
- **AND** compact-layout mutual exclusion is represented without losing the
  user's requested visibility intent

#### Scenario: Snapshot identifies search and notes state
- **WHEN** search, notes, bookmarks, local history, or workspace search surfaces
  are active
- **THEN** the snapshot reports bounded workflow state such as query text when
  safe, result counts, selected result identity, empty-state kind, in-progress
  status, and last error summary
- **AND** the snapshot avoids unbounded document excerpts or raw user content
  dumps

### Requirement: Automation readiness is deterministic
The system SHALL provide deterministic readiness checks for workflows that
complete asynchronously or after GTK layout settles. Smoke helpers MUST wait on
these checks instead of fixed sleeps whenever an app-owned predicate is
available.

#### Scenario: Wait for idle observes background work
- **WHEN** a smoke helper requests an idle or settled state after opening,
  saving, searching, restoring, or refreshing
- **THEN** LushText reports readiness only after relevant GTK idle work,
  background task callbacks, layout synchronization, and workflow state updates
  have settled
- **AND** the helper receives a clear timeout error if the predicate never
  becomes true

#### Scenario: Workflow events bracket async operations
- **WHEN** a long-running workflow such as file load, save, workspace refresh,
  content search, replace preview, session restore, or recovery restore starts
  and finishes
- **THEN** the automation surface emits or records start and finish events with
  a stable workflow ID and bounded result summary

#### Scenario: Readiness does not block the GTK thread
- **WHEN** an automation client waits for readiness
- **THEN** LushText remains responsive to normal GTK events
- **AND** expensive state collection or filesystem work stays off the GTK main
  thread

### Requirement: Action catalog is authoritative and checked
The project SHALL maintain a machine-readable action catalog that maps every
public app/window action and visible command to its scope, type, state,
enablement rules, user-facing surfaces, documentation anchor, and test coverage.
The catalog MUST be generated from code or verified against registered actions
so changes cannot drift silently.

#### Scenario: Registered public actions appear in catalog
- **WHEN** the action catalog check runs
- **THEN** every public app/window action registered by LushText appears in the
  catalog with its parameter type, state type, external-activation safety, and
  owning workflow
- **AND** missing catalog entries fail the check

#### Scenario: Visible commands point to catalog entries
- **WHEN** menus, shortcuts, command palette entries, toolbar buttons, status
  controls, and notes/sidebar context actions are audited
- **THEN** each visible command maps to a cataloged action or documents why it
  cannot be activated through a stable action
- **AND** automation-only commands do not appear as user-visible commands unless
  they are intentionally user-facing

#### Scenario: Catalog records coverage
- **WHEN** the catalog is generated or checked
- **THEN** each externally supported action names at least one coverage lane
  such as unit, integration, widget, accessibility smoke, visual smoke,
  crash-recovery smoke, portal/sandbox smoke, or manual diagnostic coverage

### Requirement: Automation documentation is complete and kept up to date
The project SHALL provide extensive user-facing and developer-facing
documentation for every exposed automation surface. The implementation MUST NOT
be considered complete unless documentation and automated drift checks prove the
docs match the code.

#### Scenario: User-facing automation guide exists
- **WHEN** users, agents, or maintainers read the automation documentation
- **THEN** it explains supported use cases, safety boundaries, example `gdbus`
  or helper commands, scenario runner usage, portal/screenshot caveats, and
  troubleshooting steps
- **AND** it clearly distinguishes stable public automation from
  development-only diagnostics

#### Scenario: Developer reference documents every exposed member
- **WHEN** developers inspect the automation reference
- **THEN** every public app/window action, automation D-Bus method, property,
  signal, snapshot field, readiness predicate, scenario helper flag, and
  environment gate is documented with type, meaning, stability level, and test
  coverage

#### Scenario: Documentation drift fails validation
- **WHEN** an exposed action, D-Bus member, snapshot field, signal, scenario
  helper flag, or stability classification changes
- **THEN** the documentation check fails until the user-facing guide, developer
  reference, action catalog, and relevant testing/debugging guidance are updated
  in the same change

### Requirement: Scenario runner covers end-to-end user workflows
The project SHALL provide a scenario runner or scenario-capable smoke helpers
that drive real LushText processes through actions, state predicates, AT-SPI
assertions, screenshots, warning scans, and artifact manifests.

#### Scenario: Search scenario is pure D-Bus plus state where possible
- **WHEN** the search scenario opens a file, sets a query, navigates matches,
  toggles minimap, and captures a screenshot
- **THEN** command steps use documented actions where possible
- **AND** state assertions verify active file, query, match count, and minimap
  visibility before screenshot capture

#### Scenario: Workspace and notes scenarios cover state extremes
- **WHEN** workspace, notes, bookmarks, command palette, and search-panel
  scenarios run
- **THEN** they cover no required context, representative populated data, many
  or awkward items, and constrained geometry where those states are supported
- **AND** they assert reachable commands, readable empty states,
  item-region-only scrolling, preserved headers/close/actions, and absence of
  unintended scrollbars or fake rows

#### Scenario: Scenario artifacts are reviewable
- **WHEN** a scenario finishes, fails, or skips
- **THEN** it writes a bounded artifact manifest with command steps, state
  assertions, screenshots when requested, AT-SPI snippets, D-Bus summaries,
  warning scans, environment details, and skip or failure reason

### Requirement: Automation preserves user data and privacy
The automation spine SHALL preserve user data safety and privacy. It MUST NOT
mark failed operations as successful, bypass save/close safety, expose unbounded
document text, or mutate files outside normal user-command paths.

#### Scenario: Save and close safety is not bypassed
- **WHEN** an automation client activates save, close, discard, replace, or
  destructive workspace/file actions
- **THEN** the normal save-in-progress, modified-document, confirmation,
  durable-write, and recovery safety behavior applies
- **AND** automation cannot force success for an operation that failed or was
  cancelled

#### Scenario: Snapshot content is bounded
- **WHEN** a snapshot includes document-related diagnostics
- **THEN** it uses bounded summaries such as length, hash, status, active path
  identity, or explicitly safe query text
- **AND** it does not dump full document contents or recovery payloads

#### Scenario: External clients cannot access development-only controls accidentally
- **WHEN** development-only automation helpers are disabled
- **THEN** external clients cannot invoke those helpers through D-Bus,
  command-line flags, or hidden actions
- **AND** documentation identifies how release and development builds differ

### Requirement: Automation snapshots expose bounded visual geometry state
Automation1 SHALL expose bounded visual geometry state for smoke helpers and agents. The state MUST identify named surfaces, visibility, rectangles, allocation sizes, scroll anchors, scale factor, and visual readiness details without exposing document text or private persistence identifiers.

#### Scenario: Snapshot includes safe visual anchors
- **WHEN** an automation client requests a snapshot after a visual scenario step settles
- **THEN** the snapshot includes a documented visual geometry object or equivalent fields for named surfaces such as header bar, tab strip, editor viewport, source view, minimap, status bar, workspace sidebar, document properties, preview, and active transient surface when present
- **AND** each entry is bounded to safe geometry and state metadata

#### Scenario: Snapshot omits user content
- **WHEN** visual geometry state includes editor, minimap, preview, notes, bookmarks, or search surfaces
- **THEN** it does not include document text, minimap-rendered text, note bodies, draft bodies, local-history contents, complete search result text, or private persistence identifiers
- **AND** any path-like values follow the existing automation snapshot privacy policy

#### Scenario: Absent surfaces are explicit
- **WHEN** a named visual surface is not present because it is hidden, compact-suppressed, unsupported, or unavailable for the active document
- **THEN** the visual geometry state records the absence reason
- **AND** clients can distinguish intentional absence from a missing or stale snapshot field

### Requirement: Automation readiness includes visual geometry settlement
Automation1 SHALL provide a readiness predicate for visual geometry settlement. The predicate MUST wait for known UI blockers that affect screenshot correctness, including GTK idle layout work, shell split-view synchronization, minimap refresh/debounce, relevant animations, workspace refresh, and active visual scenario setup.

#### Scenario: Visual geometry wait succeeds after layout settles
- **WHEN** a smoke helper toggles a shell surface and waits for visual geometry readiness
- **THEN** Automation1 reports ready only after the affected layout, minimap, and visual anchors have settled
- **AND** the final snapshot matches the requested visible state

#### Scenario: Visual geometry wait reports blocker on timeout
- **WHEN** visual geometry readiness does not settle before the timeout
- **THEN** Automation1 returns a timeout with a bounded blocker such as `workspace-refresh`, `split-view-layout`, `minimap-refresh`, `animation`, `search`, or `unknown-visual-blocker`
- **AND** the helper preserves state and screenshot artifacts produced before failure

### Requirement: Visual geometry automation stays documented and versioned
Visual geometry snapshot fields, readiness predicates, helper flags, and scenario manifest fields SHALL be part of the documented automation contract and guarded by existing documentation drift checks.

#### Scenario: New geometry field requires docs
- **WHEN** a visual geometry snapshot field or readiness predicate is added, renamed, or removed
- **THEN** automation documentation and reference checks fail until the field, meaning, type, privacy boundary, and coverage lane are documented

#### Scenario: Helper flag drift is caught
- **WHEN** a visual capture helper flag related to geometry, masks, paired captures, or comparison artifacts changes
- **THEN** the automation reference drift check fails until the helper flag documentation is synchronized
