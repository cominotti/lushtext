## MODIFIED Requirements

### Requirement: The externally reachable test-seam count is ratcheted
`docs/workflow-readability-matrix.md` SHALL record the count of `pub fn` / `pub(crate) fn` declarations ending in `_for_test` under `crates/lushtext-ui/src`, and `make check-policy` SHALL fail only if the recomputed count exceeds it (`#[cfg(feature = "test-utils")]` sites remain outside the ratchet). The self-test SHALL verify `pub`, `pub(crate)`, and uncounted `pub(super)` visibilities.

#### Scenario: Growth beyond the recorded figure fails
- **WHEN** the recomputed count of externally reachable `*_for_test` declarations
  exceeds the figure recorded in the matrix
- **THEN** the check fails, reports both counts, and names the two remedies

#### Scenario: Reduction passes without forcing the figure down
- **WHEN** the recomputed count is below the recorded figure
- **THEN** the check passes with no finding
- **AND** the change is not required to lower the recorded figure

#### Scenario: Deliberate growth is recorded rather than hidden
- **WHEN** a change genuinely needs more externally reachable seams
- **THEN** it raises the recorded figure in the same change with a stated reason

#### Scenario: Gated attribute sites are outside the ratchet
- **WHEN** a migrated workflow adds a `test-utils`-gated evidence surface
- **THEN** the resulting rise in `cfg(feature = "test-utils")` sites produces no
  finding

#### Scenario: Narrowing the predicate fails the self-test
- **WHEN** the counting predicate is narrowed so that `pub(crate) fn *_for_test`
  declarations stop being counted
- **THEN** the self-test fails on its own fixture's asserted count
- **AND** a `pub(super) fn *_for_test` declaration in the same fixture is still
  not counted
