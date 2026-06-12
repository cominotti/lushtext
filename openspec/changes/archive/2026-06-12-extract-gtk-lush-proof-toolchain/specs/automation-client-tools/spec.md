## ADDED Requirements

### Requirement: Automation client remains stable while generic proof moves out
The LushText automation client SHALL keep its documented Automation1 client
commands, result envelope, exit-code classes, and artifact-summary behavior
stable while generic visual proof schemas, corpus checks, and policy behavior
move into `cargo-gtk-proof`. The client MAY delegate generic visual summaries
to the Rust tool only after the delegated artifact shape is parity-tested, and
it MUST preserve existing command names or documented compatibility aliases.

#### Scenario: Existing artifact-summary command still works
- **WHEN** a developer runs `scripts/lushtext-automation.py artifact-summary`
  against a visual-geometry artifact directory after the Rust tool migration
- **THEN** the command returns the documented stable result envelope
- **AND** it reports scenario status, invariant IDs, capture mode, summary
  paths, warning-scan status, skip/failure reason, and bounded evidence paths

#### Scenario: Delegation preserves exit classes
- **WHEN** the automation client delegates visual summary parsing to
  `cargo-gtk-proof`
- **THEN** pass, fail, skipped, unsupported-host, usage-error, and
  artifact-error outcomes map to the documented automation-client exit classes
- **AND** callers do not need to parse raw cargo output to determine status

### Requirement: Client self-test covers proof boundary documentation
The automation client self-test SHALL keep covering the artifact-summary shape
that Rust proof tooling must preserve before any future delegation. The
self-test MUST run without a live LushText D-Bus app and MUST validate
representative success and failure envelopes.

#### Scenario: Self-test validates visual summary shape
- **WHEN** `make automation-client-self-test` runs
- **THEN** it validates at least one representative visual summary fixture
  through the automation client's artifact-summary path
- **AND** it proves the client keeps the documented `ok`, `status`, `command`,
  `detail`, and `data` envelope fields

#### Scenario: Future delegation must fail gracefully
- **WHEN** a later phase delegates artifact summary parsing to the Rust proof
  tool
- **THEN** missing proof-tool or unsupported-host paths report stable
  unsupported-host-tooling or artifact-error statuses
- **AND** they do not claim the summary passed

### Requirement: Client documentation tracks proof-tool boundary
Automation documentation SHALL describe which commands are still owned by the
LushText automation client and which visual proof operations are owned by
`cargo-gtk-proof`. The documentation drift check MUST fail when command names,
flags, status names, result fields, or delegation behavior changes without
matching docs.

#### Scenario: Docs identify the right tool for visual proof
- **WHEN** users read automation and proof documentation
- **THEN** they can tell when to use `scripts/lushtext-automation.py` for
  Automation1 inspection and artifact summaries
- **AND** when to use `cargo gtk-proof` for schema validation, corpus replay,
  and Rust proof-policy checks, with live scenario execution still owned by
  the Python wrapper in this phase

#### Scenario: Drift check catches boundary changes
- **WHEN** a Phase 4 change renames a client command, wrapper flag, output
  field, status name, or proof-tool delegation path
- **THEN** `make check-automation-docs` fails until the docs and self-tests are
  updated
