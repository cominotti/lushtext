## ADDED Requirements

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
redaction or omission behavior for private state.

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
