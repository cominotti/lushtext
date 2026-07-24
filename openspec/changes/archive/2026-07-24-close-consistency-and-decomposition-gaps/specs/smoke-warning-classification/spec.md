## ADDED Requirements

### Requirement: Smoke lanes share warning classifiers per warning family
Headless smoke lanes that scan runtime logs for known-benign toolkit
warnings SHALL classify each shared warning family (for example the Gdk
broken-pipe teardown noise) through one importable classifier module rather
than per-script pattern copies. Lane-specific allowlist entries remain owned
by their lane, but a pattern that appears in more than one lane MUST have a
single source of truth so the next allowlist change cannot silently diverge
across lanes.

#### Scenario: Shared benign-warning pattern updates in one place
- **WHEN** a shared benign-warning pattern (such as Gdk broken-pipe
  teardown noise) needs its match rule adjusted
- **THEN** the change is made in the shared classifier module
- **AND** the accessibility, crash-recovery, automation, visual, and
  visual-geometry lanes all observe the updated rule without per-script
  edits

#### Scenario: Lane-specific entries stay lane-owned
- **WHEN** a warning is genuinely specific to one lane's environment
- **THEN** its classification lives with that lane
- **AND** the shared module documents which families are shared versus
  lane-owned

#### Scenario: Divergent copies are detectable
- **WHEN** policy checks or review run against the smoke scripts
- **THEN** a re-introduced hand-rolled copy of a shared warning pattern is
  identifiable against the documented single-source contract
- **AND** the shared classifier module remains part of the relevant source
  fingerprint sets where applicable
