---
deferred_work_file: '{implementation_artifacts}/deferred-work.md'
specLoopIteration: 1
---

# Step 4: Review

## RULES

- YOU MUST ALWAYS SPEAK OUTPUT in your Agent communication style with the config `{communication_language}`
- Review subagents get NO conversation context.
- Do not mutate code, spec, deferred-work artifacts, or version control state until the human resolves the mandatory review gate.

## INSTRUCTIONS

Change `{spec_file}` status to `in-review` in the frontmatter before continuing.

### Pending Decision Resume

1. Read `review_decision_pending` from `{spec_file}` frontmatter.
2. If it is `true`, read the latest entry in `## Review Findings Log`.
3. Check whether the latest human message clearly resolves the pending decision recorded there.
   - If it does **not**, re-present the stored findings summary and HALT. Ask the human to provide the recorded clarification or explicit approval before continuing.
   - If it **does**, reuse the stored classification, do **not** rerun reviewers, do **not** append a new findings entry, and continue at `### Apply Approved Outcome`.

### Construct Diff

If no pending decision is active:

Read `{baseline_commit}` from `{spec_file}` frontmatter. If `{baseline_commit}` is missing or `NO_VCS`, use best effort to determine what changed. Otherwise, construct `{diff_output}` covering all changes — tracked and untracked — since `{baseline_commit}`.

Do NOT `git add` anything — this is read-only inspection.

### Review

If no pending decision is active:

1. Launch three subagents without conversation context.
2. If runtime or tool policy requires explicit user authorization before spawning those review subagents, HALT and ask the human for that authorization first. Do **not** inline the reviews, skip the reviews, or collapse the reviewer set.
3. If subagents are unavailable even after authorization, or the environment genuinely cannot launch them, generate three review prompt files in `{implementation_artifacts}` — one per reviewer role below — and HALT. Ask the human to run each in a separate session (ideally a different LLM) and paste back the findings.

- **Blind hunter** — receives `{diff_output}` only. No spec, no context docs, no project access. Invoke via the `bmad-review-adversarial-general` skill.
- **Edge case hunter** — receives `{diff_output}` and read access to the project. Invoke via the `bmad-review-edge-case-hunter` skill.
- **Acceptance auditor** — receives `{diff_output}`, `{spec_file}`, and read access to the project. Must also read the docs listed in `{spec_file}` frontmatter `context`. Checks for violations of acceptance criteria, rules, and principles from the spec and context docs.

### Classify

1. Deduplicate all review findings.
2. Classify each finding. The first three categories are **this story's problem** — caused or exposed by the current change. The last two are **not this story's problem**.
   - **intent_gap** — caused by the change; cannot be resolved from the spec because the captured intent is incomplete. Do not infer intent unless there is exactly one possible reading.
   - **bad_spec** — caused by the change, including direct deviations from spec. The spec should have been clear enough to prevent it. When in doubt between bad_spec and patch, prefer bad_spec — a spec-level fix is more likely to produce coherent code.
   - **patch** — caused by the change; trivially fixable without human input. Just part of the diff.
   - **defer** — pre-existing issue not caused by this story, surfaced incidentally by the review. Collect for later focused attention.
   - **reject** — noise. Drop silently. When unsure between defer and reject, prefer reject — only defer findings you are confident are real.
3. Determine the highest-priority category in cascading order: `intent_gap` > `bad_spec` > `patch` > `defer` > `reject` > `none`.

### Persist Findings And Stop

If no pending decision is active:

1. Set `review_decision_pending` to `true` in `{spec_file}` frontmatter.
2. Append a new entry to `## Review Findings Log` recording: timestamp, reviewer roles used, grouped findings by category, highest-priority category, and the exact human decision required next.
3. HALT and ask the human how to proceed before any further action:
   - If `intent_gap` findings exist: ask for the missing clarification(s). Explain that accepted clarifications will trigger a code revert and a return to `./step-02-plan.md`.
   - Else if `bad_spec` findings exist: ask for approval to amend the non-frozen spec sections and re-derive the implementation. Explain that approval will trigger a code revert, a spec change-log entry, and a return to `./step-03-implement.md`.
   - Else if `patch` findings exist: ask `[A] Apply the classified patch findings` | `[S] Stop for later`.
   - Else: ask `[C] Continue` | `[S] Stop for later`.
4. Do **not** apply patches, append defer items, revert code, amend spec, commit, or continue to `./step-05-present.md` until the human has answered the mandatory review gate.

### Apply Approved Outcome

1. Clear `review_decision_pending` in `{spec_file}` frontmatter before mutating anything else.
2. Process the stored classification in cascading order. Increment `{specLoopIteration}` on each loopback. If it exceeds 5, HALT and escalate to the human.
   - **intent_gap** — Root cause is inside `<frozen-after-approval>`. Revert code changes. Once the human resolves the missing intent, read fully and follow `./step-02-plan.md` to re-run steps 2–4.
   - **bad_spec** — Root cause is outside `<frozen-after-approval>`. Before reverting code: extract KEEP instructions for positive preservation (what worked well and must survive re-derivation). Revert code changes. Read the `## Spec Change Log` in `{spec_file}` and strictly respect all logged constraints when amending the non-frozen sections that contain the root cause. Append a new change-log entry recording: the triggering finding, what was amended, the known-bad state avoided, and the KEEP instructions. Read fully and follow `./step-03-implement.md` to re-derive the code, then this step will run again.
   - **patch** — Apply only the patch findings the human approved. Then process any `defer` or `reject` findings from the same stored classification.
   - **defer** — Append to `{deferred_work_file}`.
   - **reject** — Drop silently.
   - **none** — No further action inside this step.

## NEXT

Read fully and follow `./step-05-present.md`
