# Amendment basis, established from the code, the live specs, and the tool (task 1.1)

Each of the four bases below is confirmed against the **live** artifact rather than
from memory or from the proposal's own prose. Where the confirmation produced a
number, the number is recorded, because a stated floor with no number is not a
stated floor.

## 1. Dissolution — the live spec's only response to "no name fits" is amendment

`openspec/specs/gtk-adapter-module-boundaries/spec.md:214-216`, quoted verbatim:

> A coordination job that no existing role name describes MUST be added
> to the bounded set by amending this specification, rather than reusing an ill-fitting
> name or inventing an unlisted one.

**The word `dissolv*` appears zero times in the live spec** (`grep -c 'dissolv'`
→ `0`). The prior question — *is this module one coordination job at all* — is
therefore genuinely unstated, and the amendment adds it rather than restating it.

Confirmed from the code that each of the three modules contains more than one kind
of thing, so the prior question has a real answer for this row:

| Module | Raw / production | Contents that do not cohere into one job |
| --- | --- | --- |
| `workspace_section/tree_index.rs` | 969 / 844 | pure index arithmetic (splice windows, changed-path→owning-directory, common prefix/suffix, desired-versus-current diff) **plus** child-store lookup and cache maintenance (`find_store_for_dir` mutates `dir_stores`, `find_dir_row` evicts from `dir_rows`) **plus** the expansion-set mutation sites (`:61`, `:81-108`, `:120-131`, `:140-158`) **plus** a capture derivation that advances evidence counters (`:28`, counters `:31-39`) |
| `workspace_section/watch_targets.rs` | 337 / 264 | pure mirror arithmetic **plus** two generation newtypes **plus** a snapshot **plus** the destructive `take_touched_rows` reset (`:237`) |
| `workspace_section/tree_loading.rs` | 1,269 / 1,269 | process-global scan admission statics (`:77-78`) and the admission retry (`:419-478`) **plus** the child scan worker, child-store identity/mirror/splice, batched reconciliation, directory-state clearing and the deferred expansion restore **plus** `build_children_model` (`:115`, the `GtkTreeListModel` create function) **plus** the DnD drag-hover empty child model (`:130`) with its `thread_local!` counter (`:109-111`) and two seams |

No bounded role name (`admission`, `execution`, `retirement`, `watch`, `journal`)
describes any of the three **as a whole**. Naming any of them would add a role for a
pre-convention *topic*, which is what the closed taxonomy exists to prevent.

**Three dissolutions rather than the two slot 5a decided.** `tree_loading.rs` — the
row's **largest** file — was never classified in 5a's module map, whose own heading
read "Every module classified". A pattern recurring three times in one row is
evidence the escalation-only spec text was wrong rather than merely incomplete.

## 2. Already-correctly-named — the qualification paragraph does not scope itself

`openspec/specs/gtk-adapter-module-boundaries/spec.md:230-237`, quoted verbatim:

> Where **one** workflow owns more than one ordered stage order in a single directory,
> and more than one of those stage orders needs a coordination module of the same
> shape, a coordination module name MAY qualify a bounded role name with the stage
> order it serves. The qualifier names the stage order in the workflow's own domain
> vocabulary and the suffix remains a bounded role name, so the role stays readable
> and the bounded set is not widened by the qualification. A workflow MUST NOT take an
> ill-fitting bounded name merely because the fitting one is already spent on a
> different stage order of the same workflow.

**Confirmed: the paragraph says nothing about which modules it applies to.** It does
not scope itself to modules a migration creates or renames, and it says nothing
about a stable sibling that is already correct. Slot 2b read it narrowly and every
slot since has followed that reading, but the narrow reading exists **only in task
prose**, not in the requirement. The amendment states the scope the practice already
has.

This row is where the gap bites: `workspace_section/watch.rs` already carries the
bounded role name `watch` and is correct. Renaming it to `watch_execution.rs` for
symmetry with eight newly named `*_execution.rs` siblings would be churn a reader
must diff to understand.

## 3. Mutation floor — confirmed from the tool, with an exact number

**Not confirmed from memory.** `cargo-mutants 27.0.0` (matching the
`CARGO_MUTANTS_VERSION=27.0.0` CI pin) documents `--re` as:

```
-F, --re <EXAMINE_RE>
        Regex for mutations to examine, matched against the names shown by `--list`
```

Field-deletion mutants **do** appear in `--list` output with names of the form
`delete field <f> from struct <S> expression in <fn>`, so the filter looks as if it
should apply to them. It does not. Confirmed by running the repo's own wrapper with
a pattern that cannot match any mutant name:

```
MUTANTS_RE='ZZZ_NO_SUCH_MUTANT_ZZZ' make mutants-list
```

**Result: 34 mutants are still generated, and all 34 are `delete field` mutants.**
That is the unfilterable floor for this repository's configured
`examine_globs` scope, and it is the number every focused run in this change must
state. A second control run confirms the arithmetic composes:

```
MUTANTS_RE='WorkspaceScanFlight' make mutants-list
→ 50 mutants = 16 in model/workspace_scan.rs (the intended filter target)
             + the same 34-mutant floor, spread across 7 unrelated files
```

Per-file distribution of the 34-mutant floor:

| File | Field-deletion mutants |
| --- | --- |
| `services/file_tree.rs` | **12** |
| `services/draft_service.rs` | 11 |
| `services/markdown_render.rs` | 3 |
| `ui/window/session_restore/policy.rs` | 2 |
| `ui/window/notes/policy.rs` | 2 |
| `services/single_flight.rs` | 2 |
| `services/local_history_service.rs` | 2 |
| **total** | **34** |

**A correction the inherited handoff needs, in the *upward* direction for generated
and unchanged for surviving.** Slots 4 and 5a handed on "**11** pre-existing
surviving field-deletion mutants in `services/file_tree.rs`". The tool generates
**12** field-deletion mutants in that file. Those are two different quantities:
12 **generated**, of which 11 were recorded as **surviving** — so exactly one is
already killed. Task 10.7 triages against the generated list of 12 and reports which
one is killed, rather than assuming the inherited 11 is the whole population. The 12,
enumerated from the tool:

| Line | Field | Enclosing function |
| --- | --- | --- |
| `:297` | `examined_entries` | `scan_directory_bounded_with_cancel_and_bytes` |
| `:298` | `error` | same |
| `:305` | `examined_entries` | same |
| `:306` | `cancelled` | same |
| `:348` | `examined_entries` | same |
| `:349` | `peak_retained_entries` | same |
| `:350` | `peak_retained_bytes` | same |
| `:351` | `error` | same |
| `:441` | `examined_entries` | `scan_directory_without_byte_limit` |
| `:442` | `peak_retained_entries` | same |
| `:443` | `peak_retained_bytes` | same |
| `:444` | `error` | same |

All twelve are `DirectoryScan` struct-literal fields in two scan functions —
i.e. **scan-metrics reporting fields**, which is what makes them survive: the
functions' behavioral contract is asserted through the returned entries, not through
the metrics. That shape is the triage input for task 10.7.

**The live `mutation-testing` spec states none of this**: `grep -ni
'floor|field-deletion|unfilterable'` over
`openspec/specs/mutation-testing/spec.md` returns **zero** occurrences.

## 4. Parity versus gain — the live spec does not require separation

`grep -ni 'separate|separately|gain'` over the live
`openspec/specs/mutation-testing/spec.md` returns three hits, and **none** is about
reporting relocation parity separately from extraction gain:

- `:20` — "a workflow's pure policy is **separated** from its GTK adapter by module" (a scenario precondition about code placement, not reporting);
- `:33` — CI running cargo-mutants against the PR diff;
- `:135` — adding a "**separate** documented mutation mode" rather than changing the default lane.

The live *Policy relocation requires mutation parity evidence* requirement demands
before/after parity for a relocation, and says nothing about a change that **also**
extracts new policy from an adapter.

Confirmed that slot 4 had only the gain case, so the gap could not bite there:
its relocations landed in files that were **not previously** inside `examine_globs`,
so there was no before-count and parity was not a real claim. **This change has
both**, which is what makes the statement load-bearing here:

| Kind | Modules | Before-count exists? | Claim |
| --- | --- | --- | --- |
| **relocation → parity** | `model/workspace_scan.rs`, `model/workspace_persistence.rs` | **yes** — both already inside `examine_globs` via `crates/lushtext-core/src/model/**/*.rs` | parity is a real claim that **can fail** |
| **extraction → gain from zero** | task 3.1's extraction out of the GTK adapters into `ui/sidebar/policy.rs` | **no** — GTK adapters are outside the scope by design | gain from zero, **cannot fail** |

Baseline generated counts captured from the tool for the parity claim:

| Module | Mutants generated (before the move) |
| --- | --- |
| `crates/lushtext-core/src/model/workspace_scan.rs` | **16** |
| `crates/lushtext-core/src/model/workspace_persistence.rs` | **34** |

One aggregate figure over these two populations plus the extraction would let a
parity loss in the 16 or the 34 disappear behind the extraction's gain.

## 5. Both deltas are pure additions (task 1.2)

Checked per requirement, comparing non-blank lines of each requirement body in the
delta against the same requirement body in the live spec:

| Spec | Requirement | Live non-blank | Delta non-blank | **Removed** | Added |
| --- | --- | --- | --- | --- | --- |
| `gtk-adapter-module-boundaries` | Decomposed workflow modules carry named roles | 168 | 208 | **0** | 40 |
| `mutation-testing` | Mutation Triage Policy | 7 | 32 | **0** | 25 |
| `mutation-testing` | Policy relocation requires mutation parity evidence | 20 | 36 | **0** | 16 |

**Zero removed non-blank lines in all three requirements**, so both deltas are pure
additions carrying the full updated requirement text, and every requirement named in
a delta already exists in the live spec (so `MODIFIED` is the correct disposition for
each — none is a disguised addition).

## Verdict

All four bases confirmed, and both deltas verified as pure additions. Both spec deltas are supported by the live text rather
than by restatement, and both mutation statements are backed by a measured number
rather than by recollection.
