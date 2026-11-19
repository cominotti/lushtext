# Spec Delta

## ADDED Requirements

### Requirement: Widget tests compile from the files contributors edit
The widget registry SHALL include each widget file from its own path, without copying or rewriting it, and SHALL register exactly the top-level functions declared `pub(crate) fn test_<name>()` in each file that is not a support file (stem `common` or ending in `_support`). The shard discovery SHALL apply the same rule, and a registered name SHALL be `<file stem>::<function name>`.

#### Scenario: A failing test points at its source
- **WHEN** a widget test fails to compile or panics
- **THEN** the reported path is the file under `crates/lushtext/tests/widget/` that declares it, not a generated copy

#### Scenario: Registry and discovery agree
- **WHEN** a widget file declares a new `pub(crate) fn test_<name>()`
- **THEN** the compiled binary's `--list` and the shard table's discovery both include `<stem>::test_<name>`
- **AND** a test no shard owns fails `make check-widget-shards`

#### Scenario: Support files register nothing
- **WHEN** `common.rs` or a `*_support.rs` file declares `pub(crate) fn test_<name>()` helpers
- **THEN** the file is compiled as a module and none of its functions is registered or discovered

#### Scenario: A helper cannot pass for a test
- **WHEN** a function declared `pub(crate) fn test_<name>` takes parameters or returns a value
- **THEN** the build fails, because the registry only accepts `fn()`
- **AND** a private helper named `test_<name>` is not registered
