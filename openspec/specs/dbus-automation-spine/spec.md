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

### Requirement: Preview automation remains stable across shell migration
The automation spine SHALL preserve the documented preview actions, snapshot
fields, and readiness behavior while the preview shell moves from `GtkPaned`
animation to Adwaita-native presentation. Automation consumers MUST be able to
drive the same target states and observe the same bounded preview state without
mutating private widgets or depending on implementation-specific pane geometry.

#### Scenario: Preview target-state actions still converge
- **WHEN** an automation client activates `win.set-preview-pane-visible` or `win.set-preview-mode` with a boolean parameter
- **THEN** the action routes through the normal window preview workflow
- **AND** repeated calls with the same parameter converge on the same visible preview state
- **AND** side-by-side preview and preview-only mode remain mutually exclusive

#### Scenario: Snapshot fields keep their meaning
- **WHEN** a snapshot is requested after preview layout settles
- **THEN** `surfaces.preview_pane_visible` reports whether side-by-side preview is requested and visible according to the shell's explicit preview state
- **AND** `surfaces.preview_mode` reports whether preview-only mode is the active content presentation
- **AND** the snapshot does not expose private widget identities, preview document text, or implementation-specific layout-node paths

#### Scenario: Readiness tracks preview presentation work
- **WHEN** a preview target-state action starts a shell transition, layout-view switch, or embedded preview layout repair
- **THEN** `visual-geometry-settled` and `idle` readiness do not report ready until the preview presentation work has settled
- **AND** any renamed or newly exposed preview readiness blocker is documented in the automation guide, developer reference, action catalog checks, and automation client self-test before it is treated as stable

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

### Requirement: Automation exposes bounded animation geometry diagnostics
Automation1 SHALL expose bounded visual geometry diagnostics that allow visual
smoke tooling to correlate animation frames with application state. The
diagnostics MUST include named surface rectangles, visibility, transition phase,
timing information, readiness blockers, and minimap/source-map diagnostic fields
needed to explain rendered drift. The diagnostics MUST NOT expose document text,
note bodies, draft bodies, local-history contents, full search result text, or
private persistence identifiers.

#### Scenario: Animation sample includes phase and surface geometry
- **WHEN** a visual smoke helper samples Automation1 during workspace-sidebar animation
- **THEN** each sample includes a monotonic timestamp or equivalent timing field
- **AND** it identifies the workspace sidebar, editor viewport, source view, minimap shell, source map, marker strip, and status bar rectangles when present
- **AND** it identifies whether the transition phase is settled, showing, hiding, or intermediate by documented fields

#### Scenario: Snapshot reports sidebar animation geometry
- **WHEN** an automation snapshot is requested while the workspace sidebar is animating
- **THEN** the visual geometry payload includes bounded workspace-sidebar, editor viewport, minimap shell, source-map, and native minimap diagnostic geometry for the current frame
- **AND** it reports enough state to distinguish fully shown, fully hidden, and intermediate sidebar positions

#### Scenario: Minimap diagnostics explain rendered anchors without text
- **WHEN** the native minimap is visible during an animation sample
- **THEN** Automation1 reports bounded minimap diagnostics such as allocation, top inset policy, adjustment values, anchor state, refresh blockers, and detector crop bounds
- **AND** it does not report minimap-rendered text or document body content

#### Scenario: Snapshot reports native minimap frame inputs
- **WHEN** an automation snapshot is requested while the minimap is visible during sidebar animation
- **THEN** the native minimap diagnostics include bounded source-map visible state, source-map adjustment state, editor visible state, document-height ratio inputs, compensation margin or equivalent top-inset diagnostic, and estimated native slider rect when available
- **AND** absent or unprojectable diagnostics use stable absence reasons

#### Scenario: Readiness distinguishes animation sampling from final settle
- **WHEN** a smoke helper starts animation-frame capture
- **THEN** it can begin from a settled baseline without waiting through the action being sampled
- **AND** after stream capture it can wait for the existing final visual geometry readiness predicate to prove endpoint stability

#### Scenario: Missing animation diagnostics fail clearly
- **WHEN** an animation-frame visual invariant requires Automation1 timing or geometry fields
- **AND** the running app does not expose those fields
- **THEN** the smoke helper reports a distinct contract failure
- **AND** it does not silently downgrade to fixed sleeps or final-settle-only proof

### Requirement: Automation exposes bounded native minimap render diagnostics
The Automation1 visual geometry snapshot SHALL expose bounded diagnostics for the native minimap rendering path when the minimap is visible. The diagnostics MUST be sufficient to compare app-estimated native slider geometry with rendered screenshot anchors, while preserving the automation privacy boundary.

#### Scenario: Snapshot reports native minimap diagnostic fields
- **WHEN** a snapshot is requested while the active editor has a visible minimap
- **THEN** the visual geometry payload includes bounded native minimap diagnostic fields such as source-map allocation, editor visible-rect summary, source-map visible-rect or adjustment summary, estimated native slider rect, first-content-row rect, and projection source classification
- **AND** the payload does not include document text, note bodies, draft bodies, local-history contents, complete search result text, or private persistence identifiers

#### Scenario: Hidden or unavailable minimap reports explicit absence
- **WHEN** the active editor has no visible native minimap
- **THEN** the visual geometry payload reports stable absent native-minimap diagnostic rows with a bounded absence reason
- **AND** visual tooling can distinguish unavailable state from a missing snapshot schema

### Requirement: Visual geometry readiness includes native minimap frame work
Automation readiness for visual geometry SHALL account for native minimap refresh, source-map allocation, and post-frame invalidation work that can affect rendered minimap anchors. The `visual-geometry-settled` predicate MUST NOT return ready for native minimap rendered-effect scenarios while known app-owned minimap work or required post-frame native-map sampling is still pending.

#### Scenario: Minimap frame work blocks readiness
- **WHEN** a sidebar or editor width transition schedules minimap projection refresh, source-map redraw or resize, dynamic overscroll refresh, or final native minimap frame sampling
- **THEN** `visual-geometry-settled` reports a bounded minimap-related blocker until that work has settled
- **AND** the application remains responsive while readiness is pending

#### Scenario: Readiness still uses screenshots for rendered truth
- **WHEN** `visual-geometry-settled` reports ready for a native minimap scenario
- **THEN** visual tooling may capture screenshots
- **AND** the final pass/fail result still depends on screenshot-derived pixel anchors rather than the readiness predicate alone

### Requirement: Visual readiness distinguishes settled state from animation capture
Automation readiness SHALL keep the existing `visual-geometry-settled` predicate
for final-state proof while allowing animation-frame capture to observe
intermediate geometry intentionally. Animation capture MUST NOT wait for final
sidebar geometry before sampling frames, but it MUST still wait for the initial
document, minimap, and action state needed to start from a known baseline.

#### Scenario: Animation capture starts from ready baseline
- **WHEN** a visual runner prepares a native minimap animation scenario
- **THEN** Automation1 readiness confirms the file is loaded, the minimap is visible, the editor starts at the requested scroll position, and the sidebar starts in the requested shown or hidden state
- **AND** frame sampling starts immediately after the sidebar action rather than after final sidebar geometry settles

#### Scenario: Final readiness remains available after animation capture
- **WHEN** animation-frame sampling finishes
- **THEN** visual tooling can still wait for `visual-geometry-settled`
- **AND** final settled before/after assertions continue to use the existing readiness and geometry predicates

### Requirement: Visual geometry readiness covers final animated allocations
The automation readiness contract SHALL expose or support deterministic visual-geometry waits that remain blocked until animated shell allocations relevant to the requested workflow have reached their final stable state. For workspace-sidebar transitions, readiness MUST not be considered sufficient for visual proof while the sidebar or editor viewport is still between final allocations.

#### Scenario: Sidebar hide readiness waits for final allocation
- **WHEN** an automation client hides the workspace sidebar and then waits for visual geometry readiness intended for visual proof
- **THEN** readiness remains blocked until the sidebar allocation is fully hidden, the editor viewport has expanded to its final left edge, and relevant visual geometry rows have remained stable across multiple samples
- **AND** the readiness result or snapshot evidence distinguishes final geometry from an intermediate animation frame

#### Scenario: Sidebar show readiness waits for final allocation
- **WHEN** an automation client shows the workspace sidebar and then waits for visual geometry readiness intended for visual proof
- **THEN** readiness remains blocked until the sidebar allocation is fully visible, the editor viewport starts after the sidebar, and relevant visual geometry rows have remained stable across multiple samples
- **AND** a mid-animation sidebar allocation such as a negative `x` while requested visible is not reported as final visual readiness

#### Scenario: Visual readiness timeout exposes blocker detail
- **WHEN** final animated allocations do not settle before the timeout
- **THEN** the readiness wait reports a timeout with bounded blocker detail naming the unsettled surface or relationship
- **AND** it does not return a generic ready status that would allow visual proof to capture stale or transitional geometry

### Requirement: Automation distinguishes workspace-sidebar animation phases
Automation1 SHALL expose bounded visual-geometry state that allows automation and visual tooling to distinguish fully hidden, fully visible, and intermediate workspace-sidebar transition phases for both show and hide actions. The diagnostic state MUST remain privacy-preserving and MUST support final readiness after animation capture without forcing animation-frame sampling to wait for final geometry.

#### Scenario: Snapshot exposes intermediate sidebar state
- **WHEN** an automation snapshot is requested while the workspace sidebar is between hidden and visible endpoints
- **THEN** the visual geometry payload identifies the workspace sidebar as transitional or intermediate through documented bounded fields
- **AND** it includes enough geometry to determine whether the sidebar is moving toward shown or hidden state
- **AND** it does not expose document text, note bodies, draft bodies, local-history contents, full search result text, or private persistence identifiers

#### Scenario: Final readiness waits after show
- **WHEN** an automation client shows the workspace sidebar and then waits for final visual geometry readiness
- **THEN** readiness remains blocked until the workspace sidebar reaches fully visible geometry, relevant editor/minimap geometry is stable across required samples, and any app-owned minimap transition work has settled
- **AND** the readiness result distinguishes final geometry from any sampled intermediate frame

#### Scenario: Final readiness waits after hide
- **WHEN** an automation client hides the workspace sidebar and then waits for final visual geometry readiness
- **THEN** readiness remains blocked until the workspace sidebar reaches fully hidden geometry, relevant editor/minimap geometry is stable across required samples, and any app-owned minimap transition work has settled
- **AND** the readiness result distinguishes final geometry from any sampled intermediate frame

#### Scenario: Animation sampling starts from a known baseline
- **WHEN** a visual runner prepares to capture workspace-sidebar animation frames
- **THEN** Automation1 can confirm the app has a loaded baseline state, the requested initial sidebar state, and any required minimap or content fixture state
- **AND** the runner can trigger the sidebar action and sample frames before final sidebar geometry settles

### Requirement: Visual geometry snapshots expose enough state for final-geometry assertions
Automation snapshots SHALL expose bounded visual-geometry state needed by smoke helpers to assert final sidebar/editor/minimap relationships without private widget access or coordinate guesses.

#### Scenario: Snapshot supports sidebar final-state checks
- **WHEN** a visual geometry snapshot is requested during or after a workspace sidebar transition
- **THEN** it includes surface names, visibility, absence reason when any, screen-space rectangles, allocations, scale factor, and requested/visible shell state needed to determine whether the sidebar is fully hidden, fully visible, or transitional
- **AND** it does not require clients to inspect private GTK widgets

#### Scenario: Snapshot supports rendered anchor diagnostics
- **WHEN** the minimap is visible
- **THEN** the snapshot includes bounded app-owned minimap surfaces and pixel-anchor rectangles for the minimap viewport top edge, viewport fill, viewport bottom edge, and first content row when available
- **AND** screenshot-derived pixel comparison remains responsible for proving rendered-pixel stability

### Requirement: Automation1 adopts reusable proof spine without public drift
LushText's Automation1 implementation SHALL adopt `gtk-lush-proof-spine`
primitives behind its existing D-Bus surface. The migration MUST preserve the
documented bus name, object path, interface name, methods, properties, signals,
readiness predicates, snapshot field meanings, workflow-event semantics, action
catalog behavior, status vocabulary, and privacy boundaries except for
explicitly documented additive fields.

#### Scenario: D-Bus introspection diff is stable
- **WHEN** Automation1 introspection is compared before and after the spine
  migration
- **THEN** existing public members and signatures are unchanged
- **AND** any additive member or field is documented in `docs/automation.md`,
  `docs/automation-reference.md`, the action catalog reference where relevant,
  and automation client self-tests

#### Scenario: Existing readiness clients still work
- **WHEN** an existing client waits for `idle`, `visual-geometry-settled`, or
  another documented predicate through Automation1
- **THEN** the readiness result uses the same predicate name and semantic
  meaning as before the migration
- **AND** unknown predicates still fail explicitly rather than falling back to
  broad idle waits

### Requirement: Automation snapshot mapping remains bounded
The Automation1 adapter SHALL map LushText app state into proof-spine snapshot
objects without broadening the exposed data surface. Snapshot serialization
MUST remain bounded to documented diagnostics and MUST preserve existing
redaction or omission behavior for private state. For workflows that expose a
typed evidence surface, the adapter SHALL project snapshot fields from that
evidence surface rather than independently re-deriving the same state from
widgets, and the externally visible snapshot contract MUST remain unchanged by
that projection.

#### Scenario: Visual geometry fields remain safe
- **WHEN** a visual proof tool reads an Automation1 snapshot after layout
  settles
- **THEN** it can access documented safe surface names, rectangles,
  visibility, allocation sizes, scroll anchors, scale factor, and readiness
  detail
- **AND** it cannot access arbitrary widget pointers, document contents, note
  bodies, draft bodies, local-history contents, or private persistence IDs

#### Scenario: Snapshot field meanings do not change
- **WHEN** a smoke test compares representative pre-migration and
  post-migration snapshots for the same app state
- **THEN** fields such as active tab metadata, visible surfaces, search state,
  minimap state, preview state, workflow readiness, and recent notifications
  retain their documented meanings
- **AND** any intentionally additive field is optional for older clients

#### Scenario: Migrated workflow state is projected, not re-derived
- **WHEN** a workflow exposes a typed evidence surface and an automation snapshot
  reports that workflow's state
- **THEN** the adapter reads the evidence surface and projects the documented
  snapshot fields from it
- **AND** it does not maintain a second independent derivation of the same state
  from widget properties

#### Scenario: Projection does not widen the external surface
- **WHEN** an evidence surface exposes internal fields that are not part of the
  documented automation contract
- **THEN** those fields are not serialized into the snapshot
- **AND** existing redaction and omission behavior for private state is preserved

#### Scenario: Evidence-to-snapshot drift is detected
- **WHEN** an evidence surface gains, removes, or renames a field that a snapshot
  projects
- **THEN** `make check-automation-docs` fails until the automation documentation is
  updated
- **AND** the failure names both the evidence field and the affected snapshot field

### Requirement: Automation documentation proves spine migration
The Automation1 spine migration SHALL be backed by documentation and drift
checks. `make check-automation-docs` and
`make automation-client-self-test` MUST pass after the migration, and the docs
MUST explain which parts are reusable proof-spine concepts versus LushText's
app-specific D-Bus contract.

#### Scenario: Docs distinguish generic spine from LushText Automation1
- **WHEN** maintainers read the automation guide and developer reference
- **THEN** they can see that readiness/snapshot/value-object concepts are
  backed by `gtk-lush-proof-spine`
- **AND** they can also see that the D-Bus object, action names, snapshot field
  selection, and app workflows remain LushText-specific

#### Scenario: Drift check catches undocumented adapter changes
- **WHEN** a spine adapter change alters an exposed action, D-Bus member,
  snapshot field, readiness predicate, scenario helper flag, status name, or
  stability classification
- **THEN** `make check-automation-docs` fails until the documentation and
  client self-test coverage are updated

### Requirement: Automation SHALL support accessibility scenario readiness
The automation spine SHALL expose bounded readiness and state needed by accessibility scenarios to drive normal user-visible actions and wait for accessibility-sensitive UI settlement without mutating private widgets.

#### Scenario: Accessibility scenarios wait on named readiness
- **WHEN** an accessibility smoke scenario opens or changes editor, search, workspace search, Open popover, command palette, notes, local history, properties, preview, preferences, or save/close surfaces
- **THEN** it can wait on the narrowest relevant readiness predicate before querying AT-SPI
- **AND** fixed sleeps are used only as a documented fallback when no app-owned predicate exists

#### Scenario: Accessibility-sensitive state is bounded
- **WHEN** an accessibility scenario requests an automation snapshot
- **THEN** the snapshot reports only bounded state such as visible surfaces, active tab identity class, result counts, selected row identity class, busy/error state, and active transient surface
- **AND** it does not expose unbounded document text, note bodies, local-history contents, complete search results, or private persistence identifiers

#### Scenario: Surface readiness includes accessibility metadata updates
- **WHEN** a UI transition, row rebind, dialog presentation, preview render, search completion, or announcement-producing workflow updates accessible metadata
- **THEN** the relevant readiness predicate does not report ready until accessible names, roles, states, and focus targets have settled
- **AND** timeout failures preserve state, logs, and AT-SPI artifacts
### Requirement: Automation actions SHALL drive accessibility smoke through normal UI paths
Accessibility smoke scenarios SHALL use documented GTK/GIO actions and visible editables rather than private widget mutation, and automation support SHALL keep those paths aligned with normal user behavior.

#### Scenario: Visible workflows use public actions where possible
- **WHEN** accessibility smoke opens search, workspace search, Open popover, command palette, notes, local history, properties, preview, focus mode, minimap, or preferences
- **THEN** it uses documented app/window actions or keyboard shortcuts that share behavior with normal UI controls
- **AND** resulting UI state is verified through both automation state and accessibility-tree assertions

#### Scenario: Text entry uses visible editables
- **WHEN** accessibility smoke must type into a visible entry or editor
- **THEN** it uses keyboard input, AT-SPI editable-text APIs, or a documented action that routes through the normal workflow
- **AND** the final app-owned state is verified without relying only on coordinates or widget depth

#### Scenario: Private widget mutation is rejected
- **WHEN** a proposed accessibility helper attempts to mutate private GTK widget state outside user-visible actions or visible editables
- **THEN** the automation contract does not treat that path as a stable accessibility scenario API
- **AND** the design is changed to use normal actions, keyboard input, or bounded read-only observation
### Requirement: Automation artifacts SHALL include accessibility proof hooks
Automation scenario artifacts SHALL preserve enough bounded accessibility context to diagnose failures in accessibility and visual accessibility lanes.

#### Scenario: Artifact summary reads accessibility results
- **WHEN** `scripts/lushtext-automation.py artifact-summary` reads accessibility smoke artifacts
- **THEN** it reports passed, failed, skipped, or unsupported AT-SPI assertions, focus assertions, warning-scan status, scenario manifests, and environment details
- **AND** it keeps output bounded for terminal use

#### Scenario: Scenario manifests identify proof chain
- **WHEN** an accessibility-backed automation scenario writes a manifest
- **THEN** it names the normal action or keyboard step, readiness predicate, automation state assertion, AT-SPI assertion, screenshot or visual assertion when present, and warning scan
- **AND** failures identify which proof link failed

#### Scenario: Documentation covers accessibility automation fields
- **WHEN** automation snapshot fields, readiness predicates, helper flags, or artifact fields are added for accessibility scenarios
- **THEN** `docs/automation.md`, `docs/automation-reference.md`, action catalog coverage, and drift checks are updated in the same change
- **AND** unsupported host limitations are documented as diagnostics, not passing proof
