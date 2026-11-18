## ADDED Requirements

### Requirement: Proven-protocol shells run through one shared driver
Every shell that executes an I/O-free, Kani-checked protocol core SHALL run
that core through one shared, I/O-free driver loop. This covers the durable
write, move, and rename, the Replace All file apply, and the Undo entry. The
driver SHALL stop at the core's terminal action, SHALL execute every other
action with exactly one executor call, and SHALL feed that call's outcome back
to the core unchanged.

#### Scenario: One execution site per action
- **WHEN** any durable write, move, rename, Replace All file, or Undo entry runs
- **THEN** each non-terminal action of its core is executed by exactly one executor call inside the shared driver, and the core receives that call's outcome unchanged

#### Scenario: A nested protocol run is a nested drive
- **WHEN** a durable move executes its copy action
- **THEN** the copy's write protocol runs as a nested call of the same driver, completing before the move's outcome for that action is fed back

### Requirement: Adopting the shared driver changes no core and no behaviour
A shell that uses the shared driver SHALL keep only two things: its own
resources, such as the temp file, writer, guard, and last error, and the
mapping from the core's terminal action to its result. It SHALL keep no loop
of its own, unless the adopting change records why. Adopting the driver MUST
NOT change the Kani-checked cores, their harnesses, or the sequence of actions
any shell executes.

#### Scenario: Behaviour is unchanged by the driver
- **WHEN** the durable-write, Replace All, and Undo fault-injection and crash tests run against the shells after they adopt the driver
- **THEN** every result classification, leftover, kill-point window, and on-disk state is the same as before

### Requirement: The app-data layout has one owner
Every app-data file and directory SHALL be named once, in one GTK-free layout
owner. Each entry SHALL record its relative path, its kind (file, directory,
or nested family), whether durable writes create temp files there, and
whether the format-upgrade inventory scans it, or why not. Services SHALL
resolve their app-data paths through the owner. Changing the owner MUST NOT
change any existing location's resolved path, unless a format-upgrade
converter migrates it.

#### Scenario: Existing paths are unchanged
- **WHEN** the layout owner resolves every existing location for a given data home
- **THEN** each resolved path is byte-identical to the path the pre-owner services resolved

### Requirement: The leftover sweep and the upgrade inventory take their coverage from the layout owner
The startup leftover sweep and the format-upgrade inventory SHALL derive the
locations they visit from the layout owner's entries, through exhaustive
matches. Adding an app-data location then makes it swept and inventoried, or
explicitly exempt, without editing either consumer's list.

#### Scenario: A new durable-write directory is swept without editing the sweep
- **WHEN** a contributor adds a layout entry for a new app-data directory that holds durable writes
- **THEN** the startup leftover sweep visits it under the same provably-ours, stale-only, bounded rules
- **AND** no hand-written directory list outside the layout owner needed to change

#### Scenario: A location outside the upgrade inventory is explicit
- **WHEN** a layout entry is not scanned by the format-upgrade inventory
- **THEN** the entry records why, and the build or a unit test fails if an entry is neither scanned nor exempted
