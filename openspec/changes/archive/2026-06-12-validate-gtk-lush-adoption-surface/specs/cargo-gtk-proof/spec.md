## ADDED Requirements

### Requirement: Proof tool wording reflects post-parity Rust authority
`cargo-gtk-proof` docs and specs SHALL describe Rust live visual proof as the
default authoritative proof path after Phase 4 parity. They MUST NOT describe the Rust
live runner as a future implementation slice, a staged non-coverage surface, or
secondary to Python. Python MAY be described only as an explicit diagnostic,
oracle, or compatibility path. Historical fixture identifiers or serialized
compatibility metadata MAY retain old names only when documented as historical
compatibility data.

#### Scenario: Stale staged wording is removed
- **WHEN** maintainers search current proof-tool docs, source module docs, and
  canonical OpenSpec wording for staged-runner or future-live-runner language
- **THEN** no user-facing or canonical text claims Rust live visual proof is
  still staged, future, or non-authoritative
- **AND** any retained historical `rust-staged` metadata is documented as
  compatibility fixture data rather than current tool status

#### Scenario: Python path is explicitly diagnostic
- **WHEN** a developer reads `cargo-gtk-proof` help, README, source docs, or
  proof-tool specs
- **THEN** Python is described as an explicit oracle, diagnostic, or
  compatibility path
- **AND** the docs do not imply that Python is the default execution oracle for
  current visual proof

## RENAMED Requirements

- FROM: `### Requirement: Cargo GTK proof tool exposes stable staged subcommands`
- TO: `### Requirement: Cargo GTK proof tool exposes stable proof subcommands`
