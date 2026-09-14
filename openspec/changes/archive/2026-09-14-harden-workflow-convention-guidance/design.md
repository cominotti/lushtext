## Context

The workflow-readability programme closed at slot 7b (`a54133b8`,
`ed22c9f4`): all 29 matrix rows are terminal — 22 `migrated`, 5 `cross-cutting`,
1 `exempt`, 1 `superseded` — and `make check-workflow-boundaries` rejects any
transitional status. What survives the programme is the **convention**, and a
review of how that convention reaches a future session found four problems:

1. **The guidance does not load the way it claims to.** `.claude/CLAUDE.md` is a
   symlink to `../.agents/AGENTS.md`, which has never existed; the real file is
   `AGENTS.md` at the repository root. Separately, six of seven `.agents/rules/*.md`
   files carry a Cursor-style `globs:` frontmatter key. Claude Code's documented
   path-scoping key is `paths:` (a YAML list); a file with no `paths:` key loads
   unconditionally. So every one of those six loads globally today while reading
   as if it were scoped — including the 60 KB `build.md` and the 41 KB
   `widget-wiring.md`.
2. **The convention is scattered and historical.** Its obligations are stated in
   `rust.md` (≈213 lines), `widget-wiring.md` (≈114 lines), `AGENTS.md` (≈56
   lines), and `build.md` (≈23 lines), with substantial overlap, and much of the
   prose is programme narrative — "slot 7b was written from…", "three of sixteen
   migrated rows were unchecked this way" — that a reader must decode to find the
   rule.
3. **Post-closure wording assumes a running programme.** `documentation.md` tells
   a change to "advance the matching slot"; `widget-wiring.md` and the
   `gtk-testing` skill tell readers that unmigrated workflows keep their
   `*_for_test` getters and to consult the matrix for "upcoming slots". No
   workflow is unmigrated and no slot is upcoming.
4. **Two convention obligations are ungated.** The role-home classification
   (every module in a migrated home is a role, a presentation surface, or an
   explicitly classified non-role) and the `*_for_test` retirement (165 today,
   down from a census of 300) were both enforced by review during the programme.
   With the programme closed, review pressure is gone.

Constraints on this change: headless only; no edits under
`crates/lushtext-core/src/` or `crates/lushtext/tests/widget/`, because the
accessibility/visual fingerprints key on those **directory prefixes** and a single
byte there voids three screenshot proofs; everything left uncommitted.

## Goals / Non-Goals

**Goals:**

- Make every rule file load exactly as its frontmatter says, and make that
  mechanically checked.
- Give the convention one normative, path-scoped home, with the other files
  pointing at it.
- Move the procedural half of the convention into a skill, where a checklist
  belongs.
- Freeze post-closure wording so a future session does not conclude the programme
  is still running.
- Convert the two review-enforced obligations into gates with self-tests, and fix
  the real findings by declaring, not by weakening.

**Non-Goals:**

- Re-opening the programme, re-migrating any workflow, or amending the convention
  itself. Every normative sentence in the new rule is a condensation of one that
  already exists.
- Changing any application behaviour. No Rust source is touched.
- Rewriting the programme record's history. Only its deferral inventory is
  appended to.
- Ratcheting `cfg(feature = "test-utils")` sites. That count legitimately rises as
  workflows gain gated evidence surfaces.

## Decisions

### D1 — `paths:` lists, and global by omission

Claude Code's documented format (`code.claude.com/docs/en/memory.md`,
"Path-specific rules") is a `paths:` YAML list of globs; a file with no `paths:`
key loads unconditionally. `globs:` is not recognised.

- `build.md`, `documentation.md`, `rust.md`, `ui.md` → `paths:` lists equivalent
  to their current `globs:` intent. `documentation.md` is the exception: its
  `globs: "**/*.{rs,ui,css,toml}"` understates it — it governs *every* change,
  including docs-only ones like this one — so it becomes global.
- `git.md`, `preexisting-blockers.md` were `globs: *`; the key is removed, which
  is the documented always-load form.
- `widget-wiring.md` has no frontmatter at all and gains one.

`description:` is retained on every file. It is not a scoping key, and its
presence is already proven harmless: the current files all carry it and all load.

**Loading scope.** Every one of these files previously loaded *unconditionally*,
because `globs:` was never parsed. Scoping them is therefore a deliberate
narrowing, not an accident of the migration, and the review pass added two
`paths:` entries so that no Rust edit loses a rule it used to get:

- `.agents/rules/build.md` gains `crates/**/*.rs`. Its `make fmt` guidance (the
  widget test modules `cargo fmt --all` cannot reach), the rustdoc lint gate that
  is in no `make` target, and the `git add -N`-before-a-diff-aware-gate hazard all
  apply to any Rust source edit, not only to a build-file edit. The same entry is
  what now reaches its `## Runtime Warnings` section from a Rust change, which is
  the section most likely to be needed there. It stays a list rather than becoming
  global: build.md is the largest rule file and has no bearing on a docs-only or
  packaging-free change.
- `.agents/rules/widget-wiring.md` gains
  `crates/lushtext-core/src/services/action_catalog/**`, so its "Action Catalog
  And Automation Docs" section loads when `services/action_catalog/mod.rs` is
  edited. That file is the one the section governs, and it sits outside `ui/`.

**Always-load marker.** Rather than invent a new frontmatter key (which risks the
agent rejecting an unknown key), the repository's marker is a comment line
`<!-- global rule: loaded for every request -->` immediately after the
frontmatter. It is inert to the agent, visible to a reader, and greppable by the
check. Alternative considered: a `scope: global` frontmatter key — rejected
because an unrecognised key's handling is undocumented, and the whole point of
this item is to stop relying on undocumented frontmatter behaviour.

### D2 — one normative rule, four pointers, one skill

`.agents/rules/workflow-convention.md` becomes the normative home, `paths:`-scoped
to the six populations the convention governs: `crates/lushtext-core/src/ui/**`,
`crates/lushtext/tests/widget/**`, the matrix, the programme record, the gate
script, and `.cargo/mutants.toml`.

- `rust.md` keeps `## Coordination Vocabulary` — it is broader than the convention
  and is reached from *any* workflow — and replaces its convention sections with a
  pointer.
- `widget-wiring.md`'s evidence-surface block condenses to the invariants (one
  accessor per surface, per-object pair for a nested home; no mutation on read;
  `try_get()` including transitively; no toolkit work or materialization; bounded
  aggregation; no read inside a mutable borrow; quiesce worker-thread counters
  before comparing) plus the three driven proofs naming the two reference tests.
  The war stories become at most a one-clause "why".
- `AGENTS.md` keeps the role table (it is the architecture map's natural
  companion) but drops the duplicated normative bullets.
- `build.md` keeps the gate's *operational* description and points at the rule for
  the convention it enforces.

Procedure — the ordered checklist, the proof recipes, the gate ordering — goes to
`.agents/skills/lushtext-workflow/`. A rule states what must be true; a skill
states what to do.

### D3 — role home = facade directory + row-named subdirectories, non-recursive

The check needs a definition of "role home" that cannot make one workflow
responsible for a neighbour's files. Three candidates:

| Candidate | Problem |
| --- | --- |
| every directory containing a declared role path | `WFR-RECENT-DOCUMENTS` declares `ui/window/recent_documents_journal.rs`, so `ui/window/` — which hosts eight other workflows — becomes its role home |
| facade directory, fully recursive | sweeps in nested workflow directories the row does not own |
| **facade directory, plus subdirectories of it the row names, non-recursive in each** | matches the convention's own "nested role home" clause exactly |

The third is chosen. It selects `ui/sidebar/` **and** `ui/sidebar/workspace_section/`
for `WFR-WORKSPACE-TREE` (the nested home the matrix already declares), and it
leaves `ui/window/recent_documents_journal.rs` as what it is: a declared role at a
path outside the home.

`mod.rs` and `imp.rs` are exempt by stem, per the convention (`imp.rs` is GTK
subclass state, named as a presentation surface throughout but present in every
home).

### D4 — declaration is a backticked repository path in the row's own text

The check's haystack for a row is that row's Product Matrix cells **plus** its
`### WFR-*` subsection in `Migrated Workflow Roles`. Both are "the row's text": the
subsection exists only for that row and is already parsed by the gate.

The prototype run against the current tree found **20** real hits in 7 rows. Every
one is a module whose own `//!` doc already classifies it correctly — the matrix
simply names it in a form the check cannot resolve:

- `WFR-SEARCH-REPLACE`, `WFR-WORKSPACE-TREE`: named as bare stems
  (`` `history.rs` ``) or brace expansions
  (`` workspace_section/{mod,imp,row_factory,…}.rs ``), neither of which
  `normalize_claim` resolves.
- the rest: classified in the module doc and in prose elsewhere in the matrix, but
  not in the row's own text.

So the fix is a **machine-readable declaration line** per affected row —
`- called presentation surfaces: <backticked paths>` and, where applicable,
`- non-role modules: …` — added to the row's `Migrated Workflow Roles` subsection.
`parse_role_declarations`'s existing `ROLE_LINE_RE` already accepts those keys, and
`role_findings` ignores roles outside `REQUIRED_ROLES`, so no parser change is
needed. Each path is then also existence-checked by the pre-existing evidence rule,
which is a free strengthening.

Rejected: accepting a bare stem inside the row's text. It would resolve
`` `item.rs` `` against every directory and make a declaration in one row satisfy
another row's home.

### D5 — the `*_for_test` ratchet is one-directional

Predicate, taken verbatim from the programme record's own refreshed Measurement
Definitions: `pub fn` / `pub(crate) fn` whose name ends `_for_test`, under
`crates/lushtext-core/src`. Recomputed today: **165** (162 `pub` + 3 `pub(crate)`),
matching the record exactly; a further 18 are `pub(super)` and are outside the
predicate by design.

Failing only on *excess* is deliberate. A two-sided ratchet would fail every
cleanup that removes a getter until the change also edits the matrix, which trains
a reader to treat the figure as a number to adjust rather than a ceiling to stay
under. The failure message names the preferred remedy first (extend the evidence
surface, delete the getter) and the escape second (raise the figure, with a
reason), so the escape reads as a reviewed decision.

The figure lives in the matrix's `Measurement Definitions` table — the same place
the programme's other denominators live — so the gate and the documentation cannot
disagree about what is being counted.

### D6 — scope of the guidance-loading check

The frontmatter check lands in `scripts/validate-agent-skills.py` rather than in
`check-agent-docs.sh`'s shell body, because that script already owns a
dependency-free frontmatter parser (`parse_yaml_subset`) and already has a
self-test harness (`scripts/test-validate-agent-skills.py`) that
`check-agent-docs.sh` runs. Re-implementing YAML subset parsing in shell would be a
second predicate to keep in sync — the exact failure mode the path-keyed-gate rule
warns about.

The review pass added two further findings to the same check, both of which are
about a scope that *reads* as authoritative while being neither:

- a file carrying **both** a `paths:` list and the always-load marker states its
  scope twice and the two statements disagree, so the file cannot be read as
  authoritative either way;
- a `paths:` glob matching **no tracked file** is dead scope. This is the
  inclusion-side blind spot of the narrowing above: a glob that selects nothing
  cannot fail, so a later rename would silently narrow a rule to nothing while the
  gate kept exiting 0. The tracked set comes from `git ls-files`; when git cannot
  answer, that half is skipped rather than guessed, and the self-test asserts the
  other findings still fire in that case. Glob translation reuses the same `**`
  semantics `scripts/check-workflow-boundaries.py` uses, so the two gates cannot
  disagree about what a repository glob means.

## Risks / Trade-offs

- **Removing `globs:` changes which rules load for a given request.** → That is the
  point, but it means `build.md` and `ui.md` stop loading for unrelated changes. The
  `paths:` lists are written to be at least as wide as the current `globs:` intent,
  and `documentation.md` — the one file whose scope was genuinely *understated* — is
  widened to global rather than narrowed.
- **A path-scoped `workflow-convention.md` will not load for a change that only
  edits the matrix prose.** → The `paths:` list includes the matrix, the programme
  record, the gate script, and the mutation config for exactly this reason.
- **The role-home check could be disarmed by a future rename** (it derives homes
  from declared facade paths, not literal paths). → It is convention-keyed, not
  path-keyed: a moved facade moves the home with it. The self-test asserts the
  finding fires, so a check that matches nothing is caught.
- **The ratchet's figure can be raised to silence it.** → Accepted, and named in the
  spec as the reviewed escape. The alternative — an unraisable ceiling — makes the
  gate unfixable when a seam is genuinely needed, and a change that raises it
  without reason is a review finding, not a gate failure.
- **`make mutants-diff` for parity is not applicable here.** → No `policy.rs` moves
  and no Rust changes at all, so there is no mutation parity claim to make; the
  gate-order reference records when it *is* required.
