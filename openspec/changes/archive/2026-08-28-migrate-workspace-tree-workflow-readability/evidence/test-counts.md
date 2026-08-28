# Test counts and seam census (tasks 6.4, 10.4)

## 1. Project test count — must not decrease

| Lane | Before (`origin/main`) | After | Delta |
| --- | --- | --- | --- |
| non-widget (`cargo nextest run --workspace --all-features`) | — | **1741 passed, 11 skipped, 0 failed** | **+25 net new** |

The +25 are attributable rather than merely larger. **The per-file table below was
written mid-change and its "After" column is superseded**; the figures re-derived in the
fix cycle are:

| File | HEAD | Now | Delta |
| --- | --- | --- | --- |
| `ui/sidebar/policy.rs` | 9 | **34** | +25 |
| `ui/sidebar/seams.rs` | 4 | **8** | +4 |
| `services/file_tree.rs` | 24 | **27** | +3 |
| `services/filesystem/mod.rs` | 21 | **23** | +2 (fix cycle: the two `link_inode` contract tests) |
| `model/workspace_persistence.rs` | 6 | *deleted* | −6 |
| `model/workspace_scan.rs` | 3 | *deleted* | −3 |
| **total** | **67** | **92** | **+25** |

The two deleted modules' 9 tests **moved with them** into `policy.rs`, which is what made
the mutant-by-mutant relocation parity provable; netting the move out, `policy.rs`'s own
new tests are 16. The fix cycle contributed 3 of the 25: the two `link_inode` contract
tests and `a_vanished_target_is_still_distinguished_from_a_substituted_one`, with a fourth
existing test renamed rather than added
(`a_vanished_target_is_refused_rather_than_treated_as_already_done` →
`a_vanished_target_reconciles_the_row_instead_of_reporting_a_refusal`).

### Where the 17 came from

Counted as `#[test]` functions in the affected population, which is the only
population this change adds to:

| File | Before | After | Delta |
| --- | --- | --- | --- |
| `ui/sidebar/policy.rs` | 9 | **28** | +19 |
| `ui/sidebar/seams.rs` | 4 | **8** | +4 |
| `model/workspace_persistence.rs` | 6 | *deleted* | −6 |
| `model/workspace_scan.rs` | 3 | *deleted* | −3 |
| `services/file_tree.rs` | 24 | **27** | +3 |
| **total** | **46** | **63** | **+17** |

The three `file_tree.rs` tests are the inherited-survivor triage; that file's
behaviour is unchanged and only its `#[cfg(test)]` module grew.

The −9 from the two deleted modules is not a loss: their tests **moved with them**
into `policy.rs`, which is why the relocation preserved mutation parity exactly (see
`evidence/mutation-workspace-tree-policy.md` §2). Netting the move out, the genuinely
new tests are **17**:

| # | Test | Purpose |
| --- | --- | --- |
| 1–4 | `a_confirmed_delete_proceeds_only_against_the_identity_the_user_was_shown`, `a_same_name_different_object_is_refused`, `a_vanished_target_is_refused_rather_than_treated_as_already_done`, `an_unreadable_original_identity_is_refused` | the extracted `confirmed_delete_verdict`, which is the fix for a confirmed **HIGH** data-safety defect |
| 5–9 | `a_generation_reports_its_own_ordinal_rather_than_a_constant`, `a_busy_worker_refuses_a_second_start_even_with_newer_work_pending`, `a_settled_state_reports_no_pending_work`, `dirty_work_alone_is_pending_work_without_a_failure_or_a_worker`, `the_durable_generation_advances_past_the_default_on_success` | triage of **six** of the seven inherited persistence survivors, killed by tightening assertions |
| 10 | `an_in_flight_write_and_a_recorded_failure_are_mutually_exclusive_and_both_imply_dirt` | pins the invariant that makes the seventh survivor provably **equivalent**, so its narrow exclusion cannot silently outlive its justification |
| 11–14 | `an_unchanged_install_may_adopt_its_watcher`, `a_stale_lifetime_retires_rather_than_restarting`, `a_stale_target_generation_restarts_rather_than_retiring`, `a_stale_lifetime_wins_over_a_stale_target_generation` | the reified `WorkspaceWatchTicket`, including the **ordering** guarantee that a stale lifetime beats a stale target generation |
| 15–17 | `byte_bounded_scan_reports_read_errors_on_its_own_path`, `byte_bounded_scan_reports_cancellation_with_the_entries_it_had_examined`, `a_pre_cancelled_byte_bounded_scan_examines_nothing` | the `services/file_tree.rs` triage: the byte-bounded scan function's error and cancellation paths were **never exercised at all**, because the pre-existing tests route to the no-byte-limit variant |

**No test was weakened or deleted to make anything pass.** The count does not
decrease on any axis.

## 2. Seam census — **60 / 111 → 41 / 93**, retirement performed

| Quantity | Before | After | Delta |
| --- | --- | --- | --- |
| `*_for_test` functions under `ui/sidebar/` | **60** | **41** | **−19** |
| `#[cfg(feature = "test-utils")]` gate sites | **111** | **93** | **−18** |
| new seams added | — | **1** | the budgeted M-4 load-worker delay |

**Nineteen inspection seams retired into the evidence surface, and every one of their
~114 widget-test call sites rewritten to read it.** Net −19 despite adding the one
budgeted seam.

### What made retirement possible: a second granularity, not a second surface

Most of this workflow's observable state is **per-section**, and many tests hold only a
`LushtextWorkspaceSection`. The evidence module therefore owns two types —
`WorkspaceTreeEvidence` (sidebar) and `WorkspaceSectionEvidence` (section, 29 fields) —
with the sidebar's aggregates **derived from** the per-section vector it carries, so a
reader can always check an aggregate against its parts. That is one evidence *module*
per workflow, at the workflow's two real granularities; it is not two surfaces.

### The seams retired

| Retired | Now read as |
| --- | --- |
| `workspace_refresh_blocks_readiness_for_test` (20 sites) | `refresh_blocks_readiness` |
| `child_scan_pressure_for_test` (19) | `scan_pressure` |
| `watch_targets_for_test` (16) | `watch_targets` |
| `workspace_watcher_is_current_for_test` (12) | `watcher_is_current` |
| `reconciliation_metrics_for_test` (6, a 5-tuple) | five named fields |
| `refresh_pressure_for_test` (5, a 2-tuple) | two named fields |
| `expanded_paths_for_test` (5) | `expanded_paths` |
| `watch_target_generation_for_test` (5) | `watch_target_generation` |
| `workspace_watch_pressure_for_test` (4, a 4-tuple) | four named fields |
| `context_target_{path,workspace_folder_id}_for_test` (7) | two named fields |
| `expansion_capture_metrics_for_test` (2, a tuple) | two named fields |
| `take_watch_target_rows_touched_for_test` (2, **destructive**) | a non-destructive field **plus** a separate reset drive |
| `child_cache_rebuild_metrics_for_test` (a 2-tuple), `empty_probe_reads_for_test`, `workspace_watcher_{worker_starts,unavailability_is_current}_for_test`, `workspace_folder_reorder_drag_hover_fallback_count_for_test`, `workspace_scan_{active_tasks,task_high_water}_for_test` | named fields |

**Five tuple-returning seams became named fields** — `reconciliation_metrics`,
`refresh_pressure`, `workspace_watch_pressure`, `expansion_capture_metrics`, and
`child_cache_rebuild_metrics` — which is the readability win beyond the count:
`reconciliation_metrics_for_test().4` is now `child_reconcile_sources`. (An earlier
revision of this file said six; the count was re-derived from the five `-> (...)`
signatures in `HEAD` and corrected.)

### The destructive read, and the trap splitting it set

`take_watch_target_rows_touched_for_test` was a `take`: counting mutated. It became a
non-destructive `watch_target_rows_touched` field plus a separate
`reset_watch_target_rows_touched_for_test` **drive** — exactly what the evidence rules
require.

**The mechanical rewrite then dropped a reset.** One test used the seam *purely* for its
reset side effect, and rewriting it into a discarded read left a cumulative counter
asserted against `<= 2` after 32 expansions. The widget lane caught it. **Carry this
forward: when a destructive read is split, every former call site must be classified as
observation *or* reset — the two are indistinguishable at the call site.**

### The one budgeted seam, spent

`set_workspace_load_worker_delay_for_test` — the third and final counted seam, budgeted
in advance for M-4's driven race test (task 8.1), justified individually at its
definition beside the two slot 5a added. It delays the load worker **after** the read,
not before: M-4 is about adopting a snapshot that has since gone stale, so the worker
must carry pre-mutation state across the interposition window.

### What is deliberately *not* retired

- **Actuation seams stay, counted**: the six `dialogs.rs` bypasses, the watcher
  merge/disconnect/poll/pause/stop drives, the refresh queue/apply drives, and the DnD
  hover simulations. These are programme-level deferrals with their reason recorded.
- **Probes and oracles stay with their reason**: `derived_expanded_paths_for_test` is the
  **oracle** for the full model derivation and must remain outside production reach —
  the dissolution of `tree_index.rs` was precisely the move that could have turned it
  into a production caller, and did not.
- **Configuration seams stay** in `test_policy.rs`, now three.

### The two ungated bench seams: still outside the census, still undisposed

Narrowing did not force their disposition, because the surface was **added alongside**
the existing observations rather than replacing the bench path. Both are untouched and
the bench target compiles. A later slot must decide them before narrowing further, and
must remember the second belongs to the **service**, not this row.

## 3. Widget lane — clean at `--retries 0`

```
./scripts/run-widget-tests.sh --headless --retries 0
→ exit 0,  all tests passed,  FLAKY lines: 0,  WARNING/CRITICAL lines: 0
```

**No retry relied upon.** The lane grew by this change's four evidence proofs and its
driven M-4 race test, and `make test-workspace-row-states` passes independently.

### Two real defects the lane caught, both introduced by this change

Recorded because a green lane is unremarkable and a lane that *catches* things is the
point:

1. **`test_one_row_collapse_touches_only_its_incremental_watch_delta`** failed after the
   seam retirement, because splitting the destructive "take touched rows" read into a
   read plus a reset silently dropped a call site that had used the seam **purely for
   its reset**. Fixed by calling the reset.
2. **`test_workspace_tree_evidence_answers_honestly_across_a_real_section_teardown`**
   failed `left: 3, right: 2` after the pass-2 CRITICAL fix, because the first version
   of that fix **resurrected a workspace the user had deleted** — a merge cannot express
   a deletion. Fixed by gating merge on whether any load has been adopted, and pinned by
   a pure unit test.

### One flake risk hardened rather than tolerated

Comparing the **full** evidence surface twice with live watchers on real tempdirs could
differ if an inotify notice lands between the reads. `evidence_without_live_mailbox`
normalizes only the mailbox and poll-notice count and compares everything else exactly:
the reentrancy claim is that *reading* does not mutate, not that the kernel is quiescent.

## 4. Proof lanes, run last from clean artifact roots

Ordered after **all** source, documentation, and rules edits, because the accessibility
policy gate fingerprints the *contents* of accessibility-relevant files.

| Lane | Result |
| --- | --- |
| `make accessibility-smoke` | **pass** |
| `make visual-smoke` | **pass** |
| `make visual-geometry-smoke` | **pass** — pixel- and animation-verified invariants plus the 6-case workspace-sidebar animation matrix |
| `make automation-smoke` | **pass** — live D-Bus capture of `window.workspace` matches the contract field for field |
| `make performance-smoke` | **pass** across all 17 filters |
| `make test-workspace-row-states` | **pass** |

`make check` and `make check-policy` both exit **0**, including
`check-workflow-boundaries`, `check-automation-docs`, `check-accessibility-policy`,
`check-visual-proof-policy`, and `check-filesystem-boundary`. The rustdoc lint gate —
CI-only, in none of those targets — is clean, run by hand after every new `pub` module.

**One ordering note worth carrying:** an accessibility-policy false positive appeared
because a module doc said "drag-hover" while describing what had moved *away* to another
module. The gate looks for hover affordances lacking keyboard or accessible parity; that
file owns none. Reworded to name the module that does own it — and the gate was right to
ask, since that module does satisfy the rule.
