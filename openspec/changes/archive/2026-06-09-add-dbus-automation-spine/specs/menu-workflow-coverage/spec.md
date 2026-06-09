## ADDED Requirements

### Requirement: Visible commands SHALL be represented in the action catalog
The test suite and documentation SHALL keep visible commands, menus, shortcuts, command-palette entries, toolbar buttons, context-menu items, and status-bar controls aligned with the action catalog. A visible command MUST either map to a stable action or document why it requires a different interaction surface.

#### Scenario: Primary menu commands map to actions
- **WHEN** the primary menu model is audited
- **THEN** each actionable menu item maps to a cataloged app or window action
- **AND** the catalog names the owning workflow, label, shortcut when present, parameter type, and coverage lane

#### Scenario: Notes, sidebar, tab, and search commands map to actions
- **WHEN** notes menu items, sidebar context menu items, tab context menu items, search controls, status controls, and command-palette commands are audited
- **THEN** each visible command maps to a cataloged action or documented non-action control
- **AND** unsupported automation gaps are tracked as explicit follow-ups rather than hidden test assumptions

#### Scenario: Command palette uses cataloged commands
- **WHEN** the command palette indexes commands
- **THEN** indexed commands use cataloged action IDs and labels
- **AND** stale command IDs, missing actions, or undocumented command entries fail tests or catalog checks

### Requirement: Menu and action workflow tests SHALL use the same public actions as users
Menu workflow coverage SHALL prove behavior through user-visible actions and their D-Bus/catalog representation, while still using widget assertions for visible state and accessibility assertions where appropriate.

#### Scenario: Action activation and menu activation agree
- **WHEN** a workflow can be invoked through both a menu item and direct action activation
- **THEN** tests verify both paths reach the same state change or documented no-op
- **AND** the action catalog records both invocation surfaces

#### Scenario: Disabled commands are observable
- **WHEN** a command is unavailable because no document, no workspace folder, no search context, save in progress, or another precondition applies
- **THEN** the action enablement, menu sensitivity, command-palette availability, and automation snapshot agree according to the documented rule

#### Scenario: Parameterized commands are covered
- **WHEN** a command accepts a parameter such as search text, workspace scope, tab identity, preview mode, or zoom direction
- **THEN** tests cover valid values, invalid values, and no-context behavior
- **AND** the catalog documents parameter type and accepted values

### Requirement: Command documentation SHALL stay synchronized
The project SHALL document public commands in user-facing and developer-facing references and MUST keep command docs synchronized with action registration, menu resources, shortcuts, command palette entries, and tests.

#### Scenario: User-facing command docs are current
- **WHEN** a public command, shortcut, menu label, command-palette label, or visible control changes
- **THEN** user-facing docs such as README, shortcuts references, and automation examples are updated in the same change

#### Scenario: Developer command reference is current
- **WHEN** an action is added, removed, renamed, retargeted, or changes parameter/state type
- **THEN** the developer reference and action catalog are updated
- **AND** the documentation drift check fails if they are stale
