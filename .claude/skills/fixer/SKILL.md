---
name: fixer
description: >-
  Platform-aware bug diagnosis and fix workflow with parallel subagents.
  Auto-triggers when the user mentions fixing bugs, test failures, CI failures,
  flaky tests, error messages, race conditions, or debugging any issue.
  Also user-invocable as /fixer with an issue description, error output,
  PR number, or CI run URL. Spawns up to 3 parallel diagnostic subagents
  based on failure type: platform investigator (consults windows-testing,
  macos-testing, linux-testing), code path tracer, and CI log analyzer.
  Produces a structured diagnosis report with root cause, fix, and prevention.
  Use this skill whenever you encounter test failures, CI failures, runtime
  errors, race detector reports, or any situation that requires diagnosing
  and fixing a bug — even if the user doesn't explicitly say "fix" or "debug".
disable-model-invocation: false
---

# Fixer Skill

Platform-aware bug diagnosis and fix workflow. Produces structured root cause
analysis, applies fixes, and prevents recurrence through tests and guards.

## Normative Precedence

1. `.agents/rules/checklist.md` — mandatory verification after any fix.
2. `.agents/rules/testing.md` — test policy for new/modified tests.
3. `.agents/rules/go-patterns.md` — code quality for production fixes.
4. This skill — diagnosis workflow, subagent orchestration, platform routing.

## When This Skill Activates

**Auto-trigger contexts** (the skill should activate without explicit invocation):
- User pastes error output or stack traces
- User mentions CI failures, test failures, or flaky tests
- User says "fix", "debug", "broken", "failing", "investigate"
- User provides a GitHub Actions run URL or PR with failing checks
- Race detector output is shown or mentioned
- User asks "why is this test failing" or "what's wrong with..."

**User-invocable**: `/fixer <description>` or `/fixer` (then provide context).

Arguments can be:
- A description of the issue: `/fixer the watcher tests are flaking on macOS`
- Error output pasted inline
- A GitHub PR or run reference: `/fixer check CI failures on PR #42`
- A test name: `/fixer TestContainerExec hangs on ubuntu-24.04/podman`

## Workflow Overview

```
Input (error, CI failure, user description)
    │
    ▼
Phase 1: Evidence Gathering ──── up to 2 parallel subagents
    │
    ▼
Phase 2: Diagnosis ──────────── up to 3 parallel subagents
    │                            (platform investigator, code tracer, pattern matcher)
    ▼
Phase 3: Fix Design ─────────── propose fix + prevention
    │
    ▼
Phase 4: Apply & Verify ─────── apply fix, run tests, checklist
```

---

## Phase 1: Evidence Gathering

Collect all available evidence before spawning diagnostic agents. The quality
of the diagnosis depends entirely on the quality of the evidence.

### What to Gather

| Source | How to Access | What to Extract |
|--------|--------------|----------------|
| User-provided error output | Read from conversation | Error message, stack trace, test name |
| CI run logs | `gh run view <id> --log-failed` | Failing tests, platform, exit codes |
| CI JUnit artifacts | `gh run download <id> -n test-results-*` | Test names, durations, rerun status |
| Local test output | `go test -v -run TestName ./pkg/...` | Full output with timing |
| Race detector report | From CI or local `-race` run | Goroutine stacks, access locations |
| Git blame | `git log --oneline -10 -- <file>` | Recent changes to failing code |
| GitHub PR checks | `gh pr checks <pr-number>` | Which CI jobs failed |

### Platform Detection

Before spawning diagnostic agents, determine which platform(s) are affected:

| Signal | Platform | Skill to Consult |
|--------|----------|-----------------|
| `windows-latest` in CI job name | Windows | `windows-testing` |
| `macos-15` in CI job name | macOS | `macos-testing` |
| `ubuntu-*` in CI job name | Linux | `linux-testing` |
| Exit code 137 | Linux (OOM) | `linux-testing` |
| `TerminateProcess` or exit code 1 with timeout | Windows | `windows-testing` |
| `ERROR_SHARING_VIOLATION` or `Access is denied` | Windows | `windows-testing` |
| `ENOSPC` from fsnotify | Linux | `linux-testing` |
| kqueue-related or watcher event missed | macOS | `macos-testing` |
| Container test hang | Linux | `linux-testing` |
| `/private/tmp` path mismatch | macOS | `macos-testing` |
| `\r\n` or CRLF in assertion mismatch | Windows | `windows-testing` |
| `sync.Once` race in lipgloss | Windows | `windows-testing` |
| Timer/sleep-based flakiness | macOS or Windows | `macos-testing` + `windows-testing` |
| `-race` timeout on large suite | Windows | `windows-testing` |
| gotestsum false-FAIL on parallel subtests | macOS | `macos-testing` |
| `flock` contention | Linux | `linux-testing` |
| CUE thread-safety race | Any (serial subtests) | `go-testing` |

If the failure is platform-independent (logic bug, missing nil check, wrong
algorithm), skip platform investigation entirely.

### CI Failure Fast Path

When the input is a CI failure (PR checks, run URL, or "CI is failing"):

```bash
# Get failing checks for a PR
gh pr checks <pr-number> --json name,status,conclusion --jq '.[] | select(.conclusion == "FAILURE")'

# View failed run logs (most recent)
gh run list --branch $(git branch --show-current) --status failure --limit 1 --json databaseId --jq '.[0].databaseId'
gh run view <run-id> --log-failed 2>&1 | head -200

# Download test artifacts
gh run download <run-id> -n 'test-results-*' -D /tmp/ci-artifacts/
```

Extract from the logs: test name, package, platform (from job name), error type
(panic, timeout, race, assertion failure), and exit code.

---

## Phase 2: Diagnosis

Spawn parallel subagents based on the evidence. Not every failure needs all
agents — spawn only what's relevant.

### Subagent Dispatch Table

| Subagent | When to Spawn | Focus |
|----------|--------------|-------|
| **Platform Investigator** | Platform-specific symptoms detected | Read the relevant platform skill's Failure Matrix. Match symptoms to known causes. Check if the fix pattern is documented. |
| **Code Path Tracer** | Failing test identified | Read the failing test AND the production code it exercises. Trace the execution path. Identify data flow, concurrency, and resource lifecycle. |
| **Pattern Matcher** | Multiple failures or flaky behavior | Search git log for similar past fixes (`git log --oneline --grep="fix(test)" -20`). Check if the same test failed before. Look for patterns across the failure set. |

### Subagent Prompts

Use these prompt templates for consistent subagent behavior. Adapt the details
to the specific failure.

#### Platform Investigator Prompt

```
You are diagnosing a platform-specific test failure in the invowk Go project.

## Failure Evidence
{paste error output, test name, platform, CI job name}

## Your Task
1. Read the platform skill at `.agents/skills/{platform}-testing/SKILL.md`
2. Check the Failure Matrix section — does this symptom match a known pattern?
3. If yes: report the known cause and documented fix
4. If no: read the relevant reference files for deeper investigation
5. Check `.agents/skills/go-testing/SKILL.md` for test toolchain issues

## For race detector reports
Read `.agents/skills/go-testing/references/race-detector-guide.md` for
how to interpret the output and common race patterns.

## Output Format
- **Platform**: {windows|macos|linux}
- **Symptom match**: {yes/no — cite the Failure Matrix entry if yes}
- **Root cause hypothesis**: {one sentence}
- **Evidence**: {file:line references supporting the hypothesis}
- **Recommended fix**: {specific code change or configuration}
- **Prevention**: {test guard, CI config, or code pattern to prevent recurrence}
```

#### Code Path Tracer Prompt

```
You are tracing the execution path of a failing test in the invowk Go project.

## Failure Evidence
{test name, error message, package}

## Your Task
1. Read the failing test file. Understand what it asserts.
2. Read the production code it exercises. Trace the path from test input to
   the failing assertion.
3. Identify: resource lifecycle (open/close/cleanup), concurrency
   (goroutines, channels, mutexes), context propagation, and error handling.
4. Check for: nil dereferences, race conditions, resource leaks, incorrect
   assertions, environment assumptions.

## Consult these skills as needed
- `.agents/rules/testing.md` for test policy
- `.agents/skills/go-testing/SKILL.md` for parallelism/context patterns
- `.agents/rules/go-patterns.md` for code quality patterns

## Output Format
- **Test**: {file:line}
- **Production code under test**: {file:line, function name}
- **Execution path**: {step-by-step trace}
- **Root cause**: {what's wrong and why}
- **Fix location**: {which file(s) and function(s) to change}
```

#### Pattern Matcher Prompt

```
You are searching for patterns related to a test failure in the invowk Go project.

## Failure Evidence
{test name, error type, platform}

## Your Task
1. Search git log for similar fixes:
   git log --oneline -30 --grep="fix(test)\|fix(ci)\|flaky\|race\|timeout"
2. Search for the test name in git log to see if it failed before:
   git log --oneline -10 --grep="{test_name}"
3. Search the codebase for similar patterns to the failing code
4. Check the MEMORY.md and memory files for known pitfalls

## Output Format
- **Similar past fixes**: {commit hashes and summaries}
- **Recurrence**: {has this test failed before? when?}
- **Pattern**: {is this a known pattern type? which?}
- **Cross-reference**: {other tests or code with the same vulnerability}
```

### When NOT to Use Subagents

For simple, obvious bugs (typo, missing nil check, wrong variable name), skip
the subagent overhead entirely. Apply the fix directly and verify. Use subagents
when:
- The root cause is not immediately obvious
- The failure is platform-specific or intermittent
- Multiple tests fail simultaneously
- The failure involves concurrency or timing

---

## Phase 3: Fix Design

After diagnosis, design the fix with three components:

### 1. Root Cause Report

Write a concise report before coding:

```markdown
## Root Cause

**Symptom**: {what the user sees}
**Cause**: {what's actually wrong}
**Platform**: {all | windows | macos | linux}
**Category**: {race condition | resource leak | timing | assertion | logic | configuration}

## Fix

{describe the change — what, where, and why}

## Prevention

{what test, guard, or pattern prevents recurrence}
```

### 2. The Fix Itself

Apply the minimum change that resolves the root cause. Follow the project's
existing patterns:

- **Race condition**: add synchronization (mutex, channel, `atomic`) or make
  tests serial (`//nolint:tparallel` with justification). Consult
  `go-testing` § "Parallelism Decision Framework".
- **Timing flakiness**: replace `time.Sleep` with event-based synchronization
  (channels, `sync.WaitGroup`, poll loops with deadlines). Consult
  `macos-testing` § "Timer Coalescing" and `windows-testing` § "Timer Resolution".
- **Platform mismatch**: add `skipOnWindows`, `[!windows]`/`[windows]`
  conditionals, or platform-agnostic assertions. Consult the relevant platform
  skill's Failure Matrix.
- **Container hang**: add `ContainerTestContext`, `WaitDelay`, or engine health
  probe. Consult `linux-testing` § "Container Test Infrastructure".
- **Context misuse**: replace `context.Background()` with `t.Context()` or
  bounded context. Consult `go-testing` § "Context Patterns Decision Tree".

### 3. Prevention

Every fix should include at least one prevention measure:

| Prevention Type | When to Use | Example |
|----------------|------------|---------|
| Regression test | Logic bugs, nil checks | Add test case to existing table-driven test |
| Platform guard | OS-specific behavior | `skipOnWindows: true` with reason |
| Linter rule | Pattern-level prevention | New golangci-lint check or goplint rule |
| CI configuration | Timeout/resource issues | Adjust `-timeout`, `-parallel`, semaphore |
| Documentation | Subtle gotcha | Add entry to platform skill Failure Matrix |

### 4. Test-Writing Guardrails

When the fix involves writing or modifying test code, apply the testing skill's
Pre-Write Checklist (`.agents/skills/testing/SKILL.md` § "Pre-Write Checklist")
before committing. The most common "fix creates new problem" patterns are:

| Mistake | Consequence | Prevention |
|---------|-------------|------------|
| Adding `t.Parallel()` without checking safety | Data races, crashes, multi-round follow-up | Consult `go-testing` § Parallelism Decision Framework BEFORE adding |
| Leaving stale `//nolint:` after removing suppressed code | `nolintlint` CI failure | Run `make lint` after any code removal near `//nolint` directives |
| New test helper without `t.Helper()` | Confusing failure locations in CI output | First statement in any helper that calls `t.Error`/`t.Fatal` |
| `t.Errorf` before nil dereference | Panic in test binary | Use `t.Fatalf` when next line dereferences the result |
| Orphaned imports after moving tests | Compilation failure | Run `go build ./path/...` after moving test functions |

---

## Phase 4: Apply & Verify

After applying the fix, run verification. This is non-negotiable — a fix that
introduces new failures is worse than the original bug.

### Targeted Verification

Run the minimum verification that confirms the fix:

```bash
# Run the specific failing test
go test -v -race -run TestName ./path/to/package/...

# Run the full package (catches side effects)
go test -v -race ./path/to/package/...

# If the fix touches multiple packages
go test -v -race ./path/to/pkg1/... ./path/to/pkg2/...
```

### Full Verification (before commit)

Follow `.agents/rules/checklist.md`:

1. `make lint` — fix ALL issues
2. `make test` — full test suite (NOT `-short`)
3. `make check-baseline` — no new goplint findings
4. `make check-file-length` — all files under 1000 lines

If the fix required new tests or modified test infrastructure, also run:
- `make test-cli` (if CLI tests changed)
- `make sonar-local` (if production code changed)

### Cross-Platform Verification

If the bug was platform-specific, verify the fix doesn't break other platforms.
Since local testing is limited to the current OS:

- Check that any new `skipOnWindows`/`[!windows]` guards are justified
- Verify assertions are platform-agnostic (use `filepath.Join`, not hardcoded separators)
- If adding a `*_<platform>_test.go` file, ensure a corresponding `*_<other_platform>_test.go` exists or the behavior is covered by the generic test

---

## Failure Category Quick Reference

For rapid diagnosis, map the failure type to the right investigation path:

### Test Failures

| Category | First Action | Skills to Consult |
|----------|-------------|------------------|
| Assertion mismatch | Read test + production code | `testing`, `go-testing` |
| Race detector report | Read the two conflicting goroutine stacks | `go-testing` (race-detector-guide.md) |
| Timeout (exit code 1) | Check `-timeout` value and test duration | `go-testing` (test-flags-matrix.md) |
| Timeout (exit code 137) | OOM killer — check memory pressure | `linux-testing` (OOM Killer section) |
| Panic / nil pointer | Read stack trace, trace nil source | `go-patterns` |
| Flaky (passes sometimes) | Check for timing, concurrency, or platform issues | platform skills + `go-testing` |

### CI-Specific Failures

| Category | First Action | Skills to Consult |
|----------|-------------|------------------|
| Windows-only failure | Check path separators, CRLF, TerminateProcess | `windows-testing` |
| macOS-only failure | Check kqueue, timer coalescing, /tmp symlink | `macos-testing` |
| Container test hang | Check WaitDelay, ContainerTestContext | `linux-testing` |
| All tests `(unknown)` | Binary killed — timeout or OOM | `go-testing` + `linux-testing` |
| Lint failure | Read linter output, check `.golangci.toml` | `go-patterns` |
| Baseline regression | Run `make check-types-json`, triage findings | CLAUDE.md § goplint |

### Build Failures

| Category | First Action | Skills to Consult |
|----------|-------------|------------------|
| Compilation error | Read error message, check imports | — (straightforward) |
| CUE schema mismatch | Run `/schema-sync-check` | `cue` |
| Missing dependency | `make tidy` | — |

---

## Relationship to Other Commands and Skills

| Command/Skill | Relationship |
|---|---|
| `/fix-it` | Legacy command — simple one-line prompt. The `fixer` skill provides the structured version with subagents and platform awareness. |
| `/fix-it-simple` | Legacy command — concise variant. Use when you want a quick fix without the full workflow. |
| `ci-readiness` agent | Runs pre-commit checks. The fixer skill uses these same checks in Phase 4 verification. |
| `code-reviewer` agent | Reviews code diffs. Can be spawned as an additional subagent in Phase 2 for complex fixes. |
| `go-testing` skill | Primary reference for test toolchain issues. The fixer consults this for context/parallelism patterns. |
| `windows-testing` / `macos-testing` / `linux-testing` | Platform diagnostic lookup tables. The fixer routes to these based on failure symptoms. |
| `testing` skill | Invowk-specific test patterns. The fixer consults this for testscript and container testing. |

---

## Reference Files

- **[references/ci-failure-diagnosis.md](references/ci-failure-diagnosis.md)** —
  How to query CI failures with `gh` CLI, parse JUnit XML artifacts, interpret
  rerun reports, and map CI job names to platforms. Read when diagnosing CI failures.
- **[references/subagent-dispatch.md](references/subagent-dispatch.md)** —
  Detailed subagent configurations, prompt templates for edge cases, and
  dispatch decision flowchart. Read when spawning subagents for complex failures.
- **[references/failure-pattern-catalog.md](references/failure-pattern-catalog.md)** —
  Comprehensive catalog of failure patterns seen in the invowk project, organized
  by category. Each entry has: symptom, root cause, fix template, and platform
  skill cross-reference. Read for pattern matching during diagnosis.
