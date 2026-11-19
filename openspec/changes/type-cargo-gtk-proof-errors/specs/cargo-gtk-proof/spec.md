# Spec Delta

## ADDED Requirements

### Requirement: Proof-tool failures are typed with their cause
`cargo-gtk-proof` SHALL report failures through typed error values that keep the underlying cause reachable through `std::error::Error::source()`, and SHALL render the full cause chain once at the command-line edge. Library functions MUST NOT return `Result<_, String>`.

#### Scenario: PNG read failure names its cause
- **WHEN** a policy or corpus command reads a PNG file that does not exist
- **THEN** the command fails with a message that names the path and the underlying I/O error
- **AND** the I/O error is the error's source, not only text inside its message

#### Scenario: No string-typed results remain
- **WHEN** `crates/cargo-gtk-proof/src` is searched for `Result<` with a `String` error type
- **THEN** no signature matches

### Requirement: Tool-originated proof statuses are a closed set with an unchanged wire form
Every status or exit-class value that `cargo-gtk-proof` itself originates into an artifact, envelope or summary SHALL come from a closed enumeration whose serialized form is identical to the string written before the change. Values carried from another program or from a scenario manifest MAY stay strings.

#### Scenario: Statuses and exit classes are byte-identical
- **WHEN** the embedded PNG corpus, the policy self-tests and the artifact unit tests run before and after the change
- **THEN** every `status` and `exit_class` field and every artifact path they write is byte-identical

#### Scenario: Application readiness status passes through
- **WHEN** the application reports a readiness status the tool does not know
- **THEN** the tool records it as given, without failing to serialize
