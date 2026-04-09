# CI Failure Diagnosis

How to query, parse, and interpret CI failures from the command line.
Read this reference whenever diagnosing a CI failure — before spawning subagents.

---

## Querying CI Status with `gh` CLI

### List check statuses for a PR

```bash
# All checks with name, status, and conclusion
gh pr checks <pr-number>

# JSON output filtered to failures only
gh pr checks <pr-number> \
  --json name,status,conclusion \
  --jq '.[] | select(.conclusion == "FAILURE")'
```

### Find recent failed runs on a branch

```bash
# Most recent 5 failed runs on the current branch
gh run list --branch $(git branch --show-current) --status failure --limit 5

# Get just the run ID of the most recent failure
gh run list --branch $(git branch --show-current) --status failure --limit 1 \
  --json databaseId --jq '.[0].databaseId'
```

### View failed step logs

```bash
# Show only the failing step logs (most useful for quick diagnosis)
gh run view <run-id> --log-failed 2>&1 | head -200

# Show full logs for a specific job (when you need more context)
gh run view <run-id> --log --job <job-id> 2>&1 | head -500
```

Pipe to `head` to avoid flooding the terminal. CI logs can be thousands of lines.

### Download JUnit XML artifacts

```bash
# Download all test result artifacts to a local directory
gh run download <run-id> -n 'test-results-*' -D /tmp/ci-artifacts/

# Download a specific platform's results
gh run download <run-id> -n 'test-results-ubuntu-24.04-docker' -D /tmp/ci-artifacts/
```

### Get job details with platform info

```bash
# List all jobs in a run with their status and platform
gh api repos/{owner}/{repo}/actions/runs/<run-id>/jobs \
  --jq '.jobs[] | {name, status, conclusion, runner: .labels[0]}'

# Filter to failed jobs only
gh api repos/{owner}/{repo}/actions/runs/<run-id>/jobs \
  --jq '.jobs[] | select(.conclusion == "failure") | {name, conclusion, started_at, completed_at}'
```

For the invowk repo specifically:

```bash
gh api repos/invowk/invowk/actions/runs/<run-id>/jobs \
  --jq '.jobs[] | select(.conclusion == "failure") | .name'
```

---

## Parsing JUnit XML

### Structure

The CI produces JUnit XML files with this hierarchy:

```xml
<testsuites>
  <testsuite name="github.com/invowk/invowk/internal/runtime" tests="45" failures="1" time="12.34">
    <testcase name="TestContainerExec/happy_path" classname="github.com/invowk/invowk/internal/runtime" time="2.45">
      <failure message="assertion failed" type="">
        Error message and stack trace appear as text content here.
        === RUN   TestContainerExec/happy_path
        runtime_test.go:123: expected "ok" but got "error: connection refused"
      </failure>
    </testcase>
    <testcase name="TestVirtualShell/basic" classname="github.com/invowk/invowk/internal/runtime" time="0.12"/>
  </testsuite>
</testsuites>
```

### Key attributes

| Attribute | Location | Meaning |
|-----------|----------|---------|
| `name` | `<testcase>` | Full test name including subtests (slash-separated) |
| `classname` | `<testcase>` | Go package path |
| `time` | `<testcase>` | Duration in seconds (decimal) |
| `tests` | `<testsuite>` | Total test count in the package |
| `failures` | `<testsuite>` | Number of failures in the package |
| `message` | `<failure>` | Short failure description |
| text content | `<failure>` | Full error output including stack traces |

### Quick extraction

```bash
# Find all failing test names and packages
grep -B1 '<failure' test-results.xml | grep '<testcase'

# Extract failure messages with 2 lines of context
grep -A2 '<failure' test-results.xml

# Count failures per file
grep '<testsuite' test-results.xml | grep 'failures="[^0]"'
```

### Artifact naming conventions

The CI workflow (`.github/workflows/ci.yml`) uploads artifacts with platform-specific names:

| Artifact name pattern | Platform | Engine |
|-----------------------|----------|--------|
| `test-results-ubuntu-24.04-docker` | Linux | Docker |
| `test-results-ubuntu-24.04-podman` | Linux | Podman |
| `test-results-ubuntu-latest-docker` | Linux | Docker |
| `test-results-ubuntu-latest-podman` | Linux | Podman |
| `test-results-windows-unit` | Windows | none |
| `test-results-macos-unit` | macOS | none |

### JUnit files produced per run

**Full-mode runs** (Linux with Docker or Podman) produce 3 JUnit files:

| File | Contents | gotestsum step |
|------|----------|----------------|
| `test-results.xml` | All packages except `tests/cli` and `internal/runtime` | "Run tests (full, non-CLI)" |
| `runtime-test-results.xml` | `internal/runtime` package only | "Run internal/runtime tests (full)" |
| `cli-test-results.xml` | `tests/cli/...` (testscript integration tests) | "Run CLI integration tests (full)" |

**Short-mode runs** (Windows and macOS) produce 2 JUnit files:

| File | Contents | gotestsum step |
|------|----------|----------------|
| `test-results.xml` | All packages (`./...`) in `-short` mode | "Run tests (short)" |
| `cli-test-results.xml` | `tests/cli/...` (testscript, no container tests) | "Run CLI integration tests" |

---

## Interpreting Rerun Reports

### File locations

| File | Produced by | Scope |
|------|-------------|-------|
| `rerun-report.txt` | Main test step | All packages except `tests/cli` and `internal/runtime` |
| `runtime-rerun-report.txt` | Runtime test step | `internal/runtime` only |
| `cli-rerun-report.txt` | CLI test step (short-mode only) | `tests/cli/...` |

### Format

Plain text, one test name per line:

```
TestWatcherDebounce
TestContainerExec/timeout_handling
TestTUIConfirm/cancel_path
```

### Interpretation rules

1. **Tests listed in rerun reports passed on retry.** They are flaky, not broken.
   The CI run still passed, but flakiness should be investigated.

2. **If ALL failing tests appear in rerun reports**, the CI passed overall.
   The failures were transient. Check for timing dependencies, resource contention,
   or platform-specific issues using the platform skills.

3. **If a failing test is NOT in any rerun report**, it is a deterministic failure.
   The test failed on every attempt. This is a real bug, not flakiness.

4. **Rerun limits**: Full-mode reruns up to 5 failures (`--rerun-fails-max-failures 5`).
   Short-mode reruns up to 3 (`--rerun-fails-max-failures 3`). If more tests fail
   than the limit, gotestsum skips reruns entirely — this signals a real regression,
   not flakiness.

5. **CI flags flaky tests as warnings**. The "Flag flaky tests" step emits
   `::warning::` annotations for any non-empty rerun report. These appear in the
   GitHub Actions summary.

---

## Mapping CI Job Names to Platforms

The CI matrix in `.github/workflows/ci.yml` defines these test configurations:

| Job name pattern | Runner | Platform | Engine | Test mode | Timeout flags |
|------------------|--------|----------|--------|-----------|---------------|
| `Test (ubuntu-24.04 / docker)` | `ubuntu-24.04` | Linux | Docker | full | `-race -timeout 15m` |
| `Test (ubuntu-24.04 / podman)` | `ubuntu-24.04` | Linux | Podman | full | `-race -timeout 15m` |
| `Test (ubuntu-latest / docker)` | `ubuntu-latest` | Linux | Docker | full | `-race -timeout 15m` |
| `Test (ubuntu-latest / podman)` | `ubuntu-latest` | Linux | Podman | full | `-race -timeout 15m` |
| `Test (windows)` | `windows-latest` | Windows | none | short | `-race -short -v` |
| `Test (macos)` | `macos-15` | macOS | none | short | `-race -short -v` |

Key differences:
- **Full mode** runs all tests including container integration tests.
- **Short mode** skips integration tests (`-short` flag) and container tests
  (no engine available). `-v` is required on short-mode for correct gotestsum behavior.
- **Container parallelism**: Full-mode Linux tests set `INVOWK_TEST_CONTAINER_PARALLEL=2`
  to limit concurrent container operations.
- **Engine masking**: Linux runners have both Docker and Podman installed. CI masks
  the non-tested engine (moves its binary) so `AutoDetectEngine()` picks the correct one.

---

## Common CI Failure Patterns

### Binary-level timeout

**Symptom**: `panic: test timed out after 15m0s` or all test results show `(unknown)`.

**Cause**: The entire test binary was killed by Go's `-timeout` flag. This masks the
actual failing test — all results become `(unknown)` because the binary could not
write the remaining JUnit entries.

**Diagnosis**: Look for the last test that was running before the timeout. The
`gotestsum` output typically shows `RUNNING` lines for in-progress tests at the time
of the kill.

**Cross-reference**: `go-testing` skill (test-flags-matrix.md, timeout behavior).

### OOM killer (exit code 137)

**Symptom**: Exit code 137, possibly preceded by `signal: killed`.

**Cause**: The Linux OOM killer terminated the process. Common with `-race` flag
(10x memory overhead) combined with heavy container operations.

**Diagnosis**: Check if the failure is on a Linux runner with many parallel container
tests. The container test semaphore (`INVOWK_TEST_CONTAINER_PARALLEL`) should limit
parallelism, but races in semaphore acquisition can still cause spikes.

**Cross-reference**: `linux-testing` skill (OOM Killer section, container-testing-deep.md).

### Compilation error (exit code 2)

**Symptom**: Exit code 2, `go build` or `go test` errors in output.

**Cause**: Code does not compile. Usually a missing import, type mismatch, or
syntax error introduced by the PR.

**Diagnosis**: Read the compiler error message directly — no subagents needed.

### Flaky test with rerun pass

**Symptom**: Test appears in `rerun-report.txt` but the CI run passed.

**Cause**: Timing dependency, resource contention, or platform-specific behavior.

**Diagnosis**: Check the platform skill failure matrices. Common culprits:
- Timer-based assertions (macOS coalescing, Windows 15.6ms resolution)
- File system race (Windows Defender scanning, kqueue event coalescing)
- Container startup time variation (Linux, engine-dependent)

### All tests `(unknown)`

**Symptom**: JUnit XML shows all tests as `(unknown)` status. No PASS or FAIL.

**Cause**: The test binary was killed mid-run (timeout or OOM). gotestsum could
not reconcile the output.

**Diagnosis**: Check for timeout or OOM first. If the binary timeout was hit,
increasing `-timeout` may reveal the actual failing test. If OOM, reduce parallelism.

**Cross-reference**: MEMORY.md "Timeout cascade masks failures" entry.

### Engine masking failure

**Symptom**: Wrong container engine used in CI, or container tests run when they
should not.

**Cause**: The `sudo mv` step to mask the non-tested engine failed silently
(`|| true`). Both engines remain available and `AutoDetectEngine()` picks Docker
by default.

**Diagnosis**: Check the "Mask Docker for Podman tests" or "Mask Podman for Docker
tests" step output in the CI logs.

**Cross-reference**: `linux-testing` skill, `.github/workflows/ci.yml` lines 112-117.
