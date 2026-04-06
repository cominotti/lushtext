# Failure Pattern Catalog

Comprehensive catalog of failure patterns seen in the invowk project. Each entry
includes symptom, root cause, fix template, prevention measure, and platform skill
cross-reference.

Use this catalog during Phase 2 (Diagnosis) of the fixer workflow to match observed
symptoms against known patterns. If a match is found, apply the documented fix
rather than investigating from scratch.

---

## Race Conditions

### RC-1: lipgloss sync.Once terminal detection on Windows

**Symptom**: Race detector fires on `lipgloss` internal state during parallel TUI
tests on `windows-latest`. Goroutine 1 writes terminal capabilities via `sync.Once`;
goroutine 2 reads the cached result before initialization completes.

**Root cause**: lipgloss lazily detects terminal capabilities on first render.
When multiple parallel tests render simultaneously, the detection races. The
`sync.Once` protects the write, but the read path in some lipgloss versions
accesses the result outside the `Once`.

**Fix template**: Add a `TestMain` that pre-warms lipgloss before any tests run.
See `internal/tui/testmain_windows_test.go` for the pattern:

```go
func TestMain(m *testing.M) {
    // Pre-warm lipgloss terminal detection to avoid sync.Once race
    // in parallel tests.
    _ = lipgloss.NewStyle().Render("warmup")
    os.Exit(m.Run())
}
```

**Prevention**: Any package that uses lipgloss in parallel tests on Windows must
have a `TestMain` with the pre-warm pattern.

**Platform skill**: `windows-testing` SKILL.md (sync.Once race section),
`go-testing` SKILL.md (Platform Skill Router table).

---

### RC-2: CUE cue.Value / cue.Context concurrent access

**Symptom**: Race detector fires on CUE internal state. Typically during
`BehavioralSync` tests or any test that calls `cue.Value.Unify()`,
`cue.Context.CompileString()`, or iterates CUE fields in parallel.

**Root cause**: `cue.Value` and `*cue.Context` are NOT safe for concurrent use.
`Unify()` and `CompileString()` mutate internal state. Sharing a CUE context
across parallel subtests causes data races.

**Fix template**: Run CUE-touching subtests serially. Add `//nolint:tparallel`
with a justification comment:

```go
//nolint:tparallel // CUE cue.Value is not safe for concurrent use
func TestBehavioralSync_SomeField(t *testing.T) {
    t.Parallel() // parent can be parallel
    // subtests must NOT call t.Parallel()
    for name, tc := range cases {
        t.Run(name, func(t *testing.T) {
            // no t.Parallel() here — serial execution required
        })
    }
}
```

**Prevention**: Never share `cue.Value` or `*cue.Context` across goroutines.
Create a new context per subtest if parallelism is needed.

**Platform skill**: `go-testing` SKILL.md (Parallelism Decision Framework).

---

### RC-3: SSH host key file collision

**Symptom**: Race detector fires or tests produce `permission denied` / `file exists`
errors when multiple `sshServerController` tests run in parallel. The `wish` library
writes host keys to `.ssh/` in the working directory.

**Root cause**: Parallel SSH server tests share the same working directory. The
`wish` library writes `id_ed25519` and `id_ed25519.pub` to `.ssh/` relative to the
current directory. Concurrent writes to the same files cause races.

**Fix template**: Run SSH server tests sequentially. Do not call `t.Parallel()` on
the parent test or any subtest in `sshServerController` test groups. Unique temp
directories per test also work but require plumbing the path to the wish config.

**Prevention**: `.gitignore` entries for `internal/app/commandsvc/id_ed25519{,.pub}`.
Sequential execution for all `sshServerController` tests.

**Platform skill**: `go-testing` SKILL.md (Parallelism Decision Framework).

---

### RC-4: os.Stdin replacement in parallel tests

**Symptom**: Race detector fires on `os.Stdin` access. One goroutine replaces
`os.Stdin` for a TUI test; another goroutine reads the original `os.Stdin`.

**Root cause**: `os.Stdin` is a package-level global. Replacing it in one test
while another test reads it is a data race. This affects tests in `internal/tui/`
and `cmd/invowk/` that simulate piped input.

**Fix template**: Do not call `t.Parallel()` on tests that replace `os.Stdin`.
Use the `isStdinPiped()` and `readStdinAll()` helpers from `cmd/invowk/tui_stdin.go`
instead of direct `os.Stdin` manipulation where possible.

**Prevention**: Tests that touch `os.Stdin` must omit `t.Parallel()`. Document the
reason in a comment: `// no t.Parallel(): replaces os.Stdin`.

**Platform skill**: `go-testing` SKILL.md (Parallelism Decision Framework, shared
mutable state section).

---

### RC-5: Shared MockCommandRecorder across subtests

**Symptom**: Race detector fires on `MockCommandRecorder` fields. Multiple parallel
subtests write to the same mock recorder simultaneously.

**Root cause**: A single `MockCommandRecorder` instance is created in the parent test
and shared across parallel subtests. The recorder's `Record()` method writes to a
shared slice without synchronization.

**Fix template**: Create a fresh mock recorder per subtest:

```go
for name, tc := range cases {
    t.Run(name, func(t *testing.T) {
        t.Parallel()
        recorder := &MockCommandRecorder{} // per-subtest instance
        // ... use recorder ...
    })
}
```

**Prevention**: Never share mock recorders across parallel subtests. Each subtest
must have its own instance.

**Platform skill**: `go-testing` SKILL.md (Parallelism Decision Framework).

**Codebase locations**: `internal/container/engine_mock_test.go`,
`internal/container/engine_docker_mock_test.go`,
`internal/container/engine_podman_mock_test.go`.

---

## Timing / Flakiness

### TF-1: Watcher missing events on macOS (kqueue coalescing)

**Symptom**: Watcher tests fail on `macos-15` with missed file events. A file is
created or modified but the watcher callback is never invoked within the deadline.

**Root cause**: macOS uses `kqueue` for filesystem watching. kqueue coalesces rapid
events more aggressively than Linux's `inotify`. Multiple rapid file changes may
produce a single event, or the event may be delayed by macOS timer coalescing.

**Fix template**: Increase debounce intervals and use poll loops with generous
deadlines instead of expecting immediate events:

```go
// Bad: tight timing
time.Sleep(50 * time.Millisecond)
assert(eventReceived)

// Good: poll with deadline
deadline := time.After(2 * time.Second)
for {
    select {
    case <-eventCh:
        return // success
    case <-deadline:
        t.Fatal("timed out waiting for watcher event")
    }
}
```

**Prevention**: All watcher tests must use event-based synchronization (channels,
poll loops) instead of `time.Sleep`. Minimum poll deadline: 2 seconds for macOS.

**Platform skill**: `macos-testing` SKILL.md (kqueue coalescing section),
`macos-testing/references/filesystem-kqueue.md`.

---

### TF-2: tmux TUI test timing

**Symptom**: tmux-based TUI tests fail intermittently with assertion mismatches.
The terminal output does not contain the expected text when the assertion runs.

**Root cause**: tmux pane capture happens before the TUI has finished rendering.
The `time.Sleep` between sending input and capturing output is too short, or the
TUI render cycle takes longer than expected under CI load.

**Fix template**: Use poll loops with deadline instead of fixed sleeps. Query tmux
pane content repeatedly until the expected text appears or the deadline expires.

**Prevention**: All tmux test assertions must use polling. Document the expected
render time in a comment for calibration.

**Platform skill**: `macos-testing` SKILL.md (Timer Coalescing section),
`tmux-testing` SKILL.md.

---

### TF-3: Windows timer resolution (15.6ms)

**Symptom**: Timer-based tests flake on `windows-latest`. A test expects an operation
to complete within a tight deadline, but Windows timer ticks only every 15.6ms by
default. A 10ms sleep actually sleeps for 15.6ms (or 31.2ms under load).

**Root cause**: Windows uses a 15.625ms default timer interrupt period. `time.Sleep`,
`time.After`, and `time.Tick` have minimum resolution of one timer tick. Under CI
load, timer granularity can effectively double.

**Fix template**: Use generous timeouts on Windows (at least 100ms for sub-second
operations). Avoid assertions that depend on sub-15ms timing precision.

**Prevention**: Never assert exact timing on Windows. Use `>=` assertions with wide
margins. For cross-platform tests, use the largest timeout needed by any platform.

**Platform skill**: `windows-testing` SKILL.md (Timer Resolution section).

---

### TF-4: macOS timer coalescing

**Symptom**: A `time.Sleep(100 * time.Millisecond)` takes 150-200ms on macOS.
Tests that depend on the sleep completing within a tight window fail intermittently.

**Root cause**: macOS power management coalesces timers to reduce wake-ups.
Short sleeps may be delayed to align with other scheduled timer events. This is
most pronounced on battery power (laptops) but also affects CI runners.

**Fix template**: Use event-based synchronization instead of sleep-based timing.
When sleep is unavoidable, allow 2x the expected duration on macOS.

**Prevention**: Replace `time.Sleep` with channel-based synchronization wherever
possible. Document macOS timer coalescing as the reason when wider timeouts are
needed.

**Platform skill**: `macos-testing` SKILL.md (Timer Coalescing section),
`macos-testing/references/process-signals.md`.

---

### TF-5: Container test slow start

**Symptom**: Container integration tests fail on first run or under CI load with
`connection refused` or `context deadline exceeded` during container startup.

**Root cause**: The container engine (Docker or Podman) takes variable time to start
a container. Under CI load with limited resources, startup can take several seconds.
Without an engine health probe, tests may attempt to interact with a container before
it is ready.

**Fix template**: Use the engine health probe and pre-pull patterns from CI:

```go
// Use testutil.ContainerTestContext for bounded context
ctx := testutil.ContainerTestContext(t, testutil.DefaultContainerTestTimeout)

// CI pre-pulls base images: .github/workflows/ci.yml line 139
// ${{ matrix.engine }} pull debian:stable-slim
```

**Prevention**: All container tests must use `testutil.ContainerTestContext()` with
an explicit timeout, never bare `t.Context()`. CI pre-pulls `debian:stable-slim` to
remove network latency from the critical path.

**Platform skill**: `linux-testing` SKILL.md (Container Test Infrastructure section),
`linux-testing/references/container-testing-deep.md`.

---

## Platform-Specific

### PS-1: Windows CRLF in regex assertions

**Symptom**: Testscript `stdout` or `stderr` regex assertions fail on Windows even
though the output looks correct. The regex matches on Linux/macOS but not Windows.

**Root cause**: PowerShell's `Write-Output` and many Windows utilities emit `\r\n`
(CRLF) line endings. Go's `(?m)$` in regex matches before `\n`, not before `\r`.
The `\r` is treated as part of the line content.

**Fix template**: Use `\r?$` in anchored testscript assertions:

```
# Bad: fails on Windows
stdout '^done$'

# Good: handles CRLF
stdout '^done\r?$'
```

**Prevention**: All testscript assertions using `$` anchors must use `\r?$` for
cross-platform compatibility, or guard with `[!windows]`/`[windows]` conditionals.

**Platform skill**: `windows-testing` SKILL.md,
`windows-testing/references/powershell-testing.md`.

---

### PS-2: Windows TerminateProcess exit code masking

**Symptom**: A test expects a specific non-zero exit code from a cancelled context,
but gets exit code 1 on Windows. The error message does not mention context cancellation.

**Root cause**: `exec.CommandContext` on Windows calls `TerminateProcess`, which sets
exit code 1. This is indistinguishable from a normal failure. The `ctx.Err()` is
silently dropped because the exit code looks like a valid non-context error.

**Fix template**: Use `promoteContextError()` from `internal/runtime/native_helpers.go`
to check for context errors after `extractExitCode`:

```go
// In native runtime execution path:
result := extractExitCode(err)
if result.Error == nil {
    result = promoteContextError(ctx, result)
}
```

**Prevention**: Always check `ctx.Err()` after process exit on Windows when the
context may have been cancelled.

**Platform skill**: `windows-testing` SKILL.md (TerminateProcess section),
`windows-testing/references/process-lifecycle.md`.

**Codebase locations**: `internal/runtime/native_helpers.go`, `internal/runtime/native.go`.

---

### PS-3: macOS /tmp to /private/tmp symlink

**Symptom**: Path comparison assertion fails on macOS. Test creates a file in `/tmp/`
but the resolved path shows `/private/tmp/`. Strings do not match.

**Root cause**: On macOS, `/tmp` is a symlink to `/private/tmp`. `os.TempDir()` returns
`/tmp`, but `filepath.EvalSymlinks()` and `os.Getwd()` resolve to `/private/tmp`.
Comparing the two without resolution fails.

**Fix template**: Use `t.TempDir()` (which returns the resolved path) instead of
`os.TempDir()`. For path comparisons, resolve both sides with `filepath.EvalSymlinks()`.

**Prevention**: Never hardcode `/tmp` in tests. Always use `t.TempDir()` for temporary
directories. For assertions comparing paths, resolve symlinks on both sides.

**Platform skill**: `macos-testing` SKILL.md (/tmp symlink section),
`macos-testing/references/filesystem-kqueue.md`.

---

### PS-4: Windows ERROR_SHARING_VIOLATION on temp files

**Symptom**: `Access is denied` or `The process cannot access the file because it is
being used by another process` during test cleanup on Windows.

**Root cause**: Windows uses mandatory file locking. If a file is opened by one process
(including Windows Defender real-time scanning), another process cannot delete or
rename it. Test cleanup that deletes temp files races with Defender scans.

**Fix template**: Close all file handles explicitly before cleanup. For stubborn cases,
retry deletion with a short backoff:

```go
t.Cleanup(func() {
    // Close handle first
    f.Close()
    // Retry removal (Defender may hold a brief lock)
    for range 3 {
        if err := os.Remove(f.Name()); err == nil {
            return
        }
        time.Sleep(100 * time.Millisecond)
    }
})
```

**Prevention**: Always close file handles before calling `os.Remove` or relying on
`t.TempDir()` cleanup. Use `t.TempDir()` which handles retry internally.

**Platform skill**: `windows-testing` SKILL.md (Sharing Violations section),
`windows-testing/references/filesystem-pitfalls.md`.

---

### PS-5: Linux ENOSPC from inotify

**Symptom**: `inotify: too many watches` or `no space left on device` error from
`fsnotify.NewWatcher()` or `watcher.Add()` on Linux.

**Root cause**: Linux has a per-user limit on inotify watches
(`/proc/sys/fs/inotify/max_user_watches`, default 8192 on older kernels, 65536+
on newer). Parallel tests each creating watchers can exhaust this limit.

**Fix template**: Increase the limit in CI environment, or limit test parallelism
for watcher tests:

```bash
# In CI setup
echo 524288 | sudo tee /proc/sys/fs/inotify/max_user_watches
```

For local tests, limit watcher creation or use serial execution for watcher tests.

**Prevention**: Watcher tests should clean up watchers in `t.Cleanup()`. CI workflows
should increase inotify limits during setup.

**Platform skill**: `linux-testing` SKILL.md (inotify section),
`linux-testing/references/filesystem-inotify.md`.

---

### PS-6: Windows PATHEXT affecting exec.LookPath

**Symptom**: `exec.LookPath` finds (or fails to find) an executable on Windows
when it works on Linux/macOS. Or it finds a `.bat` file instead of the expected
binary.

**Root cause**: Windows `exec.LookPath` searches `PATHEXT` extensions (`.COM`,
`.EXE`, `.BAT`, `.CMD`, etc.) in order. A `foo.bat` in the PATH will be found
before `foo.exe` if `.BAT` precedes `.EXE` in `PATHEXT`. Tests creating executable
files must use `.bat` or `.exe` extensions on Windows.

**Fix template**: Use `[windows]`/`[!windows]` conditionals in testscript tests
to create platform-appropriate executables. In Go tests, use `platform.IsWindows()`
to choose the correct extension.

**Prevention**: Tests that create or look up executables must account for PATHEXT
on Windows. Use `pkg/platform` constants for OS detection.

**Platform skill**: `windows-testing` SKILL.md (PATHEXT section),
`windows-testing/references/filesystem-pitfalls.md`.

---

## Container

### CT-1: Container test hangs indefinitely

**Symptom**: A container integration test hangs forever on Linux CI. The test
never completes, and the CI job is eventually killed by the 30-minute job timeout.
No output is produced after the container command starts.

**Root cause**: The test uses `t.Context()` (which has no deadline) or
`context.Background()` instead of a bounded context. The container process blocks
in an uninterruptible kernel state (Podman cgroup operations), and no timeout
fires to cancel it.

**Fix template**: Use `testutil.ContainerTestContext()` with an explicit timeout:

```go
ctx := testutil.ContainerTestContext(t, testutil.DefaultContainerTestTimeout)
result, err := runtime.Execute(ctx, cmd)
```

**Prevention**: All container tests must use `testutil.ContainerTestContext()`.
The 5-layer timeout strategy in this project (engine probe, testscript deadline,
Go `-timeout`, job `timeout-minutes`, and `ContainerTestContext`) provides defense
in depth.

**Platform skill**: `linux-testing` SKILL.md (Container Test Infrastructure),
`linux-testing/references/container-testing-deep.md`.

**Codebase locations**: `internal/testutil/container_context.go`,
`internal/runtime/container_integration_validation_test.go`.

---

### CT-2: OOM with -race (exit code 137)

**Symptom**: Container tests exit with code 137 on Linux CI. The race detector
is enabled. Multiple container tests run in parallel.

**Root cause**: The Go race detector adds ~10x memory overhead. Combined with
container operations (which fork processes and allocate cgroup memory), parallel
container tests can exceed the runner's memory limit. The Linux OOM killer
terminates the process with SIGKILL (exit code 137).

**Fix template**: Limit container test parallelism with the semaphore:

```go
// Set via environment variable in CI
// INVOWK_TEST_CONTAINER_PARALLEL=2
```

The CI workflow sets `INVOWK_TEST_CONTAINER_PARALLEL: "2"` for all full-mode runs.

**Prevention**: Never increase `INVOWK_TEST_CONTAINER_PARALLEL` above 2 for race-enabled
builds. Monitor CI memory usage if new container tests are added.

**Platform skill**: `linux-testing` SKILL.md (OOM Killer section),
`linux-testing/references/container-testing-deep.md`.

---

### CT-3: Podman ping_group_range race

**Symptom**: Podman container tests fail intermittently with permission errors or
`newuidmap`/`newgidmap` failures on Linux CI. Multiple Podman tests start
containers simultaneously.

**Root cause**: Podman rootless mode uses user namespaces and reads
`/proc/sys/net/ipv4/ping_group_range` at startup. Concurrent Podman invocations
can race on reading this file. Additionally, `newuidmap`/`newgidmap` invocations
can collide when multiple containers start simultaneously.

**Fix template**: Use `CONTAINERS_CONF_OVERRIDE` to configure Podman, and use
`flock`-based serialization for container startup in tests:

```go
// Container tests use flock-based cross-binary serialization
// See internal/runtime/run_lock_linux_test.go
```

**Prevention**: The container test semaphore (`ContainerSemaphore`) and
`INVOWK_TEST_CONTAINER_PARALLEL` limit concurrency. `flock` provides cross-binary
serialization when multiple test binaries run simultaneously.

**Platform skill**: `linux-testing` SKILL.md (Podman rootless section),
`linux-testing/references/container-testing-deep.md`.

**Codebase locations**: `internal/runtime/run_lock_linux_test.go`,
`internal/runtime/container_exec_test.go`.

---

### CT-4: Engine masking failure in CI

**Symptom**: Container tests use the wrong engine, or a Podman-only test runs
against Docker (or vice versa). The test may pass but produce incorrect coverage.

**Root cause**: The CI workflow masks the non-tested engine by moving its binary
(`sudo mv /usr/bin/docker /usr/bin/docker.disabled`). If this step fails silently
(the `|| true` suffix), both engines remain available and `AutoDetectEngine()`
picks Docker by default (higher priority).

**Fix template**: Verify the mask step output in CI logs. If the mask failed,
check if the binary path has changed on the runner image. Update the mask command
to match the actual binary location.

**Prevention**: The mask steps use `|| true` intentionally (the binary may not
exist on all runner images). However, the test should verify which engine is
active before proceeding.

**Platform skill**: `linux-testing` SKILL.md,
`.github/workflows/ci.yml` (lines 112-117 for mask steps).

---

### CT-5: Container stderr in exit-code tests

**Symptom**: A testscript container error-path test fails because `! stderr .`
matches unexpected stderr output. The container command produced incidental
stderr (e.g., shell prompt `#`, container runtime warnings).

**Root cause**: Container commands may produce stderr output that is not part of
the error being tested. The shell inside the container may emit a prompt character.
Container runtimes may emit warnings about deprecated features or configuration.

**Fix template**: Remove `! stderr .` from container error-path tests. Instead,
assert on specific error content:

```
# Bad: fails due to incidental container stderr
! invowk cmd broken-cmd
! stderr .

# Good: assert specific error content only
! invowk cmd broken-cmd
stderr 'expected error message'
```

**Prevention**: Never use `! stderr .` in container-related testscript tests.
Assert on specific stderr patterns instead.

**Platform skill**: `linux-testing` SKILL.md,
`testing` SKILL.md (testscript patterns).

---

## Assertion / Logic

### AL-1: Path separator in assertions

**Symptom**: A test assertion comparing file paths fails on Windows. The expected
path uses `/` separators but the actual path uses `\`.

**Root cause**: `filepath.Join()` uses the OS-native separator (`\` on Windows,
`/` on Linux/macOS). Hardcoded path strings with `/` do not match.

**Fix template**: Use `filepath.Join()` in both the expected and actual values:

```go
// Bad: hardcoded separator
expected := "internal/config/config.go"

// Good: platform-agnostic
expected := filepath.Join("internal", "config", "config.go")
```

For testscript assertions, use `[!windows]`/`[windows]` conditionals when path
output differs by platform.

**Prevention**: Never hardcode path separators in test assertions. Use
`filepath.Join()` for all path construction.

**Platform skill**: `windows-testing` SKILL.md (Filesystem Pitfalls),
`windows-testing/references/filesystem-pitfalls.md`.

---

### AL-2: Case-sensitive filename collision on macOS

**Symptom**: A test creates two files with names differing only in case (e.g.,
`Config.go` and `config.go`). On macOS, only one file exists after creation.

**Root cause**: APFS (the default macOS filesystem) is case-preserving but
case-insensitive. `Config.go` and `config.go` refer to the same file. Creating
the second silently overwrites the first.

**Fix template**: Use unique filenames that differ in more than just case. For
tests that specifically test case sensitivity, skip on macOS:

```go
if runtime.GOOS == "darwin" {
    t.Skip("macOS APFS is case-insensitive")
}
```

**Prevention**: Never rely on case-sensitive filenames in cross-platform tests.
Use `t.Skip` with explanation for tests that specifically exercise case sensitivity.

**Platform skill**: `macos-testing` SKILL.md (APFS Case Sensitivity section),
`macos-testing/references/filesystem-kqueue.md`.

---

### AL-3: gotestsum false-FAIL on parallel subtests

**Symptom**: The parent test shows `FAIL` in gotestsum output even though all
subtests passed. The CI JUnit report marks the parent as failed. This occurs
primarily on macOS CI.

**Root cause**: Without the `-v` flag, `gotestsum` does not receive per-subtest
PASS/FAIL lines from `go test`. It cannot reconcile the parent test status when
subtests run in parallel and complete in non-deterministic order. The `testdox`
format requires `-v` to correctly attribute subtest outcomes to their parents.

**Fix template**: Ensure `-v` is passed to `go test` when using `gotestsum` with
parallel subtests:

```bash
gotestsum --format testdox --packages ./... -- -v -race
```

The CI workflow already includes `-v` on short-mode steps. Verify it is present
on all `gotestsum` invocations.

**Prevention**: All `gotestsum` invocations in CI must include `-v` on the
`go test` side. This is documented in the MEMORY.md "gotestsum `-v` required"
entry.

**Platform skill**: `macos-testing` SKILL.md (gotestsum false-FAIL section),
`go-testing` SKILL.md (gotestsum integration section).

---

### AL-4: Testscript dead commands after file marker

**Symptom**: A testscript command appears to be silently ignored. No error, no
output — the command simply does not execute.

**Root cause**: Commands placed after a `-- filename --` marker in a `.txtar` file
are treated as file content, not as testscript commands. The testscript parser
considers everything after the first `-- filename --` line as part of the file
section.

**Fix template**: Move the command above the file section, or add it as a separate
command block before the file markers:

```
# Commands go here, before file markers
invowk cmd my-command
stdout 'expected output'

-- invowkfile.cue --
{file content}
```

**Prevention**: Review testscript files to ensure all commands appear before the
first `-- filename --` marker.

**Platform skill**: `testing` SKILL.md (testscript patterns).

---

### AL-5: Native txtar assertions mirroring virtual assertions verbatim

**Symptom**: A `native_*.txtar` test fails because its assertions were copied
verbatim from the corresponding `virtual_*.txtar` test. The native test has
different scripts per platform (bash vs PowerShell), but the assertions assume
identical output format.

**Root cause**: Native tests run platform-specific shell scripts (bash on
Linux/macOS, PowerShell on Windows). Script output format may differ from the
virtual shell. Assertions must be platform-agnostic or conditionally guarded.

**Fix template**: Use `[!windows]`/`[windows]` conditionals for platform-specific
assertions, or make assertions generic enough to match both:

```
# Platform-agnostic assertion (matches both formats)
stdout 'result: ok'

# Platform-specific assertions when output format differs
[!windows] stdout '^done$'
[windows] stdout '^done\r?$'
```

**Prevention**: When generating native mirrors from virtual tests (using the
`native-mirror` skill), review all assertions for platform-specific assumptions.
Do not copy virtual assertions verbatim.

**Platform skill**: `testing` SKILL.md (testscript patterns),
`native-mirror` SKILL.md.
