## Why

C0's budget verdict left the work that runs after a document opens without an owner: the
post-open stall of a 400-byte `.txt`, the size-independent ~42 ms first highlight of an HTML
document, and the first overlay-indicator frame. It recorded this as candidate C6. Since then
C4b and C4c moved window composition out of the way, and C2's revision (2026-10-30, open
question 11) handed C6 one more item: the line-validation cost of dense documents that stay
editable now that Protected View no longer enters on density.

This change's attribution measurement (design, "Measured breakdown") found where the
post-open time goes on `main` (b7f56d45). For small plain-text documents every post-open
dispatch already fits `B_settle`. The worst is the activation turn, 11.2–13.8 ms. Everything
over budget comes from five sources:

- **Language definitions.** On the first HTML open in a process, attaching the language costs
  36.3–39.7 ms in the activation dispatch, which then takes 43.7–49.3 ms. That is the "~42 ms
  first highlight". It is GtkSourceView parsing the HTML language and the CSS and JavaScript
  definitions it embeds. It is not highlighting.
- **Highlighting.**
  - Re-enabling highlighting after a sliced install takes one dispatch that grows with the
    document: 70.6–72.2 ms at 5 MB.
  - The first highlight of a 1 MB HTML document takes 24.6 ms.
  - GtkSourceView's background analysis then runs 40–47 ms per dispatch, for seconds on a
    5 MB document, and it also stalls scrolling: 47 ms per dispatch, with input-to-frame at
    52 ms.
- **The direct install.** The direct install of a 1 MiB document takes 11.2–12.0 ms in one
  turn.
- **Dense validation.** After a dense document of 710-character lines opens, GtkTextView's
  validation idles take 21.4–21.6 ms and 14 ms. The paint of a dense screen takes up to
  11.6 ms.
- **The first settings flush on a fresh keyfile profile** is followed by a 4.4–14.9 ms
  dispatch.

No LushText post-open projection costs more than 0.3 ms. That covers file monitor, local-history
seed, minimap refresh, accessibility metadata, preview republish, draft check, session save,
recent documents, status bar, EditorConfig application, memory policy and overscroll. The
problem is the toolkit work those projections trigger, and the order in which it runs.

## Scope split with C2

| Stall | Owner |
|---|---|
| Pango layout of a giant paragraph. Every 300–780 ms stall on C1's 1.9M-character fixture (open, background, click, scroll, find, type-end) | **C2** `long-line-protected-view` (fold at insertion, Protected View) |
| Recovery of two 32 MiB single-line drafts (31–53 s CPU-bound) | **C2** (design D13); the residual one-turn hidden-tail insert goes to **C3** `sliced-protected-install` |
| Everything else in the post-open settle: activation turn, language attach, highlighting attach and background analysis, install threshold, first frames, settings flush follow-up, transition relayout, and **dense short-line validation** of editable documents | **C6** (this change) |
| Re-validation of a dense editable document after a user-started invalidation (word wrap, zoom, font) | **C6** (coordinator decision 2026-10-30), through the same bounded validation mechanism |

## Decisions recorded (coordinator, 2026-10-30; the maintainer may override)

1. **Budgets are never relaxed.** Above the measured `S_BG_HIGHLIGHT`, highlighting is reduced
   through C1's `DocumentScale`. A GtkSourceView upstream report about the fixed analysis slice
   is drafted in `.claude/handoffs/` for the maintainer to file.
2. **The ~37 ms language-definition parse.** First, axiom probe S1 tries to move the parse off
   the GTK thread. If it cannot, attaching the language is deferred to an idle with no pending
   input, after the first frame, and the parse is recorded as a **toolkit-floor exception**
   against its axiom. The exception carries measured numbers and a drafted upstream report, as
   C4b did for A46. It is never accepted silently.
3. **Input-to-frame is reported, not gated.** The headless lane's double-buffered floor is
   documented.
4. **User-started invalidations on dense documents are C6's.** Each has a scenario.
5. **If C1 is unmerged**, the total-size gate lands on C1's branch.

## What Changes

- **Post-open settle evidence.** The measurement build records one span per post-open stage,
  so every post-open stall is attributed in the lane report. The stages are the activation
  turn, editor construction, language attach, install, publish, highlight attach, first
  frames and validation. The lane gains a post-open settle table and a typing-during-settle
  phase.
- **Language catalog off the activation dispatch.** The language list and the path guess run
  on a worker, through an application-owned catalog handed to the GTK thread. Parsing the
  language definition moves out of the activation dispatch into its own stage after the
  document's first paint. Axiom probe S1 decides whether that parse can run on a worker
  (design D2). If it cannot, the attach waits for an idle with no pending input after the
  first frame, and the parse is recorded as a toolkit-floor exception against S1, with
  measured numbers and a drafted upstream report.
- **Highlight attach without a document-sized dispatch.** The language and highlighting stay
  attached through a sliced install, or are attached in an order the axiom probe shows to be
  O(1) at publish (D3).
- **A background-highlight policy.** GtkSourceView analyses in fixed slices of about 30 ms that
  LushText cannot shorten. Above a measured total-size threshold `S_BG_HIGHLIGHT`, highlighting
  is reduced through C1's `DocumentScale` with the reason `TotalSize`. A drafted upstream
  report asks for a configurable or deadline-aware slice (D4).
- **A measured direct-install threshold.** One-turn installs are limited to what fits a stated
  share of `B_settle` (D5). The existing paragraph-aligned slicing, generation and
  finalization rules are unchanged.
- **Dense validation is paced.** The pure install slice plan bounds each slice by visible
  characters as well as bytes, so the validation the slice causes fits the budget. A37's rule
  stays: the allocation is queued before the paint. LushText also stops invalidating every
  line after open: the end-of-document overscroll no longer sets a default-style margin, and
  EditorConfig formatting is applied before the text is installed, not after (D6).
- **User-started invalidations on dense documents are paced.** For documents over `V_SLICE`
  visible characters, the word-wrap toggle, zoom and font changes are carried by a document
  style tag. The tag is applied first to the visible paragraphs, then outward in
  paragraph-aligned ranges of at most `V_SLICE` visible characters per dispatch. GtkTextView
  therefore never has more than one bounded range of invalid lines to validate at a time (D9,
  gated by axiom probe S6).
- **The fresh-profile flush follow-up and the end-of-transition relayout** are attributed by
  the first task and fixed or recorded (D7).
- **The verdict.** `main` and the branch are measured interleaved, in the four construction
  profiles, with 7 repetitions each. Every dispatch from the activation until the document is
  settled must fit `B_settle`, and every input during settle must be dispatched within
  `B_interact` of its event time (D8). The dense cases also measure a word-wrap toggle, a zoom
  step and a font change. Input-to-frame is reported against the documented lane floor, and is
  not gated.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`:
  - post-open settle stages fit `B_settle` per dispatch and never starve input;
  - the install slice plan is weighted by visible characters as well as bytes;
  - language and highlighting attach stay out of document-sized dispatches;
  - user-started invalidations on dense documents are paced.
- `responsiveness-measurement`:
  - post-open stage spans and the post-open settle table;
  - the typing-during-settle phase, the wrap, zoom and font phases, and the dense, Markdown and
    EditorConfig cases;
  - the C6 verdict rule;
  - the documented input-to-frame floor.
- `gtk-axiom-ledger`: provisional axioms S1–S6, and the toolkit-floor exception for S1:
  - language-definition parse cost, and whether it can run off the GTK thread;
  - highlight attach cost against buffer size;
  - the context engine's slice;
  - validation cost per slice against visible characters;
  - what the first keyfile write on a fresh profile triggers;
  - whether tag-carried wrap, scale and family invalidate only the tagged ranges.
- `dbus-automation-spine`: a `document-settled` readiness predicate and bounded settle evidence
  in the tab snapshot.

## Impact

- **Code:**
  - `ui/editor_page/load/`: the policy for the slice plan and direct threshold, and the
    execution of the publish order;
  - `ui/editor_page/document_identity.rs` (language attach);
  - a new application-owned language catalog (`ui/language_catalog.rs`, cross-cutting);
  - `ui/editor_page/overscroll.rs`;
  - `ui/window/documents.rs`: the activation turn and the EditorConfig join;
  - `model/document_scale.rs` (the `TotalSize` reduction; on C1's branch if C1 is unmerged);
  - `ui/editor_page/` style tag for wrap, zoom and font on dense documents (D9);
  - `ui/responsiveness_probe.rs` (spans);
  - `ui/automation.rs` and `model/automation.rs`.
- **Lane:** `scripts/responsiveness-smoke-driver.py`:
  - the post-open settle table and the settle-until-quiet background phase;
  - the typing-during-settle phase and the new cases;
  - the verdict rule and failing-first self-tests.
- **GTK Lush:** probes and samples S1–S6 in `gtk-lush-axioms`.
- **Upstream drafts:** `.claude/handoffs/c6-upstream-gtksourceview-worker-slice.md` and `.claude/handoffs/c6-upstream-gtksourceview-language-parse.md`, filed by the maintainer.
- **Kani:** harnesses for the two-budget slice boundary in `model/buffer_replacement/kani_proofs.rs`
  (shard `core-post-open-settle`). C1's `DocumentScale` harnesses are re-run if a total-size component is
  added.
- **Docs:**
  - `docs/next/long-line-documents.md` (C6 row and section);
  - `docs/next/long-line-evolution.md`;
  - `AGENTS.md`;
  - `docs/workflow-readability-matrix.md` (`WFR-DOCUMENT-LOAD` cells re-derived);
  - `docs/automation.md` and `docs/automation-reference.md`;
  - the axiom ledger and `docs/next/formal-verification.md`;
  - `.agents/rules/build.md` (lane).
- **User-visible:** opening any document, HTML included, never freezes input for more than
  one settle budget. Toggling word wrap, zooming or changing the font on a dense document
  updates the visible text first and the rest progressively. HTML documents above
  `S_BG_HIGHLIGHT` open with syntax highlighting reduced, and the app says so.
- **No change** to persisted data, settings keys, packaging or permissions.
