## MODIFIED Requirements

### Requirement: Developer Mutation Commands
The project SHALL expose local developer commands for smoke, changed-code, and configured full-scope mutation testing. The local commands SHALL run in disposable in-place mirror slots, as described by the `build-disk-footprint` capability, and SHALL keep parallel throughput by sharding across slots.

#### Scenario: Developer runs a smoke check
- **WHEN** a developer runs the mutation smoke command
- **THEN** the command MUST exercise a small bounded mutation target and verify that cargo-mutants, nextest, configuration, timeouts, and output handling work together

#### Scenario: Developer runs a local in-place mutation command
- **WHEN** a local mutation command uses in-place source mutation
- **THEN** the command MUST either require a clean worktree before starting or use a disposable checkout so user changes are not overwritten or mixed with mutant edits

#### Scenario: Developer mutation output stays in the checkout
- **WHEN** a local mutation command finishes, on one slot or several
- **THEN** its report is in the checkout's `mutants.out`, with the previous report rotated to `mutants.out.old`

## ADDED Requirements

### Requirement: Local mutation test processes run under a per-process address-space limit
Local mutation runs and the proof-strength tests arm SHALL run each test process under an address-space limit, applied through a Cargo target runner. The limit SHALL NOT apply to the compiler, the linker, build scripts, nextest or cargo-mutants. The unmutated baseline SHALL pass under the limit before any mutant is tested.

#### Scenario: The default limit is derived from the baseline
- **WHEN** the default limit is set or revised
- **THEN** it is twice the smallest power-of-two GiB at which the unmutated baseline of the run's scope passes, at least 4 GiB, and recorded with that measurement

#### Scenario: The baseline proves the limit is safe
- **WHEN** the unmutated baseline cannot pass under the configured limit
- **THEN** the run fails before testing any mutant and names the limit

#### Scenario: The compiler is not limited
- **WHEN** a local mutation run compiles the workspace
- **THEN** rustc and the linker run without the per-test address-space limit

### Requirement: Mutants caught by the address-space limit are reported for triage
A mutant whose test log shows an allocation failure SHALL be listed in `mutants.out/address-space-caught.txt` and counted in the run summary, so that triage can confirm each one. A run containing such mutants SHALL NOT be discarded.

#### Scenario: A runaway mutant is stopped alone
- **WHEN** a mutant makes a test allocate past the limit
- **THEN** only that test process fails, the run continues with the next mutant, and the mutant is listed in `address-space-caught.txt`
