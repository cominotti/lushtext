# Nucleo Full Framework Evaluation

> Moved from `.agents/skills/gtk-perf-scale/references/search-scaling.md`.
> This note records the current recommendation as of 2026-04-15. It is not an implementation plan.

## Recommendation

Do not migrate from `nucleo-matcher` to the full `nucleo` framework right now.

The overall idea is still reasonable, but the main justification in the earlier version of this note is now outdated. LushText already moved command-palette search and full index rebuilds off the GTK main thread, and the current measured performance is already comfortably fast at the codebase's present scale.

This should stay a "revisit later if needed" topic, not active roadmap work.

## What Changed Since The Original Idea

The original migration pitch assumed the current palette path was still synchronous on the main thread. That is no longer true.

Current command-palette behavior:

- Query scoring already runs in a background task.
- Full file-index rebuilds already run in a background task.
- The palette already has debounced incremental create/delete/rename index updates.
- The file index is intentionally capped at 100,000 files.

That means the migration is no longer solving "make the palette async". That problem is already solved.

## Current Strengths Of The Existing Design

The current `nucleo-matcher` + `FileIndex` design fits LushText well:

- It is simple and explicit: plain Rust `FileIndex`, plain search helpers, GTK-free service code.
- It matches the current command palette UX, which only needs the top 50 results.
- It already supports cheap incremental sidebar-driven updates instead of forcing full index replacement for every mutation.
- It is easy to benchmark directly with the existing Criterion setup.

The current design is also aligned with the rest of the codebase's structure:

- `services/palette/` owns indexing and scoring as GTK-free logic.
- `ui/command_palette/` owns debounce, activation, selection, and result presentation.

That separation is working well today.

## What Full `nucleo` Would Actually Improve

The full framework still has real upside.

### 1. Incremental result streaming

The biggest genuine benefit is not "background work" anymore. It is the ability to stream partial results while matching is still running.

That becomes more interesting if:

- the file cap is raised significantly above 100k,
- queries become much less selective,
- or the palette starts mixing in much larger result sets.

### 2. Match-position highlighting

The full framework can expose match positions, which would allow visually highlighting matched characters in the palette row title.

That is a real UX improvement, especially if the palette becomes more central to navigation.

### 3. Better headroom for larger future scopes

If LushText eventually wants a more ambitious picker model, such as:

- richer file ranking across very large workspaces,
- more persistent background matcher state,
- or more advanced multi-column matching,

then full `nucleo` becomes more compelling.

## Costs And Risks

These are the main reasons not to do the migration now.

### 1. The payoff is smaller than it first appears

Because the current palette is already async, a migration would mostly be buying:

- streaming,
- highlight indices,
- and future scaling headroom.

Those are nice improvements, but not strong enough on their own to justify replacing a working system.

### 2. The current incremental update model is a better fit than the note implied

LushText currently relies on fine-grained index mutations driven by sidebar operations:

- create file,
- delete file or directory,
- rename file or directory.

The full `nucleo` API is naturally strong at injecting items and restarting the matcher, but it is not an obvious drop-in replacement for the current in-place create/delete/rename model. In practice that likely means one of these tradeoffs:

- rebuild larger portions of the matcher state more often, or
- add a translation layer that reintroduces complexity above `nucleo`.

That weakens the "full `nucleo` is simpler" argument.

### 3. Memory cost goes up during active search

The earlier memory warning still matters: active matching with full `nucleo` is expected to use meaningfully more memory than the current plain `IndexedFile` collection.

That is not automatically disqualifying, but it is the wrong direction for a migration whose benefits are currently marginal.

### 4. The high-level crate is still a more opinionated dependency

The low-level matcher is already stable and useful on its own. The high-level `nucleo` crate is more opinionated, more integration-heavy, and more likely to shape LushText around its lifecycle.

That is a reasonable trade when the app clearly needs what it offers. Today, LushText does not clearly need it.

## Current Performance Snapshot

Local benchmark runs on 2026-04-15 support keeping the current design for now.

Command used:

```bash
cargo bench -p lushtext-core --bench benchmarks -- search_all
cargo bench -p lushtext-core --bench benchmarks -- file_index_search
```

Representative results on this machine:

- `search_all/files/100000`: about `2.34 ms`
- `search_all/all/100000`: about `2.32 ms`
- `file_index_search/query_match/100000`: about `2.34 ms`
- `file_index_search/no_match/100000`: about `0.86 ms`

Those numbers do not point to an urgent architecture problem in the current palette path.

## Strong Recommendation

Do not schedule a full `nucleo` migration now.

If time is available for command-palette work, it is more likely to pay off in one of these directions instead:

- improve result presentation,
- add matched-character highlighting only if it can be done without the full migration,
- improve ranking or UX polish based on real usage,
- or keep investing in benchmark coverage and measured regressions.

## Revisit Triggers

Reopen this idea only if one or more of these become true:

1. The command palette starts feeling slow in measured benchmarks or real use at current scale.
2. The 100k file cap becomes too restrictive for real user workflows.
3. Matched-character highlighting becomes a clear product goal.
4. The palette grows into a richer picker where streaming partial results materially improves UX.
5. The existing `FileIndex` mutation model becomes harder to maintain than a persistent matcher model.

## If Revisited Later

Do not jump straight to a full migration.

Start with a narrow spike:

1. Prototype full `nucleo` behind a small branch-local experiment.
2. Keep command search separate; only prototype file-result matching.
3. Prove how create/delete/rename updates would work without regressing the current sidebar integration.
4. Re-run the existing Criterion benchmarks plus any new UI-latency measurements.
5. Only continue if the measured UX or maintainability gain is clearly worth the added complexity.
