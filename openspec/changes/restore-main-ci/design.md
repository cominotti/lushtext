## Context

`main` (`b129f378`, GNOME 51 toolkit: GTK 4.24, GtkSourceView 5.22, Libadwaita
1.10; CI image `fedora:45`) has had no green `ci.yml` run since `3dcbe682`.
This design covers the blockers in the proposal. It records how each was
classified, what is measured and what is still a hypothesis.

## Measured baseline

**B1. CI failure census.** Taken with `gh run view --log` over every `ci.yml`
run since 2026-11-12, plus `43108136719`.

The spec review re-took the `FLAKY` row independently. It read 52 widget job
logs, one per completed `Widget Tests (*)` job of every `ci.yml` run since
2026-11-11, through `gh run view --job <id> --log`. That form is required:
`gh run view <run> --log` omits the jobs of a cancelled run, such as
43627976239.

- It found exactly the three tests below and no other.
- The directory charge appears five times, not four: 43108136719 is
  included.

| Blocker | Runs | Signature |
| --- | --- | --- |
| window draft test | 43614093974, 43619204060, 43623373361, 43625734736, 43629270107 | "the restored text is still owed a draft" (both attempts, both job tries) |
| editor-page minimap test | 43108136719, 43147416345, 43612937829, 43615947076, 43619204060, 43622208418, 43627345020, 43627976239, 43632470998 | "precondition: the editor stays scrolled to the middle (Rectangle { x: 0, y: -6, width: 808, height: 664 })" |
| GTK Lush API Advisory | every run since 2026-11-11 | `openssl-sys-0.9.88` build script panics on OpenSSL `4_0_3` |
| retry-rescued (`FLAKY`) | directory charge ×5 (43108136719, 43612937829, 43614093974, 43615947076, 43629270107); bookmark preview-only ×1 (43614093974); protected periodic capture ×1 (43627976239) | see B4 |

Not in scope because they are already fixed on `main`:

- rustdoc: `df573812`; Lint passed on `9ff19f2e` and on `b129f378`.
- the xattr non-widget failures: on branches without `0a7368d0`.
- the surfaces Glycin warning: `6bc39e93`.
- Dependency Policy: `df573812`.

**B2. Window draft test, timing.** The figures are the start-to-end times of
this test on CI.

| Outcome | Test time |
| --- | --- |
| failing | 5.33 s, 5.39 s, 5.43 s, 5.52 s |
| passing | 3.34 s, 4.13 s |

The window starts `AUTOSAVE_TICK_INTERVAL` (5 s) at construction. The test only
fails when the window lives long enough for a real tick to run before the
replacement.

**B3. Window draft experiments.** They ran on `main` `b129f378`, headless, on
CPUs 8–23. The patch is
`.claude/handoffs/restore-main-ci/spec-experiments.patch` and the log is
`exp1.log`. Both failed on both attempts with "the restored text is still owed
a draft":

- `settled_draft_restore_owes_a_draft` is the existing test without its manual
  `set_draft_dirty(true)`: the draft had settled before the restore.
- `inflight_pass_before_restore_owes_a_draft` runs a tick, with the manifest
  stage delayed 1.5 s, just before the Restore click.

**B4. Retry-rescued tests.**

- Directory charge:
  - per-row growth is bimodal: 2362–2375 B/row (passes) or 2534–2537 B/row
    (fails);
  - the charge is 31,850,496 or 32,112,640 bytes for 16,384 rows (about
    1944–1960 B/row);
  - the bound is 5/4 of the charge.
- Bookmark preview-only: one 10 s wait expired.
- Protected periodic capture: one 30 s wait expired.

None of the three is classified yet.

**B5. Minimap test locally.**

- 12 of 12 runs of the original test passed: 6 idle and 6 with 16 busy
  loops on CPUs 8–23 (`minimap-repro.sh`, logs `minimap-repro-load*.log`).
  Spreading the load over 16 cores does not starve the GTK thread the way a
  2–4 vCPU runner does.
- The two experiments in the patch then ran on `main` `b129f378`, headless
  (`exp2.log`; load 13.0 before, 11.8 after; not a timing figure):
  - `scroll_during_content_width_growth_is_kept` **passed**. This is the
    control: an upper-only change opens no burst, because `ViewportObserver`
    filters on page size.
  - `scroll_during_a_passive_width_reflow_is_kept` **failed deterministically**
    on both attempts with `Rectangle { x: 0, y: -6, width: 927, height: 664 }`.
    That is CI's signature: the view sits at the top margin after scrolling to
    line 1,000.
  - Its setup: at the top, hiding the minimap opens a passive burst, and the
    view is scrolled before the burst settles.

## Decisions

### D1. Text a replacement installs is owed a draft, from one operation

- Every `BufferReplacementWorkflow` suspends the buffer handlers that mark a
  tab draft-dirty (`load_projection_suspended()`). The terminal of a workflow
  that leaves the tab modified with content its draft does not hold therefore
  owes the draft.
- The operation:
  - marks the tab draft-dirty with `set_draft_dirty(true)`, which advances
    `dirty_generation`;
  - then schedules the first-dirty autosave, or marks the running pass
    pending, exactly as the buffer `changed` handler does.
- It lives once, in the draft workflow (`ui/window/drafts/`). The `changed`
  handler and every owing terminal call it.

The inventory is keyed by **call site**, not by workflow enum: one enum
(`DraftRecovery`) has two callers with opposite answers. These are all six
production `replace_buffer_bounded` callers (`grep -rn replace_buffer_bounded`,
excluding the `Test` workflow's test-utils seams):

| Call site | Workflow | Owes a draft? | Why |
| --- | --- | --- | --- |
| `drafts/restore_execution.rs` `open_set_aside_draft` | `DraftRecovery` | yes (already does it inline) | a new untitled tab whose own draft holds nothing; it sets it so the text survives even if the set-aside copy is deleted |
| `drafts/restore_execution.rs` `apply_draft` (startup recovery, terminal `finish_applied_draft`) | `DraftRecovery` | **no** | the draft on disk *is* the installed body; marking it would rewrite every restored draft on the first tick for nothing |
| `local_history/restore_execution.rs` `restore_local_history_snapshot` | `LocalHistoryRestore` | **yes (missing)** | the buffer now holds the snapshot; the draft holds the pre-restore text |
| `local_history/restore_execution.rs` `undo_local_history_restore` | `LocalHistoryUndo` | **yes (missing)** | same, in reverse |
| `editor_page/save/execution.rs` (formatting mirror-back) | `SaveFormatting` | no | the terminal mirrors the saved bytes and `finish_accepted_save` leaves the tab unmodified |
| `editor_page/mod.rs` `evict` | `MemoryEviction` | no | evicted tabs are never draft candidates (unmodified only); reload restores from disk |

**The clean-tab case is not covered by the `modified-changed` handler.** Both
B3 experiments start from an already-modified tab. For a clean tab, GTK sets
the buffer's modified flag on the replacement's first delete, while the
handlers are suspended (`documents.rs`: `if editor.load_projection_suspended()
{ return; }`), so the terminal's `set_modified(true)`
(`finish_local_history_buffer_replacement`) emits nothing. Reading the code,
a Restore into a clean tab therefore leaves it **not draft-dirty and without
the "•" title bullet or the header and discard-action refresh**, which only that
handler projects. *Hypothesis, not shown:* task 1.1 adds a clean-tab test and
records red or green. If the title is stale too, the operation also re-projects
the modified indicator through the handler's own code. That is the same defect
class and the same fix site.

Every other `set_draft_dirty(true)` site was inventoried so the operation is
held once without silently re-routing unrelated paths:

| Site | Routes through the operation? |
| --- | --- |
| `documents.rs` buffer `changed` handler | yes: it is the operation's reference behaviour |
| `documents.rs` `modified-changed` handler | yes: same contract (became modified → owed) |
| `drafts/restore_execution.rs` `open_set_aside_draft` terminal | yes |
| `documents.rs` rename re-stamp (`if editor.is_modified()`) | no: it re-stamps an identity, and its comment documents why it deliberately does not schedule |
| `dialogs.rs` close-save freshness abort, `abort_async_close_safety` | no: batch restores after an aborted close, each already scheduling one first-dirty pass for the batch |

The implementation re-checks this list (`grep -rn "set_draft_dirty(true)"`) and
records any change in the matrix row.

- Advancing the generation at the terminal covers both orderings in B3:
  - a pass committed **before** the terminal cleared the flag, and the
    terminal sets it again;
  - a pass committing **after** the terminal sees a stale generation and
    leaves the flag set.
- Marking at the replacement **start** as well is not needed. A capture
  during the replacement is refused anyway (`draft_capture_blocked`).
- **Rejected: compensating from `fire_content_republished`.** That hook fires
  at every guard-restoring terminal, including `SaveFormatting`, cancelled
  sessions and startup `apply_draft`. Each listener would have to re-derive
  "owed" from state the terminal already knows. An explicit call on the
  owing terminals' `Complete` arms keeps the decision where the facts are.
- **Rejected: fixing the test.**
  - Removing the test's real timer would hide a deterministic production
    defect: B3's first experiment needs no timer at all.
  - The existing test still changes its post-terminal assertion to "owed, or
    the restored text already persisted". A real tick that runs and commits
    between the terminal and the assertion is correct behaviour, not a
    failure.

**Recorded (stage 3, failing first on `b129f378` code, logs in
`.claude/handoffs/restore-main-ci/g1-red/`):**

- Red with "still owed a draft", both attempts each: the settled-draft
  restore, the pass captured before a restore, the **small (direct)** restore
  (so the defect hit every restore, not only large ones), and Undo Restore.
- The clean-tab restore: red on **both** assertions (`draft_dirty=false`,
  title without "•").
- Two more tabs of the same class lacked the "•" title: a startup-recovered
  draft (`recovered.txt`; its `draft_dirty` was correctly false) and a
  set-aside draft opened from Preferences (`Untitled`). Both terminals called
  `set_modified(true)` after GTK had set the flag on the install's first edit.
- Green, as expected: the save-formatting mirror-back owes no draft.

**Implemented as two operations**, because startup recovery needs the bullet
but must owe nothing:

- `LushtextWindow::note_draft_owed` (`drafts/autosave_execution.rs`): mark
  draft-dirty (advancing the generation), then the first-dirty schedule or
  mark a running pass pending. The `changed` and `modified-changed` handlers,
  Restore, Undo Restore and the set-aside open call it.
- `LushtextWindow::project_modified_indicator` / `reproject_modified_indicator`
  (`documents.rs`): the "•" title, header bar and discard action. The
  `modified-changed` handler and every replacement terminal that leaves the
  tab modified (the three above and startup `finish_applied_draft`) call it.

`grep -rn "set_draft_dirty(true)"` after the change: the rename re-stamp and
the two close-abort sites (unchanged, as decided), the set-aside open's
no-window fallback, and `note_draft_owed` itself.

### D2. A scroll during a passive burst is kept; pin the writer first

The mechanism is reproduced deterministically (B5): a scroll made during a
passive width burst is undone. *Hypothesis, not shown:* CI reaches it when a
starved main loop leaves the startup burst pending at the test's scroll.
What remains is which site writes the value back. The leading candidate comes from reading the code:

- A horizontal page-size change (`ViewportObserver` filters on page size) opens
  a passive burst. `schedule_minimap_reflow_settle_impl` then pauses the rest
  state for the whole burst.
- A scroll during the burst is not recorded, so the rest state still says "at
  top".
- `finish_minimap_reflow_settle` then calls `adjustment.set_value(lower)`.
- The second candidate is `schedule_top_edge_vertical_scroll_clamp`, which
  documents that it keeps a value that moved. It must be ruled out by
  instrumentation, not by its comment.

A prior flake had the same class: `bound-notes-browser-by-budget`, recorded in
`view_attachment.rs`, where a stale "at top" rest state after a reattach sent
the tab to line 0.

The discriminating test from the patch is the failing-first test. Instrument
it to name the writer before fixing.

**The open design problem: telling a GTK-caused move from a scroll.**

- `finish_minimap_reflow_settle` writes only when `at_lower` holds and the
  value is more than 0.5 away from `lower`. So the snap fires *only* when the
  value moved during the burst.
- "Snap only if the value did not move" would therefore never fire. That
  equals deleting the snap, which the rejected list forbids.
- The fix must separate the two causes of a move during the burst:
  - GTK reallocation preserving or clamping an offset (snap);
  - a scroll the user or the application made (keep).
- **Constraint on any discriminator: it is decided at the adjustment.**
  - Both the CI test and the failing-first experiment scroll through raw
    `GtkTextView::scroll_to_mark`, not an app entry point.
  - So do GTK's own cursor-onscreen, keyboard-navigation and search scrolls.
  - Marking intent only at LushText entry points (`scroll_to_document_iter`,
    the scroll controller) would leave both tests red. Making it pass by
    routing the tests through an app entry point is the "fix the test" this
    design rejects.
  - A discriminator decided anywhere else must ship with an exhaustive
    scroll-source inventory (every caller of `scroll_to_mark`,
    `scroll_to_iter`, `scroll_mark_onscreen` and `set_value` on the editor's
    vadjustment, plus GTK's internal ones) and a test per source.
- Candidate discriminators, chosen by evidence in task 2.1b:
  - classify each vertical `value-changed` during the burst by whether the
    same allocation also changed the vertical bounds or page size. A
    reallocation preserves or clamps the value together with a bounds change.
    A scroll changes the value alone. `ViewportObserver` already separates the
    bounds and value callbacks;
  - compare the moved value with what GTK's reallocation can produce (for
    example a clamp to the new `upper - page_size`);
  - intent marking at entry points, only together with the inventory above.
- Task 2.1b first deletes the snap and runs the `editor-page` shard. The tests
  that go red show what a GTK-driven move looks like. If none go red, the
  top-anchor scenario gets its own failing-first test before any fix.

**A kept scroll must also leave the rest state true.**

- The pause swallowed the scroll's `value-changed`, so after the settle,
  `rest_state.at_lower(Vertical)` still says "at top" at line 1,000.
- Keeping the value is therefore not enough:
  - the next height change runs `schedule_top_edge_vertical_scroll_clamp`
    (`on_viewport_bounds_changed`), whose value has not moved since it was
    scheduled, so it clamps to the top;
  - the next width burst snaps again.
- So the settle re-records the rest state from the settled adjustment
  whenever it keeps a moved value. The spec scenario "A kept scroll survives
  later reflows" pins it, failing first.

**Rejected:**

- waiting longer before the precondition in the test;
- removing the settle snap, which would regress "Minimap top content remains
  visible after shell reflow";
- moving the decision into `gtk-lush-viewport`'s `RestState`, unless the
  diagnosis shows the crate's contract is what is wrong. Its scenario "At-top
  state ignores reflow preservation" stays as it is.

**Diagnosed (stage 3; traces in `.claude/handoffs/restore-main-ci/g2-red/`,
`g2-nosnap/`, `g2-green/`).**

- **The writer is the settle snap.** An env-gated trace of every vertical
  `changed`/`value-changed` (test-utils build only, never committed) showed,
  in the failing experiment: the burst starts (`paused=true`, `at_lower=true`),
  `scroll_to_mark` delivers `value-changed` 0 → 4,511.5 → 12,220.9 with every
  `changed` still carrying 0, then `finish_minimap_reflow_settle` logs
  `settle-snap from 12220.9 to 0.0`. The top-edge clamp and the reattach hold
  never fired.
- **2.1b, snap deleted:** the whole `editor-page` shard (404 tests) and all 44
  minimap visual-geometry cases passed (`LUSHTEXT_DIAG_NOSNAP=1`, the debug
  binary gated the same way for the lane). No existing test reached a
  GTK-driven move at the top on GTK 4.24, so the top-anchor repair got its own
  test, `test_a_reallocation_offset_at_the_top_is_still_repaired_after_a_reflow`:
  during the burst it reconfigures the adjustment to a 120 px offset together
  with a 240 px larger upper (what a rewrap does). Red with the snap deleted
  (`y = 114`), green on `main` and with the fix.
- **Discriminator chosen: the value the last `changed` carried.**
  `gtk_adjustment_configure` sets the value before it emits `changed`, then
  emits `value-changed`; `set_value` emits `value-changed` alone; and
  `scroll_to_mark` moves the value with no `changed` (new ledger axiom **A112**,
  probe and sample; the sample holds on GTK 4.24.1). The page records the value
  each vertical `changed` carries (a raw handler: `ViewportObserver` reports
  page-size changes only), and a vertical `value-changed` during a paused burst
  to any other value sets `reflow_scrolled`. That is decided at the adjustment,
  so it covers every scroll source alike, which is why no scroll-source
  inventory ships. A reallocation that moves the value without changing a bound
  would read as a scroll; GTK's offset preservation and clamping change `upper`
  or the page with it, and no test or visual case showed another kind.
- **The settle** consumes `reflow_scrolled` on the current handle: when set it
  skips the snap and re-records the rest state from the settled adjustment;
  otherwise the snap runs as before.
- **Re-record proved separately.** `test_a_kept_reflow_scroll_survives_later_reflows`
  scrolls by one `set_value` during the burst: a `scroll_to_mark` keeps refining
  its value after the settle, and that later `value-changed` re-records the rest
  state by itself (seen in the trace), which hid a missing re-record. With the
  re-record disabled it is red at the next width burst (`y = -6`); with the
  classifier disabled (the `main` behaviour) it is red at its precondition.

### D3. Link the system libcurl and override one lockfile entry

`cargo-public-api` has no features of its own. OpenSSL enters through `curl`
and `curl-sys`, and a dependency's feature cannot be selected through
`cargo install`.

Verified in the host's `fedora:45` image (OpenSSL 4.0.3, libcurl 8.21.0).
Script and log: `api-advisory-container{,2}.{sh,log}`.

| Candidate | Result |
| --- | --- |
| CI today: `cargo install --locked cargo-public-api@0.52.0` | fails: the `openssl-sys 0.9.88` version check |
| + `libcurl-devel`, lock unchanged | fails: the same `openssl-sys` check |
| `openssl-sys` 0.9.118 only | fails: `curl-sys 0.4.77` builds its bundled curl 8.10.1, whose `vtls/openssl.c` does not compile against OpenSSL 4's `const X509_NAME` |
| **`libcurl-devel` + `openssl-sys` 0.9.118** | **installs**; `cargo public-api --version` prints 0.52.0; the binary links `/lib64/libcurl.so.4` and `libssl.so.4` |

The lockfile diff of the chosen candidate is exactly one entry: `openssl-sys`
0.9.88 → 0.9.118 (version and checksum). There are no resolver
consequences.

The chosen install:

1. The job's dnf step adds `libcurl-devel`, so `curl-sys` links the image's
   libcurl, which supports OpenSSL 4.
2. Download `cargo-public-api-${CARGO_PUBLIC_API_VERSION}.crate` from
   static.crates.io.
3. Verify it against `CARGO_PUBLIC_API_SHA256`. For 0.52.0 that is
   `80f55cea4022db86641f1ece5098c9df2f13d79624e61a461d5379dc5d4b7511`, the
   sparse-index `cksum`; the container run confirmed it.
4. Run `cargo update --package openssl-sys --precise
   ${CARGO_PUBLIC_API_OPENSSL_SYS_VERSION}` (0.9.118, `rust-version` 1.80).
5. Fail if the lockfile changed anywhere except that one entry.
6. Run `cargo install --locked --path .`.

Before step 4, the script also fails if the crate's own lock already pins an
`openssl-sys` at or above `CARGO_PUBLIC_API_OPENSSL_SYS_VERSION`. Otherwise,
after a tool bump, the precise update would quietly *downgrade* a dependency,
or keep an override nothing needs. With this check the retirement condition
below enforces itself: the first `CARGO_PUBLIC_API_VERSION` bump that no
longer needs the override fails until the override is removed.

The script's self-test runs offline from a locally built fake `.crate`, with no
network. It covers a tampered checksum, an unexpected lock change and an
override that is no longer needed. It is wired into an existing CI-exercised
policy target (`make check-policy`, or the one `build.md` names for script
self-tests), so it cannot rot unexercised. **As built:** `make
check-cargo-public-api-install` in `make check-policy`, and a step of the
advisory job itself, because no CI job runs `check-policy`.

The container verification built with Fedora's packaged `rust`/`cargo`, not
the job's pinned `nightly-2026-06-01`. `openssl-sys` 0.9.118 declares
`rust-version` 1.80, so no difference is expected. The branch's CI run (task
3.5) is the proof on the pinned toolchain. That run also records the
install step's duration on a cold cache against the job's 20-minute timeout,
because `cargo install --path` rebuilds the tool even when `rust-cache`
restores `~/.cargo/bin`. If the cold install leaves too little margin, the
script skips the build when `cargo public-api --version` already reports the
pinned version.

All three values go in `ci.yml`'s `env` and in `build.md`'s pin list. A small
`scripts/install-cargo-public-api.sh` holds the steps, so a developer can run
them in a `fedora:45` container. Drop the override, and `libcurl-devel` if it
is no longer needed, when a release locks `openssl-sys >= 0.9.114` and a
`curl-sys` that builds there.

**Rejected:**

- dropping `--locked`: every transitive dependency would float and could
  outgrow the pinned nightly toolchain;
- also raising `curl-sys` to a bundled curl that supports OpenSSL 4: that
  is a second override where the image's libcurl already works;
- moving the job to `fedora:44` (OpenSSL 3.x): the GTK Lush crates need GTK
  4.24's development files, and `fedora:44` has 4.22;
- a prebuilt binary: none is published that is linked for this image;
- removing `cargo-public-api`: `build.md` requires inspectable snapshots.

### D4. Retry-rescued tests are classified, then fixed at the cause

For each test, follow the gtk-testing skill's flake discipline:

1. Reproduce it in isolation under CPU starvation pinned to one or two cores.
   Build first on CPUs 8–23, and pin only the run.
2. Classify the wait: a synchronous UI flip, an asynchronous completion, or a
   production race.
3. Fix the cause failing first.
4. Prove the fix with at least 20 consecutive isolated passes under the same
   starvation.

**When starvation does not reproduce it.** The bookmark and capture tests each
failed once, so a local reproduction may not come. Then:

- the diagnosis comes from the CI log (which predicate, at which line) and from
  instrumentation behind `test-utils`;
- the failing-first test is a deterministic test of the diagnosed mechanism,
  as 2.1 did for the minimap;
- the flake's regression proof is CI: the affected shard without a `FLAKY`
  line for that test on at least **eight** branch runs.

  Four runs are not enough. Even a test failing on about half of all runs
  passes four in a row about 6 % of the time (0.5⁴), and these failed on 1
  of about 15 runs. That count is recorded, not presented as proof of absence.
- The same branch applies to task 2.2's minimap original.

A longer timeout is a fix only for a wait classified as an asynchronous
completion whose budget is below the gtk-testing skill's class budget (≥5–10 s).
These waits are already 10 s and 30 s, so a timeout bump is expected **not** to
be the fix here.

Each test has its own first question:

- **Directory charge:** the upper mode exceeds the bound on about every
  second CI attempt. It was rescued 5 times: 43108136719 (2026-11-11), 43612937829,
  43614093974, 43615947076 and 43629270107.
  - Both modes already exceed the charge itself: about 1.21× and 1.30× of
    roughly 1,944–1,960 B/row. Granularity alone therefore cannot make a
    1,024 B fixed charge right, and the bound holds only through its 5/4
    margin.
  - The two modes differ by a near-constant 2.7 MB (≈ 165 B/row × 16,384).
    That points at a fixed-size event: an allocator arena, a `Vec`/store
    doubling, or a worker scan batch alive at the same instant as the GTK
    rows (`VmHWM` is a high-water mark, so a schedule-dependent overlap is
    bimodal). Three hypotheses, not two:
    - (a) undercharge: per-row cost really is higher on this toolkit;
    - (b) a constant-size allocator or capacity step;
    - (c) a schedule-dependent transient peak that the budget is meant to
      cover.
  - Decision procedure: measure `VmHWM` growth at two row counts (16,384 and
    32,768, on the same fixture shape). Fit slope and intercept: the slope is
    the per-row cost the charge must cover; a mode-dependent intercept is (b)
    or (c).
  - (c) is a production question, not a test one. If the budget's resource
    claim is the high-water, the charge covers the transient. Otherwise the
    test measures steady retention, and the rustdoc says so.
  - **Recalibrating the charge cascades. The design must choose before
    editing.**
    - `SIDEBAR_ROW_FIXED_CHARGE` is a coverage window (`rust.md`): it rounds
      up, so any value above 1,024 becomes 2,048.
    - Then `SIDEBAR_TYPICAL_FLOOR_RETENTION` = 131,072 × ((256 + 1 + 32 + 16)
      × 8 + 2,048) = 588,251,136 bytes, above the 512 MiB
      `SIDEBAR_DIRECTORY_RETAINED_BYTES`. The `const` assertion stops
      compiling.
    - The budget is a presentation cap, so the only rounding is *up*, to
      1 GiB. That doubles the memory one expanded directory may retain, and
      `rust.md` requires measuring that the resource bound still holds.
    - The alternative is a recorded exception: an exact charge with its
      measurement.
    - User experience first: keep listing 131,072 typical entries in full.
      Never lower the floor to fit 512 MiB.
    - Whichever is chosen updates the `docs/thresholds.md` rows, passes `make
      check-thresholds` and builds the benches, which read
      `SIDEBAR_DIRECTORY_RETAINED_BYTES` and `sidebar_scan_budget`. The
      `core-list-budget` harnesses import only `model/list_budget.rs`, not the
      sidebar constants, so that shard reruns only if that module changes.
  - The 5/4 bound is a budget claim; widening it is not a fix.
- **Bookmark preview-only, protected periodic capture:** find which predicate
  timed out and what it waits on.

**Diagnosed (stage 3).**

- **Directory charge: (c), asynchronous setup work overlapping the window,
  not an undercharge.** Fitted in separate processes on the workstation (10
  runs per size, the old 300 ms settle): 16,384 rows 32.28 MB, 32,768 rows
  59.21 MB median, so 1,644 B/row plus about 5.3 MB, against a charge of
  1,960 B/row. Four CPUs instead of sixteen changed nothing. The constant is
  not a fixed cost of the rows: after the refresh went idle the resident set
  kept rising for about a second (a single-directory trace: +2.5–3 MB, then
  flat), and with two more large sibling directories in the fixture a 16,384
  row first expansion grew 2,328–2,561 B/row after 300 ms (CI's modes; four of
  five over the 2,430 B/row bound) but 1,322–1,329 B/row after a 5 s settle.
  How much of that tail lands in the measured window depends on the machine's
  speed: CI's bimodal excess. Decision: no recalibration, so no cascade, no
  budget, floor or thresholds change and no `core-list-budget` rerun. The test
  now waits for the resident set to be quiescent before `reset_vm_hwm`
  (`common::wait_for_resident_quiescence`: four readings 250 ms apart within
  256 KiB) and measures 32,768 rows, so a residual constant weighs half as much
  per row while a per-row undercharge still fails: 20/20 isolated passes,
  1,369–1,552 B/row against a 2,450 B/row bound. The expansion may reuse memory
  the setup tail freed, so the figure checks the charge and does not calibrate
  it. The cause is diagnosed locally, CI's figure with the fix is not yet
  measured: the `surfaces` shard joins the eight-CI-run proof list. A warm-up
  expansion before the measurement was tried and rejected (the measured rows
  reuse what it freed, 1,381–1,612 B/row by fixture).
- **Protected periodic capture: an asynchronous admission the test raced.**
  The CI log names the snapshot wait (`not met within 30s`). The edit that makes
  the document protected also starts its baseline capture, which holds the
  process-wide automatic-capture admission on a worker; a periodic capture that
  finds it taken reschedules a whole interval (5 min) later. The deterministic
  test delays the baseline by 3 s: red without the fix, green with it; the
  fix waits for `automatic_capture_inflight` to clear. Data-safety reading:
  production defers a colliding periodic snapshot by one interval, never loses
  one, and drafts protect the text meanwhile, so production is unchanged.
- **Bookmark preview-only: not reproduced.** 20/20 starved runs on the
  `b129f378` core and 40/40 on the branch's. The CI failure (document shown, top line 69,
  cursor 300) is not explained by the reflow snap (it goes to line 0) or the
  top-edge clamp; the trace of a passing run shows the reattach scroll and
  GTK's anchor-keeping reconfigurations only. The failure message now carries
  the attach/detach counts and the vertical adjustment, and the proof is the
  `window` shard without its `FLAKY` line on eight branch CI runs (open).

### D5. Scope stays "every `ci.yml` job green on the first attempt"

- Lanes outside `ci.yml` are a separate concern: weekly Kani, end-user smoke,
  release.
- A new failure that appears while this change is being applied joins it, as
  `preexisting-blockers.md` requires.

## Axioms

- D1 relies on no toolkit behaviour beyond the suspended handlers the app
  owns, and on `GtkTextBuffer` setting its modified flag on any insert or
  delete. That is documented API behaviour, not a ledger axiom. The clean-tab
  test in task 1.1 observes it.
- D2 relies on the emission order: **A112** (added, with probe
  `probe_a112` and sample `a112_a_scroll_moves_the_value_no_changed_emission_carried`):
  `configure` sets the value before `changed`, then `value-changed`;
  `set_value` emits `value-changed` alone; `scroll_to_mark` moves the value
  with no `changed`.
- D3 and D4 rely on none.

## Kani

- None expected: no proven module is touched.
- Conditional on the outcome of the diagnosis:
  - If D2 adds a pure decision to `ui/editor_page/minimap/policy.rs`, rerun
    `core-geometry-policies` once on the final code.
  - If D4 changes a constant or function in a module a harness reads
    (`model/list_budget.rs`, or the sidebar policy reached through it), find
    the harnesses with `kani-shards.py oracle` and `grep -rln` and run those
    shards once.

## Timing

Timing: none (no timed path's cost changes).

- D1 adds one flag write per owing replacement terminal.
- D1 also schedules the first-dirty autosave of the restored body. That body
  goes through the existing bounded snapshot-write pipeline (chunked capture
  across GTK turns, worker write), whose budgets are unchanged and which
  already runs for any edit of the same document. No new GTK-turn cost.
- D2 adds one cell write per vertical `changed` emission and one comparison
  per vertical `value-changed`, both O(1) on the GTK thread, plus one
  rest-state record when a settle keeps a scroll. No timed path's cost
  changes measurably, so no timing campaign runs.

## Risks

- D2 can turn out to be a second mechanism: the vertical clamp, or a
  reattach hold. The spec's behaviour still holds; only the fix site moves.
- D3 depends on static.crates.io being reachable from CI, as `cargo install`
  already does.
