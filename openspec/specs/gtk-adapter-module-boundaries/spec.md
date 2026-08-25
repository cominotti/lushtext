# GTK Adapter Module Boundaries Specification

## Purpose

Define behavior-neutral decomposition boundaries for large GTK adapters while preserving dependency direction, lifecycle safety, persistence guarantees, accessibility, and public automation contracts.
## Requirements
### Requirement: GTK adapter decomposition preserves dependency direction
The project SHALL keep dependency direction `ui -> services -> model` while decomposing large GTK adapters. Extracted modules MUST remain inside the owning UI subtree, MUST keep services and models GTK-free, and MUST NOT introduce new crates, generic repository/controller traits, or manager objects solely to move code.

#### Scenario: Window workflow module needs service data
- **WHEN** an extracted window notes module loads, persists, or searches app data
- **THEN** it continues to call the existing GTK-free service boundary from background work
- **AND** no GTK type is added to the service or model contract

#### Scenario: Shared widget state remains widget-owned
- **WHEN** multiple extracted modules need the same template child or runtime state
- **THEN** that state remains owned by the existing widget implementation
- **AND** modules receive narrow owner references or plain inputs instead of a new manager

### Requirement: Window notes are organized by existing workflows
The `ui/window/notes` module SHALL separate bookmark lifecycle/navigation, document and folder note editors, and notes-browser/command-palette coordination into focused sibling modules under one private notes facade. Existing window actions, callback routes, sidecar safety, migration behavior, menu availability, note previews, and target activation MUST remain unchanged.

#### Scenario: Bookmark workflow remains complete
- **WHEN** a bookmark is toggled, edited, navigated, browsed, previewed, saved, renamed, or activated after decomposition
- **THEN** the same bookmark service and editor/window paths are used
- **AND** debounce, generation, excerpt, accessibility, and failure behavior remain intact

#### Scenario: Document and folder notes remain complete
- **WHEN** a document or folder note is opened, edited, previewed, saved, discarded, or migrated after decomposition
- **THEN** the same target identity and durable sidecar workflow is used
- **AND** no note body or pending migration is lost

#### Scenario: Notes browser and palette stay synchronized
- **WHEN** note sources change while the Notes browser or command palette is active
- **THEN** existing generation and refresh rules still update the current surface
- **AND** stale async previews or source loads cannot replace newer state

#### Scenario: Notes state extremes remain readable
- **WHEN** there are no notes, one note, representative category rows, many notes, awkward paths, no active editor, or constrained geometry
- **THEN** the existing empty states, category structure, actions, focus, and item-region scrolling remain usable
- **AND** no fake note row is introduced by the module split

### Requirement: Adaptive-shell policy is pure and behavior-equivalent
The project SHALL place adaptive-shell width, breakpoint, presentation, and compact-surface decision math in a plain Rust `ui/window` module with explicit input and output value objects. GObject lifecycle, template state, settings reads, breakpoint objects, focus, signals, and widget mutation MUST remain in the window adapter, and applying the pure decision MUST preserve current shell behavior.

#### Scenario: Pure policy derives wide layout
- **WHEN** explicit inputs describe a wide window with both secondary surfaces requested
- **THEN** the pure policy returns the same workspace, properties, breakpoint, and center-width decision as the pre-extraction implementation
- **AND** it does not inspect GTK widgets or GSettings directly

#### Scenario: Pure policy derives compact arbitration
- **WHEN** explicit inputs describe a compact window and an explicitly selected secondary surface
- **THEN** the pure policy preserves that compact surface and requested desktop intent
- **AND** the window adapter alone applies visibility and focus changes

#### Scenario: Allocation path remains runtime-only
- **WHEN** split-view allocation changes repeatedly during animation
- **THEN** the adapter uses the pure calculations without persisting GSettings or reparsing unchanged breakpoints
- **AND** the status bar remains visible at constrained heights

### Requirement: Workspace-section wiring has focused owners
The workspace-section implementation SHALL keep subclass state, template children, construction, and disposal in `imp.rs` while row factory projection, context-menu/gesture lifecycle, and row accessibility projection live in focused sibling modules. Setup, bind, unbind, and disposal MUST preserve signal cleanup, binding cleanup, object-data ownership, DnD behavior, inline rename, selection, expansion, and popup lifecycle.

#### Scenario: Recycled row clears old wiring
- **WHEN** a virtualized list item is unbound and rebound to another file-tree row
- **THEN** prior signal bags, bindings, context targets, accessibility state, DnD state, and inline-rename triggers are cleared before new projection
- **AND** the old item cannot receive callbacks through the recycled row

#### Scenario: Context menus keep keyboard and pointer parity
- **WHEN** a user opens file or workspace context menus by right-click, Menu, or Shift+F10
- **THEN** the same target and action set is presented
- **AND** click-away, action activation, rebuild, refresh, and disposal pop down stale surfaces exactly once

#### Scenario: Row accessibility tracks expansion and position
- **WHEN** rows are bound, expanded, collapsed, reordered, disabled, or recycled
- **THEN** accessible name, description, set position, expanded state, and disabled state remain synchronized
- **AND** expanded-state signal hooks are disconnected on unbind

#### Scenario: Sidebar state extremes preserve geometry
- **WHEN** a section has zero folders, one folder, many expanded rows, long names, reorder DnD, focused-folder mode, or constrained width
- **THEN** header controls remain visible and only the item region scrolls
- **AND** tree disclosure, no-horizontal-scrollbar, empty-state, and context-menu contracts remain unchanged

### Requirement: Decomposition is verified as behavior-neutral
The project SHALL inventory public and private workflow seams before moving code and SHALL preserve or strengthen existing tests. Verification MUST cover compile/source fidelity, unit/integration/property tests, widget lifecycle and recycling, accessibility, automation actions and snapshots, visual geometry, runtime warnings, persistence failure paths, and updated module/agent documentation.

#### Scenario: Public automation inventory is unchanged
- **WHEN** the decomposition is complete
- **THEN** action IDs, D-Bus members, snapshot fields, readiness predicates, and automation-client behavior match the baseline
- **AND** any accidental difference is fixed rather than documented as a feature change

#### Scenario: Persistence failure paths remain safe
- **WHEN** note, bookmark, migration, or related async writes fail after the move
- **THEN** dirty/retryable state and diagnostics match the existing safety contract
- **AND** stale completions cannot clear newer state

#### Scenario: Module maps stay current
- **WHEN** `notes.rs`, adaptive policy, or workspace-section wiring is materially reorganized
- **THEN** root and crate README/module maps plus nearest `AGENTS.md` ownership guidance are updated in the same change
- **AND** agent-documentation checks pass

#### Scenario: GTK lifecycle stress remains clean
- **WHEN** tests repeatedly open, close, bind, unbind, resize, switch themes, and dispose the affected surfaces
- **THEN** no unexpected GTK, GDK, Libadwaita, GIO, accessibility, borrow, or signal warnings occur
- **AND** no extracted callback retains disposed widget state

### Requirement: Markdown preview is organized by rendering workflows
The `ui/markdown_preview` widget SHALL separate its rendering workflows —
image admission and decode, table building, code-block theming and repair,
footnote and link handling, and render-plan orchestration/projection — into
focused sibling modules under the existing widget folder, keeping the public
widget wrapper, template contract, and automation surface unchanged. The
resulting `mod.rs` MUST no longer mix these workflows in one multi-thousand
line implementation block, and extracted modules MUST follow the existing
decomposition rules: no new crates, no generic manager or controller layers,
widget-owned state stays on the existing `imp` struct, and services/models
remain GTK-free.

#### Scenario: Image pipeline is a focused module
- **WHEN** Markdown preview admits, decodes, applies, or retires an embedded
  image
- **THEN** that workflow lives in a dedicated sibling module rather than the
  render-orchestration file
- **AND** bounded admission, worker handoff, generation rejection, and
  disposal behavior are unchanged

#### Scenario: Table and code-block rendering are focused modules
- **WHEN** Markdown preview builds table cells or themes and repairs code
  blocks
- **THEN** each workflow lives in its own sibling module with the same
  observable buffer output
- **AND** the documented idle-plus-timeout code-block repair exception keeps
  its existing mechanism and timing

#### Scenario: Decomposition is behavior- and pixel-neutral
- **WHEN** the existing markdown-preview widget tests, visual lanes, and
  smoke coverage run against the decomposed widget
- **THEN** they pass without weakened assertions
- **AND** render output, sliced GTK application budgets, readiness
  reporting, and accessibility metadata are byte- and behavior-identical

### Requirement: Decomposed workflow modules carry named roles
Decomposition of a GTK adapter workflow SHALL assign each resulting module one
named role: narrative facade, seam value objects, pure policy, coordination, or
evidence. A module MUST NOT combine the facade role with the coordination or policy
role. Splitting a large file into sibling modules without assigning roles does not
satisfy this requirement.

The existing prohibitions still apply: role assignment MUST NOT introduce new
crates, generic repository or controller traits, or manager objects solely to move
code, and services and models MUST remain GTK-free.

The coordination role SHALL be named from a bounded set of role names that state the
job the module performs, such as admission, execution, retirement, or watch. The
convention MUST NOT fix one single coordination file name, because a workflow may
own more than one coordination job and a directory may host more than one workflow.
A coordination job that no existing role name describes MUST be added to the bounded
set by amending this specification, rather than reusing an ill-fitting name or
inventing an unlisted one.

The pure policy role is named `policy.rs` and the evidence role is named
`evidence.rs`, one of each per workflow. The facade role is the workflow's public
module surface.

#### Scenario: Coordination file is named for its job
- **WHEN** a workflow's coordination module is created or renamed during migration
- **THEN** its file name states the coordination job it performs
- **AND** a name that only says the module is runtime machinery is not sufficient

#### Scenario: One directory hosting several workflows keeps flat role names
- **WHEN** a directory hosts more than one workflow
- **THEN** each workflow's coordination modules use role names scoped to that
  workflow within the shared directory
- **AND** migration does not require restructuring the directory into one
  subdirectory per workflow

#### Scenario: Novel coordination job extends the bounded set explicitly
- **WHEN** a workflow needs a coordination job outside the recorded role names
- **THEN** the change amends this specification to add the role name
- **AND** it does not overload an existing role name to fit

#### Scenario: Decomposition assigns roles rather than only splitting lines
- **WHEN** a large GTK adapter workflow is decomposed
- **THEN** each resulting module has one named role
- **AND** the facade module delegates to the others rather than implementing their
  work

#### Scenario: Role split does not add indirection layers
- **WHEN** role assignment would require a new trait, manager type, or crate to
  express
- **THEN** the split is expressed with plain modules and narrow owner references
  instead
- **AND** the existing prohibition on abstraction added solely to move code stands

#### Scenario: Line-count-only split is insufficient
- **WHEN** a file is split into siblings that each still mix narration,
  coordination, and policy
- **THEN** the decomposition does not satisfy the workflow readability convention
- **AND** the workflow's matrix row remains unmigrated

### Requirement: Pure policy hoisted for tooling reach may move into its workflow
Pure decision logic that was placed in `model/` in order to obtain test or mutation
tooling reach, and that has a single owning workflow, SHALL be permitted to move
into that workflow's directory as a pure `policy.rs` module. The move MUST preserve
purity, MUST preserve behavior, and MUST preserve mutation coverage. Dependency
direction `ui -> services -> model` MUST remain intact, and the relocated module
MUST NOT acquire dependencies on GTK types.

#### Scenario: Relocation preserves purity and direction
- **WHEN** single-consumer pure policy moves from `model/` into its owning workflow
- **THEN** the relocated module still has no GTK-family imports
- **AND** `services/` and `model/` remain GTK-free and no new upward dependency is
  created

#### Scenario: Relocation preserves mutation coverage
- **WHEN** pure policy relocates out of `model/`
- **THEN** the same mutants are generated and killed after the move as before
- **AND** the change records that parity as evidence

#### Scenario: Multi-consumer policy is not relocated
- **WHEN** pure policy has several genuinely unrelated consumers
- **THEN** it remains in its shared location
- **AND** the workflow readability matrix records it as cross-cutting

