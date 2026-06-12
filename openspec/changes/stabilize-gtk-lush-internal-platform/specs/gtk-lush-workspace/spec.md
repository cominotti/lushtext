## ADDED Requirements

### Requirement: Workspace path dependencies are the steady-state integration
LushText SHALL consume functional GTK Lush crates through workspace path
dependencies as the default steady-state integration. This path-based
integration MUST be valid for ordinary development, testing, OpenSpec
implementation, and LushText release preparation unless a future dedicated
publication/graduation change explicitly migrates the dependency source.

#### Scenario: Path dependencies remain intentional
- **WHEN** maintainers inspect root workspace dependencies and LushText crate
  manifests after this change
- **THEN** GTK Lush dependencies remain workspace path dependencies
- **AND** documentation describes that arrangement as the current intended
  internal-platform state rather than a temporary defect

#### Scenario: Published dependency migration requires a new change
- **WHEN** a future change attempts to replace GTK Lush workspace path
  dependencies with crates.io dependencies or an external repository source
- **THEN** it requires a dedicated publication or graduation proposal
- **AND** that proposal records release, versioning, repository-history, and
  rollback plans before implementation

### Requirement: Internal-platform checks remain first-class workspace gates
The workspace SHALL keep GTK Lush policy, doctest, example, adoption, MSRV,
public-API advisory, and proof-related checks discoverable as local gates for
the in-tree platform. The checks MAY remain advisory where publication has not
started, but they MUST catch stale crate lists, dependency-direction drift,
missing examples, stale adoption matrix rows, and malformed proof-policy
artifacts.

#### Scenario: Check list stays synchronized
- **WHEN** a GTK Lush crate is added, removed, renamed, or changes its adoption
  workflow
- **THEN** Makefile targets, policy scripts, adoption matrix rows, README
  crate lists, and advisory package lists are updated in the same change
- **AND** `make check-gtk-lush-policy` and `make check-gtk-lush-adoption`
  pass before archive

#### Scenario: Publication-only checks are clearly labeled
- **WHEN** a check is advisory because the crates are not published
- **THEN** documentation labels it as advisory for the internal platform
- **AND** does not imply that a failing publication-only credential or release
  setup blocks ordinary LushText development

### Requirement: Internal-platform artifacts stay bounded
GTK Lush internal-platform artifacts SHALL remain bounded and reviewable.
Generated adoption artifacts, visual proof outputs, external checkouts, large
logs, screenshots, and temporary worktrees MUST stay in ignored or documented
artifact locations and MUST NOT be committed as source evidence.

#### Scenario: Generated artifacts are not committed
- **WHEN** adoption or proof checks create local outputs
- **THEN** those outputs live under documented ignored build or fixture target
  paths
- **AND** committed evidence remains bounded markdown, TOML, schemas,
  examples, or small fixtures suitable for code review
