# Close Consistency And Decomposition Gaps

## Why

A four-angle post-programme quality review (abstractions/type design, error
handling, pattern consistency, and a landed-as-intended audit of the nine
archived quality changes) confirmed every archived change landed and found no
correctness or data-safety defects. What remains is a small, closed set of
consistency debts: one genuinely mixed-responsibility 4,281-production-line
GTK adapter the decomposition programme skipped, one surviving
`unreachable!`-policed representable-illegal-state pairing, one admission
ledger that hand-rolls the manual charge/release pattern the rules forbid,
duplicated single-flight coordination vocabulary, a re-fragmented widget-test
presentation helper that recreates a documented flake incident, a latent
callbacks-under-borrow panic class, and a handful of silent-diagnostic and
dead-hook stragglers. Closing them in one final change lets the completed
programme rest on structural contracts instead of vigilance.

## What Changes

- Decompose `ui/markdown_preview/mod.rs` (4,281 production lines; image
  decode pipeline, table builder, code-block theming, footnote/link handling,
  and a ~2,100-line render-orchestration impl in one file) into focused
  sibling modules under the existing widget folder, behavior-neutral, using
  the same workflow-split approach already applied to `ui/window/` and
  `ui/editor_page/`.
- Make buffer-replacement request construction pair cancellation callbacks
  with body kinds by type shape, eliminating the last production
  `unreachable!` arm that polices a representable illegal state
  (`ui/editor_page/buffer_replacement.rs:68`, `Guarded` callback × `Plain`
  body).
- Extend scope-owned byte-admission charge release to the palette file-index
  build ledger (`services/palette/index.rs`, 8+ scattered manual
  `release_scratch`/`release_installed` calls) and the residual manual
  paired releases in `services/palette/notes.rs`, reusing the
  `with_construction_charge` ownership shape instead of per-exit vigilance.
- Consolidate one-active/one-latest coordination: converge
  `model/search_flight.rs` and `services/palette/runtime.rs` on one shared
  coordinator primitive with workflow-neutral naming, replace the copied
  cancellation token in `services/local_history_service.rs` with an alias of
  the shared token (as `services/bookmark_excerpt.rs` already does), and
  either share or explicitly justify the three hand-rolled `Guarded*Outcome`
  worker-guard adapters.
- Restore one shared widget-test presentation helper: `present_window`
  currently exists as three divergent copies (sidebar's copy lacks the
  allocation wait), recreating the documented "five copy-pasted
  `wait_until`" incident; the widget-wiring rule claiming it lives in
  `tests/widget/common.rs` is stale.
- Sweep the callbacks-invoked-under-`RefCell::borrow()` sites (~5:
  `load_save.rs:828`, `bookmarks.rs:326/426`,
  `workspace_section/actions.rs:311`, `sidebar/mod.rs:295`) to the
  clone-then-call pattern the codebase already uses elsewhere, closing a
  latent GTK-thread reentrancy panic class.
- Close diagnostic and cleanup hygiene stragglers: log the silent cancel-path
  temp-item cleanup (`workspace_section/actions.rs:364`), recover instead of
  double-panicking on a poisoned session save-ordering lock
  (`services/session_service.rs:87`, using the existing `lock_unpoisoned`
  helper), and share smoke warning classification per warning family instead
  of the hand-duplicated Gdk broken-pipe predicate across five smoke drivers.
- Convert the remaining process-global single-slot fault seam
  (`FAIL_REPLACE_BEFORE_RENAME_PATH` in `services/content_search/replace.rs`)
  to the per-target keyed, cleanup-owned registry shape its sibling
  after-metadata hook already uses.
- Remove the 10 dead `pub` functions (mostly orphaned `*_for_test` hooks),
  convert the hand-rolled boolean-flag one-shot timer in
  `ui/sidebar/workspaces.rs:179` to `SupersedingTimer`, consolidate the two
  byte-similar fire-and-forget cleanup spawn blocks in
  `workspace_section/actions.rs`, and group the ~12 loose `*_generation`
  fields on `ui/editor_page/imp.rs` per the existing rust.md smell guidance.
- Document the coordination vocabulary (Admission / Budget / Coordinator /
  Ledger / Retirement / Continuation and generation-counter conventions) as a
  short glossary in the agent rules so the programme's concept count stops
  taxing newcomers, and refresh the stale widget-wiring helper claim.
- Explicitly out of scope: refactoring `DisposalOwned` internals, splitting
  the other 24 over-budget-but-cohesive files, introducing a shared
  `retained_byte_weight` trait, and shrinking the coordination vocabulary
  itself.

## Capabilities

### New Capabilities

- `shared-single-flight-coordination`: one shared one-active/one-latest
  coordinator and cancellation-token primitive with workflow-neutral naming,
  consumed by palette search, notes browsing, bookmark excerpt previews,
  local-history preview, and workspace search instead of parallel
  per-workflow reimplementations.
- `ui-runtime-hygiene`: registered UI callbacks are invoked outside held
  `RefCell` borrows; best-effort cleanup paths emit diagnostics; ordered-save
  lock poisoning degrades to recovery instead of a second panic.
- `smoke-warning-classification`: headless smoke lanes classify known-benign
  toolkit warnings through shared importable classifiers per warning family
  rather than per-script pattern copies.

### Modified Capabilities

- `gtk-adapter-module-boundaries`: add the Markdown preview widget to the
  decomposed-adapter contract — rendering workflows (images, tables, code
  theming, footnotes/links, render orchestration) get focused sibling
  modules with behavior-neutral proof.
- `typed-payload-ownership-contracts`: buffer-replacement requests pair body
  kind and cancellation-callback kind by construction (no panic-arm
  policing); the scope-owned charge-release requirement extends beyond
  note-source loops to every palette admission ledger, including the
  file-index build ledger and residual manual releases.
- `gtk-lush-proof-harness`: widget-test window presentation is a single
  shared helper with a realization wait, not per-module divergent copies.

## Impact

- **Code**: `crates/lushtext-core/src/ui/markdown_preview/*` (decomposition),
  `ui/editor_page/buffer_replacement.rs`, `services/palette/{index,notes,runtime}.rs`,
  `model/search_flight.rs`, `services/local_history_service.rs`,
  `services/bookmark_excerpt.rs` (naming only), `ui/window/{focus_indexing,notes/browser,local_history}.rs`
  (guarded-outcome adapters), `ui/editor_page/{load_save,bookmarks,imp}.rs`,
  `ui/sidebar/{mod,workspaces}.rs`, `ui/sidebar/workspace_section/actions.rs`,
  `services/session_service.rs`, `services/content_search/replace.rs`,
  `services/recovery_metadata.rs`, `ui/window/drafts.rs`,
  `ui/command_palette/mod.rs`, `model/automation.rs` (dead-hook removal).
- **Tests**: `crates/lushtext/tests/widget/{common,sidebar,command_palette,window}.rs`
  (shared presentation helper); existing markdown-preview widget/visual
  coverage re-run as behavior-neutral proof; new/updated unit coverage for
  charge-release ownership, coordinator consolidation, and keyed fault seams.
- **Scripts**: `scripts/{crash-recovery-smoke-driver.py,automation-smoke-driver.py,visual-geometry-smoke.py,run-visual-smoke.sh,compare-blueprint-visuals.sh}`
  (shared warning classifiers).
- **Docs**: `.agents/rules/widget-wiring.md` (stale `present_window` claim),
  `.agents/rules/rust.md` or `AGENTS.md` (coordination-vocabulary glossary),
  `README.md`/`AGENTS.md` module-layout updates for the markdown_preview
  split.
- **No** new crates, dependencies, persisted formats, user-facing behavior,
  or public automation contracts change; markdown preview decomposition must
  be pixel- and behavior-neutral under the existing visual/widget lanes.
