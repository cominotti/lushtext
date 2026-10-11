## Why

LushText decides which editor features a document can afford from its **byte size alone** (`services/file_limits.rs`, `FileSizeCheck`). The cost that actually freezes the app is **paragraph length**: one 5.1 MB HTML file with a 1,894,586-character line enables syntax because it is under 10 MB. GtkSourceView's context engine then analyses that line atomically for 2 s, emits a `g_critical`, and turns highlighting off. The minimap re-lays out every giant paragraph (8.1 s of stalls measured), and its availability is only known after the map is attached. Detaching the map can also leave a pending update tick, which produces reproducible `Gtk-CRITICAL` and `GLib-GObject-CRITICAL` warnings. Nothing re-evaluates these features when an edit, such as a paste, creates a giant paragraph.

A save rewrites the byte classification, so a save that shrinks a document followed by a rename or Save As re-applies a gated language.

Separately, the installation slicer splits only on `'\n'`, while GTK splits paragraphs on `\n`, `\r`, `\r\n` and U+2029. A CR-only or U+2029-separated document of any admitted size therefore installs in one GTK turn. That is a pre-existing blocker under the project's rule, and this change fixes it.

This is change **C1** of the long-line stream in `docs/next/long-line-responsiveness.md`. It follows `probe-long-line-toolkit-costs` (C0), which measures the thresholds this change turns into policy, and it comes before `long-line-protected-view` (C2). The user's requirement is absolute: LushText stays instantly responsive at open, while scrolling and during every interaction, whatever the file size, and file bytes are never changed.

## What Changes

- **Worker-side paragraph profile with no extra pass.** The existing fused decode analysis (`analyze_decoded_content_cancellable` and the `LineEndingTally` slice scanner in `services/editor_io.rs`) also records:
  - the longest paragraph, in characters;
  - per-threshold counts of long paragraphs;
  - a bounded list of their offsets;
  - a wrap-off density summary: the largest number of characters in any `R_batch` consecutive paragraphs.

  The profile is carried in `LoadMetadata` and in `FileLoadPlan` successors. The other installers (draft recovery, local-history restore and undo, and save-formatting mirror-back) run the same accumulator on the worker that already prepares their body.
- **One GTK-matching paragraph-boundary function** in `model/paragraph_boundary.rs`, covering `\n`, `\r`, `\r\n` and U+2029, never split between CR and LF. `next_replacement_boundary`, `next_install_boundary` and the profile all use it. A property test checks it against `pango::find_paragraph_boundary`, and Kani proves its slice-safety properties, its slice choice and its agreement with the delimiter rule on bounded inputs, and that the profile does not depend on how the text is sliced. This fixes the pre-existing one-turn install for CR-only and U+2029 documents. **Behaviour change:** slices now end after any GTK paragraph delimiter, not only after `'\n'`.
- **`DocumentScale` replaces `FileSizeCheck`.** It is a GTK-free policy in `model/document_scale.rs`.
  - **Input:** a `DocumentProfile` of total bytes, longest paragraph and wrap-off batch density.
  - **Outputs:** open admission, undo, `estimated_buffer_multiplier`, local-history availability, syntax and bracket matching, minimap availability, invisibles, automatic search highlighting, and the `protected_view` flag. C1 computes `protected_view` and C2 consumes it.
  - **Two kinds of output.** Open admission, undo, the multiplier and local history depend on total bytes alone and keep following the latest loaded or saved bytes, as today. The rest are presentation features, which the session floor holds monotone. `protected_view` holds when the longest paragraph exceeds `L_protect` or the density exceeds `V_batch`, which is the rule C2 consumes.
  - **Kani** proves that it is monotone over the componentwise partial order (a larger profile never re-enables a feature, and `protected_view` is upward-closed), that the session join is a meet, and that the byte tiers equal the retired ones.
  - **Migration:** all 17 consumer files are migrated in the same change, and `services/file_limits.rs` is retired. **BREAKING (internal API):** `services::file_limits` and `EditorPage::size_check()` are removed. The widget and integration tests and the benches move to `DocumentScale`.
- **The policy applies at load and, synchronously, on edits.**
  - A `connect_after("insert-text")` handler and a `connect_after("delete-range")` handler on the editor buffer update the profile for the touched paragraphs. The delete handler is needed because joining lines, or Backspace at a line start, can create a giant paragraph without inserting anything. Each update is bounded by the inserted run plus a constant number of O(segments) line-length reads. Its decisions live in a GTK-free core, `model/paragraph_profile_watch.rs`, and Kani proves that the incremental update agrees with recomputation from scratch over bounded edit sequences.
  - When a paragraph crosses a feature threshold, the handler downgrades that feature in the same signal emission. It runs before any GTK validation or highlight idle can lay out the paragraph.
  - Downgrades of presentation features are monotone for the document session. Edits and saves never re-enable one.
- **Minimap.**
  - Availability is decided from the profile **before** any `GtkSourceMap` is attached.
  - Every install path, including the direct path below the synchronous threshold that today leaves the map attached, retires the map before the first byte enters the buffer when the profile disallows it.
  - The detach CRITICAL is fixed **failing-first**: a live `GtkSourceMap` never has its view cleared while its update tick can still fire. The map is retired (unparented and dropped) and rebuilt.
- **Reduced-features notice.** One inline alert per document says which features were reduced and why. Its accessible text is bounded. The editor's accessible description names the reduced features in place of the old size-tier sentences.
- **Thresholds are policy constants.** C0 records `S_syntax`, `L_syntax`, `L_minimap`, `L_invisibles`, `L_search_highlight`, `L_protect`, `N_visible`, `R_batch` and `V_batch` (its derivation table, design D7). A feature limit C0 leaves open defaults to `min(L_syntax, N_visible)` unless the maintainer decides otherwise. The working budgets B_interact = 8 ms per interaction dispatch, input-to-presented-frame ≤ 2 frames, and B_settle = 16 ms per settle dispatch are the targets those values must meet; code reads them from C0's constants module and never restates them.
- **GTK axiom ledger growth.** New entries, with `gtk-lush-axioms` probes and samples, cover any behaviour this change relies on that C0 did not already pin:
  - GtkTextBuffer line delimiters equal Pango's paragraph delimiters;
  - a line-length read costs O(segments), not O(characters);
  - downgrading syntax inside the `insert-text` after-handler prevents context-engine analysis of the inserted paragraph;
  - unparenting a widget with an armed tick callback stops that callback.

  C1 also cites C0's A27, A29, A31 and A32 and is listed as a dependent design on each.
- **The minimap's modified-line marks** count inserted lines with `'\n'` only; they move to the shared delimiter definition.

## Capabilities

### New Capabilities
- `long-line-document-presentation`: covers the document scale profile, the GTK-matching paragraph-boundary contract, the monotone `DocumentScale` policy and its feature outputs, edit-time synchronous downgrades, the gating of syntax, bracket matching, invisibles and search highlighting, and the reduced-features notice. C2 (`long-line-protected-view`) extends this capability with Protected View.

### Modified Capabilities
- `main-thread-responsiveness`:
  - "Large file installation yields in bounded GTK slices" now aligns slices to GTK paragraph boundaries (`\n`, `\r`, `\r\n`, U+2029) instead of newline only;
  - "Expensive preview and analysis workflows degrade before blocking input" now covers syntax highlighting and edit-time paragraph growth.
- `editor-minimap`:
  - "Minimap availability adapts…" is decided from the paragraph profile before attach;
  - "Wrapped-layout admission uses conservative live bytes" no longer gates availability; it only admits marker analysis;
  - a new requirement says a map is never detached while its update tick may fire.
- `editor-inline-alerts`: adds the reduced-features notice.
- `invisible-character-visualization`: invisible-character modes become unavailable when the scale policy gates them.

`live-editor-memory-budget` and `local-history` keep their requirement text. Their inputs (`estimated_buffer_multiplier` and the 10/50 MB history tiers) move to `DocumentScale` unchanged.

## Impact

**Code**
- New: `model/document_scale.rs`, `model/paragraph_boundary.rs` and `model/paragraph_profile_watch.rs`, each with a `kani_proofs.rs`.
- Changed: `model/buffer_replacement.rs`, `model/file_load.rs`, `services/editor_io.rs`.
- Retired: `services/file_limits.rs`. Its consumers are migrated:
  - services: `services/{bookmark_excerpt,draft_service,file_peek,local_history_service,mod}.rs`;
  - editor page: `ui/editor_page/{accessibility,imp,document_identity}.rs`, `ui/editor_page/load/execution.rs`, `ui/editor_page/save/execution.rs`;
  - other UI: `ui/plain_disposal.rs`, `ui/window/local_history/journal.rs`;
  - tests and benches: `crates/lushtext/tests/{integration/draft.rs,widget/editor_page.rs,widget/window.rs}` and `benches/benchmarks.rs`.
- Callers of the removed `size_check()` accessor that do not name the type: `ui/editor_page/local_history.rs`, `ui/window/encoding/execution.rs`, `ui/editor_page/minimap/admission.rs`; plus `ui/editor_page/minimap/policy.rs` (`MinimapAvailabilityPolicy`, `TooLarge`), `ui/automation.rs` (absence-reason mapping) and the ownership tables in `ui/editor_page/{load,save}/mod.rs`.
- Also changed: `ui/editor_page/minimap/{admission,projection_execution,retirement,watch}.rs`, `ui/editor_page/invisibles.rs` and `ui/search_bar/execution.rs`.
- Non-Rust: `scripts/run-mutants.sh` (its default `make mutants-smoke` file is `file_limits.rs`), `docs/mutation-testing.md`, `.agents/skills/gtk-perf-scale/references/large-file-patterns.md`, `docs/next/{minimap,file-peek}.md`.
- New edit-time coordination in `ui/editor_page/document_scale.rs` (cross-cutting, no role).

**Workflow convention**
- A new `WFR-DOCUMENT-SCALE` row with status `cross-cutting`.
- Re-derived measured cells for the `WFR-DOCUMENT-LOAD`, `WFR-BUFFER-REPLACEMENT`, `WFR-MINIMAP`, `WFR-EDITOR-FIND` and `WFR-LOCAL-HISTORY` rows.

**Verification**
- Kani harnesses in two measured shards (`core-document-scale`, `core-paragraph-profile`), listed in the proof-strength oracle table with a recorded proof-strength run (design D9).
- Property tests for slice independence and Pango agreement.
- Failing-first widget tests for the minimap CRITICAL, edit-time downgrades and CR-only/U+2029 slicing.
- Synthetic fixtures only.

**Docs**
- `AGENTS.md` (large-file handling, minimap classifier, and a new design decision).
- `.agents/rules/ui.md`.
- `docs/accessibility.md` and `docs/accessibility-matrix.md`.
- `docs/end-user-coverage.md`, the README features, and the GTK axiom ledger.
- `docs/accessibility-orca-checklist.md` if a manual expectation changes.
- The long-line programme record `docs/next/long-line-documents.md`.

**Dependencies and data**
- No new crates.
- No persisted format changes.
- No file bytes change, ever.
