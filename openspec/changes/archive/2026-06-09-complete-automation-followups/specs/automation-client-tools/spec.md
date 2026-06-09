## ADDED Requirements

### Requirement: Supported Automation Client Exists
The project SHALL provide a supported command-line automation client for same-user developers and agents to consume the documented Automation1 interface and cataloged GTK/GIO action surface without hand-writing raw `gdbus` calls for common workflows.

#### Scenario: Client exposes documented subcommands
- **WHEN** a developer runs the automation client help command
- **THEN** it lists supported subcommands for introspection, action catalog reads, snapshot reads, readiness predicate reads, workflow-event reads, readiness waits, cataloged action activation, artifact summaries, and self-test
- **AND** the help text names the default bus name, object path, interface, and action path assumptions or the flags that override them

#### Scenario: Client uses existing Automation1 defaults
- **WHEN** the client runs without explicit D-Bus destination flags
- **THEN** it targets the session bus name `dev.cominotti.lushtext`, object path `/dev/cominotti/lushtext/Automation`, and interface `dev.cominotti.lushtext.Automation1`
- **AND** it does not require a system bus, elevated privileges, or a portals-only environment

#### Scenario: Missing host tooling reports a stable error
- **WHEN** the client cannot find a required host tool such as `gdbus`
- **THEN** it exits nonzero with a stable `unsupported-host-tooling` status
- **AND** it does not report the command as an application failure

### Requirement: Client Provides Bounded Read-Only Inspection
The automation client SHALL expose bounded read-only commands for Automation1 introspection, action catalog reads, snapshots, readiness predicates, and workflow events.

#### Scenario: Snapshot read returns machine-readable state
- **WHEN** LushText is running on the caller's session bus
- **AND** the developer runs the client snapshot command with JSON output
- **THEN** the command returns valid JSON containing a stable top-level result envelope
- **AND** the embedded snapshot is the bounded Automation1 snapshot without document text, note bodies, draft bodies, local-history contents, complete search result text, or private persistence identifiers

#### Scenario: Field extraction stays bounded
- **WHEN** the developer requests a specific snapshot field such as the active tab title, modified flag, or search match count
- **THEN** the client extracts that field without printing unrelated snapshot payloads
- **AND** missing fields report a stable error instead of printing misleading empty success output

#### Scenario: Workflow events are readable
- **WHEN** workflow events have been recorded by Automation1
- **THEN** the client events command returns the bounded workflow event snapshot
- **AND** it preserves event sequence, workflow id, phase, status, summary, and blocker fields without adding a new event source

### Requirement: Client Waits On Readiness Predicates
The automation client SHALL provide a readiness wait command that wraps Automation1 `WaitForReady` and reports stable statuses and exit codes for agents.

#### Scenario: Ready predicate succeeds
- **WHEN** the requested readiness predicate settles before the timeout
- **THEN** the client exits successfully with status `ready`
- **AND** JSON output records the predicate, timeout, ok flag, status, detail, and elapsed time or equivalent timing detail

#### Scenario: Predicate timeout is distinguishable
- **WHEN** the requested readiness predicate remains blocked until timeout
- **THEN** the client exits nonzero with status `predicate-timeout`
- **AND** the detail identifies the blocker returned by Automation1 when available

#### Scenario: Unknown predicate is distinguishable
- **WHEN** the developer requests a predicate not supported by the current Automation1 version
- **THEN** the client exits nonzero with status `unknown-predicate`
- **AND** it does not silently fall back to broad idle waits

### Requirement: Client Activates Only Cataloged Supported Actions
The automation client SHALL activate state-changing behavior only through documented GTK/GIO actions that are represented in the action catalog as supported exported actions.

#### Scenario: Supported action activation succeeds
- **WHEN** LushText is running and the developer activates a supported exported action such as `win.set-search-query` with a valid string parameter
- **THEN** the client validates the requested action against `GetActionCatalog`
- **AND** it calls `org.gtk.Actions.Activate` with the correct object path, action name, and GVariant parameter
- **AND** it reports success without directly mutating private widgets or Automation1 state

#### Scenario: Unsupported gap is rejected
- **WHEN** the developer requests an action cataloged as `unsupported-gap` or `visible-unregistered-gap`
- **THEN** the client refuses activation with status `unsupported-action`
- **AND** it prints the catalog row's docs anchor or label so the caller can find the documented blocker

#### Scenario: Parameter mismatch is rejected
- **WHEN** the developer supplies a parameter kind that does not match the cataloged action parameter type
- **THEN** the client refuses activation before calling D-Bus
- **AND** it exits with status `parameter-mismatch`

#### Scenario: Contextual disabled action remains app-owned
- **WHEN** a cataloged action requires UI context that is not currently present
- **THEN** the client reports the D-Bus or action-group failure without inventing private widget context
- **AND** the app's normal safety and enablement rules remain authoritative

### Requirement: Client Summarizes Smoke Artifacts
The automation client SHALL provide an artifact summary command that reads known smoke artifact directories and emits bounded review summaries for humans and agents.

#### Scenario: Automation smoke artifacts are summarized
- **WHEN** the developer runs artifact summary on `build/smoke/automation`
- **THEN** the client reports the scenario status, manifest path, summary path, warning-scan status, D-Bus assertion artifacts, action/catalog artifacts, readiness artifacts, workflow-event artifact, snapshot artifacts, and skip or failure reason when present
- **AND** it does not embed unbounded logs or full snapshot payloads in the summary

#### Scenario: Failed or skipped lane points to evidence
- **WHEN** a smoke artifact directory records failure or skip state
- **THEN** the client exits nonzero for failed artifacts and successfully or distinctly for skipped artifacts according to the documented exit-code contract
- **AND** it prints the relative paths to the most useful evidence artifacts

#### Scenario: Unknown artifact directory is handled clearly
- **WHEN** the developer points artifact summary at a directory without a recognized manifest, summary, or warning-scan shape
- **THEN** the client exits nonzero with status `artifact-error`
- **AND** it does not claim coverage passed

### Requirement: Client Output And Errors Are Stable
The automation client SHALL document and preserve a stable result envelope, status vocabulary, and exit-code contract for automation consumers.

#### Scenario: Successful JSON output has a stable envelope
- **WHEN** any client command succeeds with JSON output enabled
- **THEN** the output contains `ok`, `status`, `command`, `detail`, and `data` fields
- **AND** additional fields are additive and documented before release

#### Scenario: Error JSON output has a stable envelope
- **WHEN** any client command fails with JSON output enabled
- **THEN** the output contains `ok=false`, a stable `status`, the `command`, a bounded `detail`, and any safe diagnostic `data`
- **AND** it does not include document contents or private persistence identifiers

#### Scenario: Exit codes are documented
- **WHEN** maintainers read the automation reference
- **THEN** it documents exit codes for success, app or predicate failure, usage or parameter mismatch, automation unavailable, and unsupported host tooling
- **AND** client tests prove representative commands return the documented classes

### Requirement: Client Documentation Stays Current
The automation client SHALL be documented as part of the public automation contract and guarded by the automation documentation drift check.

#### Scenario: Client commands are documented
- **WHEN** users or maintainers read the automation guide and developer reference
- **THEN** they can find the supported client commands, flags, examples, status names, output envelope, artifact-summary behavior, safety boundaries, and troubleshooting guidance

#### Scenario: Drift check catches missing client docs
- **WHEN** a client command, flag, output field, status name, or exit-code class is added, removed, or renamed
- **THEN** `make check-automation-docs` fails until the automation documentation is updated
- **AND** the drift check self-test proves at least one representative missing client documentation case is caught
