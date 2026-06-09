## ADDED Requirements

### Requirement: Automation client summarizes visual geometry artifacts
The automation client SHALL summarize visual geometry smoke artifacts through the stable result envelope. Summaries MUST identify scenario status, capture steps, geometry snapshots, crop comparison reports, warning scans, screenshots, masks, skip reasons, and failure evidence without embedding unbounded logs or image data.

#### Scenario: Passing visual comparison is summarized
- **WHEN** a developer runs the automation client artifact-summary command on a visual geometry smoke artifact directory
- **THEN** the client reports the scenario id, pass status, compared capture steps, protected regions with zero differences, allowed-changing regions, warning-scan result, and manifest path
- **AND** it exits successfully through the documented result envelope

#### Scenario: Failing visual comparison points to evidence
- **WHEN** a visual geometry smoke artifact directory records a failed crop comparison, readiness timeout, state mismatch, or warning scan
- **THEN** the client exits nonzero with a stable status such as `visual-comparison-failed`, `predicate-timeout`, `state-mismatch`, or `warning-scan-failed`
- **AND** it prints relative paths to the most useful bounded evidence artifacts

#### Scenario: Skipped visual geometry lane is distinct
- **WHEN** a visual geometry artifact directory records unsupported host tooling or compositor capture limitations
- **THEN** the client reports a skip status distinct from pass and fail
- **AND** it does not count the skipped invariant as verified

### Requirement: Automation client can wait for visual geometry readiness
The automation client SHALL support waiting on the documented visual geometry readiness predicate through its existing readiness wait command and stable status vocabulary.

#### Scenario: Visual readiness wait succeeds
- **WHEN** LushText is running and the client waits for visual geometry readiness
- **THEN** the client calls the documented Automation1 readiness predicate
- **AND** successful JSON output records the predicate, timeout, ready status, and bounded detail

#### Scenario: Visual readiness timeout is distinguishable
- **WHEN** visual geometry readiness times out
- **THEN** the client reports `predicate-timeout`
- **AND** the output includes the bounded Automation1 blocker detail without falling back to broad idle waits

### Requirement: Automation client preserves visual artifact privacy
The automation client SHALL keep visual artifact summaries bounded and privacy-preserving.

#### Scenario: Summary omits image payloads and content text
- **WHEN** the client summarizes screenshots, crop diffs, geometry state, or automation snapshots
- **THEN** it reports relative artifact paths and bounded counters or statuses
- **AND** it does not print image payloads, full logs, document text, note bodies, draft bodies, local-history contents, or complete search result text
