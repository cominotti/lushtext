# Proposal

## Why

The refactor-1 audit (disposition in `docs/next/refactor-1-disposition.md`) left the suite's ergonomics for humans largely open. Three items cost every contributor, human or agent, and none needs a policy decision. Measured on `origin/main` `6f0b638f`; task 0.1 re-measures:

- **`crates/lushtext/tests/widget/window.rs` is 22,325 lines,** with 407 tests across unrelated themes: notes browser 24, local history 17, Focus Mode 10, drafts, session restore, menus, close safety, split views, print and more. Finding, reviewing or moving a test means scrolling a file no editor handles comfortably.
- **The harness compiles rewritten copies, not the sources** (`crates/lushtext/build.rs`).
  - `harness = false` compiles `#[test]` items out.
  - So `transform_test_source` copies every widget file into `OUT_DIR/widget_mod_<module>.rs`, dropping each `#[test]` line and turning `fn test_` into `pub fn test_`, and the registry includes those copies.
  - As a result, compiler errors, panic locations and backtraces name the generated copies, not the files a contributor edits.
  - `cargo fmt` does not reach these files either way (they are not modules of a crate root it can see), which is why `make fmt` lists them explicitly; this change does not alter that.
- **There is no contributor quickstart.** The README's "Contributor Map" is a module tour. The rules (`AGENTS.md`, `.agents/rules/`) are written for agents and run to thousands of lines. A first-time contributor has no one page that says how to build, run, test and commit.

## What Changes

- **Widget tests compile from their own files.**
  - A test is declared `pub(crate) fn test_<name>()` at the top level of a widget file, with no `#[test]` line.
  - `build.rs` only generates the registry, by that one rule, and includes each file by its real path, with no copy and no rewrite.
  - `scripts/widget-shards.py`'s discovery uses the same rule, so the shard check still mirrors the registry.
  - Support files are never scanned for tests. A file whose stem is `common` or ends in `_support` is included as a module with no registration, in `build.rs` and in the shard discovery alike. Today `build.rs` and the discovery skip `common` by name; under the new rule that skip is what keeps its `pub fn test_window()` and similar helpers from being registered.
  - A misdeclared test fails the build instead of vanishing: any top-level `fn test_*` in a test file that is not `pub(crate)` fails `build.rs` and the discovery. Private helpers in test files that carry the prefix (`test_window()` and its variants in `window.rs`) are renamed or move to a support file.
- **`window.rs` is split by theme** into sibling modules `window_<theme>.rs` (for example `window_notes`, `window_local_history`, `window_drafts`, `window_session`, `window_shell`), with shared helpers in `window_support.rs`, a support file under the convention above.
  - A test's name becomes `window_<theme>::test_<name>`.
  - Every live reference to an old name moves in the same change, with the full mapping in `evidence/renames.md`: the shard table, the smoke and portal scripts, `builder-diagnostics-coverage.json`, the docs, the skills, the axiom ledger and test comments.
  - Archived OpenSpec records keep the names they used, as history.
- **`CONTRIBUTING.md`,** one page at the repository root:
  - prerequisites and `make dev-tools`;
  - `make run`, `make check`, `make test` and the hook;
  - where tests go, and the commit conventions;
  - which rule file to read for which kind of change.
  
  It links to the rules and copies none of them. The README's "Contributor Map" links to it.

## Non-goals

- **Changing what any test asserts,** or which shard runs it. Shard membership moves with the tests: every `window` entry in the table becomes the modules that hold the same tests.
- **A proc-macro or `linkme`/`inventory` registry.** It would mean a new crate and dependency, with no guarantee the one-line rule lacks (design D1).
- **A file-size gate.** It would be another policy check, and the split is a one-time fix.
- **Rewriting the rules for humans.** The quickstart points into them.

## User-visible impact

None for LushText users. For contributors:
- widget-test diagnostics and backtraces name the real files (editor navigation is checked in task 1.2 and claimed only if it holds);
- the window tests are findable by theme;
- a first contributor has one page to start from.

Tests named in scripts and docs change from `window::` to `window_<theme>::`; anyone with a personal command line that names an old name must update it. Nothing is lost compared with `main`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `widget-test-ci-sharding`: the widget registry and the shard discovery read widget sources as written, without rewriting them.

## Impact

- **Code:**
  - `crates/lushtext/build.rs`;
  - every file under `crates/lushtext/tests/widget/` (dropping `#[test]`, adding `pub(crate)`);
  - `window.rs` split into new sibling files;
  - `scripts/widget-shards.py` (discovery rule, table, self-test).
- **References:** the 12 live files that name a `window::test_` today, and the 3 documents that name the path `tests/widget/window.rs` (`docs/accessibility-matrix.md`, `docs/next/file-peek.md`, `docs/workflow-readability-matrix.md`); task 0.1 lists both. Rule frontmatter scopes use `crates/lushtext/tests/widget/**` and are unaffected; `scripts/test-validate-agent-skills.py` names the path only as an injected fixture.
- **Docs:**
  - `CONTRIBUTING.md` (new) and `README.md`;
  - `.agents/rules/testing.md`, the `gtk-testing` skill references and `crates/lushtext/AGENTS.md`, where they describe the registry.
- **Order:** the Start line in `tasks.md`. The split moves every window test, so this change runs while no other change in the ledger has `crates/lushtext/tests/widget/window.rs` open. It is recommended first of the refactor-1 set, before `close-audited-gate-gaps`, so the later changes write their tests into the split files.
