## Context

`main` (`b129f378`, GNOME 51 toolkit: GTK 4.24, GtkSourceView 5.22, Libadwaita
1.10; CI image `fedora:45`) has had no green `ci.yml` run since `3dcbe682`.
This design covers the blockers in the proposal. It records how each was
classified, what is measured and what is still a hypothesis.

## Measured baseline

**B1. CI failure census.** Taken with `gh run view --log` over every `ci.yml`
run since 2026-11-12, plus `43108136719`.

| Blocker | Runs | Signature |
| --- | --- | --- |
| window draft test | 43614093974, 43619204060, 43623373361, 43625734736, 43629270107 | "the restored text is still owed a draft" (both attempts, both job tries) |
| editor-page minimap test | 43108136719, 43147416345, 43612937829, 43615947076, 43619204060, 43622208418, 43627345020, 43627976239, 43632470998 | "precondition: the editor stays scrolled to the middle (Rectangle { x: 0, y: -6, width: 808, height: 664 })" |
| GTK Lush API Advisory | every run since 2026-11-11 | `openssl-sys-0.9.88` build script panics on OpenSSL `4_0_3` |
| retry-rescued (`FLAKY`) | directory charge ×4 (43612937829, 43614093974, 43615947076, 43629270107); bookmark preview-only ×1 (43614093974); protected periodic capture ×1 (43627976239) | see B4 |

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

| Workflow | Owes a draft? | Why |
| --- | --- | --- |
| `DraftRecovery` | yes (already does it inline) | `restore_execution.rs` sets it, so the text survives even if the set-aside copy is deleted |
| `LocalHistoryRestore` | **yes (missing)** | the buffer now holds the snapshot; the draft holds the pre-restore text |
| `LocalHistoryUndo` | **yes (missing)** | same, in reverse |
| `SaveFormatting` | no | the terminal mirrors the saved bytes and leaves the tab unmodified |
| `MemoryEviction` | no | evicted tabs are never draft candidates; reload restores from disk |

- Advancing the generation at the terminal covers both orderings in B3:
  - a pass committed **before** the terminal cleared the flag, and the
    terminal sets it again;
  - a pass committing **after** the terminal sees a stale generation and
    leaves the flag set.
- Marking at the replacement **start** as well is not needed. A capture
  during the replacement is refused anyway (`draft_capture_blocked`).
- **Rejected: fixing the test.**
  - Removing the test's real timer would hide a deterministic production
    defect: B3's first experiment needs no timer at all.
  - The existing test still changes its post-terminal assertion to "owed, or
    the restored text already persisted". A real tick that runs and commits
    between the terminal and the assertion is correct behaviour, not a
    failure.

### D2. A scroll during a passive burst is kept; pin the writer first

The mechanism is reproduced deterministically (B5): a scroll made during a
passive width burst is undone. CI reaches it when a starved main loop leaves
the startup burst pending at the test's scroll. What remains is which site
writes the value back. The leading candidate comes from reading the code:

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

The likely fix shape is to snap to the lower edge at settle only when the
value did not move after the burst began. This is the rule the vertical clamp
already applies to its own idle. Record the burst-start value with the pause,
and compare it at settle. A GTK-preserved offset keeps the value fixed while
the content moves under it, so it still snaps. A scroll moves the value, so it
is kept.

**Rejected:**

- waiting longer before the precondition in the test;
- removing the settle snap, which would regress "Minimap top content remains
  visible after shell reflow";
- moving the decision into `gtk-lush-viewport`'s `RestState`, unless the
  diagnosis shows the crate's contract is what is wrong. Its scenario "At-top
  state ignores reflow preservation" stays as it is.

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
2. Classify the wait: a synchronous UI flip, an asynchronous completion, or a
   production race.
3. Fix the cause failing first.
4. Prove the fix with at least 20 consecutive isolated passes under the same
   starvation.

Each test has its own first question:

- **Directory charge:** the upper mode exceeds the bound on every second CI
  attempt. Is it a real undercharge of `SIDEBAR_ROW_FIXED_CHARGE` on this
  toolkit? If so, recalibrate it from the measured CI growth and keep the 5/4
  bound. Or is it allocator page granularity, in which case measure growth
  over a larger row count so the granularity divides out? The 5/4 bound is a
  budget claim; widening it is not a fix.
- **Bookmark preview-only, protected periodic capture:** find which predicate
  timed out and what it waits on.

### D5. Scope stays "every `ci.yml` job green on the first attempt"

- Lanes outside `ci.yml` are a separate concern: weekly Kani, end-user smoke,
  release.
- A new failure that appears while this change is being applied joins it, as
  `preexisting-blockers.md` requires.

## Axioms

- D1 relies on no toolkit behaviour beyond the suspended handlers the app
  owns.
- D2 may rely on the order in which `GtkTextView` updates its adjustments
  relative to `value-changed` during `scroll_to_mark`, and on page-size
  `changed` emissions during reallocation. If the diagnosis depends on that
  order, the implementation adds a ledger axiom with a `gtk-lush-axioms` probe
  and sample, and runs `make gtk-axioms`.
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

Timing: none (no timed path's cost changes). D1 adds one flag write per
replacement terminal. D2 adds one comparison per settle.

## Risks

- D2 can turn out to be a second mechanism: the vertical clamp, or a
  reattach hold. The spec's behaviour still holds; only the fix site moves.
- D3 depends on static.crates.io being reachable from CI, as `cargo install`
  already does.
