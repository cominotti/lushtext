## ADDED Requirements

### Requirement: Runner warning scans hold a bounded capture and an exact verdict
A runner that scans a command's output for warnings (the widget runner, the GTK axiom runner) SHALL classify lines with its existing `grep -E` and `grep -Ev` invocations and regexes, in a stage beside the live stream. The stage SHALL keep an exact count of unexpected lines and at most 64 KiB of them for the report. The verdict SHALL fail when the count is above zero, whatever the cap kept.

#### Scenario: A flood is counted exactly and reported within the cap
- **WHEN** a command prints 1,048,576 unexpected `Gtk-CRITICAL` lines
- **THEN** the run fails, the report states 1,048,576 unexpected lines, and the excerpt printed and held is at most 64 KiB

#### Scenario: A benign flood passes
- **WHEN** a command prints 1,048,576 lines that all match the benign family
- **THEN** the count is 0 and the scan passes

#### Scenario: Existing classification cases keep their verdicts
- **WHEN** the widget self-test runs its existing samples through the stage
- **THEN** mutter's sliced-pipeline line and the shared session-services samples pass, and `Gtk-WARNING **: allocation failed` and the AT-SPI registry critical fail

#### Scenario: The scan's memory does not grow with the flood
- **WHEN** a flood larger than its memory scope's cap runs through the widget runner inside that scope
- **THEN** the run reports the unexpected lines, the scope's `oom_kill` stays 0, and its peak stays below the cap

### Requirement: The scan stage drains its input and gates the verdict
The stage SHALL read its input to EOF whatever its caps hold, so the command never sees a broken pipe or a stall. The runner SHALL wait for the stage before it reads the verdict, and a missing stage summary SHALL fail the run. A stage that dies SHALL NOT stop the command or the live stream. The command under test SHALL inherit none of the stage's descriptors, so a process it leaves running does not hold the verdict.

#### Scenario: Output past the cap does not break the command
- **WHEN** a command writes many times the report cap and exits 0
- **THEN** it sees no EPIPE, and its exit status 0 reaches the verdict

#### Scenario: A slow stage is awaited
- **WHEN** the stage is still classifying after the command has exited
- **THEN** the runner waits for the stage's summary before deciding

#### Scenario: A stage that dies fails closed
- **WHEN** the stage is killed before it writes its summary
- **THEN** the command runs to its end, the live stream stays complete, and the run fails and says the scan was incomplete

#### Scenario: A lingering daemon does not hold the verdict
- **WHEN** the command leaves a `setsid` background process, its stdio detached from the run's output, running for 5 s
- **THEN** the runner returns its verdict in under 1 s

### Requirement: Scan captures leave nothing named on any exit path
Every file a scan stage writes SHALL be unlinked once it is open and read back through its descriptor. No exit of the runner (normal, Ctrl-C, SIGTERM, an OOM kill, or `kill -9`) SHALL leave a scan entry in any temporary directory, except that a `kill -9` while the stage opens its outputs MAY leave one `lt-scan-<pid>-` directory for the stale sweep. A path a runner still owns SHALL be registered in the shell that holds its trap, never inside a subshell.

#### Scenario: kill -9 of the runner mid-flood
- **WHEN** the widget runner is killed with `kill -9` while its command floods, with `TMPDIR` set to a private base
- **THEN** the base holds no new entry while the orphaned pipeline streams or after it ends, and each stage output stays within its cap meanwhile

#### Scenario: SIGTERM to the runner's subshell
- **WHEN** the subshell that runs the command is sent SIGTERM mid-run
- **THEN** no scan entry remains in the base

#### Scenario: A normal failing run
- **WHEN** a run ends with unexpected warnings
- **THEN** the report is printed and no scan entry remains

### Requirement: Runners keep only the lines they parse, and fail closed past the cap
A runner that needs lines after the run (the GTK axiom runner's result, slow-skip and observation lines; the mutation runner's no-mutants phrase) SHALL keep only those lines, at most 1 MiB, with an exact kept count, instead of a whole-output log. Its counts SHALL equal a full log's. When a runner that reads the kept lines finds them over the cap, the run SHALL fail with a message naming the cap.

#### Scenario: Sample results are counted from kept lines
- **WHEN** `make gtk-axioms` runs every sample `--check`
- **THEN** the result count, the slow-skip count and the runtimes summary match the samples run, and no whole-output log is written

#### Scenario: The mutation runner detects its phrase without a file
- **WHEN** cargo-mutants exits nonzero and prints the no-mutants phrase, with `TMPDIR` set to a private base
- **THEN** the lane passes as today, a stream without the phrase keeps its exit status, and the base holds no new entry

#### Scenario: Kept lines past the cap fail the run
- **WHEN** a command prints more than 1 MiB of lines matching the keep pattern
- **THEN** the run fails and says that the kept output exceeded 1 MiB

### Requirement: Named runner temp entries carry the owning PID and are reclaimed
A temporary entry a runner must name for a child (the `heavy.sh` admission report, the `with-memory-cap.sh` envelope pid file) or for the scan stage's open window SHALL live in a directory named with an exact prefix and the owning PID, which the temp scope's stale sweep reclaims. Self-test scratch SHALL live under one root that its trap removes, and any other temp file a lane script creates SHALL be removed by a trap.

#### Scenario: A SIGKILLed heavy.sh's report is reclaimed
- **WHEN** an `lt-heavy-<pid>-…` directory's PID is gone and it has been idle for over an hour
- **THEN** the next temp-scope sweep removes it, and likewise an `lt-envelope-` or `lt-scan-` directory

#### Scenario: Helper self-tests leave nothing
- **WHEN** `make check-agent-helpers` runs with `TMPDIR` set to a private base
- **THEN** the base is empty afterwards

#### Scenario: A live runner's entry is kept
- **WHEN** the sweep finds an `lt-envelope-<pid>-…` directory whose PID is alive
- **THEN** the sweep leaves it in place

#### Scenario: Plain mktemp names are never swept
- **WHEN** the sweep finds a `tmp.XXXXXXXXXX` entry, whatever its age
- **THEN** the sweep leaves it in place, because nothing proves it is LushText's
