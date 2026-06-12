## ADDED Requirements

### Requirement: Automation client accepts Rust visual proof envelopes
The automation client SHALL accept visual-geometry artifact summaries produced
by the Rust proof runner after parity is recorded. It MUST preserve its stable
result envelope, status vocabulary, exit-code classes, bounded evidence paths,
and privacy constraints while recognizing Rust engine metadata, schema
versions, parity metadata, and Python-oracle diagnostic annotations.

#### Scenario: Rust visual summary is reported through the client envelope
- **WHEN** a developer runs `scripts/lushtext-automation.py artifact-summary`
  against a Rust-produced visual-geometry artifact root
- **THEN** the command returns the documented automation-client result envelope
- **AND** it reports scenario status, verified invariant IDs, engine metadata,
  schema versions, warning-scan status, skip or failure reason, and bounded
  evidence paths

#### Scenario: Python oracle summary remains diagnostic
- **WHEN** the artifact root contains Python-oracle compatibility output after
  the Rust migration point
- **THEN** the client labels it as oracle or diagnostic evidence
- **AND** it does not report Python-only output as satisfying default Rust
  proof unless a documented parity mode is being summarized

## MODIFIED Requirements

### Requirement: Automation client remains stable while generic proof moves out
The LushText automation client SHALL keep its documented Automation1 client
commands, result envelope, exit-code classes, and artifact-summary behavior
stable while generic visual proof schemas, corpus checks, live execution, and
policy behavior move into `cargo-gtk-proof`. After delegated artifact shape is
parity-tested, the client MAY delegate generic visual summaries to the Rust
tool, but it MUST preserve existing command names or documented compatibility
aliases.

#### Scenario: Existing artifact-summary command still works
- **WHEN** a developer runs `scripts/lushtext-automation.py artifact-summary`
  against a visual-geometry artifact directory after the Rust tool migration
- **THEN** the command returns the documented stable result envelope
- **AND** it reports scenario status, invariant IDs, capture mode, summary
  paths, engine metadata, warning-scan status, skip/failure reason, and bounded
  evidence paths

#### Scenario: Delegation preserves exit classes
- **WHEN** the automation client delegates visual summary parsing to
  `cargo-gtk-proof`
- **THEN** pass, fail, skipped, unsupported-host, usage-error, and
  artifact-error outcomes map to the documented automation-client exit classes
- **AND** callers do not need to parse raw cargo output to determine status

#### Scenario: Delegation failure is explicit
- **WHEN** Rust proof-tool delegation is unavailable, unsupported, or returns
  malformed output
- **THEN** the automation client reports a stable unsupported-host-tooling,
  artifact-error, or usage-error status as appropriate
- **AND** it does not claim visual proof passed

### Requirement: Client self-test covers proof boundary documentation
The automation client self-test SHALL cover the artifact-summary shape that
Rust proof tooling must preserve for delegation. The self-test MUST run
without a live LushText D-Bus app and MUST validate representative success,
failure, skipped, unsupported-host, malformed-artifact, Python-oracle, and
Rust-produced envelopes.

#### Scenario: Self-test validates visual summary shape
- **WHEN** `make automation-client-self-test` runs
- **THEN** it validates at least one representative visual summary fixture
  through the automation client's artifact-summary path
- **AND** it proves the client keeps the documented `ok`, `status`, `command`,
  `detail`, and `data` envelope fields

#### Scenario: Future delegation must fail gracefully
- **WHEN** delegation to the Rust proof tool fails because the tool is missing,
  unsupported, or returns an unsupported schema
- **THEN** the client reports stable unsupported-host-tooling or artifact-error
  statuses
- **AND** it does not claim the summary passed

#### Scenario: Rust and Python fixtures stay distinguishable
- **WHEN** the self-test reads Rust-produced and Python-oracle visual fixtures
- **THEN** it verifies that the summary exposes engine metadata and oracle
  status
- **AND** Python-only evidence is not treated as default Rust proof after the
  migration point

### Requirement: Client documentation tracks proof-tool boundary
Automation documentation SHALL describe which commands are still owned by the
LushText automation client and which visual proof operations are owned by
`cargo-gtk-proof`. The documentation drift check MUST fail when command names,
flags, status names, result fields, exit-code classes, engine metadata, or
delegation behavior changes without matching docs.

#### Scenario: Docs identify the right tool for visual proof
- **WHEN** users read automation and proof documentation after parity is
  recorded
- **THEN** they can tell when to use `scripts/lushtext-automation.py` for
  Automation1 inspection and artifact summaries
- **AND** when to use `cargo gtk-proof` for schema validation, corpus replay,
  Rust live scenario execution, and Rust proof-policy checks
- **AND** Python is described only as an explicit oracle or diagnostic
  compatibility path

#### Scenario: Drift check catches boundary changes
- **WHEN** a proof parity change renames a client command, wrapper flag,
  output field, status name, engine metadata field, exit-code class, or
  proof-tool delegation path
- **THEN** `make check-automation-docs` fails until the docs and self-tests are
  updated

#### Scenario: Documentation preserves privacy boundaries
- **WHEN** docs describe Rust visual proof summaries and automation-client
  delegation
- **THEN** they state that artifact summaries expose bounded paths, counters,
  statuses, and safe metadata
- **AND** they state that document text, note bodies, draft bodies,
  local-history contents, complete search result text, raw image data, and
  private persistence identifiers remain excluded
