# Tasks

Tests: the typed entries and rows change no behaviour; their widget tests are the oracle and must pass unchanged, and each group adds the tests named below. The audit is proved red by its self-test. The dialog conversion waits for its characterization test.
Kani: none (no proven module touched).
Timing: affected cases and evidence in design, "Verification scope".
Start: only after every edit, spec and queued change of the programme coordinator's session has finished (maintainer, 2026-11-19), and after the maintainer places this change in `PARALLEL-LEDGER.md`. Task 0.1 re-measures the baseline first. This change also starts after `spike-list-row-construction` records its decision.

## 0. Re-measure the baseline

- [ ] 0.1 On the then-current `origin/main`:
  - re-count the factories, `next_sibling` calls, object-data sites and `KIND_*` uses;
  - read the spike's decision;
  - re-read `workspace_section/` after `stop-expander-bind-time-child-scans` and `slice-large-directory-collapse` merged.
  
  Update the artifacts in place. Verify with `openspec validate adopt-typed-row-state-and-dialog-futures --strict`.

## 1. Typed entries

- [ ] 1.1 Record `size_of` of the three item imp structs on `main`.
- [ ] 1.2 Introduce `PaletteEntry`, `SearchResultEntry` and `FileTreeEntry` (design D1), keeping accessor names and `match_count` as a property. Add unit tests that each constructor yields its variant and that `retarget_from` moves the whole entry. Verify that the command-palette, search-panel and sidebar widget shards pass unchanged.
- [ ] 1.3 Run the gate order and the three smokes. Commit through `heavy.sh`, signed, and push.

## 2. Typed rows for the five non-sidebar factories

- [ ] 2.1 Add the search rebind test first: a list item rebound to another file while the first item's `match_count` keeps changing never shows the first count. Then convert `search_panel/list_factory.rs` and `search_panel/skipped_files.rs` to the spike's row form (design D2). Delete the `unsafe` blocks and the sibling walks. Verify with the new test and the search-panel shard.
- [ ] 2.2 Convert the palette (`command_palette/row_factory.rs`), the Open popover (`open_popover/imp.rs`) and the notes browser (`window/notes/browser.rs`). Verify with their shards and the four budget tests in design "Verification scope".
- [ ] 2.3 Run the gate order and the three smokes. Commit through `heavy.sh`, signed, and push.

## 3. Sidebar row and markers

- [ ] 3.1 Add widget tests for the two sidebar scenarios in the spec:
  - recycle a directory row and assert the old expanded handler no longer fires;
  - recreate a `TreeListRow` for the same item and assert one watch.
  
  Verify on `main` (the second may already pass; say so).
- [ ] 3.2 Build `WorkspaceTreeRow` (design D2), move the accessibility hook into it, and replace the two markers with weak references (design D3). Re-key `row_factory.rs` in both visual-policy implementations with parity assertions. Verify with the sidebar shards, `workspace_tree_virtualization`, `make check-visual-proof-policy`'s self-tests and `make visual-geometry-smoke`, including the `workspace-tree-reveal` scenario.
- [ ] 3.3 Run the gate order and the three smokes. Commit through `heavy.sh`, signed, and push.

## 4. Open dialog

- [ ] 4.1 Write the characterization test (design D4) and verify it on `main`. If the harness cannot present the chooser, record why and skip 4.2.
- [ ] 4.2 Rewrite `show_open_file_dialog` (design D4). Verify that 4.1 passes. Add a "Dialog futures" note to `.agents/rules/widget-wiring.md`: guard before spawn, strong after the first poll, never for worker completions.
- [ ] 4.3 Run the gate order and the three smokes. Commit through `heavy.sh`, signed, and push.

## 5. Audit and documentation

- [ ] 5.1 Add the object-data rule (design D5) to `scripts/check-signal-connections.py` with its self-test. Verify that `make check-signal-connections` passes with zero sites and fails on each planted case.
- [ ] 5.2 Update the `gtk-lush-signals` retained-site audit, `.agents/rules/widget-wiring.md` ("Signal And Binding Lifetimes", "Large Lists", "Realized List Rows"), `.agents/rules/ui.md` ("Realized List Rows", "File Tree"), `ui/sidebar/AGENTS.md` and the workflow matrix rows whose files moved. Verify with `make check-agent-docs` and `make check-workflow-boundaries`.
- [ ] 5.3 Commit through `heavy.sh`, signed, and push.

## Workflow follow-up

- `adopt-glib-clone-weak-captures` starts after this change merges.
- Archive after review and merge under the programme's merge rules.
