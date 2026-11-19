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
- **THEN** readiness is decided by the same signal, and the mode reaches the runtime guard's verdict with all four classes (`rtkit`, `bluez`, `hardware-device`, `module-rt`) reported

### Requirement: A media readiness miss fails the session
If quiet media are not ready within `READY_TIMEOUT`, the helper SHALL NOT start the command. It SHALL save the last successful dump, if any, to the log directory as `pw-dump-not-ready.json`, print one bounded line naming WirePlumber, `session.services` and `policy.linking.standard` that says where the dump was kept, that it could not be saved there, that no `pw-dump` succeeded, or that `--log-dir` keeps it, and exit with `TEST_FAILURE_EXIT_CODE`.

#### Scenario: WirePlumber never loads its linking policy
- **WHEN** WirePlumber's `policy` profile is configured without `policy.standard`, or `session.services` is never set, in a session given `--log-dir`
- **THEN** after `READY_TIMEOUT` the helper exits `TEST_FAILURE_EXIT_CODE` without starting the command
- **AND** the log directory holds `pw-dump-not-ready.json` and the session's daemons are stopped
- **AND** the exit status is neither the unsupported-host code (77) nor `SESSION_HYGIENE_EXIT_CODE` (93)

### Requirement: Lanes fail on a WirePlumber readiness miss
A smoke lane SHALL report a case whose session failed because WirePlumber did not load its linking policy as failed. No lane SHALL classify that failure as a host-support skip.

#### Scenario: A WirePlumber readiness miss fails the lane
- **WHEN** a smoke case's session fails because WirePlumber did not load its linking policy
- **THEN** the lane reports the case as failed and exits nonzero
- **AND** the lane prints no `SKIP:` line for that case

### Requirement: The exported helper path survives a rebuild
Inside a session, the `GTK_LUSH_SESSION_BIN` the helper exports SHALL name the helper's file path without the ` (deleted)` suffix that Linux gives a replaced executable, `scripts/headless-session` SHALL retry briefly before it reports that path as not executable, and the GTK axiom probes that spawn that path for a nested control run (A115, A116) SHALL retry the spawn the same way while the path is missing or busy.

#### Scenario: A rebuild replaces the helper while a session runs
- **WHEN** `target/debug/gtk-lush-session` is removed and linked again while a session runs, and code inside the session starts a nested `scripts/headless-session` stage
- **THEN** the exported path names `target/debug/gtk-lush-session`
- **AND** the nested stage starts instead of exiting as an unsupported host

#### Scenario: A rebuild relinks the helper while an axiom probe starts its control
- **WHEN** the A115 or A116 probe spawns `GTK_LUSH_SESSION_BIN` for its nested control run while a `cargo build` has removed the helper and not yet linked it again
- **THEN** the probe retries the spawn briefly and its control run starts once the helper is linked
- **AND** any spawn error other than a missing or busy executable is reported at once
