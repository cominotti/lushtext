# Design

## Context

See `proposal.md`. The parts involved:
- the custom harness (`tests/widget.rs`, `harness = false`) includes `OUT_DIR/widget_test_registry.rs`;
- `build.rs` writes that registry, plus one transformed copy per file;
- `scripts/widget-shards.py::discover` mirrors `build.rs::extract_test_functions`, which reads exact `#[test]` lines and then the first `fn`;
- each shard job cross-checks discovery against `--list`.

## Goals / Non-Goals

**Goals:** diagnostics on real files; the window tests findable by theme; one start page. **Non-Goals:** `proposal.md`.

## Decisions

### D1. One declaration rule, no rewrite

- **The rule.** A test is a top-level `pub(crate) fn test_<name>()`, and that is the whole registration rule.
  - `build.rs` generates `#[path = "<real file>"] mod <stem>;` and the push list, and writes no copy.
  - `widget-shards.py` uses the same line rule, with its self-test cases updated.
- **Why it is safe.** The registry pushes `fn()` pointers. A helper named `test_*` with parameters or a return value fails to compile at its registration line, which is loud. A zero-argument, unit-returning helper with that prefix would register as a test, so helpers stop using the prefix: `test_window` becomes `new_test_window`, and so on.
- **Rejected:**
  - a `#[widget_test]` proc macro with `linkme` or `inventory`: a new crate and dependency, and rustfmt still sees the same functions;
  - keeping `#[test]` as a marker under `#[cfg_attr]`: unidiomatic, and still a textual rule;
  - keeping the copies: the problem itself.

### D2. Splitting `window.rs`

- **Inventory first.** Task 1.1 groups the 407 tests by the workflow they drive, using the workflow matrix rows' vocabulary (notes, local history, drafts, session restore, Focus Mode, menus and help, close safety, split views and geometry, print, preview, status and notifications, documents and tabs). It records the grouping in `evidence/renames.md` before anything moves.
- **The files.** One `window_<theme>.rs` per group. Shared fixtures and helpers move to `window_support.rs`, which holds no tests and is reached as `crate::window_support`, the way `common.rs` is.
- **No file runs past about 4,000 lines.** This is a target for the split, not a gate.
- **Mechanical move.** Function bodies do not change, only their file and `use` lines. Task 1.3 diffs the sorted, whitespace-normalized function bodies before and after to prove it.
- **Shards.** The `window` shard's `modules` list becomes the new stems, and every explicit `window::test_` name in the table is renamed. Membership is unchanged, so each shard's measured `ci_minutes` still describes the same tests. That claim is checked by the shard check's own counts.
- **Rejected:**
  - a `window/` subdirectory with nested module names (`window::notes::test_x`): the shard table, `--list` parsing and every consumer split names on one `::`, and three-part names would change all of them;
  - keeping the old `window::` names through an alias table: two names per test, forever.

### D3. Moving references

- **Find them.** Task 0.1 lists every tracked file outside `openspec/changes/archive/` and `build/` that names `window::test_`; there are 12 on `origin/main`.
- **Rename them.** Each name is rewritten from the mapping in `evidence/renames.md`.
- **Prove it.** After the move, a grep for `window::test_` outside the archive returns nothing, and every `window_<theme>::test_` named in a script or document exists in `--list`.
- **No permanent gate.** This is a one-time check, recorded in the task, not a new gate.

### D4. The quickstart

`CONTRIBUTING.md` is about one page. Each section ends in a link to its normative source:
- **setup:** the toolbox, `make dev-tools`;
- **run:** `make run`;
- **check:** `make check` and `make test`, with how long they take;
- **commit:** conventional commits, signing, the hook and its docs-only path;
- **where tests go:** unit, integration, property and widget tests, linking the `gtk-testing` skill;
- **which rule to read:** a table from kind of change to rule file.

It states no limit, gate or invariant itself, so it cannot drift from the rules (`documentation.md`: normative text lives in `AGENTS.md` and `.agents/rules/`).

## Risks / Trade-offs

- **A rename the reference sweep misses** leaves a script naming a test that does not exist. → D3's post-move check. The shard check also fails on a stale explicit name.
- **A merge conflict with any change editing `window.rs`.** → Run it alone (`proposal.md`, "Impact"), ideally first.
- **A moved test that changes behaviour by moving** (a module-level `static`, an import resolving differently). → D2's body diff, and the window shards run in full before and after with identical selected counts and results.

## Verification scope

- **Kani:** none (no proven module touched).
- **Timing:** none (no timed path touched; the shards keep their tests).
- **Smokes:** whether `crates/lushtext/tests/widget/` is smoke-relevant is read from `is_relevant` of `scripts/accessibility_source_fingerprint.py` at task 0.1, and the three smokes run if it is.
