# blueprint-lint-policy Specification

## Purpose
Define how Blueprint lint diagnostics are classified, promoted, documented, and proven so safe cleanup can proceed without weakening UI-template contracts.

## Requirements

### Requirement: Blueprint lint policy SHALL distinguish blocking cleanup from advisory exceptions

The project SHALL maintain a curated Blueprint lint policy that classifies every current `blueprint-compiler lint` diagnostic as promoted blocking cleanup, accepted advisory exception, or structural follow-up requiring proof.

#### Scenario: Lint workflow reports classified diagnostics

- **WHEN** the Blueprint lint workflow runs
- **THEN** it reports diagnostics grouped by rule and file
- **AND** every reported rule is classified by the checked-in policy
- **AND** any unclassified rule or lint error fails the workflow

#### Scenario: Promoted lint findings stay clean

- **WHEN** a rule or rule/file subset is promoted to blocking cleanup
- **THEN** the Blueprint lint workflow fails if that promoted diagnostic reappears
- **AND** the failure identifies the rule and file that regressed

### Requirement: Safe Blueprint lint fixes SHALL preserve generated UI contracts

Safe lint cleanup SHALL update Blueprint source and generated GtkBuilder output together, without changing runtime resource paths, template class names, object IDs, CSS classes, accessibility metadata unrelated to the fix, or Rust `TemplateChild` bindings unless the change is explicitly required and verified.

#### Scenario: Text or accessibility cleanup regenerates matching UI

- **WHEN** a safe text, translation, Unicode, or accessibility lint fix changes a `.blp` template
- **THEN** the matching `.ui` file is regenerated from Blueprint source
- **AND** `make check-blueprint` passes
- **AND** the generated UI template contract remains valid

#### Scenario: Runtime-visible text remains intentional

- **WHEN** lint cleanup touches labels, titles, placeholders, tooltips, or accessibility strings
- **THEN** user-visible text remains appropriate for no-document states, representative populated states, and constrained geometry
- **AND** compact technical labels that are intentionally uppercase remain documented when they stay advisory

### Requirement: Structural Blueprint lint suggestions SHALL require proof before acceptance

Blueprint lint suggestions that change container type, scroll ownership, layout ownership, widget allocation, or template-child Rust types SHALL NOT be accepted as safe cleanup without proof that the affected visible surface still behaves correctly.

#### Scenario: Container or scroll changes are visually proven

- **WHEN** a `scrollable_parent`, `use_adw_bin`, or similar structural lint suggestion is implemented
- **THEN** generated `.ui` drift and template-contract checks pass
- **AND** affected Rust template-child bindings are updated intentionally
- **AND** widget tests or visual comparison cover representative populated data, empty or no-required-context states, many or awkward items where relevant, and constrained geometry

#### Scenario: Secondary surfaces remain usable

- **WHEN** a structural lint fix affects the editor shell, sidebar, properties panel, status bar, search panel, command palette, markdown preview, inline alerts, menus, dialogs, or popovers
- **THEN** preserved headers, close controls, actions, and item-region-only scrolling remain reachable
- **AND** no unintended scrollbars, fake rows, clipped actions, or unrelated-context dependencies are introduced

### Requirement: Advisory Blueprint lint exceptions SHALL remain narrow and documented

Accepted advisory exceptions SHALL name the lint rule, the affected files or diagnostic class, and the rationale for not fixing the warning immediately.

#### Scenario: Compiler-limited or semantic warnings stay classified

- **WHEN** a warning is accepted because of a Blueprint compiler limitation, compact technical label, runtime-populated placeholder, or geometry-sensitive ownership concern
- **THEN** the policy documents the reason
- **AND** the lint workflow accepts only that documented warning class

#### Scenario: Policy changes stay synchronized

- **WHEN** the promoted or advisory Blueprint lint rule set changes
- **THEN** the lint script, `docs/blueprint-validation.md`, and relevant contributor or agent guidance describe the same rule state
- **AND** strict OpenSpec validation passes for the changed capability
