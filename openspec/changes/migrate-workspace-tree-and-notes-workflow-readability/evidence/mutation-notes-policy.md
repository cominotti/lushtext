# Mutation coverage for `ui/window/notes/policy.rs` (task 3.8)

Reported as a **gain from zero** with **no parity claim attached**, because
nothing relocated into this module: all five notes domain modules stay in
`model/` (task 3.7), so every mutant below is newly generated coverage extracted
out of a GTK adapter.

## Exact invocation

```
TMPDIR=/var/home/danilo/.cache/lushtext-mutants-tmp \
MUTANTS_SMOKE_FILE=crates/lushtext-core/src/ui/window/notes/policy.rs \
MUTANTS_JOBS=3 MUTANTS_TEST_THREADS=4 \
./scripts/run-mutants.sh smoke
```

File-level anchor only: `crates/lushtext-core/src/ui/window/notes/policy.rs`. No
line-precise anchors are recorded, because they rot on the next edit.

## Infrastructure blocker found and worked around — record this for slots 5b–7

The first three attempts failed **before testing any mutant**, with

```
Error: Failed to copy .../.flatpak-builder/build/lushtext-1/target/debug/incremental/.../dep-graph.bin
Caused by: Disk quota exceeded (os error 122)
```

Diagnosis, recorded because the message says nothing about the cause:

- `.cargo/mutants.toml` sets `gitignore = true`, and `.gitignore:23` does ignore
  `.flatpak-builder/`. `git check-ignore -v .flatpak-builder/build` confirms it.
- But `.flatpak-builder/build/lushtext-1/.git` and `.../lushtext-2/.git` are
  **nested git repositories** created by `flatpak-builder`. cargo-mutants' ignore
  walker treats each as its own repository, whose `.gitignore` does not exclude
  its `target/`, so the copy pulls in **97 GB**.
- cargo-mutants copies into `$TMPDIR`, which here is a **47 GB tmpfs**. Hence a
  quota error rather than a disk-full error.

Two things do **not** fix it: hiding the nested `.git` directories (tried;
cargo-mutants still copied the subtree), and `MUTANTS_IN_PLACE=1` (correctly
refused, because the worktree is dirty and cargo-mutants rewrites sources in
place).

**The working fix is `TMPDIR` on a filesystem with room for the copy**, which is
why the invocation above sets it. The durable fix belongs to whoever owns local
build hygiene: either prune `.flatpak-builder/build/*` between Flatpak builds, or
teach `scripts/run-mutants.sh` to default `TMPDIR` to a large-filesystem scratch
directory. **Recorded rather than applied**, because changing the wrapper's
temp-directory policy for every future run is a build-infrastructure decision
this change should not make unilaterally, and pruning 97 GB of a developer's
build cache is destructive.

## Result

**Complete. 81 mutants, 78 caught, 0 missed, 3 unviable, 0 timeout — zero
survivors.**

| Quantity | Value |
| --- | --- |
| Mutants generated in `ui/window/notes/policy.rs` | **81** |
| Caught | **78** |
| **Missed (survivors)** | **0** |
| Unviable | 3 |
| Timeout | 0 |

All 78 caught mutants are in this file; the focused run's `--file` scope meant no
mutant from another file entered the count. The three unviable mutants are the
usual replace-a-`&'static str`-returning-body cases that do not type-check.

Reported as a **gain from zero**: the module did not exist before this change, so
every one of these 81 mutants is coverage that previously did not exist, extracted
out of a GTK adapter. **No parity claim is attached**, because nothing relocated
into it.

Zero survivors on the first run is unusual for this programme — slot 4's four rows
left 33 survivors on their first pass — and the reason is worth recording rather
than treated as luck. Slot 4's single most common survivor class was *an assertion
comparing a value against the constant it came from*, which cannot detect the
constant changing. The 19 co-located unit tests were written against that finding:

- `workflow_budgets_are_pinned_to_their_reviewed_literals` asserts `0x0400_0000`
  and `0x0040_0000` rather than `64 * 1024 * 1024` and `4 * 1024 * 1024`;
- `limit_message_names_the_caller_supplied_render_limit` uses `7`, deliberately
  **not** the production `500`, so the message-format mutant cannot survive by the
  production constant being substituted;
- `every_mode_string_differs_between_the_two_inventory_modes` walks all nineteen
  mode projections and asserts pairwise difference **and** non-emptiness, which is
  what kills the `Self::Bookmarks => ...` arm-swap mutants that a
  one-string-at-a-time test leaves alive;
- `every_unavailable_reason_has_its_own_explanation` does the same for the five
  excerpt-unavailable reasons, and additionally rejects a reused string;
- the zero/empty/overflow cases (`open_editor_snapshot_capacity_survives_a_zero_snapshot_size`,
  `raw_excerpt_of_no_lines_is_empty_rather_than_panicking`,
  `open_editor_snapshot_reserved_bytes_multiplies_without_overflowing`) kill the
  `saturating_*` → arithmetic and `max(1)` → identity mutants.

## The field-deletion floor, stated so it is not misattributed

`cargo-mutants` 27's `--re` filter **does not apply to struct-field-deletion
mutants**, so a run focused with `MUTANTS_RE` also runs every field-deletion
mutant in scope. **That floor did not apply here**, and the distinction is worth
recording because slot 4's note about it is easy to over-generalize: this run was
focused with `--file` (through `MUTANTS_SMOKE_FILE`), not with `--re`, and all 81
mutants were verified to be in `ui/window/notes/policy.rs`. A future `MUTANTS_RE`
run on this module *will* carry the floor.

The **11 pre-existing surviving field-deletion mutants in
`services/file_tree.rs`** that slot 4 handed to slot 5 are therefore **not
triaged here and not touched here**. They are `WFR-WORKSPACE-TREE`'s, they are
baseline rather than regressions, and their triage (task 5.6) **moves to slot 5b
with the row**.
