## ADDED Requirements

### Requirement: Internal platform posture governs GTK Lush by default
The GTK Lush governance contract SHALL recognize the internal platform as the
default current posture after adoption validation. Governance MUST continue to
enforce the anti-framework constitution, publishing gates, treadmill SLAs, and
maintenance honesty, but MUST NOT require publication, repository graduation,
or upstreaming merely because earlier roadmap text named those possible
future tracks.

#### Scenario: Governance states the default posture
- **WHEN** `crates/gtk-lush/GOVERNANCE.md` is updated by this change
- **THEN** it records that GTK Lush is currently maintained as an in-tree
  LushText platform
- **AND** functional publication, `0.1.0`, repository split, and LushText
  migration to published dependencies remain blocked until a dedicated
  maintainer-approved publication or graduation change reopens them

#### Scenario: Constitution still blocks overreach
- **WHEN** future internal-platform work changes a GTK Lush crate or API
- **THEN** the constitution checklist still rejects control-flow ownership,
  view DSLs, component/message systems, sibling runtime dependencies, and
  Libadwaita replacements
- **AND** no internal-platform shortcut can bypass the exception register

### Requirement: Publication gates are preserved as dormant gates
Publication gates SHALL remain preserved as dormant gates for future reopened
publication work. Existing publication, adoption, semver, public-API, docs,
and maintainer approval gates remain available for that track. They MUST be
described as dormant gates that apply when publication is explicitly proposed,
not as unfinished work that blocks the internal platform from being considered
complete.

#### Scenario: Publication gate text is not removed
- **WHEN** documentation is pruned for internal stewardship
- **THEN** the publication gates remain documented
- **AND** the text distinguishes dormant future-track gates from checks that
  must run for ordinary in-tree GTK Lush maintenance

#### Scenario: Reopened publication refreshes evidence
- **WHEN** a future proposal reopens functional publication or repository
  graduation
- **THEN** it cites existing adoption-validation evidence
- **AND** it refreshes any stale adoption, semver, public-API, docs,
  changelog, release, credential, and maintainer-approval evidence before
  release or split work proceeds

### Requirement: Bigger phase-level planning remains the GTK Lush default
GTK Lush planning SHALL prefer one coherent phase-level OpenSpec change for a
strategic posture, extraction, publication, or stewardship effort. Smaller
changes MAY be split out only when they have independent ownership,
validation, or risk boundaries.

#### Scenario: Strategic GTK Lush change is not fragmented by default
- **WHEN** future GTK Lush work is proposed after this stabilization
- **THEN** the proposal starts from a phase-level scope
- **AND** any split into smaller changes records the reason in design or tasks
