## Context

C6 gated highlighting by total size per language (`attach_threshold`, the
size above which the analysis of a whole document needs a worker slice,
ledger A69). C6b measured that edits inside those gates still cost more than
the budgets (programme record, C6b "Finding for the maintainer", M1, F1), and
queued C6c with a planned mechanism: gate highlighting on the longest
blank-line-free run (a profile fold field and a carried bound in the
edit-time watch, Kani-proved), plus a runtime downgrade when an edit needs a
`G_PRIORITY_LOW` worker dispatch.

What the engine does after an edit (GtkSourceView 5.20.0,
`gtksourcecontextengine.c`): the class handler of `insert-text` and
`delete-range` records the invalid region and installs the first update
(`G_PRIORITY_HIGH_IDLE`, l. 2399). `update_syntax` (l. 5308) analyses whole
lines from the first invalid segment; after each line it merges with the old
tree when the old segment at the line end has the same context and the new
state is not a fresh start (l. 5480–5505), and stops there, at the buffer end,
at the caller's `end`, or when its time slice runs out. It then reports the
analysed range in one `highlight-updated` (`refresh_range`, l. 1210) and, if
anything is still invalid, installs the 30 ms `G_PRIORITY_LOW` worker.
`gtk_source_buffer_ensure_highlight(start, end)` runs `update_syntax(end, 0)`
synchronously and tags `[start, end)` (l. 2219; `gtksourcebuffer.c`
l. 1676).

## Goals / Non-Goals

**Goals:** every ordinary key within `B_INTERACT_US` and every
context-flipping key within `B_SETTLE_US`, per dispatch, on the generated and
the worst-case (`--flavor no-closers`) fixture families, at the start and the
middle, without removing highlighting from ordinary files; every exception
measured and recorded.

**Non-Goals:** changing C6's attach thresholds (post-open analysis is
unchanged); editing GtkSourceView; the A67 parse (C6's toolkit-floor
exception).

## Decisions

### D0. Measure real files before choosing a gate (A85)

The A85 sample's `--edit-cost` types one key a third of the way into 24–48
lines spread over a document in an editor-like presented view and reports,
per key, the first update's duration (from an idle queued at
`G_PRIORITY_HIGH_IDLE` just before the insert, which dispatches immediately
before it), the range it reported and whether the worker ran. On real files
from the reference machine's checkouts (the largest blank-line-free run of
each language below its C6 gate, p90 to max; copied to scratch, never
committed), load 0.5–1.1:

| File | Size | Longest run | First update max | Range re-analysed max | Worker |
|---|---|---|---|---|---|
| js (max run) | 16.3 KB | 9.6 K | 9.4 ms | 14.5 K | no |
| js (p99) | 17.6 KB | 4.5 K | 10.9 ms | 13.7 K | yes |
| typescript (max run) | 4.2 KB | 4.1 K | 10.6 ms | 3.9 K | yes |
| css (max run) | 23.1 KB | 5.3 K | 7.5 ms | 4.5 K | no |
| sh (max run) | 31.8 KB | 31.3 K | 3.3 ms | 28.0 K | no |
| sh (p99) | 13.8 KB | 13.7 K | 3.9 ms | 11.5 K | no |
| yaml (max run) | 146.7 KB | 79.4 K | 0.6 ms | 1.3 K | no |

Three findings:

1. **The blank line is not the rule.** The js keys re-analysed 14.5 K
   characters of a document whose longest blank-line-free run is 9.6 K: the
   JavaScript grammar's analysis crosses blank lines. The yaml file has a
   79 K-character run and re-analysed 1.3 K: it merged. The cost is how far
   the analysis runs before it merges, times the grammar's speed per
   character (typescript about 2.5 µs, sh about 0.12 µs).
2. **The lane fixtures are not representative of real shell and yaml.** The
   lane's `short-language` sh fixture costs 40–55 ms per key (C6b); real
   shell scripts with longer runs cost 3.3–3.9 ms.
3. **A static run gate would remove highlighting from ordinary files.** The
   fixtures fail ordinary keys at 4 KiB (js, ts), 8 KiB (css, sh) and 16 KiB
   (yaml). Over the reference machine's checkouts (2,910 js, 108 ts, 12,572
   sh, 19,052 yaml files) the share whose longest run exceeds 4 KiB is 5 %
   (js), 34 % (ts), 29 % (sh) and 27 % (yaml), and above 2 KiB 46–72 %.

So the planned run gate is **replaced** (D3), and the planned runtime
downgrade is extended into the mechanism itself (D1, D2): the only reliable
measure of a key's cost is the engine's own progress, and the pacing below
reads it before the cost is paid.

### D1. Pace the re-analysis of every edit before the first update

The edit after-handlers (`ui/editor_page/document_scale.rs`, after the
watch step) arm one idle at `G_PRIORITY_HIGH_IDLE - 1`
(`ui/editor_page/reanalysis_pacing.rs`). Every step dispatches before the
first update (A85). A step calls `ensure_highlight(bound, bound)`: an empty
range, so the engine analyses from its invalid point to the line after
`bound` and applies no style tag (only the context-class tags every analysis maintains over what it analysed; the view still has the style tags applied for what it draws, as before), and
reports the range in one `highlight-updated`. Then:

- a report that ends before the line before `bound`, or no report, or a bound
  at the buffer end: **merged**; the session ends. The first update that runs
  next finds nothing invalid, reports nothing and installs no worker (A85,
  `paced_small_*`).
- a report that reaches the bound: the session continues from the report's
  end with the next step sized by the pure rule
  (`model::reanalysis_pacing::next_step_chars`): the size that takes
  `STEP_TARGET_US` (a quarter of `B_INTERACT_US`, 2 ms) at the rate the step
  just measured, at most twice the previous step, within 128 characters and
  1 Mi; the first step is 512 characters.

Edits during a session join it (the bound is measured from the earliest
edited line) and **do not reset its spending**, so typing fast cannot keep
frames waiting longer than the allowance. The steps stand down while a load
installation or a buffer replacement owns the buffer (the watch's own
condition) and whenever the language or highlighting is off.

*Alternatives:* timing the first update from idles around it and removing
the language from a `G_PRIORITY_LOW - 1` idle (the first form, A85's first
half) bounds a flip to the first update's 10–13 ms but leaves every ordinary
key's first update as one dispatch of up to 10 ms, above `B_INTERACT_US`;
measured on the lane fixtures, plain: 12.0–12.6 ms first updates. Rejected for
D1, kept in the probe as the timing reference.

### D2. Downgrade at `B_SETTLE_US` of re-analysis, before any worker slice

A session whose steps have spent more than `REANALYSIS_ALLOWANCE_US`
(`B_SETTLE_US`, 16 ms) without merging, or run `MAX_STEPS` (256, a liveness
bound for a coarse clock), **removes syntax highlighting for the document
session**: `DocumentScale::with_edit_reanalysis_reduction` sets
`syntax_enabled` false with the new reason `ReductionReason::EditReanalysis`;
`join` keeps it, so no edit, save or replacement brings it back, and a reopen
or external reload re-profiles the document (a new session) and highlights it
again. Bracket matching stays (it costs nothing per edit). The downgrade runs
from the step idle, never inside the engine's dispatch (removing the language
from a `highlight-updated` handler would free the engine under
`update_syntax`), and removing the language removes the pending first update
and worker (A85) and changes no text (A27). It is reported through the
existing reduced-features notice — one announced status-bar warning, the
properties row and the accessible description ("Syntax highlighting is off
because re-highlighting after an edit was too slow."), never an inline alert.

**Why `B_SETTLE_US`.** Every step is a separate dispatch within
`B_INTERACT_US`, so the allowance does not bound any dispatch; it bounds how
long the **frame** after the edit waits, because the steps run above
`GDK_PRIORITY_REDRAW` (they must run before the first update at
`G_PRIORITY_HIGH_IDLE`, and nothing can run between them and it). Two
allowances were measured with the sample on both fixture families and on the
real files (load 0.5–1.1):

| Allowance | Echo of the keys that spend it (insert to next painted frame) | Highlighting removed |
|---|---|---|
| 10 ms (the engine's `FIRST_UPDATE_TIME_SLICE`) | 14.1–19.2 ms (plain: 14.5–19.9 ms) | also by flips that cost 11–14 ms on `main` (c 64–84 KiB, python3 48 KiB, closers), by 4 KB real TypeScript, and at the edge by a 16 KB real js file |
| 16 ms (`B_SETTLE_US`, shipped) | 19.9–23.4 ms | flips whose paced re-analysis exceeds 16 ms (c 84 KiB without closers re-analyses 86 K characters in 21 ms of steps), the lane's blank-line-free js/ts/css/sh/yaml at their gates, one 17.6 KB real js file |

The user sees a removed highlight for the rest of the session; a frame
4–5 ms later than `main` on the few keys that re-analyse 10–16 ms is barely
perceptible. So the shipped allowance is `B_SETTLE_US`, the budget a flipping
key is allowed, and the echo above two reference frames on those keys is a
recorded exception (input-to-frame is reported, not gated, C6).

*Alternatives:* (a) the 10 ms allowance, above; (b) suspend highlighting while
typing and re-attach at input-quiet: the document flashes between colours and
plain text on every burst, and every re-attach re-analyses the whole document;
rejected for the user. (c) Let the engine take over after the allowance (its
first update and 30 ms worker slices): that is `main`'s behaviour.

### D3. No static run gate

Rejected on D0's measurements: the run is not what bounds the cost (finding
1), and gating on it would remove highlighting from a quarter to a third of
ordinary ts, sh and yaml files (finding 3) while leaving the flips it does not
see. The profile and the watch stay as they are; no new Kani shard is needed
for them.

### D4. The chunked caps stay dormant

With pacing, every flipping key on both fixture families stays within
`B_INTERACT_US` per dispatch, at the C6b chunked sizes too (A85 sample,
`--flip-cost --paced`, load 0.5–1.1): markdown 512 KiB 4.9–6.5 ms, c 512 KiB
5.9–7.2 ms (plain: 34.6 and 33.8 ms). So the per-dispatch criteria that capped
every language in C6b's re-review no longer do. What remains is what a cap
would mean for the user: at 512 KiB the flips' re-analysis exceeds the
allowance, so the document would open highlighted and lose highlighting at
the first flip near the top (c: `/*` at the start or middle; markdown: a code
fence or `<!--` at the start), with a status-bar notice; and the chunked
install, never active in a shipped build, would ship for the first time
(0.3–0.4 s per document, keys held meanwhile, C6b D1). Whether "highlighted
until an edit flips it" beats "not highlighted" for a 320–512 KiB Markdown or
84–512 KiB C file is a product decision with a new, unexercised install path
behind it; it is recorded for the maintainer with these numbers, and no cap is
enabled here. The mechanism stays proved and tested under its model cap.

## Risks / Trade-offs

- **TypeScript and JavaScript.** Their grammars analyse at about 2.5 and
  0.7 µs per character and re-analyse most of what follows an edit, so an edit
  near the start of a TypeScript file above about 6 KB, or of some JavaScript
  files above about 16 KB, re-analyses more than the allowance and removes
  their highlighting (on `main` such a key costs a 10 ms first update plus a
  worker slice). Recorded as the main open question.
- **The echo** of a key that spends the allowance is about the allowance plus
  a frame, up to 23.4 ms measured (D2).
- **Step granularity.** A step analyses whole lines; a step's measured
  duration includes a fixed cost of the first step after an edit (the
  engine's offset fix-up over the tree after the edit), up to 5.4 ms on the
  TypeScript fixture's first lines: the steps' worst case is measured, not
  derived.
- **Machine speed.** The allowance is time; a slower or loaded machine
  downgrades sooner, exactly as the engine there would fall to its worker
  sooner.

## Migration Plan

No persisted state. Rollback is a revert.

## Decisions on the open questions (coordinator, 2026-11-05)

Recorded by the review-and-conclusion stage on the coordinator's decisions,
user experience first; the maintainer may override any of them.

- **The allowance (D2): keep `B_SETTLE_US` (16 ms).** The engine's 10 ms
  would remove highlighting after flips that `main` handles in 11–14 ms and
  from a 4 KB real TypeScript file. The echo exception stays recorded: a key
  that spends the allowance is echoed up to about 23 ms after its insert.
- **Chunked caps (D4): markdown and c at 512 KiB stay dormant.** A document
  that opens highlighted and loses highlighting at the first flip near its
  top is a worse surprise than C6's gate, which leaves it plain from the
  start. Revisit only with a design that avoids the downgrade at that size.
- **Test-seam ceiling: 169 → 170 accepted**, for
  `model::reanalysis_pacing::override_reanalysis_allowance_for_test`, with
  the reason recorded in `docs/workflow-readability-matrix.md` (a
  configuration override returning an RAII guard, set before the edit, which
  no evidence field can do; it reads no fact).
