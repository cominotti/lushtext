## MODIFIED Requirements

### Requirement: Project-specific API policy uses the right enforcement tool
The project SHALL use `clippy.toml` `disallowed-methods` or `disallowed-types` only for globally safe bans that apply across all paths where Clippy runs, or for a ban whose lint the workspace Clippy table allows everywhere and that only modules on a path-aware, audited list raise. Path-sensitive policies, including filesystem-boundary ownership, private backend access, approved engine adapters, and fixture exceptions, MUST remain enforced by path-aware audit tooling. The one scoped ban is the whole-pixel float-method list, raised only by the whole-pixel geometry policy modules that `make check-workflow-boundaries` protects.

#### Scenario: Globally disallowed APIs are configured only when safe
- **WHEN** implementation adds or updates `clippy.toml`
- **THEN** each disallowed method or type includes a reason and replacement when one exists
- **AND** the ban does not require broad local suppressions in approved backend, build-support, fixture, generated, or test-harness code

#### Scenario: A scoped ban is raised only where an audit requires it
- **WHEN** `clippy.toml` lists methods whose lint `[workspace.lints.clippy]` allows
- **THEN** the lint is raised only by `forbid` or `deny` attributes in modules a path-aware audit enumerates, and that audit fails when one of them lacks the attribute
- **AND** the lint policy guidance states that a new list added to `clippy.toml` would inherit the workspace `allow` and check nothing unless it is globally safe or enumerated the same way

#### Scenario: Filesystem boundary remains path-aware
- **WHEN** production or test code imports raw filesystem APIs, direct backend APIs, or approved filesystem engine APIs outside allowlisted modules
- **THEN** the filesystem-boundary audit reports the violation
- **AND** the project does not rely on Clippy alone for this path-sensitive policy

#### Scenario: clippy.toml absence is intentional if no global ban is safe
- **WHEN** implementation determines there is no globally safe or audited scoped disallowed-methods or disallowed-types policy to add
- **THEN** that decision is documented in the lint policy guidance or design notes
- **AND** no empty or misleading `clippy.toml` is created solely to appear comprehensive
