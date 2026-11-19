export const meta = {
  name: 'programme-change',
  description: 'One LushText programme change through its stages: spec, spec review, implementation, simplify, parallel review lenses, one independent apply agent with the gates, merge and archive',
  whenToUse: 'LushText programme changes run by the coordinator; one run per change, one agent per stage; modes new, fast-lane, revise, continue and review-only. Rules: .agents/rules/parallel-agent-work.md "Running a change".',
  phases: [
    { title: 'Archive previous' },
    { title: 'Revision decision' },
    { title: 'Spec' },
    { title: 'Spec review' },
    { title: 'Implementation' },
    { title: 'Simplify' },
    { title: 'Review lenses' },
    { title: 'Apply and gates' },
    { title: 'Merge and archive' },
  ],
}

// Arguments (all strings unless noted):
//   mode            'new' | 'fast-lane' | 'revise' | 'continue' | 'review-only'
//                   review-only starts at /simplify on an existing PR and branch (args.pr, args.worktree, args.branch)
//   dry_run         boolean, return the planned stages and stop before any agent runs
//   change          the OpenSpec change name (also the branch and worktree name)
//   context         what the change is for (new, fast-lane, revise)
//   records         records the spec stage should read
//   programme_record  the docs/next/ record the archive stages update
//   pr              integer, the pull request when it already exists
//   worktree        absolute worktree path when it already exists (revise, continue)
//   revision        integer, the design revision this run writes (revise; 3 or more runs the decision first)
//   design_only     boolean, stop after the approved spec review (revise)
//   skip_impl       boolean, go straight to the stages after implementation (continue)
//   impl            object, the last implementation report (continue with skip_impl)
//   spec            object {change_name, worktree, branch} (continue)
//   review_findings the spec review's findings (continue)
//   prev_report     the previous implementation round's report (continue)
//   rounds          integer, implementation rounds in continue mode (default 3)
//   review_agent_type  agentType for the simplify, lens and apply agents (default: the workflow agent)
//   local           host details kept outside the repository: push command, gh account, anything host-specific
//   no_merge        when set (review-only: unless merge is true), the merge stage reviews and reports but does not merge; the text is the reason
//   prev_change, prev_pr, prev_tip, archive_extra  archive a merged change first
//   review_extra    extra instructions for the merge stage
//   merge           boolean, review-only: continue into the merge stage (default: stop before it)
// The rules every prompt points to live in .agents/rules/; nothing here restates them.

const A = args || {}
const MODES = ['new', 'fast-lane', 'revise', 'continue', 'review-only']
if (!MODES.includes(A.mode)) return { stopped: `unknown mode ${JSON.stringify(A.mode)}; use one of ${MODES.join(', ')}` }
if (!A.change) return { stopped: 'args.change is required' }

// A change's branch is never main under any name git resolves to it.
function isMainRef(ref) {
  const r = String(ref || '').trim()
  return /^(main|master|HEAD)$/.test(r) || /^(heads|refs|origin|remotes|tags)\//.test(r)
}
// A PR number from n, "#n" or a pull URL; null when it is none of those.
function normalizePr(value) {
  if (value === undefined || value === null || value === '') return null
  const m = String(value).trim().match(/^(?:#?(\d+)|https:\/\/github\.com\/[^/\s]+\/[^/\s]+\/pull\/(\d+)\/?)$/)
  const n = m ? Number(m[1] || m[2]) : NaN
  return Number.isInteger(n) && n > 0 ? n : null
}
if (isMainRef(A.change) || isMainRef(A.branch)) return { stopped: `refusing a main-branch alias as the change or branch: ${A.change} / ${A.branch}` }
if (A.pr !== undefined && normalizePr(A.pr) === null) return { stopped: `args.pr is not a PR number, #number or pull URL: ${JSON.stringify(A.pr)}` }
if (A.worktree && !/\/\.claude\/worktrees\/[^/]+\/?$/.test(A.worktree)) return { stopped: `args.worktree must be a change worktree under .claude/worktrees/: ${A.worktree}` }
if (A.mode === 'review-only' && (!normalizePr(A.pr) || !A.worktree)) return { stopped: 'review-only needs args.pr and args.worktree' }
if (A.mode === 'continue' && A.skip_impl && !A.impl) return { stopped: 'skip_impl needs args.impl' }

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
  const after = ['Simplify', 'Review lenses', 'Apply and gates']
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
When origin/main has moved, merge it into the branch; never rebase a branch that was pushed, and never push with --force or --force-with-lease (parallel-agent-work.md "Running a change" says what to do when a push is rejected).
Decide on your own, user experience first, and record each decision with its reason.
${LOCAL}`
}

const IMPL_RULES = `
Follow, by section: parallel-agent-work.md "Implementation slots and ordering" (scripts/impl-slots.sh acquire and release; release on every exit path), "Measurements" (drain first with scripts/quiet-gate.py; queue what cannot run, with the right Status), "Durability"; verification-runs.md "Kani" (which shards, and dispatching shard-budget runs once the final code is pushed, never watching them), "Proof strength", "GTK axioms", "Timing campaigns" (the verdict class); testing.md "Proof Obligations of a Change"; documentation.md "Mandatory Updates on Code Changes".
Commit and push per task group, non-visual groups first; files under the UI prefixes change in as few commits as possible, and the three smokes run once, right before that commit. Run only the tests a change affects while iterating, and the full gates once at the end; do not rerun a gate whose inputs did not change.
Do not merge and do not wait for CI.`

// ---------------------------------------------------------------------------
// Main-checkout guard (parallel-agent-work.md "Workflow agents"): a small agent
// reads the main checkout before and after each stage; the script compares.
const GUARD_SCHEMA = {
  type: 'object',
  properties: {
    porcelain: { type: 'string', description: 'verbatim output of git status --porcelain, empty string if none' },
    head: { type: 'string' },
    branch: { type: 'string' },
  },
  required: ['porcelain', 'head', 'branch'],
}
async function guardRead(label) {
  return agent(`Read-only guard reading. Run exactly these commands, in one Bash call, and nothing else:
M="$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")"; git -C "$M" status --porcelain; echo "--HEAD--"; git -C "$M" rev-parse HEAD; echo "--BRANCH--"; git -C "$M" branch --show-current
Return the status output verbatim as porcelain (an empty string when it printed nothing), the HEAD hash and the branch name. Change nothing.`,
    { label: `guard:${label}`, phase: 'guard', effort: 'low', schema: GUARD_SCHEMA })
}
function sameReading(a, b) {
  return a && b && a.porcelain === b.porcelain && a.head === b.head && a.branch === b.branch
}
// Runs fn between two guard readings; returns {result} or {stopped}.
async function guarded(label, fn) {
  const before = await guardRead(`${label}:before`)
  if (!before) return { stopped: `guard reading before ${label} failed; nothing ran` }
  const result = await fn()
  const after = await guardRead(`${label}:after`)
  if (!after) return { stopped: `guard reading after ${label} failed; check the main checkout by hand`, result }
  if (!sameReading(before, after)) {
    return { stopped: `MAIN CHECKOUT CHANGED during ${label}: before ${JSON.stringify(before)}, after ${JSON.stringify(after)}. The run stops; the coordinator checks the difference and resumes with mode continue.`, result }
  }
  return { result }
}

const out = { mode: A.mode, change: CHANGE }

// ---------------------------------------------------------------------------
// Optional: archive a change merged earlier (openspec-archive-change).
if (A.prev_change) {
  phase('Archive previous')
  const g = await guarded('archive-previous', () => agent(`Archive the OpenSpec change ${A.prev_change}, merged to origin/main as PR #${A.prev_pr} (tip ${A.prev_tip}), with the openspec-archive-change skill, in a fresh worktree from origin/main at <main>/.claude/worktrees/${A.prev_change}-archive.
Beyond the skill: record the merge and the archive in ${A.programme_record || 'the programme record the change names'}; run openspec validate --all and make check-policy; commit and push that one commit to main as parallel-agent-work.md "Merging" step 4 describes; then clean up as "Durability" describes. ${A.archive_extra || ''}
${common('archive-previous')}
Return the archive path, the commit, and which worktrees and branches you removed.`, { label: `archive:${A.prev_change}`, phase: 'Archive previous', effort: 'low' }))
  if (g.stopped) return { ...out, ...g }
  out.archive = g.result
}

// ---------------------------------------------------------------------------
// Design-loop limit (parallel-agent-work.md "Design revisions").
let spike = null
if (A.mode === 'revise' && (A.revision || 0) >= 3) {
  phase('Revision decision')
  const g = await guarded('revision-decision', () => agent(`An independent decision on the existing OpenSpec change ${CHANGE} (PR #${ARG_PR}, worktree ${A.worktree}): it has had at least two design revisions and is blocked again. You wrote none of its design or code. Read its design.md (the "Design revisions" list and the latest stage-4 verdict), the gates, the evidence and the attributions so far, and decide continue, re-scope or shelve, as parallel-agent-work.md "Design revisions" describes. Edit nothing. Append your decision with its reasons to $H/PARALLEL-LEDGER.md for the maintainer.
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

let spec = null
let review = null
if (A.mode === 'review-only') {
  spec = { change_name: CHANGE, worktree: A.worktree, branch: A.branch || CHANGE }
  review = { findings: A.review_findings || '(review-only: the spec was reviewed earlier)' }
} else if (A.mode === 'continue') {
  spec = A.spec
  review = { findings: A.review_findings || '' }
  if (!spec || !spec.worktree) return { ...out, stopped: 'continue mode needs args.spec {change_name, worktree, branch}' }
} else {
  phase('Spec')
  const fast = A.mode === 'fast-lane'
  const specPrompt = A.mode === 'revise'
    ? `Design revision ${A.revision || ''} of the existing OpenSpec change ${CHANGE} (PR #${ARG_PR}), in place, in its worktree ${A.worktree}, with the openspec-update-change skill. Do not create a new change or worktree. First merge origin/main into the branch (never rebase a pushed branch). Read the stage-4 verdict and findings in design.md and tasks.md. ${spike ? `Cite the attribution spike ${JSON.stringify(spike)}.` : ''} Add this revision to design.md's "Design revisions" list (parallel-agent-work.md "Design revisions"). Turn every stage-4 finding into a task with a failing-first test or a recorded refutation; keep checked boxes that stay true and uncheck the ones the revision voids. openspec validate ${CHANGE} --strict; commit and push. Return not-viable only when the evidence shows no mechanism can meet the bars.
Context: ${A.context || ''}`
    : `Write the OpenSpec change ${CHANGE} with the openspec-propose skill, in a new worktree <main>/.claude/worktrees/${CHANGE} on branch ${CHANGE} from origin/main.
Context: ${A.context || ''}
Records to read: ${A.records || 'the programme record and the rules the change touches'}.
Keep this stage bounded: read what the design needs, and measure only when a decision depends on a number. Decide first whether the change is worth doing as described; if not, record why in the programme record (a docs-only commit to main) and return not-viable.
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
  spec = { ...spec, worktree: spec.worktree || A.worktree, branch: spec.branch || A.branch || CHANGE, change_name: spec.change_name || CHANGE }
  if (isMainRef(spec.branch)) return { ...out, stopped: `the spec stage returned a main-branch alias as the branch: ${spec.branch}` }
  if (!spec.worktree || !/\/\.claude\/worktrees\/[^/]+\/?$/.test(spec.worktree)) return { ...out, stopped: `the spec stage returned no change worktree: ${spec.worktree}` }

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
  impl = A.impl || {
    tasks_complete: true, pr_number: ARG_PR,
    report: A.prev_report || '(review-only: implemented earlier on this PR; read its body and tasks.md)',
    gates: '', measurements_queued: A.measurements_queued || '', kani_dispatch: A.kani_dispatch || '',
  }
  impl.pr_number = normalizePr(impl.pr_number) || ARG_PR
} else if (A.mode === 'continue' && A.skip_impl) {
  impl = { ...A.impl, pr_number: normalizePr(A.impl.pr_number) || ARG_PR }
} else {
  phase('Implementation')
  const rounds = A.mode === 'continue' ? (A.rounds || 3) : 1
  let prev = A.prev_report || ''
  for (let round = 1; round <= rounds; round++) {
    const prNote = ARG_PR
      ? `PR #${ARG_PR} exists: push to its branch and update its body; do not open another.`
      : 'Open the pull request once the final code is pushed; its body lists the local evidence and ends with the Claude Code generated-with line.'
    const g = await guarded(`implementation-${round}`, () => agent(`Stage 3 (implementation${rounds > 1 ? `, round ${round} of ${rounds}` : ''}) of the OpenSpec change ${spec.change_name}, worktree ${spec.worktree}, branch ${spec.branch}. The spec was reviewed independently; findings: ${review.findings}
${prev ? `Previous round's report (continue from it; the worktree may hold its uncommitted work, which you keep): ${prev}` : ''}
Implement every unchecked tasks.md box with the openspec-apply-change skill: failing-first tests, axioms and samples, measurements, docs. The local gates are the merge evidence: make check, the rustdoc gate when doc comments changed, make test with no FLAKY line, the Kani shards the policy requires, make gtk-axioms when toolkit behaviour is relied on, the affected smokes and lane cases. Record each gate you run on the final tree with scripts/gate-trees.sh record (parallel-agent-work.md "Merging").
${A.mode === 'fast-lane' ? 'Fast lane: before the PR, run scripts/check-fast-lane.py --base origin/main in the worktree and report it as fast_lane_recheck; a not-eligible path means stop implementing and report it.' : ''}
${prNote}
${IMPL_RULES}
${common(`implementation-${round}`)}
tasks_complete is true only when every tasks.md box is checked (a verdict queued as READY counts, listed in measurements_queued) and every gate above ran on the final tree and passed.`,
      { label: `implement-${round}:${CHANGE}`, phase: 'Implementation', schema: IMPL_SCHEMA }))
    if (g.stopped) return { ...out, ...g }
    impl = g.result
    if (!impl) return { ...out, stopped: `implementation agent died in round ${round}; resume this run` }
    impl.pr_number = normalizePr(impl.pr_number) || ARG_PR
    if (impl.tasks_complete) break
    prev = JSON.stringify({ report: impl.report, gates: impl.gates, unchecked_tasks: impl.unchecked_tasks, measurements_queued: impl.measurements_queued })
    log(`implementation round ${round} incomplete: ${(impl.unchecked_tasks || '').slice(0, 200)}`)
  }
  out.impl = impl
  if (!impl.tasks_complete || !impl.pr_number) return { ...out, stopped: 'implementation incomplete: ' + (impl.unchecked_tasks || 'no PR') }
}
const PR = impl.pr_number
const WT = spec.worktree

// ---------------------------------------------------------------------------
// Stages after implementation (parallel-agent-work.md "Stages after implementation").
phase('Simplify')
const gsim = await guarded('simplify', () => agent(`Stage /simplify of the change ${spec.change_name}, worktree ${WT}. Invoke the simplify skill with exactly this argument: ${WT}
Its scope is the branch's work in that worktree: the commits ahead of origin/main and anything uncommitted. Run every command of its recipe with git -C ${WT} or absolute paths. Quality only. Run the tests of what you touched, leave your edits uncommitted, and save them: git -C ${WT} diff > $H/${CHANGE}/simplify.patch. Say in your result that the skill ran single-pass.
${common('simplify')}`, { label: `simplify:${CHANGE}`, phase: 'Simplify', ...REVIEW_AGENT,
  schema: { type: 'object', properties: {
    edited_files: { type: 'array', items: { type: 'string' } },
    tests_run: { type: 'string' }, patch_path: { type: 'string' }, summary: { type: 'string' } },
    required: ['edited_files', 'tests_run', 'summary'] } }))
if (gsim.stopped) return { ...out, ...gsim }
out.simplify = gsim.result
if (!gsim.result) return { ...out, stopped: 'simplify agent failed' }

phase('Review lenses')
const gflags = await guarded('review-flags', () => agent(`In one Bash call, run: cd ${WT} && scripts/agent-topology.py review-lenses --base origin/main
Return its data, performance and modules_moved flags and its paths. Change nothing.`,
  { label: `flags:${CHANGE}`, phase: 'Review lenses', effort: 'low',
    schema: { type: 'object', properties: {
      data: { type: 'boolean' }, performance: { type: 'boolean' }, modules_moved: { type: 'boolean' },
      paths: { type: 'array', items: { type: 'string' } } },
      required: ['data', 'performance', 'modules_moved', 'paths'] } }))
if (gflags.stopped) return { ...out, ...gflags }
const FLAG_KEYS = ['data', 'performance', 'modules_moved']
if (!gflags.result || FLAG_KEYS.some(k => typeof gflags.result[k] !== 'boolean')) {
  return { ...out, stopped: `review-lenses flags malformed or missing: ${JSON.stringify(gflags.result)}` }
}
const lenses = selectLenses(gflags.result)
out.lenses = lenses
log(`review lenses: ${lenses.join(', ')}`)

function codeReviewPrompt(attempt) {
  return `Findings-only lens "code-review" (attempt ${attempt}) for the change ${spec.change_name}, PR #${PR}, worktree ${WT}.
Invoke the code-review skill with exactly this argument: high ${WT}
That target covers the branch's commits ahead of origin/main and the uncommitted /simplify edits ($H/${CHANGE}/simplify.patch). Never add --fix and never pass a bare level. Edit nothing.
The findings come back as text in the Skill result. When the result carries none (only "Skill execution completed"), read the fork's transcript: <session transcript dir>/subagents/agent-<agentId>.jsonl, where agentId is in the Skill result; the findings are the input of its last StructuredOutput call.
Write the findings to $H/${CHANGE}/review-findings-code-review.md. Your final text is the findings verbatim, then two lines. The first is exactly one of these two, as written, alone on its line:
skill_findings_source: result-text
skill_findings_source: fork-transcript
The last line is exactly:
CODE-REVIEW-STATUS: ok
A code-review high pass always returns findings, so an empty list is a failed read, not a clean review: report it as failed. If you could read neither, end instead with "CODE-REVIEW-STATUS: failed" and the reason on the line before it.
${common(`lens-code-review-${attempt}`)}`
}
const FINDINGS_SCHEMA = {
  type: 'object',
  properties: {
    skill_ran: { type: 'string', description: 'the skill and mode that ran, e.g. "data-safety full audit, single pass"' },
    findings: { type: 'array', items: { type: 'object', properties: {
      severity: { type: 'string' }, file: { type: 'string' }, line: { type: 'integer' },
      summary: { type: 'string' }, failure_scenario: { type: 'string' } },
      required: ['severity', 'file', 'summary'] } },
    findings_file: { type: 'string' },
  },
  required: ['skill_ran', 'findings', 'findings_file'],
}
function domainPrompt(lens) {
  const how = lens === 'data-safety'
    ? 'Run the data-safety skill by name for its full audit (explicit $data-safety), over the change\'s paths.'
    : lens === 'rust-hex-arch'
      ? 'Run the rust-hex-arch skill as a review of the modules this change adds, moves or splits.'
      : `Run the ${lens} leaf checklist by name (gtk-perf-review assigns it; you cannot delegate), over the paths in the gtk-perf-review scope.`
  return `Findings-only lens "${lens}" for the change ${spec.change_name}, PR #${PR}, worktree ${WT}. ${how}
The diff is git -C ${WT} diff origin/main...HEAD plus the uncommitted edits (git -C ${WT} diff HEAD). Paths: ${JSON.stringify(gflags.result.paths)}. Read files with absolute paths under ${WT}. Edit nothing. Write the findings to $H/${CHANGE}/review-findings-${lens}.md.
${common(`lens-${lens}`)}`
}
// The explicit status line is the check; the source line and the text
// heuristics are a backstop for a lens that printed "ok" over nothing.
function codeReviewOk(text) {
  if (typeof text !== 'string') return false
  const lines = text.trim().split('\n').map(l => l.trim())
  if (lines.at(-1) !== 'CODE-REVIEW-STATUS: ok') return false
  if (lines.some(l => l === 'CODE-REVIEW-STATUS: failed')) return false
  if (!lines.some(l => /^skill_findings_source: (result-text|fork-transcript)$/.test(l))) return false
  const body = lines.filter(l => !/^(skill_findings_source:|CODE-REVIEW-STATUS:)/.test(l) && !/^(`{3}\w*|[\[\]{},]*)$/.test(l))
  return body.length > 0 && !body.every(l => /^(\(none\)|none|no findings\.?|## summary)$/i.test(l))
}
const glens = await guarded('review-lenses', async () => {
  const results = await parallel(lenses.map(lens => async () => {
    if (lens === 'code-review') {
      let text = await agent(codeReviewPrompt(1), { label: `lens:code-review:${CHANGE}`, phase: 'Review lenses', ...REVIEW_AGENT })
      if (!codeReviewOk(text)) {
        log('code-review lens returned no findings source; rerunning once')
        text = await agent(codeReviewPrompt(2), { label: `lens:code-review-2:${CHANGE}`, phase: 'Review lenses', ...REVIEW_AGENT })
      }
      return { lens, ok: codeReviewOk(text), text }
    }
    const r = await agent(domainPrompt(lens), { label: `lens:${lens}:${CHANGE}`, phase: 'Review lenses', ...REVIEW_AGENT, schema: FINDINGS_SCHEMA })
    return { lens, ok: !!r, result: r }
  }))
  return results
})
if (glens.stopped) return { ...out, ...glens }
const lensResults = (glens.result || []).filter(Boolean)
out.lens_results = lensResults
const failedLenses = lenses.filter(lens => !lensResults.some(r => r.lens === lens && r.ok))
if (failedLenses.length) return { ...out, stopped: `review lens failed: ${failedLenses.join(', ')}; a failed lens is not zero findings` }

phase('Apply and gates')
const APPLY_SCHEMA = {
  type: 'object',
  properties: {
    verdict: { type: 'string', enum: ['ready', 'blocked', 'fast-lane-violation'] },
    applied: { type: 'string' }, rejected: { type: 'string', description: 'each rejected finding with its reason' },
    gates: { type: 'string', description: 'each gate, its gate-trees.log line or reuse' },
    tree: { type: 'string' }, commits: { type: 'string' }, summary: { type: 'string' },
  },
  required: ['verdict', 'applied', 'rejected', 'gates', 'summary'],
}
const lensDigest = lensResults.map(r => `--- ${r.lens} ---\n${r.text || JSON.stringify(r.result)}`).join('\n')
const gapply = await guarded('apply', () => agent(`Apply stage for the change ${spec.change_name}, PR #${PR}, worktree ${WT}. You wrote none of its code and none of the /simplify edits. You are the last agent that edits code before the merge.
The worktree holds /simplify's uncommitted edits ($H/${CHANGE}/simplify.patch). The lens findings (also in $H/${CHANGE}/review-findings-*.md):
${lensDigest}
Dedupe them, check each against the code, apply the confirmed ones in the worktree (failing-first where behaviour changes), and record each rejected one with its reason in $H/${CHANGE}/review-findings-applied.md. When files under the UI prefixes changed, run the three smokes once, right before the commit. Commit /simplify's edits and yours together, locally, before any long gate, so a stopped gate leaves nothing uncommitted. Then run every final gate one time on that tree, recording each with scripts/gate-trees.sh record (parallel-agent-work.md "Merging"); push at the end, and update the PR body.
${A.mode === 'fast-lane' ? `Fast lane: you are also the independent reviewer of the spec artifacts (parallel-agent-work.md "Fast lane"). Re-run scripts/check-fast-lane.py --base origin/main and verify every "Gate effect: none" declaration against the diff, line by line. Return fast-lane-violation for a not-eligible path or a false or missing declaration.` : ''}
Return blocked when a must-fix stays open.
${common('apply')}`, { label: `apply:${CHANGE}`, phase: 'Apply and gates', ...REVIEW_AGENT, schema: APPLY_SCHEMA }))
if (gapply.stopped) return { ...out, ...gapply }
const applied = gapply.result
out.apply = applied
if (!applied) return { ...out, stopped: 'apply agent failed' }
if (applied.verdict === 'fast-lane-violation') {
  phase('Spec review')
  const gr = await guarded('spec-review-after-violation', () => agent(`The change ${spec.change_name} (worktree ${WT}, PR #${PR}) left the fast lane: ${applied.summary}. Review its spec artifacts independently, as a full spec review (parallel-agent-work.md "Fast lane"), against the code and the rules; fix confirmed issues in one commit and push. Return unsound if it is.
${common('spec-review-after-violation')}`, { label: `spec-review:${CHANGE}`, phase: 'Spec review', schema: REVIEW_SCHEMA }))
  if (gr.stopped) return { ...out, ...gr }
  out.review_after_violation = gr.result
  return { ...out, stopped: 'fast-lane violation: full spec review done; resume with mode continue and skip_impl after reading it' }
}
if (applied.verdict !== 'ready') return { ...out, stopped: `apply stage: ${applied.verdict}` }

// ---------------------------------------------------------------------------
// Merge and archive.
if (A.mode === 'review-only' && !A.merge) {
  return { ...out, verdict: 'ready, stopped before merge (review-only)' }
}
phase('Merge and archive')
const mergeNote = A.no_merge
  ? `DO NOT MERGE (coordinator: ${A.no_merge}). Do the checks below, then return the verdict "ready, ${A.no_merge}".`
  : 'When every check holds, merge and archive as parallel-agent-work.md "Merging" describes.'
const gmerge = await guarded('merge', () => agent(`Merge and archive stage for the change ${spec.change_name}, PR #${PR}, worktree ${WT}, branch ${spec.branch}.
Implementation report: ${impl.report}
Measurements it queued: ${impl.measurements_queued || 'none'}. Kani budget runs dispatched: ${impl.kani_dispatch || 'none'}.
Apply stage: ${applied.summary}; gates: ${applied.gates}
Review the apply stage's commit only (git -C ${WT} show <its commits>; the lenses already reviewed the rest). A code-review pass always returns unverified findings, so "no findings" is not the merge test: merge unless it confirms a defect introduced by the apply commit, or a defect in the push path, the protection of main or the guard of the main checkout (parallel-agent-work.md "Stages after implementation"). Fix such a defect in one more commit and review only that commit.
Check what the skills do not: every tasks.md box is checked; no verdict this change waits on is still queued (if one is, return "ready, measurement pending"); the data-safety audit ran when the lens flags owed it (${lenses.includes('data-safety') ? 'owed' : 'not owed'}); the docs follow documentation.md "Mandatory Updates on Code Changes".
${mergeNote}
Merging: take MERGE-LOCK; reuse gate records only for the identical tree (scripts/gate-trees.sh lookup); check that origin/main is an ancestor of HEAD before gh pr merge; read the Kani budget runs, never watch them, and follow the archive-owed path when they have not finished (parallel-agent-work.md "Merging"; verification-runs.md "Kani"). Archive with the openspec-archive-change skill and record it in ${A.programme_record || 'the programme record the change names'}. If another change has held MERGE-LOCK for more than 3 h, take nothing, break nothing, and return "ready, merge lock stale". After the origin/main merge, rerun the smokes only as "Durability" says. Release MERGE-LOCK on every exit path, then clean up as "Durability" describes.
${A.review_extra || ''}
${common('merge')}`, { label: `merge:${CHANGE}`, phase: 'Merge and archive',
  schema: { type: 'object', properties: {
    verdict: { type: 'string' }, merge_commit: { type: 'string' }, archive_commit: { type: 'string' },
    gates: { type: 'string' }, notes: { type: 'string' } },
    required: ['verdict', 'notes'] } }))
if (gmerge.stopped) return { ...out, ...gmerge }
out.merge = gmerge.result
return out
