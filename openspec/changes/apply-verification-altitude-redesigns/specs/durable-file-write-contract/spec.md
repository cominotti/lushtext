## ADDED Requirements

### Requirement: Proven-protocol shells run through one shared driver
Every shell of an I/O-free, Kani-checked protocol core (durable write, move,
rename, Replace All file apply, Undo entry, journal shrink) SHALL run it through one shared,
I/O-free driver loop, unless its module documentation records why it cannot.
The driver SHALL stop at the core's terminal action, execute every other
action with exactly one executor call, and feed that outcome back unchanged.

#### Scenario: One execution site per action
- **WHEN** any durable write, move, rename, Replace All file, Undo entry, or journal shrink runs through a shell that uses the shared driver
- **THEN** each non-terminal action of its core is executed by exactly one executor call inside the shared driver, and the core receives that call's outcome unchanged

#### Scenario: A nested protocol run is a nested drive
- **WHEN** a durable move executes its copy action
- **THEN** the copy's write protocol runs as a nested call of the same driver, completing before the move's outcome for that action is fed back

### Requirement: Proven-protocol shells hold no loop of their own
A shell of a proven protocol core SHALL hold only its own resources, such as
the temp file, writer, guard, and last error, and the mapping from the core's
terminal action to its result. It SHALL NOT loop over the core's actions
itself: the core's step is called only from the shared driver. The one
exception is a shell whose module documentation records why it cannot use the
shared driver.

#### Scenario: A shell stepping its core outside the driver is caught
- **WHEN** a proven-protocol shell calls its core's step anywhere other than through the shared driver, and its module documentation records no exception
- **THEN** the source check over the proven-protocol shells fails and names the shell

#### Scenario: Results follow the core's terminal action
- **WHEN** a proven-protocol shell returns
- **THEN** its result is the mapping of the terminal action the shared driver stopped at, carrying the last error its executor recorded

### Requirement: The app-data layout has one owner
Every app-data file and directory SHALL be named once, in one GTK-free layout
owner. Each entry SHALL record its relative path, its kind (file, directory,
or nested family), and how the startup leftover sweep covers the durable-write
temps that land there. Services SHALL resolve their app-data paths through
the owner. Changing the owner MUST NOT
change any existing location's resolved path, unless a format-upgrade
converter migrates it.

#### Scenario: Existing paths are unchanged
- **WHEN** the layout owner resolves every existing location for a given data home
- **THEN** each resolved path is byte-identical to the path the pre-owner services resolved

### Requirement: The leftover sweep and the upgrade inventory take their coverage from the layout owner
The startup leftover sweep and the format-upgrade inventory SHALL derive the
locations they visit from the layout owner's entries, through exhaustive
matches. The inventory SHALL decide, for every entry, whether it scans it,
whether another entry's scan covers it, or why it is not inventoried. Adding
an app-data location then makes it swept and inventoried, or explicitly
exempt, without editing either consumer's list.

#### Scenario: A new durable-write directory is swept without editing the sweep
- **WHEN** a contributor adds a layout entry for a new app-data directory that holds durable writes
- **THEN** the startup leftover sweep visits it under the same provably-ours, stale-only, bounded rules
- **AND** no hand-written directory list outside the layout owner needed to change

#### Scenario: A location outside the upgrade inventory is explicit
- **WHEN** a layout entry is neither scanned by the format-upgrade inventory nor covered by another entry's scan
- **THEN** the inventory's decision for that entry records why, and the build or a unit test fails if an entry is neither scanned, covered, nor exempted

### Requirement: The startup sweep spends its budget in a fixed order
The startup leftover sweep SHALL visit its flat directories, then the
format-upgrade runs, then the local-history lineages, and only then the
legacy folder-note sidecar directory, in one tested order. The directories
swept before the layout owner SHALL keep their order, because the sweep
spends one bounded budget in visit order.

#### Scenario: The sweep spends its budget in a fixed order
- **WHEN** the startup sweep runs over a data home whose directories and families together exceed its budget
- **THEN** it visits the flat directories, then the format-upgrade runs, then the local-history lineages, in the same order as before the layout owner, so the same leftovers are removed from them
- **AND** the legacy folder-note sidecar directory is visited last, with what budget remains

#### Scenario: A stale leftover in the legacy folder-note directory is swept
- **WHEN** startup finds a stale provably-ours temp file in the legacy `workspace-notes` directory, left by a format-upgrade rewrite
- **THEN** it is removed under the same rules as the other app-data directories, if the pass's budget reaches it
