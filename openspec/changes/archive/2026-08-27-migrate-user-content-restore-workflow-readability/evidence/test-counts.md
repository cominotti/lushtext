# Test counts (task 10.3)

The count must not decrease. It increased on both lanes.

## Non-widget lane

Two invocations, because they cover different feature sets and both matter:

| Invocation | Result |
| --- | --- |
| `cargo nextest run --workspace --all-features` | **1,657 passed, 11 skipped** |
| `make test`'s non-widget half (`cargo nextest run --workspace`, no `--all-features`) | **1,609 passed, 11 skipped** |

The `--all-features` figure is the one the delta is computed against, because the
`test-utils` feature gates this change's test-only policy modules:

| | Count |
| --- | --- |
| After | **1,657** |
| Before | **1,624** |
| Delta | **+33** |

An earlier draft recorded 1,632 / +8. That was measured after section 3 only;
sections 4 through 6 relocated three more policy modules and closed their mutation
survivors, and those tests account for the rest. The corrected delta reconciles
**exactly** from the diff: with the new role directories made visible to `git diff`
via `git add -N` (they are untracked, so a plain `git diff origin/main` silently
reports zero added tests — a trap worth knowing),

```
$ git diff origin/main -- crates/ ':(exclude)crates/lushtext/tests/widget' \
    | grep -cE "^\+\s*#\[test\]"      #  60 added
    | grep -cE "^-\s*#\[test\]"       #  27 removed
```

60 − 27 = **+33**, which equals 1,657 − 1,624 with no residual.

**That arithmetic was right and the explanation attached to it was wrong.** This
document previously claimed the 27 removals "reappear among the 60". The
independent review checked that claim per-test and found it **false for five of
them**: they were deleted with no same-named or equivalent survivor, and the net
`+33` concealed it because 5 lost tests plus 38 new ones nets the same as 33 new
ones. A net count cannot detect a deletion, which is the lesson — pair the net with
a per-test survivor check when a change moves whole files.

Worse, two of the five were the **only** coverage of
`services/draft_service/cleanup_types.rs::merge_committed_orphan_removals`, the
function deciding which manifest entries a *destructive* cleanup pass removes. That
file dropped to **zero** tests while sitting inside the `services/**` mutation
`examine_globs` — a live mutation-coverage regression on a destructive path, which
this change's own diff-scoped mutation runs could never have surfaced because the
file was not in their diff.

All five were restored to homes matching the new structure, and each was
**strengthened** rather than merely pasted back:

| Restored test | New home | Strengthened how |
| --- | --- | --- |
| `cleanup_merge_removes_only_exact_committed_generation` | `services/draft_service/cleanup_types.rs` | assertion message names the safety property |
| `cleanup_merge_removes_matching_generation_and_preserves_additions` | `services/draft_service/cleanup_types.rs` | unchanged assertions |
| `restore_ticket_rejects_every_stale_identity_dimension` | `ui/window/drafts/seams.rs`, beside `draft_restore_is_current` | **+3 dimensions**: absent path, absent draft id, absent manifest entry — each must reject rather than default to a match |
| `eager_preload_release_preserves_lazy_markers_for_slow_file_loads` | `ui/window/drafts/retirement.rs`, beside `detach_eager_preload_bodies` | now states *why* an already-compact `Oversized` marker must not be flattened |
| `periodic_capture_admission_allows_only_one_text_payload` | `ui/editor_page/local_history.rs`, beside `AutomaticHistoryCapturePermit::try_acquire` | asserts the permit is re-acquirable after drop **and** that a second concurrent acquire is refused |

Three further tests were added to close the gap the five had left rather than just
restoring the status quo:

- `cleanup_merge_requires_every_fingerprint_dimension_to_match` — perturbs mtime,
  path, and `saved_at_secs` **one at a time**. The two restored merge tests differ
  in all three fields at once, so a mutant dropping a single comparison from
  `DraftEntryFingerprint::matches` would survive both of them.
- `cleanup_merge_with_no_commits_removes_nothing` — an empty commit set must be a
  no-op, not a manifest clear.
- `confirmed_cleanup_count_saturates_instead_of_overflowing` — pins the saturating
  add.

Plus, for S5's relocation of the cleanup grouping walk into `policy.rs`, three
tests over **real** service enum variants with distinct multiplicities
(1 status / 2 delete / 3 manifest) so a swapped pair of match arms cannot produce
the expected string, one for the all-zero case, and one proving both
`DraftOrphanCleanupManifestError` variants count as the same category.

The remaining 22 removals were re-checked and **do** reappear among the 60,
including the 3 `pairing_tests` carried over verbatim.

The `Before` figure is derived rather than measured against a rebuilt baseline,
because a
`git stash` of `crates/` does not produce a compilable tree — this change deletes
`ui/editor_page/buffer_replacement.rs` and adds a directory in its place, so the
stash leaves `ui/editor_page/mod.rs`'s re-exports pointing at a module that exists
in neither state. (Attempted and recorded: the stashed tree fails with
`could not compile lushtext-core (lib test) due to 10 previous errors`, and the
worktree was restored intact and re-verified clean afterwards.)

The arithmetic is exact and checkable from the diff:

- the retired `ui/editor_page/buffer_replacement.rs` held **3** `#[test]`
  functions (its `pairing_tests` module);
- the new `ui/editor_page/buffer_replacement/` holds **11**: 8 in `policy.rs` and
  the same 3 pairing tests, carried over verbatim into `execution.rs`;
- no other non-widget test was added or removed.

So +8, all of them the new pure policy's own coverage — the same 8 tests that kill
15 of the 19 mutants the extraction gained (the other 4 are unviable by design).

## Widget lane

Final run, `./scripts/run-widget-tests.sh --headless --retries 0`:
**1,143 tests run, all passed, 0 `FLAKY:` lines, 0 retries used, 0 suite reruns.**

| | Count |
| --- | --- |
| After | **1,143** |
| Before | **1,134** |
| Delta | **+9** |

An earlier draft of this document recorded 1,137 / +3. That figure was measured
part-way through the change, when only buffer replacement's two evidence proofs
and the data-safety regression existed; sections 4 through 6 then added the same
two proofs for each of the remaining three surfaces. The corrected delta is
confirmed two independent ways: the harness reports `running 1143 tests`, and
`git diff origin/main -- crates/lushtext/tests/widget/` shows **9 added test
functions and 0 removed**, which reconciles exactly against the unchanged
`Before` figure of 1,134.

The nine are all new, in three groups of a shared purpose — one **reentrancy
proof** and one **disposal proof** per evidence surface, plus the single
data-safety regression:

| Test | Purpose |
| --- | --- |
| `window::test_incomplete_load_installation_blocks_draft_autosave_over_a_good_draft` | the confirmed data-safety defect's regression, proven to fail without the fix |
| `editor_page::test_buffer_replacement_evidence_reads_stay_side_effect_free_across_replacement_mutation` | reentrancy proof: drives each mutable-borrow operation, then reads the surface **after** it |
| `editor_page::test_buffer_replacement_evidence_reads_survive_widget_disposal` | disposal proof: `buffer_char_count` answers `None` rather than zero for a disposed page |
| `editor_page::test_local_history_evidence_reads_stay_side_effect_free_across_capture_mutation` | reentrancy proof for the local-history surface |
| `editor_page::test_local_history_evidence_reads_survive_widget_disposal` | disposal proof that **caught a real panic**: the availability accessor dereferenced a template child, fixed by `live_local_history_availability_for_chars` |
| `window::test_session_restore_evidence_reads_stay_side_effect_free_across_restore_mutation` | reentrancy proof for the session-restore surface |
| `window::test_session_restore_evidence_reads_survive_widget_disposal` | disposal proof: `mounted_pages` answers `None` for a disposed window |
| `window::test_draft_evidence_reads_stay_side_effect_free_across_draft_mutation` | reentrancy proof for the draft surface |
| `window::test_draft_evidence_reads_survive_widget_disposal` | disposal proof for the draft surface |

The four disposal proofs are the ones that earned their place: the rule requiring
them is stated once in `widget-wiring.md`, and applying it mechanically to all
four surfaces is what surfaced the local-history panic. Three of the four would
have passed without the rule; the fourth was a real defect.

**No widget test was deleted.** The four retired inspection seams
(`buffer_replacement_{in_progress,projection_suspended,slice_count,terminal_diagnostic}_for_test`)
were mechanically rewritten to their evidence-surface equivalents at 13 call sites
across `editor_page.rs` and `window.rs`; every assertion they carried is still
made, now through the one typed surface.

## Proof lanes

Run **after** every source, documentation, and rules edit, each from a **clean
artifact root** (`rm -rf` on the three roots first, because a stale case
directory from an earlier run can make the root summary report evidence the
current binary did not produce):

| Lane | Result |
| --- | --- |
| `make visual-geometry-smoke` | **80 cases passed, 0 failed, 0 skipped**, pixel-verifying `native-minimap-highlight-anchors` and animation-verifying `native-minimap-animation-highlight-anchors`, with the workspace-sidebar animation matrix covering 6 cases |
| `make accessibility-smoke` | **PASS** — AT-SPI anchors and focus artifacts verified. The lane keeps the accessibility bridge enabled, which the widget harness cannot (`NO_AT_BRIDGE=1`) |
| `make visual-smoke` | **PASS** — every scenario captured, including `recovery-startup`, which is this family's own visual surface |
| `make crash-recovery-smoke` | **PASS** — real process, real `SIGKILL`, real relaunch, recovery verified through AT-SPI plus app-owned metadata, with `crash-recovery-smoke-driver.py` unmodified |
| `make automation-smoke` | **PASS** on the changed tree |
| `make performance-smoke` | **PASS**, including the two-startup multi-page draft-repair survival proof and the headless reentrant buffer-replacement proofs. Its Criterion *timings* are separately recorded as not interpretable on a saturated machine — see appendix A.14 |

All six were re-run **after** the final source, documentation, and rules edits, and
`find crates/lushtext-core/src/ui crates/lushtext/tests/widget resources/ui
resources/style -newer <widget-log>` confirms **no fingerprinted source changed
after the widget lane**, so the fingerprints these lanes wrote are live rather than
stale.

After all of them, **`make check-policy` passes in full**, including
`check-workflow-boundaries` (*"8 workflow policy module(s) are pure and
mutation-scoped, every migrated matrix row names complete, existing roles, and the
programme record's slot ledger agrees with the matrix"*),
`check-automation-docs`, `check-accessibility-policy` (10,901 added UI-sensitive
lines checked, with the source fingerprint now matching the current tree),
`check-visual-proof-policy` (*"summary matches current visual-sensitive diff;
summary pixel-verified required visual invariant ids"*),
`check-filesystem-boundary`, `check-gtk-lush-policy`, and the automation client
self-test. The CI-only rustdoc gate
(`-D rustdoc::private_intra_doc_links` and siblings) also passes — checked
explicitly, because a new `pub` facade naming its own private coordination
modules is exactly the shape that gate catches and slot 3a shipped that failure
once.

## Flake discipline

Zero `FLAKY:` lines required, and no retry relied upon: the suite is run with
`./scripts/run-widget-tests.sh --headless` and **no** `--retries`, so the
suite-level net is off and only the harness's own per-test single retry remains —
which reports any recovery loudly as `ok (FLAKY: passed on attempt N)`.

3b's causation lesson was carried in and did not need to be applied: this change
adds three tests to the two heaviest widget modules, but all three use the
**shared** helpers from `tests/widget/common.rs` (`present_window`, `wait_until`,
`flush_after_delay`) with no private copy, and every async wait —
load-installation activity, cancelled-clear settle, draft persistence, replacement
terminal — is budgeted at 10 or 20 seconds rather than a tight synchronous-flip
budget. One predicate was corrected during development for a real reason worth
recording: the first version of the data-safety regression typed its keystroke
before the cancelled-clear terminal, while `load_projection_suspended()` was still
true and buffer signals were suppressed, so the edit could not mark the tab
draft-dirty at all. That was a wrong predicate, not a tight budget, and it failed
deterministically rather than flakily.
