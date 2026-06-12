## ADDED Requirements

### Requirement: Archived adoption evidence becomes a maintained baseline
The archived adoption-validation evidence SHALL become the maintained baseline
for GTK Lush internal-platform confidence. The evidence MUST be updated when a
functional GTK Lush API, example, adoption-lab workflow, stock fixture,
public-API advisory snapshot, or adoption matrix row changes, but it MUST NOT
force publication or repository graduation by itself.

#### Scenario: Evidence update follows API change
- **WHEN** a functional GTK Lush API changes after adoption validation
- **THEN** affected adoption matrix rows, examples, doctests, lab workflows,
  stock fixtures, API review notes, and advisory snapshots are updated in the
  same change
- **AND** local GTK Lush adoption checks pass before archive

#### Scenario: Evidence does not mandate publication
- **WHEN** maintainers cite the archived adoption-validation phase
- **THEN** the evidence supports internal stewardship and can support a future
  publication proposal
- **AND** it does not require publication, `0.1.0`, repository split, or
  LushText migration to published dependencies

### Requirement: Accepted limitations remain explicit
Accepted adoption limitations SHALL stay recorded in the adoption matrix or
API review until they are resolved, superseded, or moved to a reopened
publication track. They MUST NOT be silently forgotten when the project stops
treating publication as automatic.

#### Scenario: Accepted limitation remains visible
- **WHEN** GTK Lush adoption evidence is reviewed after this change
- **THEN** accepted limitations such as GObject-targeted settle scheduling and
  mapped-geometry render capture remain visible in the matrix or API review
- **AND** each limitation records whether it needs no action, future external
  adopter evidence, visual proof on internal changes, or publication-track
  reconsideration

### Requirement: External adoption spikes are optional until publication reopens
Unrelated-project adoption spikes SHALL remain optional during ordinary
internal-platform stewardship. A future publication or graduation change MUST
refresh external adoption evidence when current evidence is stale, incomplete,
or blocked before reaching GTK Lush API behavior.

#### Scenario: Internal maintenance avoids external checkout churn
- **WHEN** a GTK Lush internal maintenance change does not propose functional
  publication
- **THEN** it does not need to clone, patch, or retain an unrelated external
  project checkout
- **AND** existing bounded external-spike notes remain sufficient unless the
  changed API invalidates their conclusion

#### Scenario: Publication refreshes external evidence
- **WHEN** a future publication or graduation proposal starts
- **THEN** it reviews the external spike notes for staleness
- **AND** it either refreshes external evidence with bounded artifacts or
  records why existing evidence remains sufficient
