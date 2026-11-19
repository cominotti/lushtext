// SPDX-License-Identifier: GPL-3.0-or-later
//
// Stub-agent harness for programme-change.js, run by scripts/check-workflow-scripts.sh
// (`node programme-change.test.mjs <path to programme-change.js>`).
//
// It loads the script body the way the workflow harness does (top-level await
// and return inside an async function), replaces agent() with a responder keyed
// on each call's label, and checks the control flow of every mode: dry runs and
// bad arguments, the finished path, review-only, the prepare and scope checks,
// failed and empty lenses, the apply read-back, the final review, guard
// violations and acknowledgements, a malformed selector and the fast lane. It
// then disables the guard comparison in a copy of the source and requires the
// guard case to fail, so a harness that stopped checking the guard is caught.
//
// Every stage stub returns a value no other stage returns (spec worktree,
// branch and PR differ from the arguments), so a precedence bug, such as an
// argument overriding a stage's answer, shows as a wrong value downstream.

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const here = path.dirname(fileURLToPath(import.meta.url))
const scriptPath = process.argv[2] || path.join(here, 'programme-change.js')
const SOURCE = readFileSync(scriptPath, 'utf8').replace('export const meta = {', 'const meta = {')
const AsyncFunction = (async () => {}).constructor

const SHA = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
const READING = { status_sha256: SHA, porcelain: '', head: 'abc123', branch: 'main', own_slot: false, own_merge_lock: false }
// Both worktrees are named after the change; they differ in the checkout above them.
const SPEC_WT = '/repo-spec/.claude/worktrees/demo'
const ARG_WT = '/repo/.claude/worktrees/demo'
const FLAGS = { data: false, performance: false, modules_moved: false }
// Full hashes: the lens HEAD, and the apply stage's HEAD and gated tree.
const LH = '1'.repeat(40)
const H2 = '2'.repeat(40)
const T2 = '3'.repeat(40)
// GitHub's merge commit of the PR.
const MC = '4'.repeat(40)
const prOf = prompt => Number((/PR #(\d+)/.exec(prompt) || [])[1])

// The code-review lens's structured tail.
// count is the PR call's findings, wtCount the worktree call's (Fable F1).
function crText({ pr, files = 2, lines = 40, wt = null, count = 1, wtCount = 0, status = 'ok', sources = 1, body = '- a.rs:3 off by one' }) {
  const tail = [
    body,
    ...Array.from({ length: sources }, () => 'skill_findings_source: result-text'),
    `CODE-REVIEW-SCOPE: pr #${pr} files=${files} lines=${lines}`,
    ...(wt ? [`CODE-REVIEW-SCOPE: worktree base=${wt.base || 'HEAD'} files=${wt.files} lines=${wt.lines || 12}`] : []),
    `findings_count: pr=${count}${wt ? ` worktree=${wtCount}` : ''}`,
    `CODE-REVIEW-STATUS: ${status}`,
  ]
  return tail.join('\n')
}
function scopeWith(flags = FLAGS, extra = {}) {
  return () => ({ head: LH, same_head: true, committed_count: 2, uncommitted_count: 0,
    lenses_exit: 0, lenses_stdout: JSON.stringify(flags), ...extra })
}

const DEFAULTS = {
  guard: () => ({ ...READING }),
  archive: () => 'archived',
  decision: () => ({ decision: 'continue', reasons: 'stub' }),
  spike: () => 'spike file',
  spec: () => ({ decision: 'viable', change_name: 'demo', worktree: SPEC_WT, branch: 'demo-spec-branch', fast_lane: 'not-applicable', evidence: 'spec-evidence', summary: 'spec-summary' }),
  'spec-review': () => ({ verdict: 'approved', findings: 'none' }),
  implement: () => ({ tasks_complete: true, pr_number: '#11', gates: 'impl-gates', report: 'impl-report' }),
  prepare: () => ({ status: 'ready', head: 'h1', reason: '' }),
  simplify: () => ({ status: 'ran', edited_files: ['a.rs'], tests_run: 'simplify-tests', summary: 'simplify-summary' }),
  scope: scopeWith(),
  'lens:code-review': (_l, prompt) => crText({ pr: prOf(prompt) }),
  'lens:code-review-2': (_l, prompt) => crText({ pr: prOf(prompt), body: '- b.rs:9 leak' }),
  lens: () => ({ status: 'ran', skill_ran: 'stub', examined: ['crates/x.rs: checklist 1-5'], findings: [], findings_file: 'f.md' }),
  apply: () => ({ verdict: 'ready', applied: 'apply-applied', rejected: 'apply-rejected', gates: 'apply-gates', head: H2, tree: T2, summary: 'apply-summary' }),
  verify: () => ({ head: H2, tree: T2, clean: true, head_match: true, tree_match: true, pushed: true, lens_head_is_ancestor: true, gated_exit: 0 }),
  merge: () => ({ verdict: 'merged', reason: 'merged', merge_commit: MC, archive_commit: 'a1', final_tree: T2, notes: 'n' }),
  postmerge: () => postmergeReading(),
}
// The post-merge reading (Fable F6): the merge commit's tree, whether it is on
// main, whether the origin/main merge brought commits in, UI-prefix overlap,
// and one lookup exit per gate, printed by the shell.
function postmergeReading({ tree = T2, treeMatch = true, applyTree = true, onMain = true, mergedIn = false, ui = false, exits = {} } = {}) {
  const gates = ['make check', 'make test', 'make visual-geometry-smoke', 'make accessibility-smoke', 'make visual-smoke']
  return { merge_tree: tree, tree_match: treeMatch, apply_tree: applyTree, on_main: onMain, merged_in: mergedIn, ui_line: ui === true ? 'yes' : ui === false ? 'no' : ui,
    gates_stdout: gates.map(g => `--GATE-- ${g in exits ? exits[g] : (g.endsWith('smoke') ? 1 : 0)} ${g}`).join('\n') }
}

function kindOf(label) {
  if (label.startsWith('guard:')) return 'guard'
  if (label.startsWith('lens:code-review-2:')) return 'lens:code-review-2'
  if (label.startsWith('lens:code-review:')) return 'lens:code-review'
  if (label.startsWith('lens:')) return 'lens'
  if (label.startsWith('implement-')) return 'implement'
  return label.split(':')[0]
}

let activeSource = SOURCE
// replay: the calls of an earlier run, [{key, result}]; the longest unchanged
// prefix of (prompt, opts) is answered from it and every call after the first
// changed one runs live, as the Workflow runtime resumes (probes C5, C6).
async function run(args, overrides = {}, source = activeSource, replay = null) {
  const calls = []
  const prompts = {}
  const record = []
  const live = []
  let diverged = replay === null
  const agent = async (prompt, opts = {}) => {
    const label = opts.label || ''
    calls.push(label)
    prompts[label] = prompt
    const key = JSON.stringify([prompt, opts])
    const at = record.length
    record.push({ key, result: undefined })
    if (!diverged && at < replay.length && replay[at].key === key) {
      record[at].result = replay[at].result
      return replay[at].result
    }
    diverged = true
    live.push(label)
    const kind = kindOf(label)
    const fn = overrides[label] || overrides[kind] || DEFAULTS[kind]
    if (!fn) throw new Error(`no stub for agent label ${label}`)
    const result = await fn(label, prompt, opts)
    record[at].result = result
    return result
  }
  const parallel = async thunks => Promise.all(thunks.map(t => t().catch(() => null)))
  const body = new AsyncFunction('args', 'agent', 'parallel', 'pipeline', 'phase', 'log', 'budget', 'workflow', source)
  const result = await body(args, agent, parallel, undefined, () => {}, () => {}, { total: null, spent: () => 0, remaining: () => Infinity }, undefined)
  return { result, calls, prompts, record, live, work: calls.filter(l => !l.startsWith('guard:')) }
}

function check(cond, message) {
  if (!cond) throw new Error(message)
}
const has = (s, needle) => typeof s === 'string' && s.includes(needle)

const BASE = { change: 'demo', local: 'stub host details' }
const RO = { ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT }
const CARRIED_SPEC = { change_name: 'demo', worktree: ARG_WT, branch: 'demo' }
const CASES = {
  async 'bad mode stops before any agent'() {
    const r = await run({ ...BASE, mode: 'later' })
    check(has(r.result.stopped, 'unknown mode') && r.calls.length === 0, JSON.stringify(r))
  },
  async 'missing change stops'() {
    const r = await run({ mode: 'new' })
    check(has(r.result.stopped, 'args.change') && r.calls.length === 0, JSON.stringify(r))
  },
  async 'review-only without pr or worktree stops'() {
    const r = await run({ ...BASE, mode: 'review-only', worktree: ARG_WT })
    check(has(r.result.stopped, 'review-only needs') && r.calls.length === 0, JSON.stringify(r))
  },
  async 'main-branch aliases are refused as the change or branch'() {
    for (const ref of ['main', 'master', 'HEAD', '@', 'heads/main', 'refs/heads/main', 'origin/main', 'remotes/origin/main', 'tags/v1',
      'main~0', 'main^{}', 'Main', '761866ff', '761866ff6e1b2d9c4a3f0e8d7c6b5a4938271605']) {
      const asChange = await run({ ...BASE, change: ref, mode: 'new' })
      const asBranch = await run({ ...RO, branch: ref })
      check(has(asChange.result.stopped, 'main-branch alias') && asChange.calls.length === 0, `${ref} as change: ${JSON.stringify(asChange.result)}`)
      check(has(asBranch.result.stopped, 'main-branch alias') && asBranch.calls.length === 0, `${ref} as branch: ${JSON.stringify(asBranch.result)}`)
    }
    const fromSpec = await run({ ...BASE, mode: 'new' }, { spec: () => ({ ...DEFAULTS.spec(), branch: 'origin/main' }) })
    check(has(fromSpec.result.stopped, 'main-branch alias') && !fromSpec.work.some(l => l.startsWith('spec-review:')), JSON.stringify(fromSpec.result))
  },
  async 'a worktree outside .claude/worktrees is refused'() {
    const r = await run({ ...RO, worktree: '/repo' })
    check(has(r.result.stopped, 'change worktree') && r.calls.length === 0, JSON.stringify(r.result))
    const s = await run({ ...BASE, mode: 'new' }, { spec: () => ({ ...DEFAULTS.spec(), worktree: '/repo' }) })
    check(has(s.result.stopped, 'change worktree'), JSON.stringify(s.result))
  },
  async 'S8: a worktree not named after the change is refused unless overridden'() {
    const r = await run({ ...RO, worktree: '/other/repo/.claude/worktrees/unrelated-change' })
    check(has(r.result.stopped, 'not named after the change') && r.calls.length === 0, JSON.stringify(r.result))
    const s = await run({ ...BASE, mode: 'new' }, { spec: () => ({ ...DEFAULTS.spec(), worktree: '/repo/.claude/worktrees/other' }) })
    check(has(s.result.stopped, 'not named after the change'), JSON.stringify(s.result))
    const o = await run({ ...RO, worktree: '/repo/.claude/worktrees/demo-2', worktree_override: true })
    check(!o.result.stopped && o.result.verdict, JSON.stringify(o.result))
    const p = await run(RO)
    check(has(p.prompts['prepare:demo'], 'worktree list --porcelain'), 'the prepare stage does not check the worktree list')
  },
  async 'PR values are normalized from n, #n and pull URLs'() {
    for (const value of [7, '7', '#7', 'https://github.com/o/r/pull/7', 'https://github.com/o/r/pull/7/']) {
      const r = await run({ ...RO, pr: value, merge: true })
      check(has(r.prompts['merge:demo'], 'PR #7,') && has(r.prompts['apply:demo'], 'PR #7,'), `${JSON.stringify(value)}: ${r.prompts['merge:demo']}`)
    }
    for (const bad of ['seven', '#', 'https://example.org/pull/7', 'https://example.org/o/r/pull/7', '0', '-3']) {
      const r = await run({ ...RO, pr: bad })
      check(has(r.result.stopped, 'not a PR number') && r.calls.length === 0, `${bad}: ${JSON.stringify(r.result)}`)
    }
    const s = await run({ ...BASE, mode: 'continue', skip_impl: true, spec: CARRIED_SPEC,
      impl: { tasks_complete: true, pr_number: '#12', report: 'carried-report', gates: '' } })
    check(has(s.prompts['merge:demo'], 'PR #12,') && has(s.prompts['merge:demo'], 'carried-report'), 'a carried implementation keeps its PR')
  },
  async 'S4: a stage PR that differs from args.pr stops the run'() {
    const c = await run({ ...BASE, mode: 'continue', spec: CARRIED_SPEC, pr: 9 },
      { implement: () => ({ ...DEFAULTS.implement(), pr_number: 'https://github.com/o/r/pull/12' }) })
    check(has(c.result.stopped, 'PR #12 but args.pr is #9') && !c.work.some(l => l.startsWith('prepare:')), JSON.stringify(c.result))
    const s = await run({ ...BASE, mode: 'continue', skip_impl: true, pr: 9, spec: CARRIED_SPEC,
      impl: { tasks_complete: true, pr_number: '#12', report: 'r', gates: '' } })
    check(has(s.result.stopped, 'differ') && s.calls.length === 0, JSON.stringify(s.result))
  },
  async 'stage answers take precedence over arguments downstream'() {
    const r = await run({ ...BASE, mode: 'new', worktree: ARG_WT, branch: 'demo-arg-branch' })
    check(has(r.prompts['simplify:demo'], SPEC_WT) && !has(r.prompts['simplify:demo'], ARG_WT), 'simplify uses the spec worktree')
    check(has(r.prompts['merge:demo'], 'branch demo-spec-branch') && has(r.prompts['merge:demo'], 'PR #11,'), r.prompts['merge:demo'])
    check(has(r.prompts['merge:demo'], 'impl-report') && has(r.prompts['merge:demo'], 'apply-gates'), 'merge sees the implementation and apply reports')
    check(has(r.prompts['apply:demo'], 'a.rs:3 off by one'), 'apply sees the lens findings')
    check(has(r.prompts['spec-review:demo'], 'spec-evidence'), 'spec review sees the spec evidence')
  },
  async 'continue with skip_impl and no impl stops'() {
    const r = await run({ ...BASE, mode: 'continue', skip_impl: true, spec: { worktree: ARG_WT } })
    check(has(r.result.stopped, 'skip_impl needs') && r.calls.length === 0, JSON.stringify(r))
  },
  async 'dry runs plan each mode without agents'() {
    const plans = {}
    for (const mode of ['new', 'fast-lane', 'revise', 'continue', 'review-only']) {
      const r = await run({ ...BASE, mode, pr: 7, worktree: ARG_WT, spec: CARRIED_SPEC, dry_run: true })
      check(r.calls.length === 0 && r.result.dry_run === true, `${mode}: ${JSON.stringify(r)}`)
      plans[mode] = r.result.stages
    }
    check(plans.new[0] === 'Spec' && plans.new.includes('Spec review') && plans.new.at(-1) === 'Merge and archive', JSON.stringify(plans.new))
    check(!plans.continue.includes('Spec') && plans.continue[0] === 'Implementation', JSON.stringify(plans.continue))
    check(plans['review-only'][0] === 'Prepare' && plans['review-only'][1] === 'Simplify' && !plans['review-only'].includes('Merge and archive'), JSON.stringify(plans['review-only']))
    const merging = await run({ ...RO, merge: true, dry_run: true })
    check(merging.result.stages.at(-1) === 'Merge and archive', JSON.stringify(merging.result))
    const decided = await run({ ...BASE, mode: 'revise', revision: 3, pr: 7, worktree: ARG_WT, dry_run: true })
    check(decided.result.stages[0] === 'Revision decision', JSON.stringify(decided.result))
  },
  async 'new mode finalizes through the merge stage'() {
    const r = await run({ ...BASE, mode: 'new' })
    check(r.result.merge && r.result.verdict === 'merged' && !r.result.stopped, JSON.stringify(r.result))
    const order = ['spec:', 'spec-review:', 'implement-1:', 'prepare:', 'simplify:', 'scope:', 'lens:code-review:', 'apply:', 'verify:', 'merge:', 'postmerge:']
    let at = -1
    for (const prefix of order) {
      const next = r.work.findIndex((l, i) => i > at && l.startsWith(prefix))
      check(next > at, `stage ${prefix} missing or out of order in ${JSON.stringify(r.work)}`)
      at = next
    }
    check(r.calls.filter(l => l.startsWith('guard:')).length >= 2 * (order.length - 1), 'every stage is guarded')
  },
  async 'review-only starts at prepare and stops before merge'() {
    const r = await run(RO)
    check(r.work[0].startsWith('prepare:') && r.work[1].startsWith('simplify:'), JSON.stringify(r.work))
    check(!r.work.some(l => /^(spec|implement|merge)/.test(l)), JSON.stringify(r.work))
    check(has(r.result.verdict, 'stopped before merge') && r.result.apply_range === `${LH}..${H2}`, JSON.stringify(r.result))
    const m = await run({ ...RO, merge: true })
    check(m.work.at(-2).startsWith('merge:') && m.work.at(-1).startsWith('postmerge:') && m.result.merge, JSON.stringify(m.work))
  },
  // Fable review M1: the committed work is reviewed through the PR.
  async 'M1: the code-review lens reviews the PR, and the worktree only for uncommitted edits'() {
    const r = await run(RO)
    const p = r.prompts['lens:code-review:demo']
    check(has(p, 'argument: high 7') && !has(p, `argument: high ${ARG_WT}`), `committed work only: ${p}`)
    const u = await run(RO, {
      scope: scopeWith(FLAGS, { uncommitted_count: 1 }),
      'lens:code-review': (_l, prompt) => crText({ pr: prOf(prompt), sources: 2, wt: { files: 1 } }),
    })
    const q = u.prompts['lens:code-review:demo']
    check(has(q, 'argument: high 7') && has(q, `argument: high ${ARG_WT}`) && !u.result.stopped, `two calls: ${JSON.stringify(u.result.stopped)}`)
  },
  async 'M1: a review whose scope is not the PR or the uncommitted edits fails'() {
    for (const [scope, text] of [
      [scopeWith(), (_l, p) => crText({ pr: prOf(p), files: 25 })],
      [scopeWith(), (_l, p) => crText({ pr: 8 })],
      [scopeWith(FLAGS, { uncommitted_count: 1 }), (_l, p) => crText({ pr: prOf(p), sources: 2, wt: { base: 'main', files: 175 } })],
      // A matching count over the wrong base is still the wrong review (probe P4).
      [scopeWith(FLAGS, { uncommitted_count: 1 }), (_l, p) => crText({ pr: prOf(p), sources: 2, wt: { base: 'main', files: 1 } })],
      [scopeWith(FLAGS, { uncommitted_count: 1 }), (_l, p) => crText({ pr: prOf(p), sources: 2, wt: { base: 'origin/main', files: 1 } })],
      [scopeWith(FLAGS, { uncommitted_count: 1 }), (_l, p) => crText({ pr: prOf(p) })],
    ]) {
      const r = await run(RO, { scope, 'lens:code-review': text, 'lens:code-review-2': text })
      check(has(r.result.stopped, 'review lens failed: code-review') && !r.work.some(l => l.startsWith('apply:')), JSON.stringify(r.result))
    }
  },
  async 'M1: the lenses do not run unless HEAD is the PR head and the upstream'() {
    for (const extra of [{ same_head: false }, { head: 'h1' }, { committed_count: 0 }, { uncommitted_count: -1 }]) {
      const r = await run(RO, { scope: scopeWith(FLAGS, extra) })
      check(typeof r.result.stopped === 'string' && !r.work.some(l => l.startsWith('lens:')), `${JSON.stringify(extra)}: ${JSON.stringify(r.result)}`)
    }
  },
  async 'M1: the lens prompt does not hand the agent the counts it must report'() {
    const r = await run(RO, { scope: scopeWith(FLAGS, { committed_count: 37, uncommitted_count: 5 }),
      'lens:code-review': (_l, p) => crText({ pr: prOf(p), files: 37, sources: 2, wt: { base: `origin/demo`, files: 5 } }) })
    const p = r.prompts['lens:code-review:demo']
    check(!/\b37\b/.test(p) && !/\b5 (uncommitted|files)/.test(p) && !r.result.stopped, `${JSON.stringify(r.result.stopped)}: ${p}`)
    const sha = await run(RO, { scope: scopeWith(FLAGS, { uncommitted_count: 1 }),
      'lens:code-review': (_l, p) => crText({ pr: prOf(p), sources: 2, wt: { base: `\`${LH.slice(0, 8)}\`.`, files: 1 } }) })
    check(!sha.result.stopped, `a base named as the HEAD hash failed: ${sha.result.stopped}`)
  },
  // Fable review M2: the selector's exit status, and the prepare stage.
  async 'M2: a failing review-lenses helper stops the run'() {
    const r = await run(RO, { scope: scopeWith(FLAGS, { lenses_exit: 2, lenses_stdout: 'usage: agent-topology.py: invalid choice: review-lenses' }) })
    check(has(r.result.stopped, 'review-lenses exited 2') && !r.work.some(l => l.startsWith('lens:')), JSON.stringify(r.result))
    const m = await run(RO, { scope: scopeWith(FLAGS, { lenses_stdout: '{"data":"yes"}' }) })
    check(has(m.result.stopped, 'malformed') && !m.work.some(l => l.startsWith('lens:')), JSON.stringify(m.result))
    const t = await run(RO, { scope: scopeWith(FLAGS, { lenses_stdout: 'data: true' }) })
    check(has(t.result.stopped, 'malformed'), JSON.stringify(t.result))
  },
  async 'M2: a blocked or dead prepare stage stops before simplify'() {
    for (const prepare of [() => ({ status: 'blocked', reason: '18 uncommitted entries' }), () => null]) {
      const r = await run(RO, { prepare })
      check(typeof r.result.stopped === 'string' && !r.work.some(l => l.startsWith('simplify:')), JSON.stringify(r.result))
    }
    const r = await run(RO)
    const p = r.prompts['prepare:demo']
    check(has(p, 'merge --abort') && has(p, 'set-upstream-to=origin/demo') && has(p, 'status --porcelain'), p)
  },
  // Fable review M3 and the maintainer's rule (a): the final review edits nothing.
  async 'M3: the final review edits nothing and reviews the exact apply range'() {
    const r = await run({ ...BASE, mode: 'new' })
    const p = r.prompts['merge:demo']
    check(!has(p, 'Fix such a defect') && has(p, 'fix nothing') && has(p, 'You edit no code'), 'the merge stage may still write a fix')
    check(has(p, `log -p ${LH}..${H2}`) && has(p, `HEAD^{tree} is ${T2}`), 'the merge prompt lacks the exact range and tree')
    check(has(p, 'review-findings-applied.md') && has(p, 'wrongly rejected correctness finding'), 'rejected correctness findings are not re-checked')
    check(has(p, 'merge --abort') && has(p, 'you resolve nothing'), 'a conflicted origin/main merge is not blocked')
    const b = await run({ ...BASE, mode: 'new' }, { merge: () => ({ verdict: 'blocked', reason: 'off by one in the apply commit', notes: '' }) })
    check(has(b.result.stopped, 'final review blocked: off by one'), JSON.stringify(b.result))
  },
  // Fable review M4: a domain lens that did not run is a failed lens.
  async 'M4: a domain lens that did not run fails'() {
    for (const r of [
      { skill_ran: 'could not load the skill', findings: [], findings_file: 'f.md' },
      { status: 'failed', skill_ran: 'could not load the skill', examined: ['x'], findings: [], findings_file: 'f.md' },
      { status: 'ran', skill_ran: 'data-safety', examined: [], findings: [], findings_file: 'f.md' },
      null,
    ]) {
      const out = await run(RO, {
        scope: scopeWith({ ...FLAGS, data: true }),
        'lens:data-safety:demo': () => r,
      })
      check(has(out.result.stopped, 'review lens failed: data-safety') && !out.work.some(l => l.startsWith('apply:')), `${JSON.stringify(r)}: ${JSON.stringify(out.result)}`)
    }
  },
  async 'M4: the apply stage re-runs the selector against the flags used'() {
    const r = await run(RO, { scope: scopeWith({ ...FLAGS, data: true }) })
    const p = r.prompts['apply:demo']
    check(has(p, 'review-lenses --base origin/main yourself') && has(p, 'data=true, performance=false, modules_moved=false'), p)
  },
  // Fable review M5: a dead or confused merge agent never looks finished.
  async 'M5: a dead merge agent, or an unknown verdict, stops the run'() {
    const d = await run({ ...BASE, mode: 'new' }, { merge: () => null })
    check(has(d.result.stopped, 'merge agent died'), JSON.stringify(Object.keys(d.result)))
    const u = await run({ ...BASE, mode: 'new' }, { merge: () => ({ verdict: 'probably merged', reason: '', notes: '' }) })
    check(has(u.result.stopped, 'unknown verdict'), JSON.stringify(u.result))
    const n = await run({ ...BASE, mode: 'new' }, { merge: () => ({ verdict: 'merged', reason: '', notes: '' }) })
    check(has(n.result.stopped, 'without a merge commit'), JSON.stringify(n.result))
    const x = await run({ ...BASE, mode: 'new', no_merge: 'pending' }, { merge: () => ({ verdict: 'merged', reason: '', merge_commit: 'm', archive_commit: 'a', notes: '' }) })
    check(has(x.result.stopped, 'no_merge was set'), JSON.stringify(x.result))
  },
  // Stage-4 review of the fix round (R1): the merge prompt allows exactly the
  // two commits the stage owes (the conflict-free origin/main merge and the
  // archive) and none under no_merge; R2: every merged verdict names its merge.
  async 'R1: the merge prompt allows the archive commit and nothing else'() {
    const r = await run({ ...BASE, mode: 'new' })
    const p = r.prompts['merge:demo']
    check(!has(p, 'the only commit you may make is the origin/main merge'), 'the merge prompt forbids the archive commit it asks for')
    check(has(p, 'the archive commit') && has(p, 'You edit no code'), p.slice(0, 400))
    let seen = ''
    await run({ ...BASE, mode: 'new', no_merge: 'check pending' }, {
      merge: (_l, prompt) => { seen = prompt; return { verdict: 'ready', reason: 'check pending', notes: '' } },
    })
    check(has(seen, 'You edit no file and make no commit') && !has(seen, 'the archive commit'), seen.slice(0, 400))
  },
  async 'R2: a merged-archive-owed verdict without a merge commit stops the run'() {
    const r = await run({ ...BASE, mode: 'new' }, { merge: () => ({ verdict: 'merged-archive-owed', reason: 'kani pending', notes: '' }) })
    check(has(r.result.stopped, 'without a merge commit'), JSON.stringify(r.result))
    const ok = await run({ ...BASE, mode: 'new' }, { merge: () => ({ verdict: 'merged-archive-owed', reason: 'kani pending', merge_commit: MC, final_tree: T2, notes: '' }) })
    check(ok.result.verdict === 'merged-archive-owed' && !ok.result.stopped, JSON.stringify(ok.result))
  },
  // R3: a stop after /simplify that can leave its edits uncommitted says so,
  // and the fast-lane violation does not send the coordinator to skip_impl,
  // whose prepare stage refuses a dirty worktree.
  async 'R3: apply-stage stops name the uncommitted /simplify edits'() {
    const b = await run(RO, { apply: () => ({ ...DEFAULTS.apply(), verdict: 'blocked', summary: 'flags differ' }) })
    check(has(b.result.stopped, 'apply stage: blocked') && has(b.result.stopped, 'simplify.patch'), JSON.stringify(b.result))
    const v = await run({ ...BASE, mode: 'fast-lane' }, {
      spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'eligible' }),
      apply: () => ({ ...DEFAULTS.apply(), verdict: 'fast-lane-violation', summary: 'src path' }),
    })
    check(has(v.result.stopped, 'fast-lane violation') && has(v.result.stopped, 'simplify.patch'), JSON.stringify(v.result))
    check(!has(v.result.stopped, 'resume with mode continue and skip_impl'), 'the violation stop sends the coordinator into a prepare stage that refuses the dirty worktree')
  },
  // Maintainer rule (b): an empty code-review stops unless accepted.
  async 'an empty code-review over a quoted scope stops, and passes only when accepted'() {
    const empty = (_l, p) => crText({ pr: prOf(p), count: 0, status: 'empty', body: 'No issues found.' })
    const r = await run(RO, { 'lens:code-review': empty })
    check(has(r.result.stopped, 'empty review') && !r.work.some(l => l.startsWith('lens:code-review-2:')) && !r.work.some(l => l.startsWith('apply:')), JSON.stringify(r.result))
    const token = (/args\.accept_empty_review: "([^"]+)"/.exec(r.result.stopped || '') || [])[1]
    check(typeof token === 'string' && token.startsWith(`${LH}:`), `the stop prints no token bound to the HEAD and the review: ${r.result.stopped}`)
    const a = await run({ ...RO, accept_empty_review: token }, { 'lens:code-review': empty })
    check(!a.result.stopped && a.work.some(l => l.startsWith('apply:')), JSON.stringify(a.result))
    // A bare true, the HEAD alone, or the token of another review accepts nothing (Fable F10).
    const other = (_l, p) => crText({ pr: prOf(p), count: 0, status: 'empty', body: 'Nothing to report.' })
    for (const [value, lens] of [[true, empty], ['9'.repeat(40), empty], [LH, empty], [token, other]]) {
      const b = await run({ ...RO, accept_empty_review: value }, { 'lens:code-review': lens })
      check(has(b.result.stopped, 'empty review'), `${JSON.stringify(value)} accepted an empty review`)
    }
    const noScope = (_l, p) => crText({ pr: prOf(p), files: 2, lines: 0, count: 0, status: 'empty' })
    const z = await run({ ...RO, accept_empty_review: token }, {
      'lens:code-review': noScope, 'lens:code-review-2': noScope,
    })
    check(has(z.result.stopped, 'review lens failed: code-review'), `an empty review over no scope must fail: ${JSON.stringify(z.result)}`)
  },
  async 'guard_ack and accept_empty_review never reach a prompt'() {
    const r = await run({ ...RO, merge: true, accept_empty_review: `${LH}:0123abcd0123abcd`, guard_ack: 'simplify:x' })
    for (const [label, prompt] of Object.entries(r.prompts)) {
      check(!has(prompt, 'accept_empty_review') && !has(prompt, 'guard_ack') && !has(prompt, 'simplify:x') && !has(prompt, '0123abcd0123abcd'), `${label} carries a script-only argument`)
      check(!has(prompt, 'gate-trees.sh void'), `${label} hands a stage agent the coordinator's void`)
    }
  },
  async 'a bare (none) from code-review is a failed lens after one rerun'() {
    const r = await run(RO, { 'lens:code-review': () => '(none)', 'lens:code-review-2': () => '(none)' })
    check(has(r.result.stopped, 'review lens failed: code-review'), JSON.stringify(r.result))
    check(r.work.some(l => l.startsWith('lens:code-review-2:')) && !r.work.some(l => l.startsWith('apply:')), JSON.stringify(r.work))
  },
  async 'a missing status line is rerun once and then accepted'() {
    const r = await run(RO, { 'lens:code-review': () => '- a.rs:3 off by one\nskill_findings_source: result-text' })
    check(!r.result.stopped && r.work.some(l => l.startsWith('lens:code-review-2:')) && r.work.some(l => l.startsWith('apply:')), JSON.stringify(r.result))
  },
  async 'empty-looking reviews marked ok still fail the lens'() {
    // A zero PR count is an empty review for the coordinator (F1); findings
    // counted over an empty body are a failed lens. Neither reaches apply.
    for (const [body, count] of [['[\n]', 0], ['```json\n[\n]\n```', 0], ['## Summary\nNo findings.', 0], ['(none)', 0], ['', 0], ['', 1], ['[\n]', 1]]) {
      const text = (_l, p) => crText({ pr: prOf(p), body, count })
      const r = await run(RO, { 'lens:code-review': text, 'lens:code-review-2': text })
      const want = count === 0 ? 'empty review' : 'review lens failed: code-review'
      check(has(r.result.stopped, want) && !r.work.some(l => l.startsWith('apply:')), `${JSON.stringify(body)}/${count}: ${JSON.stringify(r.result)}`)
    }
  },
  async 'a status line that is not last, or a source line missing, fails'() {
    for (const text of [
      'CODE-REVIEW-STATUS: ok\n- a.rs:1 x\nskill_findings_source: result-text\nCODE-REVIEW-SCOPE: pr #7 files=2 lines=4\nfindings_count: 1',
      '- a.rs:1 x\nCODE-REVIEW-SCOPE: pr #7 files=2 lines=4\nfindings_count: 1\nCODE-REVIEW-STATUS: ok',
      '- a.rs:1 x\nskill_findings_source: result-text\nCODE-REVIEW-SCOPE: pr #7 files=2 lines=4\nCODE-REVIEW-STATUS: ok',
    ]) {
      const r = await run(RO, { 'lens:code-review': () => text, 'lens:code-review-2': () => text })
      check(has(r.result.stopped, 'review lens failed'), `${JSON.stringify(text)}: ${JSON.stringify(r.result)}`)
    }
  },
  async 'lens failed text is a failed lens'() {
    const r = await run(RO, {
      'lens:code-review': () => '- a.rs:1 x\nskill_findings_source: result-text\nno agentId\nCODE-REVIEW-STATUS: failed',
      'lens:code-review-2': () => 'no transcript\nCODE-REVIEW-STATUS: failed',
    })
    check(has(r.result.stopped, 'review lens failed') && has(r.result.stopped, 'no transcript'), JSON.stringify(r.result))
  },
  async 'lens selection follows the flags'() {
    const r = await run(RO, { scope: scopeWith({ data: true, performance: true, modules_moved: true }) })
    const want = ['code-review', 'data-safety', 'gtk-responsiveness', 'gtk-perf-scale', 'gtk-perf-rust-optimize', 'rust-hex-arch']
    check(JSON.stringify(r.result.lenses) === JSON.stringify(want), JSON.stringify(r.result.lenses))
    for (const lens of want) check(r.work.some(l => l.startsWith(`lens:${lens}:`)), `no agent for ${lens}`)
  },
  async 'a guard violation stops the run at that stage'() {
    const r = await run(RO, { 'guard:simplify:after': () => ({ ...READING, head: 'moved' }) })
    check(has(r.result.stopped, 'MAIN CHECKOUT CHANGED during simplify'), JSON.stringify(r.result))
    check(!r.work.some(l => l.startsWith('scope:')), JSON.stringify(r.work))
  },
  async 'S6: the guard compares the status hash, and a slot taken during the lenses stops the run'() {
    const reformatted = await run(RO, { 'guard:simplify:after': () => ({ ...READING, porcelain: ' ' }) })
    check(!reformatted.result.stopped, `a copy of the porcelain text with the same hash stopped the run: ${JSON.stringify(reformatted.result)}`)
    const changed = await run(RO, { 'guard:simplify:after': () => ({ ...READING, status_sha256: 'f'.repeat(64) }) })
    check(has(changed.result.stopped, 'MAIN CHECKOUT CHANGED during simplify'), JSON.stringify(changed.result))
    const bad = await run(RO, { 'guard:simplify:after': () => ({ ...READING, status_sha256: 'not a hash' }) })
    check(has(bad.result.stopped, 'guard reading after simplify failed'), JSON.stringify(bad.result))
    const slot = await run(RO, { 'guard:review-lenses:after': () => ({ ...READING, own_slot: true }) })
    check(has(slot.result.stopped, 'MAIN CHECKOUT CHANGED during review-lenses') && !slot.work.some(l => l.startsWith('apply:')), JSON.stringify(slot.result))
    const lock = await run({ ...RO, merge: true }, { 'guard:merge:after': () => ({ ...READING, own_merge_lock: true }) })
    check(has(lock.result.stopped, 'MAIN CHECKOUT CHANGED during merge'), 'a merge lock left held passed')
    // Giving a slot or the lock up during a stage is benign.
    const released = await run(RO, { 'guard:simplify:before': () => ({ ...READING, own_slot: true }) })
    check(!released.result.stopped, `a released slot stopped the run: ${released.result.stopped}`)
    const g = await run(RO)
    check(has(g.prompts['guard:review-lenses:before'], 'IMPL-SLOTS/demo') && has(g.prompts['guard:review-lenses:before'], 'MERGE-LOCK/owner.txt'), 'the guard does not read the slot and the merge lock')
  },
  async 'S6: an acknowledged guard change passes for that stage and that reading only'() {
    const moved = { ...READING, status_sha256: 'f'.repeat(64) }
    const token = `simplify:${'f'.repeat(64)}:abc123:main:no-slot:no-lock`
    const first = await run(RO, { 'guard:simplify:after': () => moved })
    check(has(first.result.stopped, `"${token}"`), `the stop does not print the token: ${first.result.stopped}`)
    const acked = await run({ ...RO, guard_ack: [token] }, { 'guard:simplify:after': () => moved })
    check(!acked.result.stopped && acked.work.some(l => l.startsWith('apply:')), JSON.stringify(acked.result))
    const other = await run({ ...RO, guard_ack: token.replace('simplify:', 'apply:') }, { 'guard:simplify:after': () => moved })
    check(has(other.result.stopped, 'MAIN CHECKOUT CHANGED during simplify'), 'an ack for another stage was accepted')
    const otherHash = await run({ ...RO, guard_ack: token }, { 'guard:simplify:after': () => ({ ...moved, status_sha256: 'e'.repeat(64) }) })
    check(has(otherHash.result.stopped, 'MAIN CHECKOUT CHANGED during simplify'), 'an ack for another reading was accepted')
    check(!has(first.result.stopped, 'mode continue'), 'the stop message still says to resume with mode continue')
  },
  async 'a dead guard agent stops the run'() {
    const r = await run({ ...BASE, mode: 'new' }, { 'guard:spec:before': () => null })
    check(has(r.result.stopped, 'guard reading before spec failed') && r.work.length === 0, JSON.stringify(r))
  },
  // Fable review S5: the apply stage's claims are read back before the merge.
  async 'S5: an apply stage whose tree, HEAD or push does not match stops before the merge'() {
    for (const verify of [
      () => ({ ...DEFAULTS.verify(), tree_match: false }),
      () => ({ ...DEFAULTS.verify(), head_match: false }),
      () => ({ ...DEFAULTS.verify(), pushed: false }),
      () => ({ ...DEFAULTS.verify(), clean: false }),
      () => ({ ...DEFAULTS.verify(), lens_head_is_ancestor: false }),
      () => ({ ...DEFAULTS.verify(), gated_exit: 1 }),
      () => ({ ...DEFAULTS.verify(), head: 'h2' }),
      () => null,
    ]) {
      const r = await run({ ...BASE, mode: 'new' }, { verify })
      check(typeof r.result.stopped === 'string' && !r.work.some(l => l.startsWith('merge:')), JSON.stringify(r.result))
    }
    // The comparison runs in the shell against the apply stage's own hashes, so
    // an abbreviated hash is not a mismatch; a value that is no hash never
    // reaches the command.
    const short = await run({ ...BASE, mode: 'new' }, { apply: () => ({ ...DEFAULTS.apply(), head: H2.slice(0, 12) }) })
    check(!short.result.stopped && has(short.prompts['verify:demo'], `'${H2.slice(0, 12)}^{commit}'`), JSON.stringify(short.result.stopped))
    const junk = await run({ ...BASE, mode: 'new' }, { apply: () => ({ ...DEFAULTS.apply(), tree: "x'; rm -rf /" }) })
    check(has(junk.result.stopped, 'no commit and tree hashes') && !junk.work.some(l => l.startsWith('verify:')), JSON.stringify(junk.result))
  },
  async 'S1: revise needs the existing PR and worktree'() {
    for (const extra of [{}, { pr: 7 }, { worktree: ARG_WT }]) {
      const r = await run({ ...BASE, mode: 'revise', revision: 1, ...extra })
      check(has(r.result.stopped, 'revise needs args.pr and args.worktree') && r.calls.length === 0, JSON.stringify(r.result))
    }
    const r = await run({ ...BASE, mode: 'revise', revision: 1, pr: 7, worktree: ARG_WT })
    check(has(r.prompts['spec:demo'], 'PR #7') && has(r.prompts['implement-1:demo'], 'PR #7 exists') && !has(r.prompts['implement-1:demo'], 'Open the pull request'), r.prompts['implement-1:demo'])
  },
  async 'S2: continue checks every argument before the first agent and fills the spec defaults'() {
    const r = await run({ ...BASE, mode: 'continue', spec: CARRIED_SPEC, prev_change: 'older' })
    check(has(r.result.stopped, 'prev_pr and args.prev_tip') && r.calls.length === 0, JSON.stringify(r.result))
    const bad = await run({ ...BASE, mode: 'continue', spec: { worktree: '/repo' }, prev_change: 'older', prev_pr: 3, prev_tip: 'abc' })
    check(typeof bad.result.stopped === 'string' && bad.calls.length === 0, `the archive ran before the spec check: ${JSON.stringify(bad.calls)}`)
    const d = await run({ ...BASE, mode: 'continue', pr: 9, spec: { worktree: ARG_WT } })
    check(has(d.prompts['implement-1:demo'], 'change demo, worktree') && has(d.prompts['implement-1:demo'], 'branch demo.') && !has(d.prompts['implement-1:demo'], 'undefined'), d.prompts['implement-1:demo'])
  },
  async 'S3: a later continue round is told the PR an earlier round opened'() {
    let round = 0
    const r = await run({ ...BASE, mode: 'continue', spec: CARRIED_SPEC, rounds: 2 }, {
      implement: () => (++round === 1 ? { tasks_complete: false, pr_number: 12, gates: '', report: 'r1', unchecked_tasks: '5.1' } : DEFAULTS.implement()),
    })
    check(has(r.prompts['implement-2:demo'], 'PR #12 exists') && !has(r.prompts['implement-2:demo'], 'Open the pull request'), r.prompts['implement-2:demo'])
  },
  async 'S9 and N8: simplify names its base, starts clean and saves new files'() {
    const r = await run(RO)
    const p = r.prompts['simplify:demo']
    check(has(p, `git -C ${ARG_WT} diff origin/main...HEAD`) && has(p, `git -C ${ARG_WT} status --porcelain`) && has(p, '--intent-to-add'), p)
    const b = await run(RO, { simplify: () => ({ status: 'blocked', edited_files: [], tests_run: '', summary: 'dirty' }) })
    check(has(b.result.stopped, 'simplify: blocked') && !b.work.some(l => l.startsWith('scope:')), JSON.stringify(b.result))
  },
  async 'S10: the apply stage re-dispatches Kani budget runs its edits touch'() {
    const r = await run({ ...RO, impl: { tasks_complete: true, pr_number: 7, report: 'r', gates: '', kani_dispatch: 'run 42' } })
    check(has(r.prompts['apply:demo'], 'dispatch its budget run again') && has(r.prompts['apply:demo'], 'run 42'), r.prompts['apply:demo'])
  },
  async 'S13: no_merge leaves the merging instructions out'() {
    let seen = ''
    await run({ ...BASE, mode: 'new', no_merge: 'template review pending' }, {
      merge: (_l, prompt) => { seen = prompt; return { verdict: 'ready', reason: 'template review pending', notes: '' } },
    })
    check(has(seen, 'DO NOT MERGE') && has(seen, 'template review pending'), 'no_merge reason missing')
    check(!has(seen, 'take MERGE-LOCK') && !has(seen, 'gh pr merge'), 'the merging instructions are still in a no_merge prompt')
  },
  async 'S14: review-only refuses a carried implementation that is not complete'() {
    const r = await run({ ...RO, impl: { tasks_complete: false, pr_number: 7, report: 'r', gates: '' } })
    check(has(r.result.stopped, 'not complete') && r.calls.length === 0, JSON.stringify(r.result))
  },
  async 'fast lane: an eligible spec skips the spec review'() {
    const r = await run({ ...BASE, mode: 'fast-lane' }, { spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'eligible' }) })
    check(!r.work.some(l => l.startsWith('spec-review:')) && r.result.merge, JSON.stringify(r.work))
  },
  async 'fast lane: an ineligible spec gets the spec review'() {
    const r = await run({ ...BASE, mode: 'fast-lane' }, { spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'not-eligible' }) })
    check(r.work.some(l => l.startsWith('spec-review:')), JSON.stringify(r.work))
  },
  async 'fast lane: a violation at apply sends the change to a spec review and stops'() {
    const r = await run({ ...BASE, mode: 'fast-lane' }, {
      spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'eligible' }),
      apply: () => ({ ...DEFAULTS.apply(), verdict: 'fast-lane-violation', summary: 'src path' }),
    })
    check(has(r.result.stopped, 'fast-lane violation'), JSON.stringify(r.result))
    check(r.work.at(-1).startsWith('spec-review:') && !r.work.some(l => l.startsWith('merge:')), JSON.stringify(r.work))
  },
  async 'an incomplete implementation stops before the review stages'() {
    const r = await run({ ...BASE, mode: 'new' }, {
      implement: () => ({ tasks_complete: false, gates: '', report: 'r', unchecked_tasks: '5.1' }),
    })
    check(has(r.result.stopped, 'implementation incomplete') && !r.work.some(l => l.startsWith('prepare:')), JSON.stringify(r.result))
  },
  async 'the merge prompt carries the stopping rule and the push rule'() {
    const r = await run({ ...BASE, mode: 'new' })
    const prompt = r.prompts['merge:demo']
    check(has(prompt, '"No findings" is not the merge test'), 'stopping rule missing')
    check(has(prompt, 'never push with --force or --force-with-lease'), 'force-push rule missing')
  },
  async 'every stage prompt carries the foreground rule'() {
    const prompts = []
    const readOnly = ['scope:', 'verify:', 'postmerge:']
    await run({ ...BASE, mode: 'new' }, Object.fromEntries(Object.keys(DEFAULTS).filter(k => k !== 'guard').map(k => [k, (l, p, o) => { prompts.push([l, p]); return DEFAULTS[k](l, p, o) }])))
    check(prompts.length > 0, 'no stage prompt was captured')
    for (const [label, prompt] of prompts) {
      if (readOnly.some(prefix => label.startsWith(prefix))) continue
      check(has(prompt, 'never return, or end your turn, while a command you started is still running'), `${label} lacks the foreground rule`)
      check(has(prompt, 'git fetch --prune') && has(prompt, 'origin/<branch>') && has(prompt, 'exit status'), `${label} lacks the fetch, branch-merge or exit-status rule (note 21:35)`)
    }
  },
  async 'continue mode refuses a carried spec outside a change worktree or on a main alias'() {
    for (const spec of [
      { change_name: 'demo', worktree: '/repo', branch: 'demo' },
      { change_name: 'demo', worktree: ARG_WT, branch: 'origin/main' },
    ]) {
      const r = await run({ ...BASE, mode: 'continue', pr: 9, spec })
      check(typeof r.result.stopped === 'string' && r.calls.length === 0, `${JSON.stringify(spec)}: ${JSON.stringify(r.result)} after ${JSON.stringify(r.calls)}`)
    }
  },
  async 'dot segments are not change worktrees'() {
    for (const worktree of ['/repo/.claude/worktrees/..', '/repo/.claude/worktrees/../', '/repo/.claude/worktrees/.']) {
      const r = await run({ ...RO, worktree })
      check(has(r.result.stopped, 'change worktree') && r.calls.length === 0, `${worktree}: ${JSON.stringify(r.result)}`)
    }
  },
  async 'a fast-lane run its spec stage found not eligible runs the normal stages'() {
    const r = await run({ ...BASE, mode: 'fast-lane' }, { spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'not-eligible' }) })
    check(!has(r.prompts['implement-1:demo'], 'check-fast-lane') && !has(r.prompts['apply:demo'], 'check-fast-lane'), 'fast-lane instructions reached a not-eligible change')
    check(r.result.merge && !r.result.stopped, JSON.stringify(r.result))
    const e = await run({ ...BASE, mode: 'fast-lane' }, { spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'eligible' }) })
    check(has(e.prompts['implement-1:demo'], 'check-fast-lane') && has(e.prompts['apply:demo'], 'check-fast-lane'), 'an eligible fast-lane change lost its re-checks')
  },
  async 'a resumed fast-lane change keeps the apply-stage spec review'() {
    const c = await run({ ...BASE, mode: 'continue', spec: { ...CARRIED_SPEC, fast_lane: 'eligible' } })
    check(has(c.prompts['apply:demo'], 'check-fast-lane'), 'continue lost the fast-lane review of the spec')
    const r = await run({ ...RO, fast_lane: 'eligible' })
    check(has(r.prompts['apply:demo'], 'check-fast-lane'), 'review-only lost the fast-lane review of the spec')
    const n = await run(RO)
    check(!has(n.prompts['apply:demo'], 'check-fast-lane'), 'a normal review-only run got fast-lane instructions')
    const v = await run({ ...BASE, mode: 'fast-lane' }, {
      spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'eligible' }),
      apply: () => ({ ...DEFAULTS.apply(), verdict: 'fast-lane-violation', summary: 'src path' }),
    })
    check(has(v.result.stopped, 'not-eligible'), `the violation stop must say how to resume outside the lane: ${v.result.stopped}`)
  },
  async 'a not-viable spec and a revision decision write only to the handoff'() {
    const r = await run({ ...BASE, mode: 'new' })
    check(!has(r.prompts['spec:demo'], 'commit to main') && has(r.prompts['spec:demo'], 'not-viable.md'), r.prompts['spec:demo'])
    const d = await run({ ...BASE, mode: 'revise', revision: 3, pr: 7, worktree: ARG_WT })
    const prompt = d.prompts['decision:demo']
    check(!has(prompt, 'PARALLEL-LEDGER') && has(prompt, 'decision-3.md'), prompt)
  },
  async 'continue with skip_impl and no PR anywhere stops before any agent'() {
    const r = await run({ ...BASE, mode: 'continue', skip_impl: true, spec: CARRIED_SPEC, impl: { tasks_complete: true, report: 'r', gates: '' } })
    check(has(r.result.stopped, 'pull request') && r.calls.length === 0, JSON.stringify(r.result))
    // Fable F8: not even the archive of a previous change runs first.
    const p = await run({ ...BASE, mode: 'continue', skip_impl: true, spec: CARRIED_SPEC, impl: { tasks_complete: true, report: 'r', gates: '' },
      prev_change: 'older', prev_pr: 3, prev_tip: 'abc' })
    check(has(p.result.stopped, 'pull request') && p.calls.length === 0, `the archive ran before the PR check: ${JSON.stringify(p.calls)}`)
  },
  async 'an empty pr argument means no PR'() {
    const r = await run({ ...BASE, mode: 'new', pr: null })
    check(r.result.merge && !r.result.stopped, JSON.stringify(r.result))
  },
  // Fable fix check (2026-11-19).
  async 'F1: a PR call with no findings is an empty review even when the worktree call has some'() {
    const scope = scopeWith(FLAGS, { uncommitted_count: 1 })
    const hollow = (_l, p) => crText({ pr: prOf(p), sources: 2, wt: { files: 1 }, count: 0, wtCount: 2, status: 'ok', body: '- b.rs:9 leak\n- c.rs:4 unused' })
    const r = await run(RO, { scope, 'lens:code-review': hollow })
    check(has(r.result.stopped, 'empty review') && !r.work.some(l => l.startsWith('apply:')), `a hollow PR review passed: ${JSON.stringify(r.result)}`)
    check(!r.work.some(l => l.startsWith('lens:code-review-2:')), 'an empty PR review burned the rerun instead of stopping for the coordinator')
    const token = (/args\.accept_empty_review: "([^"]+)"/.exec(r.result.stopped || '') || [])[1]
    const a = await run({ ...RO, accept_empty_review: token }, { scope, 'lens:code-review': hollow })
    check(!a.result.stopped && a.work.some(l => l.startsWith('apply:')), `the accepted hollow review did not pass: ${JSON.stringify(a.result)}`)
    // Findings on the PR and none on the small /simplify edits is a normal review.
    const ok = await run(RO, { scope, 'lens:code-review': (_l, p) => crText({ pr: prOf(p), sources: 2, wt: { files: 1 }, count: 2, wtCount: 0 }) })
    check(!ok.result.stopped, `a review with PR findings and no worktree findings stopped: ${ok.result.stopped}`)
    // An empty status over PR findings contradicts itself, and the old single count is refused.
    for (const text of [
      (_l, p) => crText({ pr: prOf(p), count: 1, status: 'empty' }),
      (_l, p) => crText({ pr: prOf(p) }).replace('findings_count: pr=1', 'findings_count: 1'),
      (_l, p) => crText({ pr: prOf(p), sources: 2, wt: { files: 1 }, count: 1 }).replace(' worktree=0', ''),
    ]) {
      const f = await run(RO, { scope: text.length && /worktree/.test(text(null, 'PR #7')) ? scope : scopeWith(), 'lens:code-review': text, 'lens:code-review-2': text })
      check(has(f.result.stopped, 'review lens failed: code-review'), `${text(null, 'PR #7')}: ${JSON.stringify(f.result)}`)
    }
    const p = r.prompts['lens:code-review:demo']
    check(has(p, 'findings_count: pr=<n> worktree=<n>'), `the lens prompt does not ask for a count per call: ${p}`)
  },
  async 'F2: no prompt carries a pass under load'() {
    const r = await run({ ...BASE, mode: 'new' })
    for (const [label, prompt] of Object.entries(r.prompts)) {
      check(!/level 3|under load counts|calibration/i.test(prompt), `${label} mentions level 3 or the calibration`)
    }
  },
  async 'F3: the archive commit may carry the spec sync and the Kani figures, and a blocked merge stage does not claim nothing was merged'() {
    const r = await run({ ...BASE, mode: 'new' })
    const p = r.prompts['merge:demo']
    check(!has(p, 'touches only the archived change and the programme record') && has(p, '"Merging" step 4'), p.slice(0, 600))
    const after = await run({ ...BASE, mode: 'new' }, { merge: () => ({ verdict: 'blocked', reason: 'archive refused', merge_commit: MC, notes: '' }) })
    check(has(after.result.stopped, 'final review blocked') && !has(after.result.stopped, 'Nothing was merged') && has(after.result.stopped, MC), after.result.stopped)
    const before = await run({ ...BASE, mode: 'new' }, { merge: () => ({ verdict: 'blocked', reason: 'off by one', notes: '' }) })
    check(!has(before.result.stopped, 'Nothing was merged') && has(before.result.stopped, 'gh pr view 11'), before.result.stopped)
  },
  async 'F4: a resumed run takes a live guard reading before its first live stage'() {
    const first = await run({ ...RO, merge: true, review_extra: 'verdict v1' })
    check(first.result.verdict === 'merged', JSON.stringify(first.result))
    const second = await run({ ...RO, merge: true, review_extra: 'verdict v2' }, {}, activeSource, first.record)
    check(second.live[0] === 'guard:merge:before', `the first live call is ${second.live[0]}, not a fresh guard reading before the merge stage: ${JSON.stringify(second.live)}`)
    check(second.live.includes('merge:demo') && second.live.includes('guard:merge:after'), JSON.stringify(second.live))
    const firstLive = second.calls.indexOf('guard:merge:before')
    check(firstLive > 0 && second.calls.slice(0, firstLive).every(l => !second.live.includes(l)), 'an earlier stage reran')
    // An unchanged resume replays every call, guard readings included.
    const same = await run({ ...RO, merge: true, review_extra: 'verdict v1' }, {}, activeSource, first.record)
    check(same.live.length === 0, `an unchanged resume ran ${JSON.stringify(same.live)} live`)
  },
  async 'F4: every guard reading before a stage names that stage\'s prompt'() {
    const a = await run({ ...RO, merge: true, review_extra: 'x1' })
    const b = await run({ ...RO, merge: true, review_extra: 'x2' })
    for (const label of Object.keys(a.prompts)) {
      if (label === 'guard:merge:before' || label === 'merge:demo') {
        check(a.prompts[label] !== b.prompts[label], `${label} did not change with the merge prompt`)
      } else {
        check(a.prompts[label] === b.prompts[label], `${label} changed although its stage did not`)
      }
    }
  },
  async 'F5: args.lens_round reruns the lenses alone on a resume'() {
    const bad = (_l, p) => crText({ pr: prOf(p), files: 99 })
    const first = await run(RO, { 'lens:code-review': bad, 'lens:code-review-2': bad })
    check(has(first.result.stopped, 'review lens failed') && has(first.result.stopped, 'args.lens_round: 2'), first.result.stopped)
    const second = await run({ ...RO, lens_round: 2 }, {}, activeSource, first.record)
    check(!second.result.stopped && second.live.includes('lens:code-review:demo'), JSON.stringify(second.result))
    check(!second.live.some(l => /^(prepare|simplify):/.test(l) || /^guard:(prepare|simplify):/.test(l)), `prepare or simplify reran: ${JSON.stringify(second.live)}`)
    check(second.live.includes('guard:review-scope:before') && second.live.includes('scope:demo'), `the scope reading was not taken again: ${JSON.stringify(second.live)}`)
    check(has(second.prompts['lens:code-review:demo'], 'attempt 3'), second.prompts['lens:code-review:demo'])
    for (const value of [0, -1, 1.5, '2']) {
      const r = await run({ ...RO, lens_round: value })
      check(has(r.result.stopped, 'lens_round') && r.calls.length === 0, `${JSON.stringify(value)}: ${JSON.stringify(r.result)}`)
    }
  },
  async 'F6: a merged verdict is checked against the gate records of the tree on main'() {
    const ok = await run({ ...BASE, mode: 'new' })
    check(ok.result.verdict === 'merged' && ok.work.at(-1) === 'postmerge:demo', JSON.stringify(ok.work))
    const p = ok.prompts['postmerge:demo']
    check(has(p, `'${MC}^{commit}'`) && has(p, `'${T2}^{tree}'`) && has(p, '--tree') && has(p, "'make test'") && has(p, '--change demo'), p)
    // One smoke predicate everywhere: the UI prefixes of testing.md, from the
    // canonical classifier's RELEVANT_PREFIXES, not the wider visual-proof set.
    check(has(p, 'a.RELEVANT_PREFIXES') && !has(p, 'is_visual_sensitive') && !has(p, 'is_relevant('), p)
    // An abbreviated hash from gh is resolved in the shell, not refused.
    const short = await run({ ...BASE, mode: 'new' }, { merge: () => ({ ...DEFAULTS.merge(), merge_commit: MC.slice(0, 12), final_tree: T2.slice(0, 12) }) })
    check(!short.result.stopped && has(short.prompts['postmerge:demo'], `'${MC.slice(0, 12)}^{commit}'`), JSON.stringify(short.result.stopped))
    for (const [why, merge, postmerge] of [
      ['no final tree', () => ({ ...DEFAULTS.merge(), final_tree: undefined }), null],
      ['a final tree that is not a hash', () => ({ ...DEFAULTS.merge(), final_tree: 'abc' }), null],
      ['the merge commit is not a hash', () => ({ ...DEFAULTS.merge(), merge_commit: "x'; rm -rf /" }), null],
      ['another tree on main', null, () => postmergeReading({ tree: '5'.repeat(40), treeMatch: false })],
      ['not on main', null, () => postmergeReading({ onMain: false })],
      ['make test not on record', null, () => postmergeReading({ exits: { 'make test': 1 } })],
      ['a no-op merge whose tree is not the apply tree', () => ({ ...DEFAULTS.merge(), final_tree: '6'.repeat(40) }), () => postmergeReading({ tree: '6'.repeat(40), applyTree: false })],
      ['smokes owed after a merge that brought UI files', null, () => postmergeReading({ mergedIn: true, ui: true, exits: { 'make visual-smoke': 1, 'make accessibility-smoke': 0, 'make visual-geometry-smoke': 0 } })],
      ['a dead reading', null, () => null],
    ]) {
      const o = {}
      if (merge) o.merge = merge
      if (postmerge) o.postmerge = postmerge
      const r = await run({ ...BASE, mode: 'new' }, o)
      check(typeof r.result.stopped === 'string' && !has(r.result.stopped, 'Nothing was merged'), `${why}: ${JSON.stringify(r.result)}`)
    }
    const owed = await run({ ...BASE, mode: 'new' }, { merge: () => ({ verdict: 'merged-archive-owed', reason: 'kani', merge_commit: MC, notes: '' }) })
    check(has(owed.result.stopped, 'final_tree'), `merged-archive-owed without final_tree passed: ${JSON.stringify(owed.result)}`)
    const merged = await run({ ...BASE, mode: 'new' }, { postmerge: () => postmergeReading({ mergedIn: true, ui: true, exits: { 'make visual-smoke': 0, 'make accessibility-smoke': 0, 'make visual-geometry-smoke': 0 } }) })
    check(merged.result.verdict === 'merged' && !merged.result.stopped, JSON.stringify(merged.result))
    const quiet = await run({ ...BASE, mode: 'new' }, { postmerge: () => postmergeReading({ mergedIn: true, ui: false }) })
    check(!quiet.result.stopped, `smokes were owed with no UI overlap: ${quiet.result.stopped}`)
    // A classifier that failed is not "no UI file", and a missing helper is no pass.
    const failedUi = await run({ ...BASE, mode: 'new' }, { postmerge: () => postmergeReading({ mergedIn: true, ui: 'failed' }) })
    check(has(failedUi.result.stopped, 'make visual-smoke'), `a failed UI classifier owed no smokes: ${JSON.stringify(failedUi.result)}`)
    const noHelper = await run({ ...BASE, mode: 'new' }, { postmerge: () => postmergeReading({ exits: { 'make check': 99, 'make test': 99 } }) })
    check(has(noHelper.result.stopped, 'lookup exit 99'), JSON.stringify(noHelper.result))
    check(has(p, '|| OK=no') && has(p, '--GATE-- 99'), 'the reading does not fail closed on a helper missing from origin/main')
    const nm = await run({ ...BASE, mode: 'new', no_merge: 'pending' }, { merge: () => ({ verdict: 'ready', reason: 'pending', notes: '' }) })
    check(!nm.work.includes('postmerge:demo'), 'a run that did not merge read the post-merge records')
  },
  async 'every stage that records gates names the gate strings the post-merge check looks up'() {
    const r = await run({ ...BASE, mode: 'new' })
    for (const label of ['implement-1:demo', 'apply:demo', 'merge:demo']) {
      for (const g of ['make check', 'make test', 'make visual-geometry-smoke', 'make accessibility-smoke', 'make visual-smoke']) {
        check(has(r.prompts[label], `'${g}'`), `${label} does not name the gate string '${g}'`)
      }
    }
  },
  async 'Merging step 2: a non-empty origin/main merge reruns every gate on the post-merge tree'() {
    const r = await run({ ...BASE, mode: 'new' })
    const p = r.prompts['merge:demo']
    check(has(p, 'run on it every gate "Merging" step 2 lists') && has(p, 'make test in full') && has(p, 'UI-prefix overlap of either side') && has(p, 'rerun nothing') && !has(p, 'only as "Durability" says'), p)
    check(has(p, 'final_tree'), 'the merge prompt does not ask for the final tree')
  },
  async 'F9: a worktree path is anchored and holds only safe characters'() {
    for (const worktree of ['/re po/.claude/worktrees/demo', '/repo/$(id)/.claude/worktrees/demo', '/repo/../x/.claude/worktrees/demo', 'repo/.claude/worktrees/demo', "/repo/'x'/.claude/worktrees/demo", '/.claude/worktrees/demo']) {
      const r = await run({ ...RO, worktree })
      check(has(r.result.stopped, 'change worktree') && r.calls.length === 0, `${worktree}: ${JSON.stringify(r.result)}`)
    }
    const s = await run({ ...BASE, mode: 'new' }, { spec: () => ({ ...DEFAULTS.spec(), worktree: '/repo;x/.claude/worktrees/demo' }) })
    check(has(s.result.stopped, 'change worktree'), JSON.stringify(s.result))
  },
  async 'a worktree_override run reads and releases the slot named after the worktree'() {
    const o = await run({ ...RO, worktree: '/repo/.claude/worktrees/demo-2', worktree_override: true })
    check(has(o.prompts['guard:prepare:before'], 'IMPL-SLOTS/demo-2"') && has(o.prompts['lens:code-review:demo'], 'IMPL-SLOTS/demo-2') && has(o.prompts['lens:code-review:demo'], 'impl-slots.sh release demo-2'), o.prompts['lens:code-review:demo'])
    const n = await run(RO)
    check(has(n.prompts['guard:prepare:before'], 'IMPL-SLOTS/demo"'), n.prompts['guard:prepare:before'])
  },
  async 'the scope stage names its target instead of changing directory'() {
    const r = await run(RO)
    const prompt = r.prompts['scope:demo']
    check(!/\bcd /.test(prompt) && has(prompt, `${ARG_WT}/scripts/agent-topology.py review-lenses`) && has(prompt, 'LENSES-EXIT'), prompt)
  },
}

async function runAll(source) {
  const failures = []
  activeSource = source
  try {
    for (const [name, fn] of Object.entries(CASES)) {
      try { await fn() } catch (e) { failures.push(`${name}: ${e.message}`) }
    }
  } finally {
    activeSource = SOURCE
  }
  return failures
}

const failures = await runAll(SOURCE)
if (failures.length) {
  for (const f of failures) console.error(`FAIL ${f}`)
  process.exit(1)
}

// The mutant: the guard comparison disabled must fail the guard case.
const GUARD_TEST = "if (!sameReading(before, after)) {"
if (!SOURCE.includes(GUARD_TEST)) {
  console.error(`FAIL mutant: the guard comparison \`${GUARD_TEST}\` is not in the script`)
  process.exit(1)
}
const mutant = SOURCE.replace(GUARD_TEST, 'if (false) {')
const mutantFailures = (await runAll(mutant)).filter(f => f.startsWith('a guard violation'))
if (mutantFailures.length === 0) {
  console.error('FAIL mutant: the harness passed with the guard comparison disabled')
  process.exit(1)
}
console.log(`programme-change harness: ${Object.keys(CASES).length} cases pass; the guard-disabled mutant is caught`)
