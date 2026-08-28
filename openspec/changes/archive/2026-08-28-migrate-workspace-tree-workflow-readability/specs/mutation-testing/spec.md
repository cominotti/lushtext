## MODIFIED Requirements

### Requirement: Mutation Triage Policy
The project SHALL document how to triage missed, unviable, timeout, and excluded mutants.

A focused mutation run SHALL report the mutant classes its filter cannot exclude.
The mutation tool's name filter does not apply to every mutant kind — struct
field-deletion mutants are generated for the whole configured scope regardless of
the filter — so a run described as focused also runs that unfilterable floor. A
change reporting focused results SHALL state the floor explicitly, so the result
is not read as narrower than it was. Pre-existing survivors inside that floor
MUST NOT be attributed to the change under review. They MUST also NOT be
inherited indefinitely: the change that owns the file where they survive SHALL
triage them in the order this requirement already states — decide whether each
represents real missed behavior, then add or tighten deterministic tests, then
consider a small refactor that makes the behavior testable, and only then a
narrow documented exclusion.

#### Scenario: Missed mutant is actionable
- **WHEN** a mutant survives in code covered by the configured mutation scope
- **THEN** maintainers MUST prefer adding or tightening tests, strengthening assertions, or extracting deterministic logic before adding an exclusion

#### Scenario: Mutant is equivalent or outside useful scope
- **WHEN** maintainers determine that a mutant is equivalent, generated, UI-glue-only, or otherwise not useful to test
- **THEN** maintainers MUST exclude it only with a narrow documented exclusion

#### Scenario: Focused run reports the floor its filter cannot exclude

- **WHEN** a change reports mutation results from a filtered or focused run
- **THEN** it states the mutant classes the filter does not apply to and the count
  they contribute
- **AND** the focused result is not presented as if the filter had bounded the
  whole run

#### Scenario: Pre-existing survivors in the floor are not attributed to the change

- **WHEN** a focused run surfaces surviving mutants that pre-date the change under
  review
- **THEN** those survivors are reported as baseline rather than as regressions
  introduced by the change
- **AND** the change that owns the file where they survive triages them in the
  documented order rather than passing them on again

### Requirement: Policy relocation requires mutation parity evidence
When pure policy relocates between directories, the change SHALL demonstrate that
mutation coverage is unchanged. The evidence MUST show that the relocated logic
still generates mutants and that those mutants are still killed. A relocation whose
mutants are no longer generated MUST be treated as a coverage regression, not as an
acceptable consequence of the move.

Where one change both relocates existing pure policy and extracts new pure policy
out of a GTK adapter, the two results SHALL be reported **separately**. A
relocation has a before-count, so parity is a real claim that can fail; an
extraction out of an adapter has no before-count, so its result is a gain from
zero that cannot fail. Merging them into one aggregate figure lets a parity loss
disappear behind a gain, which this requirement exists to prevent. Each reported
figure SHALL name the exact command invocation and the file-level anchors it was
measured against.

#### Scenario: Relocation reports parity
- **WHEN** a change relocates pure policy
- **THEN** it records mutation results for the relocated logic before and after the
  move
- **AND** the counts of generated and killed mutants for that logic are unchanged

#### Scenario: Lost mutants block the relocation
- **WHEN** relocation causes the policy's mutants to fall outside the default scope
- **THEN** the change is incomplete until the scope convention or the module
  placement is corrected
- **AND** the loss MUST NOT be recorded as accepted debt

#### Scenario: Policy module outside scope reach fails policy checks
- **WHEN** a pure policy module exists at a path the default mutation scope cannot
  reach
- **THEN** `make check-policy` fails
- **AND** the failure names the unreachable module

#### Scenario: Parity and gain are reported separately

- **WHEN** one change both relocates existing pure policy and extracts new pure
  policy out of a GTK adapter
- **THEN** it reports the relocation's before/after parity and the extraction's
  gain from zero as separate figures, each naming its invocation and file-level
  anchors
- **AND** neither figure is merged into a single aggregate count in which a parity
  loss could be masked by a gain
