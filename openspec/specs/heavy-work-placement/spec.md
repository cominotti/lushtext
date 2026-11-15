# heavy-work-placement Specification

## Purpose
Where capped heavy work runs on the shared workstation: `scripts/with-memory-cap.sh` starts its scopes in the `user-lushtext-heavy.slice` family, which systemd nests under the user manager's inner `user.slice` so the work inherits the host's toolbox CPU policy. No CPU property is set on our units, every scope checks its own placement before its command runs, and during the rename the legacy slice is counted but never changed.

## Requirements

### Requirement: Capped scopes nest under the user manager's user.slice
`scripts/with-memory-cap.sh` SHALL start every capped scope in `user-lushtext-heavy.slice`, which systemd nests as `user.slice/user-lushtext.slice/user-lushtext-heavy.slice` in the user manager, so the scope inherits the inner `user.slice`'s cpuset, CPU weight and CPU class. The wrapper SHALL set no CPU property on its scopes or slices, and SHALL keep the memory properties, caps, host-wide budget and reserve unchanged.

#### Scenario: Default placement
- **WHEN** an agent runs `scripts/heavy.sh -- cargo check` with a 12G cap
- **THEN** the command's `/proc/self/cgroup` path is `…/user@<uid>.service/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/<scope>`
- **AND** the scope has `MemoryMax=12G`, `MemorySwapMax=0` and `OOMPolicy=continue`, and the slice has the host-wide budget as its `MemoryMax`

#### Scenario: The scope follows the host's CPU set, not the caller's
- **WHEN** a caller pinned to an online CPU outside the inner `user.slice`'s `cpuset.cpus.effective` starts a capped command
- **THEN** the command's CPU affinity is that effective cpuset, as read from `/sys/fs/cgroup` at run time
- **AND** setting an affinity disjoint from that cpuset inside the scope fails

#### Scenario: A caller wider than the cpuset is narrowed to it
- **WHEN** a caller whose affinity includes every online CPU starts a capped command on a host where some online CPU lies outside the inner `user.slice`'s effective cpuset
- **THEN** the command's CPU affinity is a subset of that effective cpuset

#### Scenario: A caller pinned inside the cpuset keeps its mask
- **WHEN** a caller pinned to a subset of the inner `user.slice`'s effective cpuset starts a capped command
- **THEN** the command's CPU affinity is the caller's mask

#### Scenario: No CPU property is set
- **WHEN** a capped scope runs
- **THEN** `systemctl --user show` reports `CPUWeight=[not set]` and an empty `AllowedCPUs=` for the scope, `user-lushtext-heavy.slice` and `user-lushtext.slice`

#### Scenario: An old slice name is refused
- **WHEN** `LUSHTEXT_HEAVY_SLICE` names `lushtext-heavy-selftest.slice`
- **THEN** the wrapper exits 2 with a message naming the `user-lushtext-heavy*.slice` prefix, and starts no scope

### Requirement: A capped scope verifies its placement before its command runs
The scope's first process SHALL check that its cgroup path lies under the ControlGroups systemd reports for the inner `user.slice` and the heavy slice. When the inner `user.slice` has a configured cpuset, it SHALL also check that its CPU affinity is a subset of that slice's `cpuset.cpus.effective`, read from `/sys/fs/cgroup` at run time, never a literal. A failure SHALL exit 78 before the command runs, naming the expected and actual values.

#### Scenario: A correctly placed scope runs its command
- **WHEN** a capped scope starts in `user-lushtext-heavy.slice` and its affinity lies within the inner `user.slice`'s effective cpuset
- **THEN** the check passes and the command runs with its arguments unchanged, the same exit status and the same signals

#### Scenario: A process outside the heavy slice is refused
- **WHEN** the placement check for a heavy slice runs in a process whose cgroup lies outside that slice, such as the toolbox's `libpod-*.scope` (under `user.slice`) or a scope of another heavy slice
- **THEN** it exits 78 and the message names the heavy slice's ControlGroup and the actual path

#### Scenario: A CPU outside the inherited set is refused
- **WHEN** the check is given the affinity `0-23` and an inner `user.slice` with configured and effective cpuset `8-23`
- **THEN** it exits 78 and the message names both CPU lists

#### Scenario: No configured cpuset means only the path is checked
- **WHEN** the inner `user.slice`'s `cpuset.cpus` is empty or absent
- **THEN** a scope correctly placed in the heavy slice passes whatever its affinity

#### Scenario: A failed read refuses
- **WHEN** the check cannot read its own cgroup path or a ControlGroup from `systemctl --user show`
- **THEN** it exits 78 naming the read that failed, and the command does not run

#### Scenario: Nested and uncapped runs start no check
- **WHEN** a capped command runs a nested capped command, or a command runs with the cap `none`
- **THEN** no scope starts and no placement check runs

### Requirement: Admission counts legacy-slice scopes during the rename
While the legacy slice of the same family exists (`lushtext-heavy.slice` for `user-lushtext-heavy.slice`), admission SHALL count the scopes in both slices by the same per-scope rule. The admission lock file SHALL keep its name, so wrapper copies from before the rename serialize with current ones. The wrapper SHALL NOT stop, revert or resize a legacy slice.

#### Scenario: A legacy scope's cap is counted
- **WHEN** a 2G scope runs in the legacy slice of the self-test's private slice and a NOWAIT admission asks for 2G under a 3G budget
- **THEN** the admission exits 75 without starting a scope

#### Scenario: No legacy slice
- **WHEN** no legacy slice exists
- **THEN** admission counts the current slice alone and prints nothing about the legacy slice

#### Scenario: The legacy slice is left alone
- **WHEN** the wrapper admits and starts a scope while the legacy slice holds a running scope
- **THEN** the legacy slice's `MemoryMax`, drop-ins and scopes are unchanged

#### Scenario: Concurrent self-tests do not count each other
- **WHEN** `with-memory-cap.sh --self-test` runs while another worktree's self-test, of either the current or the pre-rename wrapper, holds scopes in its own self-test slice
- **THEN** no admission case of this run counts those scopes, because this run uses a per-run self-test slice and per-run legacy slice
