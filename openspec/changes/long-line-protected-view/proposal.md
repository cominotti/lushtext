## Why

LushText must stay instantly responsive (open, scroll, every interaction) with gigantic files, and it must never change a file's bytes. C1 (`document-scale-policy`) profiles paragraph lengths and turns off syntax, minimap, invisibles and search highlighting for giant-paragraph documents. Pango still lays out each giant paragraph whole (285–1,256 ms per layout on the 1.9M-character reference line). GtkTextBuffer also pays whole-paragraph costs that hidden text does not reduce: log attrs (about 95 ms per call), visible-cursor iteration, IM surrounding text, AT-SPI offsets and the in-tab GtkSourceSearchContext (25–28 s). The revised proposal (`docs/next/long-line-responsiveness.md`, section C2) and the maintainer decision of 2026-10-11 resolve this with **Protected View**. Documents over the protected thresholds open view-only. The buffer is never changed. Giant paragraphs are folded for display only. Every path that would place a caret, select, edit or search inside a giant paragraph is replaced by a bounded LushText-owned path. Editing those paragraphs is deferred to an optional C5.

## What Changes

- **Protected View entry.** A document enters Protected View when C1's `DocumentScale.protected_view` holds (longest paragraph > `L_protect`, or density > `V_batch`). Entry happens at load, at eviction reload, at external reload, and when an edit makes a paragraph giant. Entry is monotone for the editor session: no edit, preference, tab switch, eviction reload, draft recovery or local-history restore returns the document to editable. Only a reload of the file from disk re-derives the decision from the new content.
- **Buffer immutability.** While Protected View is active, no operation modifies the buffer. A document opened protected stays unmodified, and no draft, autosave or local-history capture is triggered for it. Save As writes the buffer's bytes with save-time transformations (EditorConfig formatting, line-ending normalization, mirror-back) suppressed and round-trips the file byte for byte. A document that enters through an edit keeps its modified state, and its save and draft paths keep working, also without transformations, so no save ever changes a protected buffer.
- **Display-only tail fold, applied at insertion.** Each paragraph over `N_visible` shows its head. Its tail is inserted already carrying one `invisible=true, editable=false` tag, so an unfolded giant paragraph never exists in a GtkTextView. Lines over `N_dense` are folded by an excess budget, which caps every paragraph at `V_batch − (R_batch − 1)·N_dense` visible characters (just above `N_dense` with C0's `N_dense = V_batch ÷ R_batch`; design D1 records the alternatives). Heads are never hidden. A pure, streaming placement policy in `model/protected_fold.rs` (Kani-proved) decides the fold on the worker over every paragraph of the body, and one cross-cutting GTK executor applies it for load, buffer replacement, eviction reload and file-monitor reload.
- **Caret-free, edit-free interaction.**
  - The view runs with `editable=false` and `cursor-visible=false`.
  - `move-cursor` is intercepted, so caret browsing is covered: arrows, Page Up/Down, Home/End and Ctrl+Home/End move the viewport.
  - A capture-phase gesture claims primary, middle and secondary presses and drags, including double- and triple-click.
  - DnD source and target are disabled.
  - Every default key binding of GtkTextView and GtkSourceView that can move a mark or change the buffer is pre-empted before its default handler: select-all (Ctrl+A), F7 caret toggle, the keyboard context menu, cut, copy, paste, delete, undo and redo, and GtkSourceView's change-number (Ctrl+Shift+A/X), change-case and join-lines, which change the buffer without checking `editable`, plus move-to-matching-bracket, move-lines and move-words. Per-instance action disabling is not used as the guard, because GtkTextView recomputes it. A widget test checks that every toolkit binding is classified. The buffer-mutating line-ending normalization and toggle-bookmark-at-cursor are disabled too. Each pre-empted request shows one bounded notice. The context menu is replaced by a matching Protected View menu.
  - The insert and selection marks are never moved.
- **Line Inspector.** "Inspect Line…" opens a secondary read-only view over a copied window of `N_inspect` characters of one paragraph, with wrap on. It has Previous, Next and Go to Column, and copying is bounded to its window. "Copy Whole Line" uses a lazy `GdkContentProvider` that streams from an immutable source off GTK. The inspector is the accessible path to folded content.
- **Navigation.** Navigation scrolls a LushText-owned mark and never moves the caret. This covers session restore position, bookmarks, local-history targets and workspace-search hits. A target column inside a hidden tail opens the inspector at that column.
- **Worker-side in-tab find.** No `GtkSourceSearchContext` is ever attached to a protected buffer. Ctrl+F runs a single-flight worker search over an immutable text source. The source is captured once through the bounded chunked snapshot. Results are paragraph+column records delivered in bounded batches. Only the current match is highlighted, and only when it is visible. A hit in a hidden tail opens the inspector. Replace is unavailable. The two selection-prefill sites are unified on `visible=true` with a bound.
- **Other surfaces.** Invisibles are forced off while any fold exists. Print is refused with a notice. Secondary views follow the policy: the local-history preview is protected and folded, the Markdown preview uses its existing `Simplified`/limited terminal, notes editors are covered, and the file peek sample is checked to be folded. A gutter fold marker shows which lines are folded and is an activatable inspector entry point.
- **Automation.** Tab snapshots gain `protected_view`, `protected_view_reason`, `folded_paragraph_count` and `hidden_character_count`. The surface snapshot gains `line_inspector_visible`. Snapshots never include content. Protected find holds the existing `editor-search` blocker (the `search-complete` predicate), and fold application stays inside the `file-load` blocker (`file-open-complete`). Inspector window reads hold a blocker. A dedicated fold-settle blocker is added only if C0 shows `file-load` does not cover it.
- **Workflow convention.** A new `WFR-PROTECTED-VIEW` row has its role home at `ui/editor_page/protected_view/`: a facade, `policy.rs`, `seams.rs`, `execution.rs`, `find_execution.rs`, `inspector_execution.rs` and `evidence.rs`. The Line Inspector and the fold-marker gutter are called presentation surfaces. The fold executor is cross-cutting with no role. The measured cells of the load, buffer-replacement, editor-find, minimap, local-history and print rows are re-derived.
- **GTK axiom ledger.** Probes and samples are added for every toolkit assumption this change relies on that C0 did not already pin: claimed capture-phase presses never move the insert mark or reset the IM, a non-editable view never focuses the IM, the protected `move-cursor` intercept holds under `gtk-keynav-use-caret`, DnD disablement, key-binding pre-emption, and `insert_with_tags` of a hidden run. AT-SPI text and AT-initiated caret writes on a folded paragraph extend C0's A35 and, like it, are pinned by the lane's AT case rather than an in-crate probe.
- **Docs.** `docs/accessibility.md`, `docs/accessibility-matrix.md`, a new `docs/accessibility-orca-checklist.md` entry, `docs/automation.md`, `docs/automation-reference.md`, `docs/end-user-coverage.md`, `docs/workflow-readability-matrix.md`, README features, `AGENTS.md`, `.agents/rules/ui.md`, and the `docs/next/` long-line programme record.
- **Dependency.** This change depends on C1 (`document-scale-policy`), which supplies the profile, `DocumentScale`, the GTK-matching paragraph-boundary function and the edit-time downgrade hook. It also depends on C0 (`probe-long-line-toolkit-costs`), which supplies the threshold values, budgets, fixtures, watchdog and axioms A23–A36.
- **BREAKING (user-visible):** documents over `L_protect` are no longer editable. Today they are editable but freeze the app. This was accepted by the maintainer on 2026-10-11.

## Capabilities

### New Capabilities

None. The Protected View requirements extend `long-line-document-presentation`, which C1 introduces.

### Modified Capabilities

- `long-line-document-presentation`: adds Protected View entry and monotonicity, buffer immutability, display-only tail fold applied at insertion, caret-free interaction, the Line Inspector, navigation into hidden tails, worker-side find, clipboard and print contracts, secondary views, and the fold marker. The capability is introduced by C1 (`document-scale-policy`), which archives first.
- `main-thread-responsiveness`: Protected View interactions and settles stay within `B_interact` and `B_settle`, which Protected View reads from C0's single responsiveness-budget module (named constants whose final values C0 records). Protected find, inspector windows and source capture are bounded per dispatch.
- `document-save-safety`: Protected View never dirties the buffer. Save As is byte-identical with save-time transformations suppressed, and a non-round-trippable decode is refused.
- `editor-inline-alerts`: the Protected View notice and the bounded disabled-action notices.
- `dbus-automation-spine`: Protected View snapshot fields, Line Inspector visibility, protected find under the `editor-search` blocker, and an inspector readiness blocker.
- `search-replace-safety`: in-tab Replace is unavailable in Protected View, and no source search context is attached.
- `gtk-axiom-ledger`: Protected View toolkit assumptions are ledger axioms with probes and samples.
- `gtk-accessibility-spine`: the folded main view's accessible text is documented, the Line Inspector is the accessible content path, and the change adds an Orca checklist entry.
- `workflow-readability-boundaries`: the `WFR-PROTECTED-VIEW` row and its role home.
- `invisible-character-visualization`: invisibles are forced off while any fold exists.
- `local-history`: no capture for unmodified protected documents, and the protected, folded preview.
- `draft-session-recovery`: session restore of protected tabs preserves the persisted cursor and restores by scroll only. Drafts are never created for unmodified protected documents.
- `live-editor-memory-budget`: the immutable find and inspector source is charged and released, and eviction reload re-folds.
- `external-file-monitor-coverage`: an external reload of a protected document re-folds at insertion.
- `line-bookmarks`: bookmark navigation scrolls without a caret, and toggle-at-cursor is unavailable with a notice.

## Impact

- **New code:**
  - `crates/lushtext-core/src/model/protected_fold.rs`, with `kani_proofs.rs`;
  - `crates/lushtext-core/src/ui/editor_page/protected_view/`: `mod.rs`, `policy.rs`, `seams.rs`, `execution.rs`, `find_execution.rs`, `inspector_execution.rs`, `evidence.rs`, the `inspector.rs` and `fold_gutter.rs` presentation surfaces, and `test_policy.rs`;
  - `crates/lushtext-core/src/ui/protected_fold.rs`, the cross-cutting executor;
  - `resources/ui/line-inspector.blp` and its generated `.ui`.
- **Changed code:**
  - `ui/editor_page/{imp,mod,load/execution,buffer_replacement/execution,monitor,search,invisibles,restore_position,bookmarks,local_history,accessibility}.rs`;
  - `ui/search_bar/{mod,execution,policy}.rs`, `ui/window/{search,actions}.rs`, `ui/window/print/{policy,execution}.rs`, `ui/window/local_history/preview_execution.rs`;
  - `ui/window/notes/` editor surfaces, `ui/markdown_preview/` admission, `services/file_peek.rs` (verification), `ui/buffer_snapshot.rs` callers;
  - `ui/automation.rs`, `model/automation.rs`, `services/action_catalog/`;
  - `ui/editor_page/save/` (suppressing save-time transformations).
- **Tests:**
  - unit, property and Kani tests for `protected_fold` and `protected_view/policy.rs`. Kani proves: the density bound, for any paragraph sequence and either wrap mode, with a `should_panic` harness showing it fails without the density fold; that only tails are hidden and folds never split UTF-8; that the column mapping is total; that no buffer-mutating action is admitted; that entry is monotone; and that inspector windows and find batches are bounded, with windows that tile the paragraph;
  - PR-gated widget tests for the precondition gates, sharded in `scripts/widget-shards.py`;
  - a byte-identity round-trip integration test;
  - axiom probes in `gtk-lush-axioms`;
  - responsiveness-smoke scenarios from C0's lane;
  - accessibility-smoke rows.
- **Tooling:** `scripts/kani-shards.py` (shards `core-protected-fold`, PR-gated, and `core-protected-view`, scheduled, with measured budgets, plus `ORACLES` entries for both modules and a proof-strength run each; `docs/next/formal-verification.md` records them), `scripts/check-workflow-boundaries` (the whole-pixel table and the new row), `scripts/lushtext-automation.py` (the new fields).
- **No new dependencies.** There are no persisted format changes and no GSettings keys. `L_protect` stays a fixed policy constant (maintainer decision, 2026-10-11).
