export const meta = {
  name: 'programme-change',
  description: 'One LushText programme change through its stages: spec, spec review, implementation, prepare, simplify, parallel review lenses, one independent apply agent with the gates, final review, merge and archive',
  whenToUse: 'LushText programme changes run by the coordinator; one run per change, one agent per stage; modes new, fast-lane, revise, continue and review-only. Rules: .agents/rules/parallel-agent-work.md "Running a change".',
  phases: [
    { title: 'Archive previous' },
    { title: 'Revision decision' },
    { title: 'Spec' },
    { title: 'Spec review' },
    { title: 'Implementation' },
    { title: 'Prepare' },
    { title: 'Simplify' },
    { title: 'Review lenses' },
    { title: 'Apply and gates' },
    { title: 'Merge and archive' },
  ],
}

// Arguments (all strings unless noted):
//   mode            'new' | 'fast-lane' | 'revise' | 'continue' | 'review-only'
//                   review-only starts at Prepare on an existing PR and branch (args.pr, args.worktree, args.branch)
//   dry_run         boolean, return the planned stages and stop before any agent runs
//   change          the OpenSpec change name (also the branch and worktree name)
//   branch          the change's branch when it differs from the change name
//   context         what the change is for (new, fast-lane, revise)
//   records         records the spec stage should read
//   programme_record  the docs/next/ record the archive stages update
//   pr              integer, the pull request when it already exists (required by revise and review-only)
//   worktree        absolute worktree path when it already exists (required by revise and review-only)
//   worktree_override  boolean, accept a worktree whose directory name is not the change name
//   revision        integer, the design revision this run writes (revise; 3 or more runs the decision first)
//   design_only     boolean, stop after the approved spec review (revise)
//   skip_impl       boolean, go straight to the stages after implementation (continue)
//   impl            object, the last implementation report (continue with skip_impl; review-only)
//   spec            object {worktree, change_name?, branch?, fast_lane?} (continue)
//   review_findings the spec review's findings (continue)
//   prev_report     the previous implementation round's report (continue)
//   rounds          integer, implementation rounds in continue mode (default 3)
//   measurements_queued, kani_dispatch  review-only: what implementation queued or dispatched
//   review_agent_type  agentType for the simplify, lens and apply agents (default: the workflow agent)
//   local           host details kept outside the repository: push command, gh account, anything host-specific
//   no_merge        when set (review-only: unless merge is true), the merge stage reviews and reports but does not merge; the text is the reason
//   prev_change, prev_pr, prev_tip, archive_extra  archive a merged change first (all three of the first are required together)
//   review_extra    extra instructions for the merge stage
//   merge           boolean, review-only: continue into the merge stage (default: stop before it)
//   fast_lane       review-only: 'eligible' when the change took the fast lane (continue carries it in args.spec)
//   guard_ack       string or array: '<token>' values a guard stop printed, accepting that one main-checkout change on resume
//   accept_empty_review  the reviewed HEAD an empty-review stop printed: accept that one empty code-review
// guard_ack and accept_empty_review are read by this script only and never reach a prompt.
// The rules every prompt points to live in .agents/rules/; nothing here restates them.

const A = args || {}
const MODES = ['new', 'fast-lane', 'revise', 'continue', 'review-only']
if (!MODES.includes(A.mode)) return { stopped: `unknown mode ${JSON.stringify(A.mode)}; use one of ${MODES.join(', ')}` }
if (!A.change) return { stopped: 'args.change is required' }

// ---------------------------------------------------------------------------
// Argument checks, all before the first agent (parallel-agent-work.md
// "Stages after implementation").
//
// A change or branch name is a lowercase name; anything git could resolve to
// main (an alias, a ref path, a revision expression or a commit hash) is not.
const NAME_RE = /^[a-z0-9][a-z0-9._-]*$/
function refProblem(what, ref) {
  const r = String(ref === undefined || ref === null ? '' : ref).trim()
  if (/^(main|master|head)$/i.test(r) || /^[0-9a-f]{7,40}$/.test(r) || !NAME_RE.test(r)) {
    return `refusing a main-branch alias or a name that is not a change branch as the ${what}: ${JSON.stringify(ref)} (a lowercase name of letters, digits, '.', '_' and '-')`
  }
  return null
}
// The worktree sits directly in .claude/worktrees/ and carries the change's
// name; args.worktree_override waives the name check alone.
function worktreeProblem(path) {
  const m = String(path || '').match(/\/\.claude\/worktrees\/([^/]+)\/?$/)
  if (!m || !NAME_RE.test(m[1])) return `the worktree is not a change worktree under .claude/worktrees/: ${path}`
  if (m[1] !== A.change && A.worktree_override !== true) {
    return `the worktree ${path} is not named after the change ${A.change}; pass args.worktree_override to accept it`
  }
  return null
}
const hasValue = value => value !== undefined && value !== null && value !== ''
// A PR number from n, "#n" or a pull URL; null when it is none of those.
function normalizePr(value) {
  if (!hasValue(value)) return null
  const m = String(value).trim().match(/^(?:#?(\d+)|https:\/\/github\.com\/[^/\s]+\/[^/\s]+\/pull\/(\d+)\/?)$/)
  const n = m ? Number(m[1] || m[2]) : NaN
  return Number.isInteger(n) && n > 0 ? n : null
}
function argProblem() {
  for (const [what, ref] of [['change', A.change], ['branch', A.branch], ['carried spec branch', A.spec && A.spec.branch], ['previous change', A.prev_change]]) {
    if (hasValue(ref)) { const p = refProblem(what, ref); if (p) return p }
  }
  if (hasValue(A.pr) && normalizePr(A.pr) === null) return `args.pr is not a PR number, #number or pull URL: ${JSON.stringify(A.pr)}`
  if (A.worktree) { const p = worktreeProblem(A.worktree); if (p) return p }
  if ((A.mode === 'review-only' || A.mode === 'revise') && (!normalizePr(A.pr) || !A.worktree)) {
    return `${A.mode} needs args.pr and args.worktree (an existing change)`
  }
  if (A.mode === 'continue') {
    if (!A.spec || !A.spec.worktree) return 'continue mode needs args.spec {worktree, change_name?, branch?, fast_lane?}'
    const p = worktreeProblem(A.spec.worktree); if (p) return p
  }
  if (A.mode === 'continue' && A.skip_impl && !A.impl) return 'skip_impl needs args.impl'
  if (A.impl && hasValue(A.impl.pr_number) && normalizePr(A.impl.pr_number) === null) return `args.impl.pr_number is not a PR: ${JSON.stringify(A.impl.pr_number)}`
  if (A.impl && normalizePr(A.pr) && normalizePr(A.impl.pr_number) && normalizePr(A.pr) !== normalizePr(A.impl.pr_number)) {
    return `args.pr #${normalizePr(A.pr)} and the carried implementation's PR #${normalizePr(A.impl.pr_number)} differ`
  }
  if ((A.mode === 'review-only' || (A.mode === 'continue' && A.skip_impl)) && A.impl && A.impl.tasks_complete !== true) {
    return 'the carried implementation report is not complete (impl.tasks_complete is not true); run an implementation round first'
  }
  if (hasValue(A.prev_change) && (!normalizePr(A.prev_pr) || !hasValue(A.prev_tip))) return 'archiving a previous change needs args.prev_change, args.prev_pr and args.prev_tip together'
  if (hasValue(A.guard_ack) && ![].concat(A.guard_ack).every(t => typeof t === 'string')) return 'args.guard_ack is a string or an array of strings'
  return null
}
const ARG_PROBLEM = argProblem()
if (ARG_PROBLEM) return { stopped: ARG_PROBLEM }

// ---------------------------------------------------------------------------
// Review lens selection: a pure function of the review-lenses flags, with its
// cases asserted before any agent runs (parallel-agent-work.md
// "Stages after implementation").
const PERF_LEAVES = ['gtk-responsiveness', 'gtk-perf-scale', 'gtk-perf-rust-optimize']
function selectLenses(flags) {
  const lenses = ['code-review']
  if (flags.data) lenses.push('data-safety')
  if (flags.performance) lenses.push(...PERF_LEAVES)
  if (flags.modules_moved && (flags.data || flags.performance)) lenses.push('rust-hex-arch')
  return lenses
}
const LENS_CASES = [
  [{ data: false, performance: false, modules_moved: false }, ['code-review']],
  [{ data: false, performance: false, modules_moved: true }, ['code-review']],
  [{ data: true, performance: false, modules_moved: false }, ['code-review', 'data-safety']],
  [{ data: false, performance: true, modules_moved: false }, ['code-review', ...PERF_LEAVES]],
  [{ data: true, performance: true, modules_moved: false }, ['code-review', 'data-safety', ...PERF_LEAVES]],
  [{ data: true, performance: true, modules_moved: true }, ['code-review', 'data-safety', ...PERF_LEAVES, 'rust-hex-arch']],
  [{ data: false, performance: true, modules_moved: true }, ['code-review', ...PERF_LEAVES, 'rust-hex-arch']],
]
for (const [flags, want] of LENS_CASES) {
  const got = selectLenses(flags)
  if (JSON.stringify(got) !== JSON.stringify(want)) {
    return { stopped: `selectLenses self-check failed for ${JSON.stringify(flags)}: ${JSON.stringify(got)}` }
  }
}

// ---------------------------------------------------------------------------
// The stages each mode runs, for dry_run and for the coordinator's ledger.
function plannedStages(a) {
  const after = ['Prepare', 'Simplify', 'Review lenses', 'Apply and gates']
  const mergeStage = a.mode === 'review-only' && !a.merge ? [] : ['Merge and archive']
  const head = []
  if (a.prev_change) head.push('Archive previous')
  if (a.mode === 'review-only') return [...head, ...after, ...mergeStage]
  if (a.mode === 'revise' && (a.revision || 0) >= 3) head.push('Revision decision')
  if (a.mode !== 'continue') {
    head.push('Spec')
    head.push(a.mode === 'fast-lane' ? 'Spec review (only when the spec stage finds the change not eligible)' : 'Spec review')
    if (a.design_only) return head
  }
  if (!(a.mode === 'continue' && a.skip_impl)) head.push('Implementation')
  return [...head, ...after, ...mergeStage]
}
if (A.dry_run) return { dry_run: true, mode: A.mode, change: A.change, stages: plannedStages(A) }

// ---------------------------------------------------------------------------
// Shared prompt parts. They name rule sections; they do not copy rule text.
const CHANGE = A.change
const ARG_PR = normalizePr(A.pr)
const LOCAL = A.local ? `Host details from the coordinator (args.local), which override nothing in the rules: ${A.local}` : 'No host details were passed (args.local); if a push or gh command needs one, stop and say so.'
const REVIEW_AGENT = A.review_agent_type ? { agentType: A.review_agent_type } : {}

function common(stage) {
  return `
Repository: the main checkout is the parent of \`git rev-parse --path-format=absolute --git-common-dir\`; call it <main>. H=<main>/.claude/handoffs, and this change's handoff directory is $H/${CHANGE}.
Before anything else, write this prompt to $H/${CHANGE}/brief-${stage}.md and use TMPDIR=$H/${CHANGE}/tmp/${stage} for every command; re-read the brief before your final check.
Follow, by section: parallel-agent-work.md "Running a change", "Workflow agents", "Host ownership", "CPU and memory", "Waiting"; verification-runs.md "Shared-machine agent work"; testing.md "Gate Order Before a Commit"; git.md "Commit Messages" (the trailer names the model you are running on) and "General".
Run every command in the foreground and wait for it to exit (a Bash timeout up to 600000 ms, or repeated foreground checks of a detached one); never return, or end your turn, while a command you started is still running.
Fetch with git fetch --prune. When origin/main has moved, merge it into the branch; if GitHub added a commit to origin/<branch> (say, through its Update branch button), merge origin/<branch> as well and keep that commit. Never rebase a branch that was pushed, and never push with --force or --force-with-lease (parallel-agent-work.md "Running a change" says what to do when a push is rejected).
A helper's empty output means nothing by itself: read its exit status, and treat any status the helper does not document as success as blocked.
Decide on your own, user experience first, and record each decision with its reason.
${LOCAL}`
}

const IMPL_RULES = `
Follow, by section: parallel-agent-work.md "Implementation slots and ordering" (scripts/impl-slots.sh acquire and release; release on every exit path), "Measurements" (drain first with scripts/quiet-gate.py; queue what cannot run, with the right Status), "Durability"; verification-runs.md "Kani" (which shards, and dispatching shard-budget runs once the final code is pushed, never watching them), "Proof strength", "GTK axioms", "Timing campaigns" (the verdict class); testing.md "Proof Obligations of a Change"; documentation.md "Mandatory Updates on Code Changes".
Commit and push per task group, non-visual groups first; files under the UI prefixes change in as few commits as possible, and the three smokes run once, right before that commit. Run only the tests a change affects while iterating, and the full gates once at the end; do not rerun a gate whose inputs did not change.
Leave the worktree clean and pushed when you return: the stages after you refuse uncommitted work.
Do not merge and do not wait for CI.`

// ---------------------------------------------------------------------------
// Main-checkout guard (parallel-agent-work.md "Workflow agents"): a small agent
// reads the main checkout, and this change's slot and merge-lock ownership,
// before and after each stage; the script compares.
const GUARD_SCHEMA = {
  type: 'object',
  properties: {
    status_sha256: { type: 'string', description: 'the 64-hex line printed after --SHA256--' },
    porcelain: { type: 'string', description: 'the lines printed after --PORCELAIN--, empty string if none' },
    head: { type: 'string' },
    branch: { type: 'string' },
    own_slot: { type: 'boolean', description: 'true when --OWN-SLOT-- printed yes' },
    own_merge_lock: { type: 'boolean', description: 'true when --OWN-MERGE-LOCK-- printed yes' },
  },
  required: ['status_sha256', 'porcelain', 'head', 'branch', 'own_slot', 'own_merge_lock'],
}
async function guardRead(label) {
  const reading = await agent(`Read-only guard reading. Run exactly these commands, in one Bash call, and nothing else:
M="$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")"; S="$(git -C "$M" status --porcelain)"; echo "--SHA256--"; printf '%s' "$S" | sha256sum | cut -d' ' -f1; echo "--PORCELAIN--"; printf '%s\\n' "$S"; echo "--HEAD--"; git -C "$M" rev-parse HEAD; echo "--BRANCH--"; git -C "$M" branch --show-current; echo "--OWN-SLOT--"; if [ -d "$M/.claude/handoffs/IMPL-SLOTS/${CHANGE}" ]; then echo yes; else echo no; fi; echo "--OWN-MERGE-LOCK--"; if [ "$(head -n1 "$M/.claude/handoffs/MERGE-LOCK/owner.txt" 2>/dev/null)" = "${CHANGE}" ]; then echo yes; else echo no; fi
Return each value as printed: the hash, the porcelain lines (an empty string when none), the HEAD hash, the branch name, and the two yes/no answers as booleans. Change nothing.`,
    { label: `guard:${label}`, phase: 'guard', effort: 'low', schema: GUARD_SCHEMA })
  return reading && /^[0-9a-f]{64}$/.test(reading.status_sha256 || '') ? reading : null
}
// A slot or merge lock this change gave up during a stage is benign; one it
// took and still holds (a review fork's acquire, a stage that died holding
// it) is a difference.
function sameReading(a, b) {
  return a && b && a.status_sha256 === b.status_sha256 && a.head === b.head && a.branch === b.branch &&
    !(b.own_slot && !a.own_slot) && !(b.own_merge_lock && !a.own_merge_lock)
}
// The acknowledgement a coordinator passes back (args.guard_ack) to accept one
// stage's main-checkout change on resume: keyed on the stage label and the
// whole after reading, so it accepts nothing else.
function ackToken(label, r) {
  return `${label}:${r.status_sha256}:${r.head}:${r.branch}:${r.own_slot ? 'slot' : 'no-slot'}:${r.own_merge_lock ? 'lock' : 'no-lock'}`
}
const ACKS = [].concat(A.guard_ack || [])
// Runs fn between two guard readings; returns {result} or {stopped}.
async function guarded(label, fn) {
  const before = await guardRead(`${label}:before`)
  if (!before) return { stopped: `guard reading before ${label} failed; nothing ran` }
  const result = await fn()
  const after = await guardRead(`${label}:after`)
  if (!after) return { stopped: `guard reading after ${label} failed; check the main checkout by hand`, result }
  if (!sameReading(before, after)) {
    const token = ackToken(label, after)
    if (ACKS.includes(token)) {
      log(`guard: the change during ${label} was acknowledged (${token})`)
      return { result, acknowledged: token }
    }
    return { stopped: `MAIN CHECKOUT CHANGED during ${label}: before ${JSON.stringify(before)}, after ${JSON.stringify(after)}. The run stops. The coordinator checks the difference; when it is not this run's doing, resume this run (same mode and arguments) with args.guard_ack including "${token}".`, result }
  }
  return { result }
}

const out = { mode: A.mode, change: CHANGE }

// ---------------------------------------------------------------------------
// Optional: archive a change merged earlier (openspec-archive-change).
if (A.prev_change) {
  phase('Archive previous')
  const g = await guarded('archive-previous', () => agent(`Archive the OpenSpec change ${A.prev_change}, merged to origin/main as PR #${normalizePr(A.prev_pr)} (tip ${A.prev_tip}), with the openspec-archive-change skill, in a fresh worktree from origin/main at <main>/.claude/worktrees/${A.prev_change}-archive.
Beyond the skill: record the merge and the archive in ${A.programme_record || 'the programme record the change names'}; run openspec validate --all and make check-policy; commit and push that one commit to main as parallel-agent-work.md "Merging" step 4 describes; then clean up as "Durability" describes. ${A.archive_extra || ''}
${common('archive-previous')}
Return the archive path, the commit, and which worktrees and branches you removed.`, { label: `archive:${A.prev_change}`, phase: 'Archive previous', effort: 'low' }))
  if (g.stopped) return { ...out, ...g }
  out.archive = g.result
  if (!g.result) return { ...out, stopped: 'archive agent died: check origin/main and the archive worktree by hand' }
}

// ---------------------------------------------------------------------------
// Design-loop limit (parallel-agent-work.md "Design revisions").
let spike = null
if (A.mode === 'revise' && (A.revision || 0) >= 3) {
  phase('Revision decision')
  const g = await guarded('revision-decision', () => agent(`An independent decision on the existing OpenSpec change ${CHANGE} (PR #${ARG_PR}, worktree ${A.worktree}): it has had at least two design revisions and is blocked again. You wrote none of its design or code. Read its design.md (the "Design revisions" list and the latest stage-4 verdict), the gates, the evidence and the attributions so far, and decide continue, re-scope or shelve, as parallel-agent-work.md "Design revisions" describes. Edit nothing in the repository. Write your decision with its reasons to $H/${CHANGE}/decision-${A.revision}.md; the coordinator records it in the ledger and tells the maintainer.
${common('revision-decision')}`, {
    label: `decision:${CHANGE}`, phase: 'Revision decision',
    schema: { type: 'object', properties: {
      decision: { type: 'string', enum: ['continue', 're-scope', 'shelve'] },
      reasons: { type: 'string' } }, required: ['decision', 'reasons'] },
  }))
  if (g.stopped) return { ...out, ...g }
  out.decision = g.result
  if (!g.result) return { ...out, stopped: 'decision agent failed' }
  if (g.result.decision === 'shelve') {
    return { ...out, stopped: 'shelved by the independent decision; mark it SHELVED in the ledger and the programme record (coordinator)' }
  }
  const gs = await guarded('spike', () => agent(`Attribution spike for the OpenSpec change ${CHANGE} before design revision ${A.revision} (decision: ${g.result.decision}; reasons: ${g.result.reasons}). One agent, and you edit no OpenSpec artifact. Find where the time or the failure goes, with figures, and write it to $H/${CHANGE}/spike-${A.revision}.md. Rounds that need timing follow parallel-agent-work.md "Measurements", or are queued. Worktree: ${A.worktree}.
${common('spike')}
Return the spike file path and a summary of its figures.`, { label: `spike:${CHANGE}`, phase: 'Revision decision' }))
  if (gs.stopped) return { ...out, ...gs }
  spike = gs.result
  out.spike = spike
}

// ---------------------------------------------------------------------------
// Spec and spec review.
const SPEC_SCHEMA = {
  type: 'object',
  properties: {
    decision: { type: 'string', enum: ['viable', 'not-viable'] },
    change_name: { type: 'string' }, worktree: { type: 'string' }, branch: { type: 'string' },
    fast_lane: { type: 'string', enum: ['eligible', 'not-eligible', 'not-applicable'] },
    evidence: { type: 'string' }, commits: { type: 'string' }, summary: { type: 'string' },
  },
  required: ['decision', 'evidence', 'summary'],
}
const REVIEW_SCHEMA = {
  type: 'object',
  properties: {
    verdict: { type: 'string', enum: ['approved', 'unsound'] },
    findings: { type: 'string' }, commits: { type: 'string' },
  },
  required: ['verdict', 'findings'],
}

// The branch and worktree every later stage works in; checked before any of them runs.
function specProblem(s) {
  return refProblem('spec branch', s.branch) || worktreeProblem(s.worktree)
}
let spec = null
let review = null
if (A.mode === 'review-only') {
  spec = { change_name: CHANGE, worktree: A.worktree, branch: A.branch || CHANGE, fast_lane: A.fast_lane }
  review = { findings: A.review_findings || '(review-only: the spec was reviewed earlier)' }
} else if (A.mode === 'continue') {
  spec = { ...A.spec, change_name: A.spec.change_name || CHANGE, branch: A.spec.branch || A.branch || CHANGE }
  review = { findings: A.review_findings || '' }
} else {
  phase('Spec')
  const fast = A.mode === 'fast-lane'
  const specPrompt = A.mode === 'revise'
    ? `Design revision ${A.revision || ''} of the existing OpenSpec change ${CHANGE} (PR #${ARG_PR}), in place, in its worktree ${A.worktree}, with the openspec-update-change skill. Do not create a new change, worktree or pull request. First merge origin/main into the branch (never rebase a pushed branch). Read the stage-4 verdict and findings in design.md and tasks.md. ${spike ? `Cite the attribution spike ${JSON.stringify(spike)}.` : ''} Add this revision to design.md's "Design revisions" list (parallel-agent-work.md "Design revisions"). Turn every stage-4 finding into a task with a failing-first test or a recorded refutation; keep checked boxes that stay true and uncheck the ones the revision voids. openspec validate ${CHANGE} --strict; commit and push. Return not-viable only when the evidence shows no mechanism can meet the bars.
Context: ${A.context || ''}`
    : `Write the OpenSpec change ${CHANGE} with the openspec-propose skill, in a new worktree <main>/.claude/worktrees/${CHANGE} on branch ${CHANGE} from origin/main.
Context: ${A.context || ''}
Records to read: ${A.records || 'the programme record and the rules the change touches'}.
Keep this stage bounded: read what the design needs, and measure only when a decision depends on a number. Decide first whether the change is worth doing as described; if not, write why to $H/${CHANGE}/not-viable.md, push nothing, and return not-viable (the coordinator records it in the programme record).
Beyond the skill: design.md (or, in the fast lane, proposal.md "Decisions") records the measured baseline and, for a change on a timed path, its verdict class (verification-runs.md "Timing campaigns"); tasks.md covers failing-first tests, axioms and samples for toolkit behaviour relied on, the Kani shards (verification-runs.md "Kani"), lane cases and docs. A change that edits parallel-agent-work.md or verification-runs.md carries the floor table ("Running a change"). openspec validate ${CHANGE} --strict. One commit; push the branch; no pull request yet.`
  const fastPrompt = fast ? `
FAST LANE (parallel-agent-work.md "Fast lane"): before writing, list every path the change will touch and run scripts/check-fast-lane.py --paths on that list from the worktree. If any path is not eligible, or a timing verdict is planned, return fast_lane "not-eligible" (the run then takes the full spec review). Otherwise write proposal.md with a "Decisions" section, a "Gate effect: none" section for every declare path, and your own spec-review checklist under "Self-review"; return fast_lane "eligible".` : ''
  const g = await guarded('spec', () => agent(`Stage 1 (spec) of the LushText programme change ${CHANGE}.
${specPrompt}${fastPrompt}
${common('spec')}`, { label: `spec:${CHANGE}`, phase: 'Spec', schema: SPEC_SCHEMA }))
  if (g.stopped) return { ...out, ...g }
  spec = g.result
  out.spec = spec
  if (!spec || spec.decision !== 'viable') return { ...out, stopped: 'spec stage: not viable, or the agent failed' }
  spec = { ...spec, worktree: spec.worktree || A.worktree, branch: spec.branch || A.branch || CHANGE, change_name: spec.change_name || CHANGE,
    fast_lane: fast ? spec.fast_lane : 'not-applicable' }
  const stageSpecProblem = specProblem(spec)
  if (stageSpecProblem) return { ...out, stopped: stageSpecProblem }

  const skipReview = fast && spec.fast_lane === 'eligible'
  if (skipReview) {
    review = { findings: 'fast lane: spec self-reviewed under proposal.md "Self-review"; the apply agent reviews the spec artifacts independently' }
    log('fast lane: no separate spec-review stage')
  } else {
    phase('Spec review')
    const gr = await guarded('spec-review', () => agent(`Stage 2 (spec review) of the OpenSpec change ${spec.change_name}, worktree ${spec.worktree}, branch ${spec.branch}. Another agent wrote it${A.mode === 'revise' ? ' (a design revision)' : ''}; review it independently against the code, the rules in .agents/rules/, the Kani shards and the GTK behaviour it relies on. The author's evidence: ${spec.evidence}
Check that the baseline is measured or reproducible, the approach sound and complete, the requirements testable failing-first, budgets and exceptions explicit, the verdict class stated for a timed path, the tasks complete and ordered so UI-prefix edits land together, no MODIFIED scenario renamed, and, for an edit of the change-delivery rules, the floor table (parallel-agent-work.md "Running a change"). openspec validate --strict must pass. Fix every confirmed issue in the artifacts in one commit and push. If the change is fundamentally unsound, return unsound with the reason rather than rewriting it into another change.
${common('spec-review')}`, { label: `spec-review:${CHANGE}`, phase: 'Spec review', schema: REVIEW_SCHEMA }))
    if (gr.stopped) return { ...out, ...gr }
    review = gr.result
    out.review = review
    if (!review || review.verdict !== 'approved') return { ...out, stopped: 'spec review: unsound, or the agent failed' }
  }
  if (A.design_only) return { ...out, stopped: 'design_only: stopped after the approved spec review' }
}
// Every mode's spec, from a stage or carried in args, passes the same checks.
const carriedSpecProblem = specProblem(spec)
if (carriedSpecProblem) return { ...out, stopped: carriedSpecProblem }
// The fast-lane instructions apply only once the spec stage found the change
// eligible, and stay with it when a resumed run carries that answer.
const FAST = spec.fast_lane === 'eligible'

// ---------------------------------------------------------------------------
// Implementation (openspec-apply-change), in rounds for continue mode.
const IMPL_SCHEMA = {
  type: 'object',
  properties: {
    tasks_complete: { type: 'boolean' },
    pr_number: { type: 'integer' },
    gates: { type: 'string', description: 'each local gate run on the final tree with its result' },
    report: { type: 'string', description: 'what shipped with before/after numbers, recorded exceptions, test/Kani/axiom counts, commits, open decisions' },
    unchecked_tasks: { type: 'string' },
    measurements_queued: { type: 'string', description: 'verdict campaigns appended to MEASURE-QUEUE.md, or empty' },
    kani_dispatch: { type: 'string', description: 'shard-budget run ids dispatched, or empty' },
    fast_lane_recheck: { type: 'string', description: 'fast lane only: output summary of check-fast-lane.py --base origin/main' },
  },
  required: ['tasks_complete', 'gates', 'report'],
}
let impl = null
if (A.mode === 'review-only') {
  impl = { ...(A.impl || {
    tasks_complete: true,
    report: A.prev_report || '(review-only: implemented earlier on this PR; read its body and tasks.md)',
    gates: '', measurements_queued: A.measurements_queued || '', kani_dispatch: A.kani_dispatch || '',
  }) }
  impl.pr_number = normalizePr(impl.pr_number) || ARG_PR
} else if (A.mode === 'continue' && A.skip_impl) {
  impl = { ...A.impl, pr_number: normalizePr(A.impl.pr_number) || ARG_PR }
} else {
  phase('Implementation')
  const rounds = A.mode === 'continue' ? (A.rounds || 3) : 1
  let prev = A.prev_report || ''
  for (let round = 1; round <= rounds; round++) {
    const knownPr = (impl && impl.pr_number) || ARG_PR
    const prNote = knownPr
      ? `PR #${knownPr} exists: push to its branch and update its body; do not open another.`
      : 'Open the pull request once the final code is pushed; its body lists the local evidence and ends with the Claude Code generated-with line.'
    const g = await guarded(`implementation-${round}`, () => agent(`Stage 3 (implementation${rounds > 1 ? `, round ${round} of ${rounds}` : ''}) of the OpenSpec change ${spec.change_name}, worktree ${spec.worktree}, branch ${spec.branch}. The spec was reviewed independently; findings: ${review.findings}
${prev ? `Previous round's report (continue from it; the worktree may hold its uncommitted work, which you keep): ${prev}` : ''}
Implement every unchecked tasks.md box with the openspec-apply-change skill: failing-first tests, axioms and samples, measurements, docs. The local gates are the merge evidence: make check, the rustdoc gate when doc comments changed, make test with no FLAKY line, the Kani shards the policy requires, make gtk-axioms when toolkit behaviour is relied on, the affected smokes and lane cases. Record each gate you run on the final tree with scripts/gate-trees.sh record --change ${CHANGE} (parallel-agent-work.md "Merging").
${FAST ? 'Fast lane: before the PR, run scripts/check-fast-lane.py --base origin/main in the worktree and report it as fast_lane_recheck; a not-eligible path means stop implementing and report it.' : ''}
${prNote}
${IMPL_RULES}
${common(`implementation-${round}`)}
tasks_complete is true only when every tasks.md box is checked (a verdict queued as READY counts, listed in measurements_queued) and every gate above ran on the final tree and passed.`,
      { label: `implement-${round}:${CHANGE}`, phase: 'Implementation', schema: IMPL_SCHEMA }))
    if (g.stopped) return { ...out, ...g }
    const result = g.result
    if (!result) return { ...out, impl, stopped: `implementation agent died in round ${round}; resume this run` }
    const stagePr = normalizePr(result.pr_number)
    if (ARG_PR && stagePr && stagePr !== ARG_PR) {
      return { ...out, impl: result, stopped: `the implementation stage reports PR #${stagePr} but args.pr is #${ARG_PR}; check which pull request is the change's` }
    }
    impl = { ...result, pr_number: stagePr || knownPr }
    if (impl.tasks_complete) break
    prev = JSON.stringify({ report: impl.report, gates: impl.gates, unchecked_tasks: impl.unchecked_tasks, measurements_queued: impl.measurements_queued, pr_number: impl.pr_number })
    log(`implementation round ${round} incomplete: ${(impl.unchecked_tasks || '').slice(0, 200)}`)
  }
  out.impl = impl
  if (!impl.tasks_complete || !impl.pr_number) return { ...out, stopped: 'implementation incomplete: ' + (impl.unchecked_tasks || 'no PR') }
}
const PR = impl.pr_number
if (!PR) return { ...out, stopped: 'no pull request: pass args.pr or a carried impl.pr_number' }
const WT = spec.worktree
const BRANCH = spec.branch

// ---------------------------------------------------------------------------
// Stages after implementation (parallel-agent-work.md "Stages after implementation").
//
// Prepare: the review stages start from a clean, pushed worktree on the PR's
// head, with origin/main merged and the branch tracking origin/<branch>.
phase('Prepare')
const gprep = await guarded('prepare', () => agent(`Prepare stage for the change ${spec.change_name}, PR #${PR}, worktree ${WT}, branch ${BRANCH}. You edit no file and resolve nothing. In this order:
1. ${WT} must appear in git -C <main> worktree list --porcelain, on branch ${BRANCH}, and git -C ${WT} status --porcelain must print nothing (untracked files included). Otherwise return blocked with what you saw: uncommitted work belongs to an implementation round, not to the review stages.
2. git -C ${WT} fetch --prune origin. gh pr view ${PR} --json headRefName,state must show head ${BRANCH} and state OPEN; otherwise return blocked.
3. When origin/${BRANCH} holds commits HEAD lacks, git -C ${WT} merge --no-edit origin/${BRANCH}. When origin/main is not an ancestor of HEAD, git -C ${WT} merge --no-edit origin/main. On any conflict: git -C ${WT} merge --abort, and return blocked naming the conflicted files (resolving them is implementation work, which the review stages must see).
4. git -C ${WT} push origin HEAD:${BRANCH} when HEAD is ahead of origin/${BRANCH}, then git -C ${WT} branch --set-upstream-to=origin/${BRANCH} ${BRANCH}. Check that git -C ${WT} rev-parse HEAD equals both git -C ${WT} rev-parse @{upstream} and the headRefOid of gh pr view ${PR}; otherwise return blocked.
Return ready with HEAD, or blocked with the reason.
${common('prepare')}`, { label: `prepare:${CHANGE}`, phase: 'Prepare',
  schema: { type: 'object', properties: {
    status: { type: 'string', enum: ['ready', 'blocked'] }, head: { type: 'string' }, reason: { type: 'string' } },
    required: ['status', 'reason'] } }))
if (gprep.stopped) return { ...out, ...gprep }
out.prepare = gprep.result
if (!gprep.result) return { ...out, stopped: 'prepare agent died; nothing after it ran' }
if (gprep.result.status !== 'ready') {
  return { ...out, stopped: `prepare: blocked (${gprep.result.reason}). Commit or resolve that work in an implementation round (mode continue), then rerun review-only.` }
}

phase('Simplify')
const gsim = await guarded('simplify', () => agent(`Stage /simplify of the change ${spec.change_name}, worktree ${WT}. First check that git -C ${WT} status --porcelain prints nothing; if it does, return blocked and change nothing.
Invoke the simplify skill with exactly this argument: ${WT}
Its scope is the branch's work in that worktree, and you name the base yourself rather than trusting the recipe's default: the committed work is git -C ${WT} diff origin/main...HEAD, and your own edits are git -C ${WT} diff HEAD. Run every command of its recipe with git -C ${WT} or absolute paths. Quality only. Run the tests of what you touched, and leave your edits uncommitted. Then save them, new files included: git -C ${WT} add --intent-to-add -- . && git -C ${WT} diff HEAD --binary > $H/${CHANGE}/simplify.patch. Say in your result that the skill ran single-pass.
${common('simplify')}`, { label: `simplify:${CHANGE}`, phase: 'Simplify', ...REVIEW_AGENT,
  schema: { type: 'object', properties: {
    status: { type: 'string', enum: ['ran', 'blocked'] },
    edited_files: { type: 'array', items: { type: 'string' } },
    tests_run: { type: 'string' }, patch_path: { type: 'string' }, summary: { type: 'string' } },
    required: ['status', 'edited_files', 'tests_run', 'summary'] } }))
if (gsim.stopped) return { ...out, ...gsim }
out.simplify = gsim.result
if (!gsim.result) return { ...out, stopped: 'simplify agent failed' }
if (gsim.result.status !== 'ran') return { ...out, stopped: `simplify: blocked (${gsim.result.summary})` }

// The scope reading: the lens flags from the helper (its exit status and raw
// output, parsed here), and what the code-review lens must cover.
phase('Review lenses')
const HEX40 = /^[0-9a-f]{40}$/
const gscope = await guarded('review-scope', () => agent(`Read-only scope reading. Run exactly these commands, in one Bash call, and nothing else:
H1="$(git -C ${WT} rev-parse HEAD)"; H2="$(gh pr view ${PR} --json headRefOid --jq .headRefOid)"; H3="$(git -C ${WT} rev-parse '@{upstream}')"; echo "--HEAD-- $H1"; if [ -n "$H1" ] && [ "$H1" = "$H2" ] && [ "$H1" = "$H3" ]; then echo "--SAME-- yes"; else echo "--SAME-- no ($H2 $H3)"; fi; echo "--COMMITTED-- $(git -C ${WT} diff --name-only origin/main...HEAD | wc -l)"; echo "--UNCOMMITTED-- $(git -C ${WT} status --porcelain --untracked-files=all | wc -l)"; F="$(${WT}/scripts/agent-topology.py review-lenses --base origin/main --flags-only)"; echo "--LENSES-EXIT $?--"; echo "--LENSES-- $F"
Return: head, the hash after --HEAD--; same_head, true when --SAME-- printed yes; committed_count and uncommitted_count, the numbers after --COMMITTED-- and --UNCOMMITTED--; lenses_exit, the number in --LENSES-EXIT; lenses_stdout, the text after --LENSES--, verbatim. Change nothing.`,
  { label: `scope:${CHANGE}`, phase: 'Review lenses', effort: 'low',
    schema: { type: 'object', properties: {
      head: { type: 'string' }, same_head: { type: 'boolean' },
      committed_count: { type: 'integer' }, uncommitted_count: { type: 'integer' },
      lenses_exit: { type: 'integer' }, lenses_stdout: { type: 'string' } },
      required: ['head', 'same_head', 'committed_count', 'uncommitted_count', 'lenses_exit', 'lenses_stdout'] } }))
if (gscope.stopped) return { ...out, ...gscope }
const scope = gscope.result
// Every stop from here to the apply commit leaves /simplify's edits uncommitted.
const LEFTOVERS = `The worktree still holds /simplify's uncommitted edits ($H/${CHANGE}/simplify.patch): a review-only run's prepare stage refuses them, so an implementation round (mode continue) commits or drops them first.`
if (!scope) return { ...out, stopped: `scope reading failed; no lens ran. ${LEFTOVERS}` }
if (scope.lenses_exit !== 0) {
  return { ...out, stopped: `review-lenses exited ${scope.lenses_exit}: ${String(scope.lenses_stdout).slice(0, 400)}. A failed selector is not "no domain lens": merge origin/main into the branch if its helper or skill policy predates the review lenses, then rerun. ${LEFTOVERS}` }
}
let flags = null
try { flags = JSON.parse(scope.lenses_stdout) } catch (e) { flags = null }
const FLAG_KEYS = ['data', 'performance', 'modules_moved']
if (!flags || FLAG_KEYS.some(k => typeof flags[k] !== 'boolean')) {
  return { ...out, stopped: `review-lenses flags malformed or missing: ${String(scope.lenses_stdout).slice(0, 400)}. ${LEFTOVERS}` }
}
if (scope.same_head !== true || !HEX40.test(scope.head || '')) {
  return { ...out, stopped: `the worktree HEAD ${scope.head} is not both the PR head and its upstream: the code-review lens would not review the committed work; rerun from prepare. ${LEFTOVERS}` }
}
const committedCount = Number.isInteger(scope.committed_count) ? scope.committed_count : -1
const uncommittedCount = Number.isInteger(scope.uncommitted_count) ? scope.uncommitted_count : -1
if (committedCount <= 0 || uncommittedCount < 0) return { ...out, stopped: `scope counts unusable (committed ${scope.committed_count}, uncommitted ${scope.uncommitted_count}): the branch needs commits ahead of origin/main. ${LEFTOVERS}` }
const LENS_HEAD = scope.head
const lenses = selectLenses(flags)
out.lenses = lenses
out.lens_flags = { data: flags.data, performance: flags.performance, modules_moved: flags.modules_moved }
log(`review lenses: ${lenses.join(', ')}`)

// The code-review lens: high <PR> covers the committed work (probes C2/C3),
// and high <worktree> on a branch tracking origin/<branch> at HEAD covers only
// the uncommitted /simplify edits (probe C4); a scope line per call lets the
// script check what each fork reviewed.
const CR_CALLS = uncommittedCount > 0 ? 2 : 1
function codeReviewPrompt(attempt) {
  return `Findings-only lens "code-review" (attempt ${attempt}) for the change ${spec.change_name}, PR #${PR}, worktree ${WT}, branch ${BRANCH} at ${LENS_HEAD}. Edit nothing, and run no repository script yourself.
Call 1: invoke the code-review skill with exactly this argument: high ${PR}
It reviews the pull request's committed work.
${CR_CALLS === 2 ? `Call 2: invoke it again with exactly this argument: high ${WT}
The branch tracks origin/${BRANCH} at HEAD, so this call reviews only the uncommitted /simplify edits ($H/${CHANGE}/simplify.patch).` : 'There are no uncommitted edits, so there is no second call.'}
Never add --fix and never pass a bare level.
For each call: the findings come back as text in the Skill result; when the result carries none (only "Skill execution completed"), read the fork's transcript, <session transcript dir>/subagents/agent-<agentId>.jsonl with the agentId of the Skill result, where the findings are the input of its last StructuredOutput call.
The fork may run repository scripts. Before call 1, note whether $H/IMPL-SLOTS/${CHANGE} exists; if it exists after the calls and did not before, run ${WT}/scripts/impl-slots.sh release ${CHANGE}. Never touch MERGE-LOCK or another change's slot. In the findings file, list under "Fork side effects:" every repository script the fork ran, or "none seen".
Write the findings to $H/${CHANGE}/review-findings-code-review.md. Your final text is the findings verbatim, then these lines, each alone on its line, in this order, with every <...> filled in:
- for each call, in call order, exactly one of these two lines, as written: skill_findings_source: result-text, or skill_findings_source: fork-transcript;
- CODE-REVIEW-SCOPE: pr #${PR} files=<n> lines=<n>
${CR_CALLS === 2 ? '- CODE-REVIEW-SCOPE: worktree base=<the base the fork said it diffed against, one word> files=<n> lines=<n>\n' : ''}- findings_count: <n>
- last, exactly one of these three lines, as written: CODE-REVIEW-STATUS: ok, or CODE-REVIEW-STATUS: empty, or CODE-REVIEW-STATUS: failed.
Write the lines themselves without the leading dash. The scope numbers are the files and changed lines each fork said it reviewed; when it did not say, count them from the base it named (gh pr diff ${PR} for call 1, git -C ${WT} diff <base> for call 2). findings_count is the number of findings of all calls together.
The status is ok when there are findings, empty when every call returned no findings over the scope it stated, and failed, with the reason on the line before it, when you could read neither source of a call.
${common(`lens-code-review-${attempt}`)}`
}
// The base a worktree-path review must name: the branch's own tip (probe C4),
// never main, whose range is the whole branch or more (probe P4).
function worktreeBaseOk(base, want) {
  const b = String(base).replace(/[`'".,;]+$/g, '').replace(/^[`'"]+/, '')
  return ['HEAD', '@{upstream}', '@{u}', `origin/${want.branch}`].includes(b) ||
    (/^[0-9a-f]{7,40}$/.test(b) && want.head.startsWith(b))
}
// Reads the lens's structured tail. Returns {status: ok | empty | failed, reason}.
function readCodeReview(text, want) {
  if (typeof text !== 'string') return { status: 'failed', reason: 'no text' }
  const lines = text.trim().split('\n').map(l => l.trim())
  const status = /^CODE-REVIEW-STATUS: (ok|empty|failed)$/.exec(lines.at(-1) || '')
  if (!status) return { status: 'failed', reason: 'the last line is not a CODE-REVIEW-STATUS line' }
  if (status[1] === 'failed') return { status: 'failed', reason: lines.at(-2) || 'the lens reported failed' }
  if (lines.slice(0, -1).some(l => /^CODE-REVIEW-STATUS:/.test(l))) return { status: 'failed', reason: 'more than one status line' }
  const sources = lines.filter(l => /^skill_findings_source: (result-text|fork-transcript)$/.test(l))
  if (sources.length !== want.calls) return { status: 'failed', reason: `${sources.length} skill_findings_source lines for ${want.calls} calls` }
  const prScopes = lines.map(l => /^CODE-REVIEW-SCOPE: pr #(\d+) files=(\d+) lines=(\d+)$/.exec(l)).filter(Boolean)
  const wtScopes = lines.map(l => /^CODE-REVIEW-SCOPE: worktree base=(\S+) files=(\d+) lines=(\d+)$/.exec(l)).filter(Boolean)
  if (prScopes.length !== 1) return { status: 'failed', reason: 'no single pr scope line' }
  if (Number(prScopes[0][1]) !== want.pr) return { status: 'failed', reason: `the review covered PR #${prScopes[0][1]}, not #${want.pr}` }
  if (Number(prScopes[0][2]) !== want.committed) return { status: 'failed', reason: `the PR review covered ${prScopes[0][2]} files, not the ${want.committed} committed ones` }
  if (wtScopes.length !== (want.calls === 2 ? 1 : 0)) return { status: 'failed', reason: `${wtScopes.length} worktree scope lines for ${want.calls} calls` }
  if (wtScopes.length && !worktreeBaseOk(wtScopes[0][1], want)) return { status: 'failed', reason: `the worktree review diffed against ${wtScopes[0][1]}, not HEAD or origin/${want.branch}, so it was not the uncommitted edits alone` }
  if (wtScopes.length && Number(wtScopes[0][2]) !== want.uncommitted) return { status: 'failed', reason: `the worktree review (base ${wtScopes[0][1]}) covered ${wtScopes[0][2]} files, not the ${want.uncommitted} uncommitted ones` }
  const counts = lines.map(l => /^findings_count: (\d+)$/.exec(l)).filter(Boolean)
  if (counts.length !== 1) return { status: 'failed', reason: 'no single findings_count line' }
  const count = Number(counts[0][1])
  const body = lines.filter(l => !/^(skill_findings_source:|CODE-REVIEW-SCOPE:|CODE-REVIEW-STATUS:|findings_count:)/.test(l) && !/^(`{3}\w*|[\[\]{},]*)$/.test(l))
  if (status[1] === 'ok') {
    return count > 0 && body.length > 0 ? { status: 'ok' } : { status: 'failed', reason: 'CODE-REVIEW-STATUS: ok over no findings' }
  }
  if (count !== 0) return { status: 'failed', reason: `CODE-REVIEW-STATUS: empty with findings_count ${count}` }
  const scoped = [...prScopes, ...wtScopes].every(m => Number(m[2]) > 0 && Number(m[3]) > 0)
  return scoped ? { status: 'empty', reason: `no findings over ${prScopes[0][2]} PR files${wtScopes.length ? ` and ${wtScopes[0][2]} uncommitted files` : ''}` }
    : { status: 'failed', reason: 'an empty review must quote a non-empty scope for every call' }
}
const CR_WANT = { pr: PR, committed: committedCount, uncommitted: uncommittedCount, calls: CR_CALLS, branch: BRANCH, head: LENS_HEAD }

const FINDINGS_SCHEMA = {
  type: 'object',
  properties: {
    status: { type: 'string', enum: ['ran', 'failed'], description: 'ran only when the skill loaded and its checklist was applied' },
    skill_ran: { type: 'string', description: 'the skill and mode that ran, e.g. "data-safety full audit, single pass"' },
    examined: { type: 'array', minItems: 1, items: { type: 'string' }, description: 'files read and checklist sections applied' },
    findings: { type: 'array', items: { type: 'object', properties: {
      severity: { type: 'string' }, file: { type: 'string' }, line: { type: 'integer' },
      summary: { type: 'string' }, failure_scenario: { type: 'string' } },
      required: ['severity', 'file', 'summary'] } },
    findings_file: { type: 'string' },
  },
  required: ['status', 'skill_ran', 'examined', 'findings', 'findings_file'],
}
function domainLensRan(r) {
  return !!r && r.status === 'ran' && Array.isArray(r.examined) && r.examined.some(e => typeof e === 'string' && e.trim()) && Array.isArray(r.findings)
}
function domainPrompt(lens) {
  const how = lens === 'data-safety'
    ? 'Run the data-safety skill by name for its full audit (explicit $data-safety), over the change\'s paths.'
    : lens === 'rust-hex-arch'
      ? 'Run the rust-hex-arch skill as a review of the modules this change adds, moves or splits.'
      : `Run the ${lens} leaf checklist by name (gtk-perf-review assigns it; you cannot delegate), over the paths in the gtk-perf-review scope.`
  return `Findings-only lens "${lens}" for the change ${spec.change_name}, PR #${PR}, worktree ${WT}. ${how}
The diff is git -C ${WT} diff origin/main...HEAD plus the uncommitted edits (git -C ${WT} diff HEAD); list the paths with git -C ${WT} diff --name-only origin/main...HEAD and git -C ${WT} status --porcelain --untracked-files=all. Read files with absolute paths under ${WT}. Edit nothing. Write the findings to $H/${CHANGE}/review-findings-${lens}.md.
Return status "ran" only when the skill loaded and you applied its checklist, with examined listing the files you read and the checklist sections you applied; when the skill did not load or you could not apply it, return status "failed" with the reason in skill_ran. A lens that did not run is never zero findings.
${common(`lens-${lens}`)}`
}
const glens = await guarded('review-lenses', async () => {
  const results = await parallel(lenses.map(lens => async () => {
    if (lens === 'code-review') {
      let text = await agent(codeReviewPrompt(1), { label: `lens:code-review:${CHANGE}`, phase: 'Review lenses', ...REVIEW_AGENT })
      let read = readCodeReview(text, CR_WANT)
      if (read.status === 'failed') {
        log(`code-review lens failed (${read.reason}); rerunning once`)
        text = await agent(codeReviewPrompt(2), { label: `lens:code-review-2:${CHANGE}`, phase: 'Review lenses', ...REVIEW_AGENT })
        read = readCodeReview(text, CR_WANT)
      }
      return { lens, ok: read.status === 'ok', status: read.status, reason: read.reason, text }
    }
    const r = await agent(domainPrompt(lens), { label: `lens:${lens}:${CHANGE}`, phase: 'Review lenses', ...REVIEW_AGENT, schema: FINDINGS_SCHEMA })
    return { lens, ok: domainLensRan(r), status: domainLensRan(r) ? 'ok' : 'failed', result: r }
  }))
  return results
})
if (glens.stopped) return { ...out, ...glens }
const lensResults = (glens.result || []).filter(Boolean)
out.lens_results = lensResults
const emptyReview = lensResults.find(r => r.lens === 'code-review' && r.status === 'empty')
if (emptyReview) {
  if (A.accept_empty_review === LENS_HEAD) {
    log(`code-review empty review accepted by the coordinator: ${emptyReview.reason}`)
    emptyReview.ok = true
  } else {
    return { ...out, stopped: `code-review returned an empty review of ${LENS_HEAD} (${emptyReview.reason}). An empty review is never passed silently: read $H/${CHANGE}/review-findings-code-review.md, and resume this run with args.accept_empty_review: "${LENS_HEAD}" only if that scope is right. ${LEFTOVERS}` }
  }
}
const failedLenses = lenses.filter(lens => !lensResults.some(r => r.lens === lens && r.ok))
if (failedLenses.length) {
  const reasons = lensResults.filter(r => !r.ok).map(r => `${r.lens}: ${r.reason || (r.result ? r.result.skill_ran : 'agent died')}`).join('; ')
  return { ...out, stopped: `review lens failed: ${failedLenses.join(', ')} (${reasons}); a failed lens is not zero findings. ${LEFTOVERS}` }
}

phase('Apply and gates')
const APPLY_SCHEMA = {
  type: 'object',
  properties: {
    verdict: { type: 'string', enum: ['ready', 'blocked', 'fast-lane-violation'] },
    applied: { type: 'string' }, rejected: { type: 'string', description: 'each rejected finding with its reason' },
    gates: { type: 'string', description: 'each gate, its gate-trees.log line or reuse' },
    head: { type: 'string', description: 'the full 40-hex HEAD after the push' },
    tree: { type: 'string', description: 'the full 40-hex HEAD^{tree} the final gates ran on' },
    kani_dispatch: { type: 'string', description: 'shard-budget runs dispatched again, or empty' },
    summary: { type: 'string' },
  },
  required: ['verdict', 'applied', 'rejected', 'gates', 'head', 'tree', 'summary'],
}
const lensDigest = lensResults.map(r => `--- ${r.lens} ---\n${r.text || JSON.stringify(r.result)}`).join('\n')
const gapply = await guarded('apply', () => agent(`Apply stage for the change ${spec.change_name}, PR #${PR}, worktree ${WT}. You wrote none of its code and none of the /simplify edits. You are the last agent that edits code before the merge.
The lenses reviewed HEAD ${LENS_HEAD} with /simplify's uncommitted edits ($H/${CHANGE}/simplify.patch), under the lens flags data=${flags.data}, performance=${flags.performance}, modules_moved=${flags.modules_moved}. First run ${WT}/scripts/agent-topology.py review-lenses --base origin/main yourself and read its exit status: return blocked when it fails or any of those three flags differs.
The lens findings (also in $H/${CHANGE}/review-findings-*.md):
${lensDigest}
Dedupe them, check each against the code, apply the confirmed ones in the worktree (failing-first where behaviour changes), and record each rejected one with its reason in $H/${CHANGE}/review-findings-applied.md, marking the ones a lens raised as correctness bugs. When files under the UI prefixes changed, run the three smokes once, right before the commit. Commit /simplify's edits and yours together, locally, before any long gate, so a stopped gate leaves nothing uncommitted. Then run every final gate one time on that tree, recording each with scripts/gate-trees.sh record --change ${CHANGE} (parallel-agent-work.md "Merging"; make check as --gate 'make check'); push at the end, and update the PR body.
When /simplify's edits or yours touch the harness inputs of a Kani shard (verification-runs.md "Kani"), run that shard as the rule says, and dispatch its budget run again once the push is done when implementation dispatched one (it dispatched: ${impl.kani_dispatch || 'none'}); report the run ids in kani_dispatch.
${FAST ? `Fast lane: you are also the independent reviewer of the spec artifacts (parallel-agent-work.md "Fast lane"). Re-run scripts/check-fast-lane.py --base origin/main and verify every "Gate effect: none" declaration against the diff, line by line. Return fast-lane-violation for a not-eligible path or a false or missing declaration.` : ''}
Return head (the full hash of HEAD after the push) and tree (the full hash of the HEAD^{tree} every final gate ran on), and blocked when a must-fix stays open.
${common('apply')}`, { label: `apply:${CHANGE}`, phase: 'Apply and gates', ...REVIEW_AGENT, schema: APPLY_SCHEMA }))
if (gapply.stopped) return { ...out, ...gapply }
const applied = gapply.result
out.apply = applied
if (!applied) return { ...out, stopped: `apply agent failed: check whether it committed /simplify's edits before rerunning. ${LEFTOVERS}` }
if (applied.verdict === 'fast-lane-violation') {
  phase('Spec review')
  const gr = await guarded('spec-review-after-violation', () => agent(`The change ${spec.change_name} (worktree ${WT}, PR #${PR}) left the fast lane: ${applied.summary}. Review its spec artifacts independently, as a full spec review (parallel-agent-work.md "Fast lane"), against the code and the rules; fix confirmed issues in one commit and push. Return unsound if it is.
${common('spec-review-after-violation')}`, { label: `spec-review:${CHANGE}`, phase: 'Spec review', schema: REVIEW_SCHEMA }))
  if (gr.stopped) return { ...out, ...gr }
  out.review_after_violation = gr.result
  return { ...out, stopped: 'fast-lane violation: full spec review done; resume with mode continue and skip_impl after reading it, with spec.fast_lane set to not-eligible' }
}
if (applied.verdict !== 'ready') return { ...out, stopped: `apply stage: ${applied.verdict} (${applied.summary})` }

// What the apply stage says it produced, read back independently and compared
// in the shell, not across two agents' copies: the merge stage reviews exactly
// this range and checks this tree.
const HEXISH = /^[0-9a-f]{7,40}$/
if (!HEXISH.test(applied.head || '') || !HEXISH.test(applied.tree || '')) {
  return { ...out, stopped: `the apply stage returned no commit and tree hashes (head ${JSON.stringify(applied.head)}, tree ${JSON.stringify(applied.tree)}); check the worktree and its gate-trees.log by hand` }
}
const gverify = await guarded('apply-verify', () => agent(`Read-only reading. Run exactly these commands, in one Bash call, and nothing else:
W=${WT}; H="$(git -C "$W" rev-parse HEAD)"; T="$(git -C "$W" rev-parse 'HEAD^{tree}')"; echo "--HEAD-- $H"; echo "--TREE-- $T"; echo "--CLEAN-- $([ -z "$(git -C "$W" status --porcelain)" ] && echo yes || echo no)"; echo "--HEAD-MATCH-- $([ "$(git -C "$W" rev-parse --verify --quiet '${applied.head}^{commit}')" = "$H" ] && echo yes || echo no)"; echo "--TREE-MATCH-- $([ "$(git -C "$W" rev-parse --verify --quiet '${applied.tree}^{tree}')" = "$T" ] && echo yes || echo no)"; echo "--PUSHED-- $([ "$(gh pr view ${PR} --json headRefOid --jq .headRefOid)" = "$H" ] && echo yes || echo no)"; echo "--ANCESTOR-- $(git -C "$W" merge-base --is-ancestor ${LENS_HEAD} HEAD && echo yes || echo no)"; "$W/scripts/gate-trees.sh" lookup "$W" --change ${CHANGE} --gate 'make check'; echo "--GATED-EXIT $?--"
Return head and tree, the hashes printed; clean, head_match, tree_match, pushed and lens_head_is_ancestor, true when the line printed yes; gated_exit, the number in --GATED-EXIT. Change nothing.`,
  { label: `verify:${CHANGE}`, phase: 'Apply and gates', effort: 'low',
    schema: { type: 'object', properties: {
      head: { type: 'string' }, tree: { type: 'string' }, clean: { type: 'boolean' }, head_match: { type: 'boolean' },
      tree_match: { type: 'boolean' }, pushed: { type: 'boolean' }, lens_head_is_ancestor: { type: 'boolean' },
      gated_exit: { type: 'integer' } },
      required: ['head', 'tree', 'clean', 'head_match', 'tree_match', 'pushed', 'lens_head_is_ancestor', 'gated_exit'] } }))
if (gverify.stopped) return { ...out, ...gverify }
const verified = gverify.result
out.apply_verify = verified
if (!verified) return { ...out, stopped: 'apply verification reading failed' }
const verifyProblems = []
if (verified.clean !== true) verifyProblems.push('the worktree is not clean')
if (verified.head_match !== true) verifyProblems.push(`HEAD is not the apply stage's ${applied.head}`)
if (verified.tree_match !== true) verifyProblems.push(`HEAD^{tree} is not the gated tree ${applied.tree}`)
if (verified.pushed !== true) verifyProblems.push('the PR head is not HEAD (not pushed)')
if (verified.lens_head_is_ancestor !== true) verifyProblems.push(`the reviewed HEAD ${LENS_HEAD} is not an ancestor of HEAD`)
if (verified.gated_exit !== 0) verifyProblems.push(`gate-trees.sh has no clean make check pass for this tree (lookup exit ${verified.gated_exit})`)
if (!HEX40.test(verified.head || '') || !HEX40.test(verified.tree || '')) verifyProblems.push('the reading has no full HEAD and tree hashes')
if (verifyProblems.length) return { ...out, stopped: `apply stage output does not match the worktree: ${verifyProblems.join('; ')}` }
const APPLY_RANGE = `${LENS_HEAD}..${verified.head}`

// ---------------------------------------------------------------------------
// Final review, merge and archive.
if (A.mode === 'review-only' && !A.merge) {
  return { ...out, verdict: 'ready, stopped before merge (review-only)', apply_range: APPLY_RANGE, tree: verified.tree }
}
phase('Merge and archive')
const MERGE_VERDICTS = ['merged', 'merged-archive-owed', 'ready', 'measurement-pending', 'merge-lock-stale', 'blocked']
const mergeNote = A.no_merge
  ? `DO NOT MERGE (coordinator: ${A.no_merge}). Do the review and the checks above, take no lock, then return the verdict ready, or blocked, with "${A.no_merge}" in the reason.`
  : `Merging: take MERGE-LOCK; make the origin/main merge of "Merging" step 2. If it conflicts, git -C ${WT} merge --abort, release MERGE-LOCK and return blocked: you resolve nothing. After a clean merge, review its diff against ${verified.head} as part of the range and return the final range and tree. Reuse gate records only for the identical tree (scripts/gate-trees.sh lookup --change ${CHANGE}); check that origin/main is an ancestor of HEAD before gh pr merge; read the Kani budget runs, never watch them, and follow the archive-owed path when they have not finished (parallel-agent-work.md "Merging"; verification-runs.md "Kani"). Archive with the openspec-archive-change skill and record it in ${A.programme_record || 'the programme record the change names'}. If another change has held MERGE-LOCK for more than 3 h, take nothing, break nothing, and return merge-lock-stale. After the origin/main merge, rerun the smokes only as "Durability" says. Release MERGE-LOCK on every exit path, then clean up as "Durability" describes.`
const gmerge = await guarded('merge', () => agent(`Final review, merge and archive stage for the change ${spec.change_name}, PR #${PR}, worktree ${WT}, branch ${BRANCH}. You edit no file and make no commit of your own; the only commit you may make is the origin/main merge below, and only when it applies without conflicts.
Implementation report: ${impl.report}
Measurements it queued: ${impl.measurements_queued || 'none'}. Kani budget runs dispatched: ${impl.kani_dispatch || 'none'}${applied.kani_dispatch ? `, and by the apply stage: ${applied.kani_dispatch}` : ''}.
Apply stage: ${applied.summary}; gates: ${applied.gates}
First check that git -C ${WT} rev-parse HEAD is ${verified.head} and HEAD^{tree} is ${verified.tree}, the tree the apply stage gated; otherwise return blocked.
Review exactly the apply stage's work, git -C ${WT} log -p ${APPLY_RANGE} (the /simplify edits, the applied fixes and any merge commit in that range); the lenses reviewed everything up to ${LENS_HEAD}. Then read $H/${CHANGE}/review-findings-applied.md and re-check every finding a lens raised as a correctness bug that the apply stage rejected: a wrong rejection is a confirmed defect.
"No findings" is not the merge test, since a review pass always returns unverified findings: block only on a confirmed defect the apply range introduced, a wrongly rejected correctness finding, or a defect in the push path, the protection of main or the guard of the main checkout (parallel-agent-work.md "Stages after implementation"). For any of those, return blocked with the defect and fix nothing: the coordinator reruns the review stages.
Check what the skills do not: every tasks.md box is checked; no verdict this change waits on is still queued (if one is, return measurement-pending); the data-safety audit ran when the lens flags owed it (${lenses.includes('data-safety') ? 'owed' : 'not owed'}); the docs follow documentation.md "Mandatory Updates on Code Changes".
${mergeNote}
Return one verdict of: ${MERGE_VERDICTS.join(', ')}; with its reason.
${A.review_extra || ''}
${common('merge')}`, { label: `merge:${CHANGE}`, phase: 'Merge and archive',
  schema: { type: 'object', properties: {
    verdict: { type: 'string', enum: MERGE_VERDICTS }, reason: { type: 'string' },
    merge_commit: { type: 'string' }, archive_commit: { type: 'string' },
    final_range: { type: 'string' }, final_tree: { type: 'string' },
    gates: { type: 'string' }, notes: { type: 'string' } },
    required: ['verdict', 'reason', 'notes'] } }))
if (gmerge.stopped) return { ...out, ...gmerge }
const merged = gmerge.result
out.merge = merged
if (!merged) return { ...out, stopped: 'merge agent died: check MERGE-LOCK, the PR state and the archive by hand' }
if (!MERGE_VERDICTS.includes(merged.verdict)) return { ...out, stopped: `merge stage returned an unknown verdict ${JSON.stringify(merged.verdict)}; check MERGE-LOCK, the PR state and the archive by hand` }
if (A.no_merge && /^merged/.test(merged.verdict)) return { ...out, stopped: `the merge stage reports ${merged.verdict} although args.no_merge was set; check the PR and main by hand` }
if (merged.verdict === 'blocked') return { ...out, stopped: `final review blocked: ${merged.reason}. Nothing was merged; fix it in an implementation round (mode continue), then rerun review-only.` }
if (merged.verdict === 'merged' && (!merged.merge_commit || !merged.archive_commit)) {
  return { ...out, stopped: `the merge stage reports merged without a merge commit and an archive commit (${JSON.stringify({ merge: merged.merge_commit, archive: merged.archive_commit })}); check by hand` }
}
out.verdict = merged.verdict
return out
