## ADDED Requirements

### Requirement: Quiet media are ready before the command starts
With media on, the helper SHALL start the session's command only once WirePlumber's client object, read from `pw-dump`, lists `policy.linking.standard` as a member of its `session.services` property. A pure function of `session::observe` SHALL decide this from the dump text. The helper SHALL NOT wait for an object that `pw-dump` may omit, such as a metadata object without entries.

#### Scenario: WirePlumber's policy is loaded
- **WHEN** a quiet `screencast` session starts and WirePlumber's client lists `policy.linking.standard` in `session.services`
- **THEN** the command starts at the next poll, not after `READY_TIMEOUT`
- **AND** a ScreenCast capture through `pipewiresrc` links and yields its first buffer on its first try

#### Scenario: Member order does not matter
- **WHEN** `session.services` lists `policy.linking.standard` at any position of its bracketed list
- **THEN** the readiness function reports ready
- **AND** a member that only starts with `policy.linking.standard`, or a dump without a `session.services` value, reports not ready

#### Scenario: The bare configuration still reaches its verdict
- **WHEN** the helper runs with `--expect-dirty`, which starts WirePlumber's `main` profile
- **THEN** readiness is decided by the same signal, and the mode reaches the runtime guard's verdict

### Requirement: A media readiness miss fails the session
If quiet media are not ready within `READY_TIMEOUT`, the helper SHALL NOT start the command. It SHALL save the last dump to the log directory as `pw-dump-not-ready.json`, print one bounded line naming WirePlumber, `session.services` and `policy.linking.standard`, and exit with `TEST_FAILURE_EXIT_CODE`.

#### Scenario: WirePlumber never loads its linking policy
- **WHEN** WirePlumber's `policy` profile is configured without `policy.standard`, or `session.services` is never set
- **THEN** after `READY_TIMEOUT` the helper exits `TEST_FAILURE_EXIT_CODE` without starting the command
- **AND** the log directory holds `pw-dump-not-ready.json` and the session's daemons are stopped
- **AND** the exit status is neither the unsupported-host code (77) nor `SESSION_HYGIENE_EXIT_CODE` (93)

### Requirement: Lanes fail on a media readiness failure
A smoke lane SHALL report a case whose session failed media readiness as failed, and SHALL NOT classify it as a host-support skip. A media daemon that is installed but does not become ready SHALL NOT count as missing host support.

#### Scenario: A media readiness failure fails the lane
- **WHEN** a smoke case's session fails because PipeWire did not become ready or WirePlumber did not load its linking policy
- **THEN** the lane reports the case as failed and exits nonzero
- **AND** the lane prints no `SKIP:` line for that case
