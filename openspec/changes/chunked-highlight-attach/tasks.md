## 1. Axiom first

- [x] 1.1 A84 probe and sample (`probes/a84.rs`, `examples/a84_highlight_updated_ends_report_how_far_the_engine_analysed.rs`): chunked inserts of short-line HTML into an attached buffer (`\n`, CRLF, unterminated), an attach to one chunk, 256 KiB chunks as the non-vacuous control; coverage read at `G_PRIORITY_DEFAULT_IDLE` and `G_PRIORITY_LOW + 1` idles. The first run found the reported end is the end of the last line's text, one character short of the count for `\n`-terminated text (`analysis_end`). Holds on GTK 4.22.5 / GtkSourceView 5.20.0.
- [x] 1.2 The sample's `--chunked-install` mode: LushText's pacing in an editor-like presented view; 1 MiB per language, two repetitions, through `scripts/measure.sh` (ledger A84, design D1).
- [x] 1.3 Ledger row A84, catalogue entry, `fixtures::a84`, README and CHANGELOG; `make check-gtk-axioms` passes.

## 2. Pure policy and proofs

- [x] 2.1 `model/highlight_chunking.rs`: `HIGHLIGHT_CHUNK_BYTES`, `MAX_SYNTAX_PARAGRAPH_BYTES` (compile-time bound under the cut), `highlight_install_order`, `next_chunk_step`, `chunk_gate`; unit tests.
- [x] 2.2 `SyntaxLanguage::chunk_highlighted` and `attach_after_install_threshold`; `syntax_size_threshold` returns `S_SYNTAX` for the chunked languages (design D1); unit test that they keep syntax up to `S_SYNTAX` and every other class keeps its gate. First for the planned ten; narrowed to markdown, rust, c, python3 and xml after task 5.0 (design D10).
- [x] 2.3 `SliceBudget::with_highlight_cut` (design D5).
- [x] 2.4 `load::policy::HighlightAttachPlan::AttachAfterFirstChunk` and `highlight_attach_plan(HighlightInstallOrder)`.
- [x] 2.5 Kani harnesses (`model/highlight_chunking/kani_proofs.rs`, shard `core-post-open-settle`, `ORACLES`): the order, the gate, at most one wait, every highlighted slice within the cut, and a `should_panic` harness without the paragraph bound; `make check-kani-shards`; the shard run with times recorded.

## 3. Implementation

- [x] 3.1 `language_attach.rs`: `begin_chunked_highlight`, `request_language_attach_for_install` (ignores the session scale, arms an unmapped editor at once), attach waiters woken whenever the pending stage resolves, the A84 coverage watch, `decide_highlight_chunk`, `analysis_wait_priority`; evidence `install_requests`.
- [x] 3.2 Load: the plan from `highlight_install_order`; chunked bodies routed to the sliced path; the highlight cut; the gate before each chunk, parking and the `G_PRIORITY_LOW + 1` wait; paced scheduling; evidence `installation_highlight_chunked`; an aborted chunked install removes its language before its cancelled clear.
- [x] 3.3 Whole-buffer replacement: the same order for bodies with a profile (draft recovery, local-history restore and undo, save-formatting mirror-back); a sliced clear-only replacement (eviction) removes the language first; a cancelled chunked replacement removes it before clearing; the guard keeps highlighting on.
- [x] 3.4 Failing-first widget tests (`post_open_settle.rs`, `window.rs`): a 400 KiB Markdown document installs highlighted chunk by chunk with every attach on at most one chunk (fails with the gate restored: syntax off); HTML stays gated; a small Markdown keeps C6's order; a background tab installs highlighted unshown (fails with the gate restored); a cancelled chunked install clears with no language; a reload removes the language before clearing; a local-history restore installs chunk by chunk (fails without the replacement change: 0 turns, direct).

## 4. Lane

- [x] 4.1 `short-language` fixture kind (twelve languages, deterministic, self-tested).
- [x] 4.2 `highlight/<ext>/1048576` cases (ten chunked, HTML and JSON gated), post-open and typing phases; the lane self-test passes.

## 5. Verdict

- [x] 5.0 First interleaved round (main 2 reps, branch 2 reps) stopped after the branch's first run: on the lane's blank-line-free fixtures js, typescript, css, sh and yaml typed at 31–39 ms per key with the language attached. Attributed with perf (`idle_worker`, `first_update_callback`) and reproduced with the A84 sample's new `--typing-limit` mode on the lane's fixtures; a blank line every four lines removes it (design D10). The chunked set is narrowed to the five that pass; the lane's gated group grows to seven.

- [x] 5.1 Interleaved `main`/branch through `scripts/measure.sh`, three repetitions per side: `highlight/*`, the construction profiles, `total-size/*`; then the construction profiles again, four per side (the host slowed down during the first set); load 2.2–5.3 recorded per run (record, "C6b").
- [x] 5.2 No dispatch over `B_SETTLE_US`, no key over `B_INTERACT_US`; no new exception. Activation to laid out on the chunked five is 0.2–1.1 s later (reported, design D9). The finding on C6's js/ts/css/sh/yaml gates is recorded for the maintainer (design D10).

## 6. Gates and docs

- [x] 6.1 `make fmt`, `make check-fmt`, `make check-clippy`, `make check-workflow-boundaries`, `make check-automation-docs`, `cargo check -p lushtext --bins --locked`, rustdoc.
- [x] 6.2 `make test` (0 FLAKY), `make gtk-axioms`, Kani shards `core-post-open-settle` and `core-document-scale`.
- [x] 6.3 Smokes: accessibility, visual, visual-geometry, crash-recovery.
- [x] 6.4 Record (`docs/next/long-line-documents.md`), AGENTS.md, workflow matrix rows where facts change.
