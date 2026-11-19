// SPDX-License-Identifier: GPL-3.0-or-later
//
// Stub-agent harness for programme-change.js, run by scripts/check-workflow-scripts.sh
// (`node programme-change.test.mjs <path to programme-change.js>`).
//
// It loads the script body the way the workflow harness does (top-level await
// and return inside an async function), replaces agent() with a responder keyed
// on each call's label, and checks the control flow of every mode: dry runs and
// bad arguments, the finished path, review-only, failed lenses, a guard
// violation, a malformed selector and the fast lane. It then disables the
// guard comparison in a copy of the source and requires the guard case to fail,
// so a harness that stopped checking the guard is caught.
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

const READING = { porcelain: '', head: 'abc123', branch: 'main' }
const SPEC_WT = '/repo/.claude/worktrees/demo-spec'
const ARG_WT = '/repo/.claude/worktrees/demo-arg'
const DEFAULTS = {
  guard: () => ({ ...READING }),
  archive: () => 'archived',
  decision: () => ({ decision: 'continue', reasons: 'stub' }),
  spike: () => 'spike file',
  spec: () => ({ decision: 'viable', change_name: 'demo', worktree: SPEC_WT, branch: 'demo-spec-branch', fast_lane: 'not-applicable', evidence: 'spec-evidence', summary: 'spec-summary' }),
  'spec-review': () => ({ verdict: 'approved', findings: 'none' }),
  implement: () => ({ tasks_complete: true, pr_number: '#11', gates: 'impl-gates', report: 'impl-report' }),
  simplify: () => ({ edited_files: ['a.rs'], tests_run: 'simplify-tests', summary: 'simplify-summary' }),
  flags: () => ({ data: false, performance: false, modules_moved: false, paths: ['docs/x.md'] }),
  'lens:code-review': () => '- a.rs:3 off by one\nskill_findings_source: result-text\nCODE-REVIEW-STATUS: ok',
  'lens:code-review-2': () => '- b.rs:9 leak\nskill_findings_source: fork-transcript\nCODE-REVIEW-STATUS: ok',
  lens: () => ({ skill_ran: 'stub', findings: [], findings_file: 'f.md' }),
  apply: () => ({ verdict: 'ready', applied: 'apply-applied', rejected: 'apply-rejected', gates: 'apply-gates', summary: 'apply-summary' }),
  merge: () => ({ verdict: 'merged', notes: 'n' }),
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
async function run(args, overrides = {}, source = activeSource) {
  const calls = []
  const prompts = {}
  const agent = async (prompt, opts = {}) => {
    const label = opts.label || ''
    calls.push(label)
    prompts[label] = prompt
    const kind = kindOf(label)
    const fn = overrides[label] || overrides[kind] || DEFAULTS[kind]
    if (!fn) throw new Error(`no stub for agent label ${label}`)
    return fn(label, prompt, opts)
  }
  const parallel = async thunks => Promise.all(thunks.map(t => t().catch(() => null)))
  const body = new AsyncFunction('args', 'agent', 'parallel', 'pipeline', 'phase', 'log', 'budget', 'workflow', source)
  const result = await body(args, agent, parallel, undefined, () => {}, () => {}, { total: null, spent: () => 0, remaining: () => Infinity }, undefined)
  return { result, calls, prompts, work: calls.filter(l => !l.startsWith('guard:')) }
}

function check(cond, message) {
  if (!cond) throw new Error(message)
}
const has = (s, needle) => typeof s === 'string' && s.includes(needle)

const BASE = { change: 'demo', local: 'stub host details' }
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
    for (const ref of ['main', 'master', 'HEAD', 'heads/main', 'refs/heads/main', 'origin/main', 'remotes/origin/main', 'tags/v1']) {
      const asChange = await run({ ...BASE, change: ref, mode: 'new' })
      const asBranch = await run({ ...BASE, branch: ref, mode: 'review-only', pr: 7, worktree: ARG_WT })
      check(has(asChange.result.stopped, 'main-branch alias') && asChange.calls.length === 0, `${ref} as change: ${JSON.stringify(asChange.result)}`)
      check(has(asBranch.result.stopped, 'main-branch alias') && asBranch.calls.length === 0, `${ref} as branch: ${JSON.stringify(asBranch.result)}`)
    }
    const fromSpec = await run({ ...BASE, mode: 'new' }, { spec: () => ({ ...DEFAULTS.spec(), branch: 'origin/main' }) })
    check(has(fromSpec.result.stopped, 'main-branch alias') && !fromSpec.work.some(l => l.startsWith('spec-review:')), JSON.stringify(fromSpec.result))
  },
  async 'a worktree outside .claude/worktrees is refused'() {
    const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: '/repo' })
    check(has(r.result.stopped, 'change worktree') && r.calls.length === 0, JSON.stringify(r.result))
    const s = await run({ ...BASE, mode: 'new' }, { spec: () => ({ ...DEFAULTS.spec(), worktree: '/repo' }) })
    check(has(s.result.stopped, 'no change worktree'), JSON.stringify(s.result))
  },
  async 'PR values are normalized from n, #n and pull URLs'() {
    for (const [value, want] of [[7, 7], ['7', 7], ['#7', 7], ['https://github.com/o/r/pull/7', 7], ['https://github.com/o/r/pull/7/', 7]]) {
      const r = await run({ ...BASE, mode: 'review-only', pr: value, worktree: ARG_WT, merge: true })
      check(has(r.prompts[`merge:demo`], 'PR #7,') && has(r.prompts['apply:demo'], 'PR #7,'), `${JSON.stringify(value)}: ${r.prompts['merge:demo']}`)
      check(want === 7, 'table')
    }
    for (const bad of ['seven', '#', 'https://example.org/pull/7', 'https://example.org/o/r/pull/7', '0', '-3']) {
      const r = await run({ ...BASE, mode: 'review-only', pr: bad, worktree: ARG_WT })
      check(has(r.result.stopped, 'not a PR number') && r.calls.length === 0, `${bad}: ${JSON.stringify(r.result)}`)
    }
    const c = await run({ ...BASE, mode: 'continue', spec: { change_name: 'demo', worktree: ARG_WT, branch: 'demo' }, pr: 9 },
      { implement: () => ({ ...DEFAULTS.implement(), pr_number: 'https://github.com/o/r/pull/12' }) })
    check(has(c.prompts['merge:demo'], 'PR #12,'), 'the stage PR wins over args.pr once implementation reports it')
    const s = await run({ ...BASE, mode: 'continue', skip_impl: true, pr: 9, spec: { change_name: 'demo', worktree: ARG_WT, branch: 'demo' },
      impl: { tasks_complete: true, pr_number: '#12', report: 'carried-report', gates: '' } })
    check(has(s.prompts['merge:demo'], 'PR #12,') && has(s.prompts['merge:demo'], 'carried-report'), 'a carried implementation keeps its PR')
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
      const r = await run({ ...BASE, mode, pr: 7, worktree: ARG_WT, dry_run: true })
      check(r.calls.length === 0 && r.result.dry_run === true, `${mode}: ${JSON.stringify(r)}`)
      plans[mode] = r.result.stages
    }
    check(plans.new[0] === 'Spec' && plans.new.includes('Spec review') && plans.new.at(-1) === 'Merge and archive', JSON.stringify(plans.new))
    check(!plans.continue.includes('Spec') && plans.continue[0] === 'Implementation', JSON.stringify(plans.continue))
    check(plans['review-only'][0] === 'Simplify' && !plans['review-only'].includes('Merge and archive'), JSON.stringify(plans['review-only']))
    const merging = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT, merge: true, dry_run: true })
    check(merging.result.stages.at(-1) === 'Merge and archive', JSON.stringify(merging.result))
    const decided = await run({ ...BASE, mode: 'revise', revision: 3, dry_run: true })
    check(decided.result.stages[0] === 'Revision decision', JSON.stringify(decided.result))
  },
  async 'new mode finalizes through the merge stage'() {
    const r = await run({ ...BASE, mode: 'new' })
    check(r.result.merge && r.result.merge.verdict === 'merged' && !r.result.stopped, JSON.stringify(r.result))
    const order = ['spec:', 'spec-review:', 'implement-1:', 'simplify:', 'flags:', 'lens:code-review:', 'apply:', 'merge:']
    let at = -1
    for (const prefix of order) {
      const next = r.work.findIndex((l, i) => i > at && l.startsWith(prefix))
      check(next > at, `stage ${prefix} missing or out of order in ${JSON.stringify(r.work)}`)
      at = next
    }
    check(r.calls.filter(l => l.startsWith('guard:')).length >= 2 * (order.length - 1), 'every stage is guarded')
  },
  async 'review-only starts at simplify and stops before merge'() {
    const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT })
    check(r.work[0].startsWith('simplify:'), JSON.stringify(r.work))
    check(!r.work.some(l => /^(spec|implement|merge)/.test(l)), JSON.stringify(r.work))
    check(has(r.result.verdict, 'stopped before merge'), JSON.stringify(r.result))
    const m = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT, merge: true })
    check(m.work.at(-1).startsWith('merge:') && m.result.merge, JSON.stringify(m.work))
  },
  async 'a bare (none) from code-review is a failed lens after one rerun'() {
    const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT }, {
      'lens:code-review': () => '(none)', 'lens:code-review-2': () => '(none)',
    })
    check(has(r.result.stopped, 'review lens failed: code-review'), JSON.stringify(r.result))
    check(r.work.some(l => l.startsWith('lens:code-review-2:')) && !r.work.some(l => l.startsWith('apply:')), JSON.stringify(r.work))
  },
  async 'a missing status line is rerun once and then accepted'() {
    const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT }, {
      'lens:code-review': () => '- a.rs:3 off by one\nskill_findings_source: result-text',
    })
    check(!r.result.stopped && r.work.some(l => l.startsWith('lens:code-review-2:')) && r.work.some(l => l.startsWith('apply:')), JSON.stringify(r.result))
  },
  async 'empty-looking reviews marked ok still fail the lens'() {
    for (const body of ['[\n]', '```json\n[\n]\n```', '## Summary\nNo findings.', '(none)', '']) {
      const text = `${body}\nskill_findings_source: result-text\nCODE-REVIEW-STATUS: ok`
      const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT }, {
        'lens:code-review': () => text, 'lens:code-review-2': () => text,
      })
      check(has(r.result.stopped, 'review lens failed: code-review'), `${JSON.stringify(body)}: ${JSON.stringify(r.result)}`)
    }
  },
  async 'a status line that is not last, or a source line missing, fails'() {
    for (const text of ['CODE-REVIEW-STATUS: ok\n- a.rs:1 x\nskill_findings_source: result-text', '- a.rs:1 x\nCODE-REVIEW-STATUS: ok']) {
      const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT }, {
        'lens:code-review': () => text, 'lens:code-review-2': () => text,
      })
      check(has(r.result.stopped, 'review lens failed'), `${JSON.stringify(text)}: ${JSON.stringify(r.result)}`)
    }
  },
  async 'lens failed text is a failed lens'() {
    const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT }, {
      'lens:code-review': () => '- a.rs:1 x\nskill_findings_source: result-text\nno agentId\nCODE-REVIEW-STATUS: failed',
      'lens:code-review-2': () => 'no transcript\nCODE-REVIEW-STATUS: failed',
    })
    check(has(r.result.stopped, 'review lens failed'), JSON.stringify(r.result))
  },
  async 'a dead domain lens agent fails the run'() {
    const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT }, {
      flags: () => ({ data: true, performance: false, modules_moved: false, paths: ['crates/x.rs'] }),
      'lens:data-safety:demo': () => null,
    })
    check(has(r.result.stopped, 'data-safety'), JSON.stringify(r.result))
  },
  async 'lens selection follows the flags'() {
    const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT }, {
      flags: () => ({ data: true, performance: true, modules_moved: true, paths: [] }),
    })
    const want = ['code-review', 'data-safety', 'gtk-responsiveness', 'gtk-perf-scale', 'gtk-perf-rust-optimize', 'rust-hex-arch']
    check(JSON.stringify(r.result.lenses) === JSON.stringify(want), JSON.stringify(r.result.lenses))
    for (const lens of want) check(r.work.some(l => l.startsWith(`lens:${lens}:`)), `no agent for ${lens}`)
  },
  async 'a malformed selector stops before any lens'() {
    const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT }, {
      flags: () => ({ data: 'yes', paths: [] }),
    })
    check(has(r.result.stopped, 'malformed'), JSON.stringify(r.result))
    check(!r.work.some(l => l.startsWith('lens:')), JSON.stringify(r.work))
  },
  async 'a guard violation stops the run at that stage'() {
    const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT }, {
      'guard:simplify:after': () => ({ ...READING, head: 'moved' }),
    })
    check(has(r.result.stopped, 'MAIN CHECKOUT CHANGED during simplify'), JSON.stringify(r.result))
    check(!r.work.some(l => l.startsWith('flags:')), JSON.stringify(r.work))
  },
  async 'a dead guard agent stops the run'() {
    const r = await run({ ...BASE, mode: 'new' }, { 'guard:spec:before': () => null })
    check(has(r.result.stopped, 'guard reading before spec failed') && r.work.length === 0, JSON.stringify(r))
  },
  async 'fast lane: an eligible spec skips the spec review'() {
    const r = await run({ ...BASE, mode: 'fast-lane' }, {
      spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'eligible' }),
    })
    check(!r.work.some(l => l.startsWith('spec-review:')) && r.result.merge, JSON.stringify(r.work))
  },
  async 'fast lane: an ineligible spec gets the spec review'() {
    const r = await run({ ...BASE, mode: 'fast-lane' }, {
      spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'not-eligible' }),
    })
    check(r.work.some(l => l.startsWith('spec-review:')), JSON.stringify(r.work))
  },
  async 'fast lane: a violation at apply sends the change to a spec review and stops'() {
    const r = await run({ ...BASE, mode: 'fast-lane' }, {
      spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'eligible' }),
      apply: () => ({ verdict: 'fast-lane-violation', applied: '', rejected: '', gates: '', summary: 'src path' }),
    })
    check(has(r.result.stopped, 'fast-lane violation'), JSON.stringify(r.result))
    check(r.work.at(-1).startsWith('spec-review:') && !r.work.some(l => l.startsWith('merge:')), JSON.stringify(r.work))
  },
  async 'an incomplete implementation stops before the review stages'() {
    const r = await run({ ...BASE, mode: 'new' }, {
      implement: () => ({ tasks_complete: false, gates: '', report: 'r', unchecked_tasks: '5.1' }),
    })
    check(has(r.result.stopped, 'implementation incomplete') && !r.work.some(l => l.startsWith('simplify:')), JSON.stringify(r.result))
  },
  async 'no_merge passes the reason to the merge stage'() {
    let seen = ''
    await run({ ...BASE, mode: 'new', no_merge: 'template review pending' }, {
      merge: (_l, prompt) => { seen = prompt; return { verdict: 'ready', notes: '' } },
    })
    check(has(seen, 'DO NOT MERGE') && has(seen, 'template review pending'), 'no_merge reason missing')
  },
  async 'the merge prompt carries the stopping rule and the push rule'() {
    const r = await run({ ...BASE, mode: 'new' })
    const prompt = r.prompts['merge:demo']
    check(has(prompt, '"no findings" is not the merge test') && has(prompt, 'review only that commit'), 'stopping rule missing')
    check(has(prompt, 'never push with --force or --force-with-lease'), 'force-push rule missing')
  },
  async 'every stage prompt carries the foreground rule'() {
    const prompts = []
    await run({ ...BASE, mode: 'new' }, Object.fromEntries(Object.keys(DEFAULTS).filter(k => k !== 'guard').map(k => [k, (l, p, o) => { prompts.push([l, p]); return DEFAULTS[k](l, p, o) }])))
    check(prompts.length > 0, 'no stage prompt was captured')
    for (const [label, prompt] of prompts) {
      if (label.startsWith('flags:')) continue
      check(has(prompt, 'never return, or end your turn, while a command you started is still running'), `${label} lacks the foreground rule`)
      check(has(prompt, 'git fetch --prune') && has(prompt, 'origin/<branch>') && has(prompt, 'exit status'), `${label} lacks the fetch, branch-merge or exit-status rule (note 21:35)`)
    }
  },
  // Stage-4 review (2026-11-18) cases.
  async 'continue mode refuses a carried spec outside a change worktree or on a main alias'() {
    for (const spec of [
      { change_name: 'demo', worktree: '/repo', branch: 'demo' },
      { change_name: 'demo', worktree: ARG_WT, branch: 'origin/main' },
    ]) {
      const r = await run({ ...BASE, mode: 'continue', pr: 9, spec })
      check(typeof r.result.stopped === 'string' && r.calls.length === 0, `${JSON.stringify(spec)}: ${JSON.stringify(r.result)} after ${JSON.stringify(r.calls)}`)
    }
  },
  async 'dot segments and @ are not change worktrees or branches'() {
    for (const worktree of ['/repo/.claude/worktrees/..', '/repo/.claude/worktrees/../', '/repo/.claude/worktrees/.']) {
      const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree })
      check(has(r.result.stopped, 'change worktree') && r.calls.length === 0, `${worktree}: ${JSON.stringify(r.result)}`)
    }
    const at = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT, branch: '@' })
    check(has(at.result.stopped, 'main-branch alias') && at.calls.length === 0, JSON.stringify(at.result))
  },
  async 'a fast-lane run its spec stage found not eligible runs the normal stages'() {
    const r = await run({ ...BASE, mode: 'fast-lane' }, { spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'not-eligible' }) })
    check(!has(r.prompts['implement-1:demo'], 'check-fast-lane') && !has(r.prompts['apply:demo'], 'check-fast-lane'), 'fast-lane instructions reached a not-eligible change')
    check(r.result.merge && !r.result.stopped, JSON.stringify(r.result))
    const e = await run({ ...BASE, mode: 'fast-lane' }, { spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'eligible' }) })
    check(has(e.prompts['implement-1:demo'], 'check-fast-lane') && has(e.prompts['apply:demo'], 'check-fast-lane'), 'an eligible fast-lane change lost its re-checks')
  },
  async 'a resumed fast-lane change keeps the apply-stage spec review'() {
    const c = await run({ ...BASE, mode: 'continue', pr: 9, spec: { change_name: 'demo', worktree: ARG_WT, branch: 'demo', fast_lane: 'eligible' } })
    check(has(c.prompts['apply:demo'], 'check-fast-lane'), 'continue lost the fast-lane review of the spec')
    const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT, fast_lane: 'eligible' })
    check(has(r.prompts['apply:demo'], 'check-fast-lane'), 'review-only lost the fast-lane review of the spec')
    const n = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT })
    check(!has(n.prompts['apply:demo'], 'check-fast-lane'), 'a normal review-only run got fast-lane instructions')
    const v = await run({ ...BASE, mode: 'fast-lane' }, {
      spec: () => ({ ...DEFAULTS.spec(), fast_lane: 'eligible' }),
      apply: () => ({ verdict: 'fast-lane-violation', applied: '', rejected: '', gates: '', summary: 'src path' }),
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
  async 'continue with skip_impl and no PR anywhere stops before simplify'() {
    const r = await run({ ...BASE, mode: 'continue', skip_impl: true, spec: { change_name: 'demo', worktree: ARG_WT, branch: 'demo' },
      impl: { tasks_complete: true, report: 'r', gates: '' } })
    check(has(r.result.stopped, 'no pull request') && !r.work.some(l => l.startsWith('simplify:')), JSON.stringify(r.result))
  },
  async 'an empty pr argument means no PR'() {
    const r = await run({ ...BASE, mode: 'new', pr: null })
    check(r.result.merge && !r.result.stopped, JSON.stringify(r.result))
  },
  async 'the flags stage names its target instead of changing directory'() {
    const r = await run({ ...BASE, mode: 'review-only', pr: 7, worktree: ARG_WT })
    const prompt = r.prompts['flags:demo']
    check(!/\bcd /.test(prompt) && has(prompt, `${ARG_WT}/scripts/agent-topology.py review-lenses`), prompt)
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
