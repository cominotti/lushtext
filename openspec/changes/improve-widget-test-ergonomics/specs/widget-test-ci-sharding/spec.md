# Spec Delta

## ADDED Requirements

### Requirement: Widget tests compile from the files contributors edit
The widget registry SHALL include each widget test file from its own path, without copying or rewriting it, and SHALL register exactly the top-level functions declared `pub(crate) fn test_<name>()` in it. The shard discovery SHALL apply the same rule, and a registered name SHALL be `<file stem>::<function name>`.

#### Scenario: A failing test points at its source
- **WHEN** a widget test fails to compile or panics
- **THEN** the reported path is the file under `crates/lushtext/tests/widget/` that declares it, not a generated copy

#### Scenario: Registry and discovery agree
- **WHEN** a widget file declares a new `pub(crate) fn test_<name>()`
- **THEN** the compiled binary's `--list` and the shard table's discovery both include `<stem>::test_<name>`
- **AND** a test no shard owns fails `make check-widget-shards`

#### Scenario: A helper cannot pass for a test
- **WHEN** a function declared `pub(crate) fn test_<name>` takes parameters or returns a value
- **THEN** the build fails, because the registry only accepts `fn()`
- **AND** a private helper named `test_<name>` is not registered
