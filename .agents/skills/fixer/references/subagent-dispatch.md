# Subagent Dispatch

Detailed decision flowchart, edge-case prompts, and subagent limits for the
fixer skill. Read this when the failure is not straightforward and subagents
are needed.

---

## Dispatch Decision Flowchart

```
Is the root cause obvious (typo, nil check, missing import, lint error)?
├── YES ──► Fix directly. Skip subagents.
│           (Build errors, lint failures, simple logic bugs.)
│
└── NO ──► Is this a CI failure (PR checks failing, run URL provided)?
    │
    ├── YES ──► Spawn CI Log Analyzer (always for CI failures)
    │   │
    │   └── Did a platform-specific job fail?
    │       │
    │       ├── YES ──► Also spawn Platform Investigator
    │       │   │       (read the matching platform skill)
    │       │   │
    │       │   └── Are multiple platforms failing differently?
    │       │       │
    │       │       ├── YES ──► Spawn one Platform Investigator per platform
    │       │       │           (up to 3 simultaneous, see limits below)
    │       │       │
    │       │       └── NO ──► Single Platform Investigator is sufficient
    │       │
    │       └── NO ──► Check if failure is in rerun reports
    │           │
    │           ├── YES (flaky) ──► Also spawn Pattern Matcher
    │           │                   (search git log for past occurrences)
    │           │
    │           └── NO (deterministic) ──► Also spawn Code Path Tracer
    │                                      (trace test → production code path)
    │
    └── NO ──► Is this a test failure (local or reported)?
        │
        ├── YES ──► Is it a race detector report?
        │   │
        │   ├── YES ──► Spawn Code Path Tracer
        │   │           (focus on the two conflicting goroutine stacks,
        │   │            read go-testing/references/race-detector-guide.md)
        │   │
        │   └── NO ──► Is it flaky (sometimes passes, sometimes fails)?
        │       │
        │       ├── YES ──► Spawn Platform Investigator + Pattern Matcher
        │       │           (timing/concurrency issues need both)
        │       │
        │       └── NO ──► Spawn Code Path Tracer
        │                   (deterministic failure, trace the logic)
        │
        └── NO ──► Is it a build or lint error?
            │
            └── YES ──► Fix directly. No subagents.
                        (Compiler errors and lint violations are self-describing.)
```

---

## Subagent Count Limits

Never spawn more than 3 subagents simultaneously. Context windows and execution
time scale with subagent count — diminishing returns above 3.

### Typical subagent counts by scenario

| Scenario | Subagents | Which ones |
|----------|-----------|------------|
| Simple bug (typo, nil, import) | 0 | Fix directly |
| Build or lint failure | 0 | Read error output directly |
| Single deterministic test failure | 1 | Code Path Tracer |
| Race detector report | 1 | Code Path Tracer (with race focus) |
| Single platform-specific failure | 2 | Platform Investigator + Code Path Tracer |
| Flaky test, single platform | 2 | Platform Investigator + Pattern Matcher |
| Complex CI failure, one platform | 2 | CI Log Analyzer + Platform Investigator |
| Complex CI failure, cross-platform | 3 | CI Log Analyzer + Platform Investigator + Code Path Tracer |
| Multiple platforms failing differently | 3 | One Platform Investigator per platform (max 3) |
| Flaky test needing history + platform | 3 | Platform Investigator + Code Path Tracer + Pattern Matcher |

### Hard rules

- **Maximum 3 simultaneous subagents.** If more investigation is needed, wait for
  the first batch to complete, synthesize findings, then spawn additional agents.
- **CI Log Analyzer is always first** when the input is a CI failure URL or PR
  with failing checks. It extracts the structured evidence other agents need.
- **Code Path Tracer and Platform Investigator are the most common pair.** They
  cover the "what happened in the code" and "what's different about this platform"
  axes simultaneously.

---

## Edge Case Prompts

Additional prompt variants for situations not covered by the standard prompts
in SKILL.md.

### Multiple platforms failing simultaneously

When different platforms fail with different symptoms (e.g., Windows times out,
macOS gets wrong output), spawn one Platform Investigator per platform:

```
You are diagnosing a {platform}-specific test failure in the invowk Go project.

## Context
Multiple platforms are failing simultaneously with different symptoms.
Focus ONLY on the {platform} failure. Another investigator handles {other_platform}.

## {platform} Failure Evidence
{paste the platform-specific error output}

## Your Task
1. Read `.agents/skills/{platform}-testing/SKILL.md`
2. Check the Failure Matrix — does this symptom match a known pattern?
3. Determine if the {platform} failure is independent or shares a root cause
   with other platforms
4. If independent: report the platform-specific cause and fix
5. If shared root cause: identify the common bug and the platform-specific
   manifestation

## Output Format
- **Platform**: {platform}
- **Independent or shared root cause**: {independent | shared with: ...}
- **Symptom match in Failure Matrix**: {yes/no, cite entry}
- **Root cause**: {one sentence}
- **Fix**: {specific change}
```

### Cascading failures

When many tests fail but they share a common upstream cause (e.g., a utility
function broke, causing all callers to fail):

```
You are analyzing cascading test failures in the invowk Go project.

## Evidence
{list of all failing tests with their packages}

## Your Task
1. Identify the FIRST failure chronologically (lowest test output line number,
   or earliest timestamp if available)
2. Check if the first failing test's production code is imported by the other
   failing test packages
3. If yes: focus on the first failure — the rest are downstream
4. If no: group failures by root cause (may be 2-3 independent bugs)

## Output Format
- **First failure**: {test name, package}
- **Cascade analysis**: {which tests are downstream of the first failure}
- **Independent groups**: {if multiple root causes, list them}
- **Root cause of primary failure**: {one sentence}
```

### CUE schema failures

CUE schema mismatches need the `cue` skill, not platform skills:

```
You are diagnosing a CUE schema failure in the invowk Go project.

## Evidence
{error output showing CUE validation or parse failure}

## Your Task
1. Read `.agents/skills/cue/SKILL.md` for the 3-step parse flow
2. Identify which schema is involved:
   - `pkg/invowkfile/invowkfile_schema.cue` (invowkfile structure)
   - `pkg/invowkmod/invowkmod_schema.cue` (invowkmod structure)
   - `internal/config/config_schema.cue` (config structure)
3. Check if the failure is a Go struct ↔ CUE schema drift (run /schema-sync-check)
4. Check if the failure involves CUE thread-safety (concurrent cue.Value access)

## Output Format
- **Schema file**: {path}
- **Drift type**: {Go→CUE mismatch | CUE syntax | thread-safety | validation logic}
- **Root cause**: {one sentence}
- **Fix**: {schema change, struct change, or test serialization}
```

### Race detector report

When the input is a race detector stack trace:

```
You are analyzing a Go race detector report from the invowk project.

## Race Report
{paste full DATA RACE output with both goroutine stacks}

## Your Task
1. Read `.agents/skills/go-testing/references/race-detector-guide.md`
2. Identify the two conflicting accesses:
   - What variable/field is being accessed?
   - Which goroutine writes? Which reads?
   - Are they in the same package or across packages?
3. Check the Failure Pattern Catalog for known race patterns:
   - lipgloss sync.Once (windows-testing)
   - CUE cue.Value concurrent access (go-testing)
   - SSH host key file collision (go-testing)
   - os.Stdin replacement in parallel tests (go-testing)
   - Shared MockCommandRecorder (go-testing)
4. If it matches a known pattern, apply the documented fix
5. If new, determine the correct synchronization strategy

## Output Format
- **Conflicting variable**: {package.Type.Field or variable name}
- **Writer goroutine**: {file:line, function}
- **Reader goroutine**: {file:line, function}
- **Known pattern match**: {yes/no, cite catalog entry}
- **Fix strategy**: {mutex | channel | serial subtests | per-test instance | atomic}
```

---

## Optional Post-Fix Code Review

After the fix is designed (Phase 3) but before applying it (Phase 4), consider
spawning the `code-reviewer` agent (`.agents/agents/code-reviewer.md`) as an
optional additional review step.

This is especially valuable for:

- **Fixes touching multiple packages**: The reviewer checks import boundaries,
  `decorder` compliance, and `wrapcheck` adherence across package boundaries.
- **Fixes adding new exported symbols**: The reviewer checks naming conventions,
  doc comments, and whether the symbol belongs in `pkg/` or `internal/`.
- **Fixes modifying CUE schemas**: The reviewer checks schema sync, field naming,
  and `BehavioralSync` test alignment.
- **Fixes adding `//nolint` directives**: The reviewer verifies the directive is
  justified and uses the correct linter name.

Do NOT spawn the code reviewer for trivial fixes (typos, nil checks, import fixes).
The overhead is not justified.

### Code Reviewer Prompt for Fix Verification

```
You are reviewing a proposed fix in the invowk Go project.

## Original Bug
{one-sentence description of the bug}

## Proposed Fix
{diff or description of changes}

## Your Task
Review the fix against project conventions:
1. Read `.agents/agents/code-reviewer.md` for the full review checklist
2. Check: decorder compliance, sentinel errors, wrapcheck, SPDX headers
3. Check: naming conventions match `.agents/rules/go-patterns.md`
4. Check: test changes follow `.agents/rules/testing.md`
5. Flag anything that would fail `make lint` or `make check-baseline`

## Output Format
- **Approval**: {approve | request changes}
- **Issues found**: {list, or "none"}
- **Suggestions**: {optional improvements}
```
