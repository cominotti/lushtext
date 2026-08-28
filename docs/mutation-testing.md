# Mutation Testing

LushText uses `cargo-mutants` to measure whether deterministic tests actually
fail when production behavior changes. It is a companion gate: it does not
replace `cargo nextest`, GTK widget tests, benchmarks, formatting, Clippy, or
dependency-policy checks. Property testing is another companion gate for
generated input invariants; it does not replace mutation testing either.

## Scope

The default scope is configured in `.cargo/mutants.toml`, and membership is
decided by **convention, not by directory**. The `examine_globs` list covers:

- Domain model code under `crates/lushtext-core/src/model/**`
- Service code under `crates/lushtext-core/src/services/**`
- Pure workflow policy modules matching
  `crates/lushtext-core/src/ui/**/policy.rs`

The `policy.rs` glob is a naming convention: a workflow's pure decision logic
lives in a `policy.rs` beside the workflow it serves, so that logic keeps
mutation coverage wherever its owning workflow lives and no new entry is needed
here when a workflow migrates. The precondition is purity — a `policy.rs` must
contain no `gtk4`, `glib`, `gio`, `libadwaita`, or `sourceview5` import.
`make check-workflow-boundaries` enforces three things: policy purity, that
every `policy.rs` in the crate is reachable from the glob list, and — because
membership is decided by *name* — that no GTK-free `ui/` module sits outside the
convention holding decision logic under some other name while every command
exits 0. **No hand-listed UI file entries remain.** The last one,
`ui/markdown_preview/inline_footnotes.rs`, retired when that module became the
Markdown preview workflow's `ui/markdown_preview/policy.rs`.

When pure policy relocates, the change must prove mutation parity: run
`make mutants-diff` for the relocated logic and record the before/after
generated and killed mutant counts. A relocation that stops generating mutants
is a coverage regression, not an acceptable consequence of the move.

Do not add broad GTK widget modules to the default mutation scope. GTK adapters
stay out of scope because they are not policy modules, not because they sit
under `ui/`. Widget construction, signal wiring, focus behavior, dialogs, file
choosers, and live allocation behavior belong in
`scripts/run-widget-tests.sh`, where the harness owns Mutter, D-Bus, renderer
settings, retries, and warning filtering.

## Local Setup

Install the tools once:

```sh
cargo install --locked cargo-mutants --version 27.0.0
curl -LsSf "https://get.nexte.st/0.9.137/linux" | tar zxf - -C "${CARGO_HOME:-$HOME/.cargo}/bin"
```

The wrapper checks for both binaries before running.

## Commands

Use the Makefile targets for day-to-day work:

```sh
make mutants-smoke
make mutants-diff
make mutants-full
make mutants-list
```

Those call `scripts/run-mutants.sh`, which centralizes flags and safety checks.
Useful environment overrides:

```sh
MUTANTS_TIMEOUT=600 make mutants-full
MUTANTS_SMOKE_FILE=crates/lushtext-core/src/services/file_limits.rs make mutants-smoke
MUTANTS_BASE=origin/main make mutants-diff
MUTANTS_SHARD=0/4 make mutants-full
MUTANTS_JOBS=8 MUTANTS_TEST_THREADS=3 MUTANTS_BUILD_JOBS=3 make mutants-full   # override local parallelism
```

`mutants-smoke` is the fast sanity check for tool installation, config parsing,
and timeout behavior. `mutants-diff` creates a diff against `origin/main` when
no diff file is supplied and filters mutants to changed hunks. `mutants-full`
runs the configured deterministic scope and can be sharded with `MUTANTS_SHARD`.
`mutants-list` prints the configured candidates without running tests.

## Local Parallelism

cargo-mutants is serial by default — one mutant builds and tests at a time —
which leaves a multi-core host mostly idle on the slowest gate. The local
Makefile targets (`mutants-smoke`, `mutants-diff`, `mutants-full`) auto-tune
this: `MUTANTS_JOBS` defaults to about `nproc / 4`, and the two per-job caps
default so that `jobs x per-job-parallelism` stays near the logical CPU count:

- `MUTANTS_TEST_THREADS` (default `4`) bounds the test phase — each mutant job
  launches its own nextest, which otherwise grabs every core.
- `MUTANTS_BUILD_JOBS` (derived, ~`nproc / jobs`) bounds the build phase via
  `CARGO_BUILD_JOBS` — without it, the concurrent cold builds each fan out to
  every core and spike load average far above `nproc` even though IO and memory
  stay quiet (the build phase is the one that pushed load to ~100 in testing).

Together these keep both phases near `nproc` instead of thrashing. Override any
knob inline (see above) or via `MUTANTS_LOCAL_JOBS` /
`MUTANTS_LOCAL_TEST_THREADS` / `MUTANTS_LOCAL_BUILD_JOBS` in the Makefile
invocation.

CI deliberately does not use this. The mutation workflow calls
`scripts/run-mutants.sh` directly and leaves `MUTANTS_JOBS` unset, so the small
sharded runners keep cargo-mutants' serial default; cross-machine fan-out there
comes from `MUTANTS_SHARD`, not local jobs.

The wrapper intentionally does not pass `--features property-tests`. The
dedicated property target runs through `make test-prop` so generated cases do
not multiply by every mutant.

## In-Place Safety

CI uses `--in-place` because the checkout is disposable and a separate nextest
baseline has already passed. Local runs are copy-based by default. If you set
`MUTANTS_IN_PLACE=1` outside CI, `scripts/run-mutants.sh` refuses to run unless
the worktree, index, and untracked-file set are clean.

Use a clean checkout or a disposable worktree for local in-place experiments:

```sh
git worktree add ../lushtext-mutants HEAD
cd ../lushtext-mutants
MUTANTS_IN_PLACE=1 MUTANTS_BASELINE_SKIP=1 make mutants-full
```

## CI Behavior

The mutation workflow has two lanes:

- Pull requests run non-widget `cargo nextest run --workspace`, generate a full
  history diff against the PR base, then run changed-code mutation with
  `--baseline=skip`.
- Scheduled and manual runs first prove the same non-widget baseline, then run
  the configured full scope in shards. The full-scope lane starts report-only
  so survivor backlog is visible without blocking unrelated work.

Every mutation job uploads `mutants.out` when it exists. Those directories are
ignored locally because they contain generated diffs, logs, and JSON outcome
data.

The mutation workflow also intentionally omits `lushtext-core/property-tests`.
The separate CI property-test job proves generated-input invariants without
folding that cost into cargo-mutants.

## Initial Calibration

The first local full-scope calibration on June 1, 2026 ran all four shards with
`cargo-mutants 27.0.0` and `MUTANTS_BASELINE_SKIP=1`. The pre-adjustment scope
selected 1,518 mutants and finished with:

| Shard | Total | Caught | Missed | Unviable | Timed out |
|-------|-------|--------|--------|----------|-----------|
| `0/4` | 380 | 205 | 142 | 33 | 0 |
| `1/4` | 380 | 210 | 127 | 43 | 0 |
| `2/4` | 380 | 144 | 189 | 47 | 0 |
| `3/4` | 378 | 165 | 189 | 24 | 0 |
| **Total** | **1,518** | **724** | **647** | **147** | **0** |

The largest missed clusters were:

| File | Missed mutants | Ratchet direction |
|------|----------------|-------------------|
| `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs` (then `ui/editor_page/minimap.rs`) | 215 | Geometry and marker-color helper assertions |
| `crates/lushtext-core/src/services/editor_io.rs` | 74 | Lossy preview, line ending, file health, and encoding-analysis assertions |
| `crates/lushtext-core/src/ui/markdown_preview/policy.rs` (then `ui/markdown_preview/inline_footnotes.rs`) | 49 | Scan-plan, delimiter, escape, and lowered-output tests |
| `crates/lushtext-core/src/services/file_tree.rs` | 8 | Bounded-scan telemetry assertions: `examined_entries`, `peak_retained_entries`, `peak_retained_bytes`, and `error` on the published `DirectoryScan` |
| `crates/lushtext-core/src/services/draft_service.rs` | 5 | Orphan-cleanup continuation assertions: `retained`, `failures`, `next_manifest_offset`, and `directory_wrapped` on the published plan and outcome |
| `crates/lushtext-core/src/services/palette/index.rs` | 26 | Index construction, root interning, recursion cap, and path filtering tests |
| `crates/lushtext-core/src/model/encoding.rs` | 24 | Table tests for IDs, labels, BOM policy, display, and mode parsing |
| `crates/lushtext-core/src/services/local_history_service.rs` | 23 | Availability, snapshot lifecycle, and pruning tests |
| `crates/lushtext-core/src/services/bookmark_service.rs` | 23 | Sidecar delete, move, root matching, and list-workspace tests |
| `crates/lushtext-core/src/services/notifications.rs` | 20 | Progress, expiry, dismiss, and inline-view reducer tests |

Calibration also found that `crates/lushtext-core/src/ui/window/tabs.rs`
produced 40 missed mutants dominated by `LushtextWindow::...` GTK adapter
methods. That file was removed from the default mutation scope rather than
excluded by a broad pattern; tab behavior stays in the widget harness until
smaller pure tab policy helpers are extracted. After that correction,
`scripts/run-mutants.sh list` reported 1,431 configured mutants.

The calibration **comment** recording that decision has since been retired from
`.cargo/mutants.toml`. It named a file the current `examine_globs` never selects
— `ui/window/tabs.rs` is neither a `policy.rs` nor a hand-listed entry — so it
documented a decision the configuration was not implementing, and a reader could
not tell it from a live exclusion. The ratchet record above is the durable home
for the finding; the configuration no longer restates it.

On June 2, 2026, the minimap cluster was ratcheted separately. The
non-widget-only focused slice for the then-live
`crates/lushtext-core/src/ui/editor_page/minimap.rs` moved from 215 missed
mutants to 86 after adding deterministic tests for minimap policy constants,
availability priority, wrapped-layout size policy, line-budget scanning,
long-line warning lines, marker bounds, lane widths, lane positioning, and the
light/dark marker palette. The remaining 86 survivors were classified as:

- `LushtextEditorPage::...` GTK adapter methods already covered by the widget
  harness for visibility, settings, marker counts, search/bookmark/modified
  markers, long-line toggles, Focus Mode, and too-large feedback.
- Mapped `GtkSourceMap` geometry and Cairo drawing functions whose observable
  contracts are asserted by widget projection tests, while their pure math and
  color helpers remain in the mutation scope.
- Five equivalent `fit_marker_bounds` exact-boundary mutants that produce the
  same final clamped marker bounds or mutate unreachable post-clamp states.

Those minimap leftovers were captured as narrow documented exclusions in
`.cargo/mutants.toml`.

**Superseded on August 28, 2026 by the minimap workflow migration.** The history
above is kept because it is the ratchet's own record, but the configuration it
describes no longer exists, and two of its statements had already stopped being
true before the migration read them:

- The hand-listed `examine_globs` entry naming `ui/editor_page/minimap.rs`
  **retired** rather than moving. It was a path-keyed scope entry that existed
  only because pre-convention pure logic sat outside a naming convention; the
  migration extracted that logic into
  `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs`, which the default
  scope reaches through `ui/**/policy.rs`. The GTK adapter beside it is now
  legitimately out of scope for not being a policy module — the first and second
  bullets above describe exactly the adapter and drawing mutants that
  retirement removes from the lane.
- **Slot 7a brought six modules into the convention, and four of them were never
  on the census's relocation candidate list.** `ui/window/print/policy.rs` (3
  mutants), `ui/search_bar/policy.rs` (36), `ui/window/notifications/policy.rs`
  (11), and `ui/window/encoding/policy.rs` (32) are all **gain from zero**: their
  rows were recorded as owning `none` pure policy, the decisions were interleaved
  with GTK calls, and none of it had mutation coverage. Configured total
  **5,216 → 5,381**: **+82** from those four, **+81** from the `adaptive_shell`
  rename, and **+2** net from the preview module — it gained 12 when the facade
  migration moved this workflow's fuzz and property entry points into it, and 10
  of those 12 are excluded (below). Pure mutation-scoped policy modules
  **11 → 17**.
- **"Unkillable by construction" is a distinct exclusion reason from
  "equivalent", and slot 7a needed it for the first time.** Ten of the preview
  module's twelve gained mutants sit in items behind
  `#[cfg(any(feature = "property-tests", feature = "fuzzing"))]`. The default lane
  deliberately runs with neither feature, so that code is **not compiled** and no
  mutation of it can change any test result. Listing them without excluding them
  reports permanent survivors no test could ever kill, which is worse than noise:
  it makes the ratchet's survivor count meaningless for that file. Exclusions for
  this reason must say so explicitly rather than borrowing the word "equivalent",
  because the remedy is different — an equivalence is re-triaged when constants
  move, whereas this one is re-triaged only if the lane's feature set changes.
- **Aggregate mutant counts hide mixed findings; enumerate before concluding.**
  That same "+12" contained two unrelated things. Ten were the accounting artifact
  above. The other two belonged to `inline_footnote_limited_plan`, which is **real
  production policy with three production callers** and had been filed under the
  module's "Fuzzing and property-test entry points" banner — whose comment
  asserted that fuzz and property tests were its only callers. Its survivor
  (`delete field source_bytes`) was a genuine untested production contract: the
  function exists to publish the refused source size next to the limit, and
  nothing asserted it. Fixed by relocating the function above the banner,
  correcting the banner comment, and adding
  `the_inline_footnote_limited_plan_reports_the_source_size_it_refused`. The
  finding is only visible from the per-mutant list; the file-level total looked
  like one uniform gain.
- **Retiring an exclusion beats narrowing one, and an extraction made for
  exclusion granularity can make the exclusion unnecessary.** Slot 7a extracted
  `properties_inner_split_width` purely so a non-binding-floor equivalence could be
  excluded at function granularity instead of swallowing an observable neighbour.
  A later run then surfaced a *different* operator on the same function — whole-body
  replacement with `-1.0`, which the operator-specific exclusion did not name — and
  it survived for the same reason: every mutation of that width is invisible
  *through its caller*, because the floor it feeds is non-binding and a mutated
  width only makes the floor less binding. Rather than widen the exclusion (which
  would have swallowed `replace - with /`, a killable mutant), the extracted
  function was given a **direct contract test**. A named pure function has a
  contract of its own, and asserting it killed all five mutants at once. The
  exclusion was deleted. Prefer this: a justified exclusion must be re-justified
  every time either constant moves, whereas a contract test fails on its own.
- **`scripts/run-mutants.sh diff <path>` used to silently substitute a different
  scope when `<path>` did not exist.** `ensure_diff_file` treats a missing file as
  a request to *create* one from `git diff origin/main...` — a **three-dot** range,
  which the rules already warn working-tree edits are invisible to. So a typo, or
  a diff whose generating command did not run, did not fail: the run proceeded
  against an unrelated scope and reported a clean-looking summary. Slot 7a hit this
  and nearly recorded the result as its verification; the only reason it was
  caught is that the survivor named a file (`ui/sidebar/policy.rs`) that was not in
  the intended set. **Closed in slot 7a:** an explicitly passed diff path must now
  exist and be non-empty, and a hunkless diff fails instead of exiting 0. The
  generating behavior remains for the *default* path, so the discipline still
  applies there: **`test -s` the diff file before invoking the runner, and check
  that the survivor paths belong to the files you scoped.** A mutation summary is
  only evidence about the diff it actually consumed.
- **`MUTANTS_RE` does not filter every mutant class, so a focused run's summary
  line is not a statement about the functions you named.** Measured on the pinned
  cargo-mutants 27.0.0: `--re zzz_no_such_symbol_zzz` — a regex matching nothing —
  still selects **35** mutants, which is exactly the number of
  `delete field <field> from struct <T> expression` mutants in the configured
  scope. That operator class is included unconditionally. The arithmetic closes
  exactly and is worth reproducing before trusting a focused figure:
  `--re properties_inner_split_width` selected **40** (35 + its 5), and
  `--re 'inline_footnote_limited_plan|properties_inner_split_width'` selected
  **41** rather than 42, because one of `inline_footnote_limited_plan`'s two
  mutants *is* a `delete field` mutant already inside the 35.
  **Consequence:** a focused run that reports "13 missed" may have zero survivors
  in the functions you scoped. Always filter `mutants.out/missed.txt` by the
  **file paths you intended** rather than reading the summary line — the same
  discipline the diff-path trap above already requires. This is the third
  scope-silently-differs-from-intent trap in this lane; treat any mutation figure
  as evidence only about the mutant list you verified, not about the command you
  believed you ran.
- **A `--in-diff` caveat worth knowing before scoping a run.** A **rename appears
  in a diff as a whole-file delete plus a whole-file add**, so `--in-diff` over a
  diff that contains a renamed file mutates that file's *pre-existing* logic too.
  Slot 7a's first attempt measured 347 mutants where the newly-in-scope figure was
  160; rescoping the diff to only the newly-written modules reproduced 160 exactly.
  Scope the diff to the files whose *logic* changed, not the files whose *paths*
  changed. Note the qualifier: with git's rename **detection on**, a pure rename
  produces no content hunks at all, so this trap needs a rename *plus* content
  changes in the same file — which is exactly what a role-assigning migration
  produces. In slot 7a's case the preview module had both, and only **12** of its
  187 mutants were genuinely new logic.
- The hand-listed entry naming `ui/markdown_preview/inline_footnotes.rs`
  retired on the same precedent when that module became
  `ui/markdown_preview/policy.rs`. Unlike the minimap case this was a **rename
  rather than an extraction**, so the entry *did* select the file beforehand and
  the relocation is a **parity claim rather than a gain from zero** — but the
  module's final count is **not** that parity figure, and conflating the two
  produced a published error worth recording. Measured from
  `scripts/run-mutants.sh list`: the module generated **175** mutants before the
  rename and **175** immediately after, then **gained 12** when the facade
  migration moved this workflow's fuzz and property entry points into it, ending
  at **187**. Report them separately: **175 relocated, 12 gained**. A
  "175 before, 175 after" claim was published from the pre-gain measurement and
  was false by the time it shipped.

  **Verify a re-key by direct measurement, not by an unchanged total.** The
  unchanged-total argument is valid only while nothing else about the file
  changes, which is precisely the assumption that failed here. The six calibrated
  entries were instead confirmed by removing them and re-listing: **210** mutants
  without them, **187** with, so all six match real generated mutants and suppress
  **23** between them.

  **An `exclude_re` entry with no function anchor can swallow a killed mutant.**
  Slot 7a's documented equivalence for the properties-fraction floor was first
  written against the enclosing `effective_properties_fraction`, where its `.*`
  matched **two** `- with +` mutants: the intended non-binding-floor one *and* the
  `remaining_fraction` subtraction one line above, which is observable and is
  killed. Measured 78 without the entry, 76 with — two suppressed, not one. The
  fix was to extract `properties_inner_split_width` so the entry anchors on a
  function containing exactly the one mutant it is justified for: now **81**
  without, **80** with.
- The 14 minimap `exclude_re` entries naming **66 methods** were reduced to
  **4 entries naming 0 methods**, all of them re-verified against a mutant the
  tool actually generates. Of the retired ones, **seven named method names had
  zero definitions anywhere in the tree** — `apply_minimap_width_from_settings`,
  `wrapped_minimap_layout_exceeds_budget`,
  `buffer_has_line_exceeding_char_budget`, `collect_long_line_warnings`,
  `line_top_in_strip`, `line_bottom_in_strip`, `buffer_y_to_strip_y` — and
  **four entries were anchored to a literal `line:column`** (`minimap.rs:2046:55`,
  `:2047:21`, `:2054:16`, `:2058:19`) that source edits had long since moved, so
  they matched **no generated mutant**, while the four mutants they were written
  for still existed and were therefore unprotected.
- The third bullet above says "five equivalent `fit_marker_bounds` mutants", and
  the four entries that encoded that claim are now **deleted rather than
  re-keyed**. Their recorded reason — "the same final clamped bounds after the
  minimum-height expansion settles" — stopped describing the mutants they matched
  once that expansion was extracted into a shared `expanded_to_min_height` helper,
  which is where the two surviving boundary claims now live, one mutant each. The
  extraction was the response to a real constraint: scoping the equivalence claim
  to either caller would have swallowed mutants the run had **caught**, in those
  functions' unrelated reject-outside and non-empty guards, and broadening a
  gate's reach is a weakening rather than a re-key.
- **All 12 survivors from the migration's first full run were triaged to zero.**
  Nine were killed by tests — three of them on a predicate that had come out of
  the GTK adapter with no unit assertion at all — and the remaining shapes were
  removed by the extraction above, which also exposed a `.min(upper - lower)` cap
  as dead code and deleted a survivor outright rather than documenting it as
  equivalent. Final scope for the file: **412 generated / 406 caught / 0 missed /
  6 unviable**.

The lesson generalized into `openspec/specs/mutation-testing/spec.md`: an
`exclude_re` entry anchored to a `line:column` or to a symbol name is re-verified
against a real generated mutant whenever its file is touched, and an entry
matching nothing is deleted rather than carried. A stale exclusion is not inert —
it is a recorded equivalence claim that no longer protects the mutant it names.

After the remaining model, service, palette, search, persistence, and
Markdown-footnote survivors were ratcheted with focused deterministic tests,
the June 2, 2026 full-scope sharded run selected 1,313 configured mutants and
finished cleanly:

| Shard | Total | Caught | Missed | Unviable | Timed out |
|-------|-------|--------|--------|----------|-----------|
| `0/4` | 329 | 299 | 0 | 30 | 0 |
| `1/4` | 329 | 292 | 0 | 37 | 0 |
| `2/4` | 328 | 281 | 0 | 47 | 0 |
| `3/4` | 327 | 311 | 0 | 16 | 0 |
| **Total** | **1,313** | **1,183** | **0** | **130** | **0** |

The scheduled/manual full-scope lane can be ratcheted from report-only to a
blocking gate once the CI runtime budget is accepted.

## Triage Policy

Start from `mutants.out/outcomes.json`, then inspect the per-mutant log and diff.
Classify each survivor:

- **Missed behavior:** add or tighten a deterministic test. Prefer unit tests
  near model and service code. For GTK behavior, move pure decision logic behind
  a deterministic helper before testing it.
- **Equivalent mutant:** the mutation does not change observable behavior.
  Prefer no exclusion when the survivor count is small; otherwise add a narrow
  `exclude_re` or `exclude_globs` entry with a reason in `.cargo/mutants.toml`.
- **Unviable mutant:** the mutation is outside the intended scope, flaky under
  the non-widget runner, or better covered by the widget harness. Exclude only
  the smallest stable path or pattern.
- **Timeout:** first check whether the baseline test is too broad or blocked.
  Increase `MUTANTS_TIMEOUT` only when the test is legitimately slow and stable.

Do not silence a survivor just because the current test suite misses it. The
preferred ratchet is tests first, small deterministic extraction second, narrow
documented exclusion last.

## Relation to Other Gates

- `cargo nextest run --workspace` remains the baseline for non-widget Rust tests.
- `make test-prop` runs bounded property tests for pure deterministic invariants.
- `scripts/run-widget-tests.sh --headless --retries 1` remains the GTK behavior
  gate for display-server-sensitive code.
- `cargo bench -p lushtext-core --no-run` still compile-checks performance
  harnesses without requiring a full benchmark run.
- `cargo fmt`, Clippy, rustdoc lints, and `cargo deny` keep their existing roles.

Mutation testing answers a narrower question: if deterministic production logic
is changed in small ways, do the tests catch it?

Property testing answers a different question: do pure invariants hold across
many generated inputs? Keep the default lanes separate. If a future change
intentionally wants mutation testing to exercise a tiny property, add a new
documented mutation mode or narrow opt-in that passes `--features property-tests`
explicitly instead of changing the default wrapper.
