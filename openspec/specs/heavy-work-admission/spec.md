# heavy-work-admission Specification

## Purpose
How heavy work is admitted to the shared workstation's memory budget without stalling a measurement: `scripts/heavy.sh` admits once after taking `LOCK` and releases it on a misfit, commands that admit their own scopes run in an envelope sized for them, no other admission waits under a shared `LOCK`, measurement admissions fail fast, and waiting admissions are served in arrival order so a large request is not starved.

## Requirements

### Requirement: heavy.sh never waits for memory while holding LOCK
`scripts/heavy.sh` SHALL perform its memory admission after taking `LOCK` shared as one non-blocking check, waiting at most 2 s for the admission lock. A misfit SHALL start no scope and no command: it releases `LOCK` and waits for memory holding no lock of the protocol, then retries. The command's own exit status, including 75, SHALL reach the caller unchanged. Its compiler parallelism SHALL be read after `LOCK` is taken, on every attempt.

#### Scenario: Memory taken between the probe and the admission
- **WHEN** the memory probe sees room for the cap, and another admission takes it before `heavy.sh`'s own admission after `LOCK`
- **THEN** `heavy.sh` releases `LOCK` without starting the command
- **AND** while it waits for memory, an exclusive lock on `LOCK`, as a measurement takes it, can be acquired

#### Scenario: The command's own exit 75
- **WHEN** the admission fits and the command itself exits 75
- **THEN** `heavy.sh` exits 75 once, without retrying the command

#### Scenario: The command's own exit 75 in an envelope
- **WHEN** a command under `heavy.sh --envelope 2G` exits 75 and the envelope's report line has been written
- **THEN** `heavy.sh` exits 75 once, without retrying the command

#### Scenario: A busy admission lock
- **WHEN** the admission lock is held by another process for longer than 2 s while the check-once admission that `heavy.sh` makes after `LOCK` runs
- **THEN** that admission exits 75 within about 3 s with a `misfit: ` line in the report, and starts no scope and no command
- **AND** `heavy.sh` then releases `LOCK`

#### Scenario: A caller that asked never to wait
- **WHEN** `heavy.sh` runs with `LUSHTEXT_ADMISSION_NOWAIT=1` and its admission after `LOCK` does not fit
- **THEN** it exits 75 without retrying and without starting the command

#### Scenario: Parallelism after each LOCK
- **WHEN** `heavy.sh` retries after a misfit
- **THEN** the number of shared `LOCK` holders that sets `CARGO_BUILD_JOBS` and the front-end threads is read after `LOCK` is taken again

### Requirement: Commands that admit their own scopes run in an envelope
`heavy.sh --envelope <size>` SHALL admit `<size>` like a scope of that cap, as a reservation that the host-wide count of the current and earlier wrapper copies sees. The command SHALL then run in a sub-slice whose `MemoryMax` is `<size>`, with no swap. Capped calls inside it SHALL get their own scopes there, admitted against `<size>` without the host reserve check, and waiting only for scopes of the same envelope.

#### Scenario: An inner admission does not wait for other agents
- **WHEN** a command runs under `heavy.sh --envelope 2G` and starts a 2G capped call while the host slice is otherwise full
- **THEN** the inner call is admitted at once, in its own scope inside the envelope

#### Scenario: An inner admission ignores the host queue
- **WHEN** an older 4G request holds the heavy slice's oldest ticket and has the host memory, and a capped 2G call starts inside a 2G envelope
- **THEN** the inner call is admitted at once

#### Scenario: A nested envelope runs in place
- **WHEN** `with-memory-cap.sh --envelope 2G` starts inside a capped scope or inside an envelope
- **THEN** it starts no placeholder and no sub-slice, and its command runs in place

#### Scenario: A nested envelope under a shared LOCK is refused
- **WHEN** `heavy.sh --mem none -- heavy.sh --envelope 2G -- true` runs
- **THEN** the inner `heavy.sh` exits 2 with a message naming `--envelope`, and no scope is admitted

#### Scenario: The envelope is counted on the host
- **WHEN** a 2G envelope is running in a slice whose budget is 3G
- **THEN** a 2G admission with `LUSHTEXT_ADMISSION_NOWAIT=1` exits 75
- **AND** the reservation is a `lushtext-cap-<pid>-<ns>.scope` in the heavy slice whose `memory.max` is 2G

#### Scenario: Inner scopes beyond the envelope queue inside it
- **WHEN** two 2G capped calls start at once inside a 2G envelope
- **THEN** the second starts after the first has exited, and both succeed

#### Scenario: An inner cap larger than the envelope
- **WHEN** a capped call inside a 2G envelope asks for 4G
- **THEN** it exits 2 at once, naming the envelope's budget

#### Scenario: Each shard keeps its own scope
- **WHEN** `make test` runs under `heavy.sh --envelope 16G`
- **THEN** each widget shard runs in its own capped scope inside the envelope and reports its own peak and OOM kills

#### Scenario: Inner calls keep the envelope's limit
- **WHEN** a capped call is admitted inside a 2G envelope
- **THEN** the envelope sub-slice's `memory.max` is still 2G afterwards

#### Scenario: An OOM kill in the envelope fails the run
- **WHEN** the kernel OOM-kills any process inside an envelope, the root command's own processes included
- **THEN** `heavy.sh` prints an `OOM-KILL:` line naming the envelope and exits non-zero, even when the command exited 0

#### Scenario: Parallelism follows the envelope size
- **WHEN** `heavy.sh --envelope <size>` computes the compiler parallelism
- **THEN** it uses `<size>` as the cap, so an envelope below 12G gets 4 jobs and 1 front-end thread

### Requirement: An envelope keeps no admission after its command
An envelope's reservation SHALL end when its root command exits, even when a daemon the command started is still in the envelope. The sub-slice's runtime limits SHALL be reverted when the root command exits.

#### Scenario: A leftover daemon in an envelope
- **WHEN** a command under a 2G envelope starts a detached daemon and exits, and the envelope's report line has been written
- **THEN** a 2G NOWAIT admission into a 3G slice then succeeds
- **AND** the envelope's sub-slice no longer has its `MemoryMax` drop-in

#### Scenario: A killed placeholder is swept
- **WHEN** an envelope's placeholder was SIGKILLed, and a later envelope is admitted into the same heavy slice
- **THEN** the dead envelope's sub-slice drop-in and queue directory are removed

### Requirement: No other admission waits under a shared LOCK
When a capped call's process tree provably holds `LOCK` shared (the marker's PID is an ancestor and its fd 7 is `LOCK`), the call SHALL be refused with exit 2 before any admission, with a message naming `heavy.sh --envelope`, unless it is `heavy.sh`'s own admission after `LOCK` or runs inside an envelope. `--preflight <cap>` SHALL apply this check and admit nothing. Calls nested in place, caps of 1G or less and `none` SHALL be unaffected.

#### Scenario: --mem none with an inner admission
- **WHEN** `heavy.sh --mem none -- scripts/with-memory-cap.sh 2G true` runs
- **THEN** the inner call exits 2 at once with a message naming `--envelope`, and no scope is admitted
- **AND** `LOCK` is not held while anything waits for memory

#### Scenario: A stale marker proves nothing
- **WHEN** a capped call carries a `LUSHTEXT_AGENT_LOCK_HELD` marker whose PID is not an ancestor holding `LOCK`
- **THEN** it is admitted as an ordinary call, which may wait because it holds no lock

#### Scenario: Commands that admit nothing
- **WHEN** `heavy.sh --mem none -- git commit` runs the pre-commit hook
- **THEN** the hook's self-tests pass, because they shed the enclosing helper's marker

#### Scenario: The preflight refuses before the first phase
- **WHEN** `heavy.sh --mem none -- make mutants-smoke` runs
- **THEN** it exits 2 with the `--envelope` message before any mutation slot is taken

#### Scenario: A lane run that admits nothing is not refused
- **WHEN** `heavy.sh --mem none -- make test-widget WIDGET_PARALLEL=0` runs
- **THEN** no preflight refuses it, because the serial run admits no scope

### Requirement: Measurement admissions keep failing fast
An admission inside a measurement (`LUSHTEXT_ADMISSION_NOWAIT=1`, or a proven exclusive `LOCK`) outside an envelope SHALL check once and exit 75 on a misfit without starting a scope. It SHALL take no place in the admission order and SHALL not be held back by older waiters.

#### Scenario: NOWAIT misfit
- **WHEN** a 2G NOWAIT admission does not fit a 2G slice with an impossible reserve
- **THEN** it exits 75 within seconds and starts no scope

#### Scenario: A measurement ignores the queue
- **WHEN** an older 4G request is waiting and a 2G NOWAIT admission fits the slice budget on its own
- **THEN** the NOWAIT admission is admitted

#### Scenario: heavy.sh's check-once admission under NOWAIT ignores the queue
- **WHEN** an older 4G request is waiting and `heavy.sh`'s check-once admission after `LOCK` runs with `LUSHTEXT_ADMISSION_NOWAIT=1` for a 2G cap that fits the slice budget on its own
- **THEN** it is admitted

### Requirement: Admission order does not starve a large request
Waiting admissions SHALL be ordered by arrival. While the oldest live waiter has the host memory for its cap plus the reserve, a later request SHALL be admitted only if it also fits the slice budget beside that waiter's cap. A waiter that is short of host memory SHALL reserve nothing. A waiter SHALL keep its place across `heavy.sh`'s retries. Dead waiters SHALL not count. The probe and the admission SHALL use the same decision.

#### Scenario: A small request behind a large one
- **WHEN** a 3G scope runs in a 6G slice, a 4G request is waiting, and a 2G request arrives
- **THEN** the 2G request waits
- **AND** the 4G request is admitted once the 3G scope ends

#### Scenario: A small request that fits beside the waiter
- **WHEN** a 4G request is waiting in a 10G slice with 3G admitted, and a 2G request arrives
- **THEN** the 2G request is admitted at once

#### Scenario: A waiter short of host memory reserves nothing
- **WHEN** the oldest waiter's cap plus the reserve exceeds the host's available memory, and a later 2G request fits on its own
- **THEN** the 2G request is admitted

#### Scenario: A dead waiter
- **WHEN** the process that owned the oldest waiting ticket has exited
- **THEN** its ticket neither reserves budget nor blocks later requests

#### Scenario: A run in place takes no place in the order
- **WHEN** `heavy.sh --mem 2G` runs inside a capped scope, inside an envelope, or with no user manager, so its command runs in place without a host admission
- **THEN** it holds no ticket in the heavy slice's admission order while its command runs
