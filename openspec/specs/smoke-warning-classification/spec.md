# smoke-warning-classification Specification

## Purpose
TBD - created by archiving change close-consistency-and-decomposition-gaps. Update Purpose after archive.

## Requirements

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

### Requirement: System-bus warnings of headless sessions are one shared family
Session-service warnings about missing system services (mutter's and the `xdg-desktop-portal` frontend's) SHALL be one shared family in the shared classifier module, matched by exact text: the existing "Headless container session services" family, extended rather than duplicated. It covers its current "Could not connect" lines and the private-system-bus `ServiceUnknown` lines. Widget, axiom and smoke runners SHALL read it from the module rather than keep copies.

#### Scenario: The widget runner classifies a private-bus warning
- **WHEN** a widget session's log contains `libmutter-WARNING **: … There is no colord server available`
- **THEN** `run-widget-tests.sh` treats it as benign through the family it read from the shared module
- **AND** a near-miss line with different text still fails the scan

#### Scenario: A copy reappears in a runner
- **WHEN** a runner adds its own pattern for a line the family already covers
- **THEN** `make check-session-hygiene` fails and names the runner and the copied text

### Requirement: Shell runners read the family through a proven rendering
The module SHALL print a POSIX ERE rendering of a family (`--print-regex <family>`) for shell runners, and its self-test SHALL prove that rendering with `grep -E`.

#### Scenario: The rendering a shell runner reads is the module's rule
- **WHEN** the module's self-test runs
- **THEN** `grep -E` with the `--print-regex` output accepts every benign sample line of the family and rejects every defect sample, so the Python rule and its shell rendering cannot disagree
