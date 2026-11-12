#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the Kani proof harnesses in named shards, and check the shard table.

Every `#[kani::proof]` harness belongs to exactly one shard. `make kani` runs
every shard in turn; the scheduled CI lane runs one shard per matrix job, so
each job stays inside the repository's 30-minute job cap even though the whole
lane takes longer than that. `check` (part of `make check-policy`, no Kani
needed) fails when a harness anywhere under `crates/` matches no shard or more
than one, so a new harness can never silently drop out of CI.

This table is the one source of truth for the shards, and the Makefile's
`KANI_VERSION` for the pinned Kani: `github-outputs` hands both to
`.github/workflows/kani.yml`, which builds its matrix from them.

Usage:
  scripts/kani-shards.py list
  scripts/kani-shards.py oracle <module> [--tier ci|all]
  scripts/kani-shards.py check [--self-test]
  scripts/kani-shards.py github-outputs
  scripts/kani-shards.py run all|<shard> [--target-dir DIR] [--measure JSON] [--self-test]

`run --measure JSON` is the measurement mode. For each shard it records the
wall time, the peak resident memory of the largest descendant process (CBMC,
from the kernel's per-child rusage), the exit status, and every harness's own
`Verification Time:`. It writes the figures to JSON and, when
`GITHUB_STEP_SUMMARY` is set, appends them to the job summary as a table.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, replace
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CRATES = REPO_ROOT / "crates"
MAKEFILE = REPO_ROOT / "Makefile"
KANI_VERSION_RE = re.compile(r"^KANI_VERSION \?= (\S+)$", re.M)

# The crates that own checked code, and each crate's source root. A harness's
# Kani name is its module path below the crate root plus its function name.
PACKAGES = {
    "gtk-lush-widgets": REPO_ROOT / "crates/gtk-lush/widgets/src",
    "lushtext-core": REPO_ROOT / "crates/lushtext-core/src",
}

# Budget margins, enforced by `check` against each shard's recorded runner
# measurement. The job cap is 30 minutes and a public `ubuntu-latest` runner has
# 16 GB: 25 minutes leaves room for runner variance and a cold Kani install,
# 12 GiB for the runner agent, the container, and kernel memory, and a
# pull-request shard must stay under 15 minutes so it never becomes the slowest
# required check. A shard over a margin is split first (design D3 of
# openspec/changes/harden-kani-lane-and-draft-token): split, then tune the
# solver, and only as a last resort reduce a proof bound.
MAX_SHARD_MINUTES = 25.0
# A `measured_in` that marks a developer-machine figure, and the owner such a
# record must name (`pending_problem`).
OPENSPEC_CHANGES = REPO_ROOT / "openspec/changes"
PENDING_MARK_RE = re.compile(r"PENDING DISPATCH|provisional", re.I)
PENDING_OWNER_RE = re.compile(r"PENDING DISPATCH \(owed by ([a-z0-9][a-z0-9-]*)\)")
MAX_SHARD_PEAK_GIB = 12.0
MAX_PULL_REQUEST_SHARD_MINUTES = 15.0
GATES = ("pull-request", "scheduled")


@dataclass(frozen=True)
class Shard:
    """One CI job's worth of harnesses.

    `filters` are matched by Kani as substrings of the fully qualified harness
    name, as `check` does. `gate` is `pull-request` (runs on pull requests and
    pushes to main as well as on schedule and dispatch) or `scheduled` (schedule
    and dispatch only). `ci_minutes` and `ci_peak_gib` are the shard's measured
    wall time and peak resident memory on the CI runner, the larger of the
    measured runs, and `measured_in` names the runs they came from. Timing from
    a developer machine does not count as a measurement.

    `kani_flags` are extra `cargo kani` arguments for this shard only, the one
    place an experimental Kani feature (for example `-Z loop-contracts`) is
    enabled: `run` appends them to this shard's command and no other, and
    `github-outputs` exports them per shard so the workflow can show them.
    """

    package: str
    filters: tuple[str, ...]
    gate: str
    ci_minutes: float | None
    ci_peak_gib: float | None
    measured_in: str
    kani_flags: tuple[str, ...] = ()

    def owns(self, package: str, harness: str) -> bool:
        return self.package == package and any(f in harness for f in self.filters)


# The runs every budget below comes from: five `workflow_dispatch` runs of
# kani.yml on `ubuntu-latest` (Fedora 44 container), cold and warm Kani install
# cache, with each shard recording the largest wall time and peak memory seen
# across them, rounded up. Per-run figures and local CBMC times (Kani 0.68.0,
# one toolbox) are in docs/next/formal-verification.md, phase 2.
MEASURED_IN = "runs 35920992670, 35923346671, 35925626107, 35927734943, 35930757261 (max of the five)"
# The two pure-policy shards (extend-kani-to-pure-policies) come from four
# dispatched runs: 36052583128 and 36054671084 on the first form of the change,
# and 36059814296 and 36061831286 on the final, whole-pixel form. The memory
# harnesses did not change between the forms, so all four count for
# core-memory-policy; core-geometry-policies uses the last two, because the
# whole-pixel rewrite changed its harnesses (the first two took 15.2 and 15.7
# minutes). Runs 36059814296 and 36061831286 also re-measured every older
# shard; where they exceeded its recorded figure (core-journal-and-write 21.1,
# core-second-writer 17.4) the figure was raised to the larger value.
POLICY_MEASURED_IN = "runs 36052583128, 36054671084, 36059814296, 36061831286 (max of the four)"
# (core-geometry-policies has been recorded from GEOMETRY_SPLIT_MEASURED_IN since
# its 2026-11-07 re-split.)
RESAMPLED_IN = (
    "runs 35920992670, 35923346671, 35925626107, 35927734943, 35930757261, "
    "36059814296, 36061831286 (max of the seven)"
)
# The shards extend-closed-loop-geometry-verification added or re-split come from
# two dispatched runs on the final form of the change (max of the two, rounded
# up); both runs also re-measured every older shard inside its recorded figure.
CLOSED_LOOP_MEASURED_IN = "runs 36136602024, 36139028016 (max of the two)"
# The shards verify-multi-window-draft-journal added or re-modelled come from
# two dispatched runs on the final form of the change (max of the two,
# rounded up).
MULTI_WINDOW_MEASURED_IN = "runs 36172512752, 36180356173 (max of the two)"
# measure-proof-strength-with-mutation changed the harnesses of
# widgets-geometry and core-journal-and-write (and only added assertions to
# core-memory-policy), so every shard was re-measured by two dispatched runs.
# A shard keeps the larger of its earlier figure and those two runs (rounded
# up); the ones that rose carry this suffix. core-memory-policy measured
# 9.5 min / 3.9 GiB, inside its recorded figure.
# probe-long-line-toolkit-costs added the responsiveness-budget harnesses to
# core-second-writer; two dispatched runs re-measured it at 6.44 and 6.42 min,
# 2.68 GiB each, inside the recorded figure, which therefore stands.
RESPONSIVENESS_BUDGET_REMEASURED_IN = (
    "; re-measured with the responsiveness-budget harnesses by runs 38181384069, "
    "38182511805 (6.44 min / 2.68 GiB max, inside the figure)"
)
PROOF_STRENGTH_RAISED_IN = "; raised by runs 38087161968, 38088387193 (max of the two)"
# document-scale-policy (C1) added the paragraph-boundary, profile, edit-watch
# and document-scale shards; two dispatched runs (max of the two, rounded up)
# measure them. core-document-scale's runs predate the join rewrite of
# c2e33d74 (same harnesses, 5.6 local minutes before and after).
DOCUMENT_SCALE_MEASURED_IN = "runs 39440638502, 39446126819 (max of the two)"
# close-protected-install-proof-strength replaced every developer-machine
# figure with two dispatched kani.yml runs on its final form (max of the two,
# rounded up to 0.1): the shards C6 (core-post-open-settle), C2
# (core-protected-fold, -fold-shape, -fold-mapping, core-protected-view,
# -view-navigation) and C3 (core-protected-install) added, the settle-evidence
# harnesses spawn-completion-priority added to widgets-slice-loop-requests, and
# its own two step shards. Both runs re-measured every other shard as well; a
# figure they exceeded is raised (`CLOSE_PROTECTED_RAISED_IN`), and none is
# lowered. Per-run tables: docs/next/formal-verification.md, "Sliced protected
# install".
CLOSE_PROTECTED_MEASURED_IN = "runs 42169248534, 42172535348 (max of the two)"
CLOSE_PROTECTED_RAISED_IN = "; raised by runs 42169248534, 42172535348 (max of the two)"
# The maximality assertion added after proof strength changed both step shards'
# harnesses, so two more sequential dispatches measured the final form at
# 83507a3b: the step shards take the largest of all four runs, and every other
# shard those two runs exceeded is raised again (none lowered).
CLOSE_PROTECTED_STEP_MEASURED_IN = (
    "runs 42169248534, 42172535348, 42281883363, 42291228680 (max of the four; "
    "the last two on the final harnesses)"
)
CLOSE_PROTECTED_FINAL_RAISED_IN = "; raised by runs 42281883363, 42291228680 (max of the two)"
# bound-lists-by-budget-not-count's two CI pull-request runs on PR #87:
# 42322273437 (2.76 min, 1.63 GiB) and 42331071390 (2.53 min, 1.63 GiB);
# the larger of the two, rounded up (the local make kani run took 1.03 min).
# drill-down-palette-categories added B5 (three harnesses, 3.0 s of local
# verification against the shard's 1.5 s before; the local run took 0.16 min
# with the build cached, 2026-11-07). PR #89's pull-request run 42372071326
# measured the nine harnesses at 1.83 min and 1.63 GiB, inside the record,
# which stands (raise, never lower).
LIST_BUDGET_MINUTES = 2.8
LIST_BUDGET_PEAK_GIB = 1.7
LIST_BUDGET_MEASURED_IN = (
    "runs 42322273437, 42331071390 (pull_request, max of the two); with the B5 "
    "rows_within harnesses by run 42372071326 (1.83 min / 1.63 GiB, inside the figures)"
)
# core-geometry-policies broke the pull-request margin on PR #87's run
# 42331071390 (16.57 min, unchanged harnesses; it had also taken 15.87 in
# 41995267728). Eleven runs of the same 37 harnesses spread 9.21-16.57 min,
# so this is the runner's ordinary variance, not an outlier a second run could
# dismiss: the larger figure stands, and the shard was re-split (D3: split
# first) instead of leaving the pull-request gate. The required check keeps its
# name and every harness except the three widest minimap fits; those three
# (about 9 of the 12.8 CBMC minutes) form core-minimap-fit, still gated
# pull-request so a minimap-policy regression is caught before merge. The
# split figures are derived from the per-harness `Verification Time:` lines of
# the two slowest runs plus each run's whole non-verification time (build,
# CBMC start-up), counted once per shard: 12.88 / 12.37 min and 7.50 / 7.04 min,
# max of the two, rounded up; peak memory is the unsplit shard's (2.36 GiB at
# most). Per-run detail: docs/next/formal-verification.md, "Geometry policy
# shard re-split".
# Measured on the split form by CI runs 42333476867 (push of e4ec2624) and
# 42352722531 (pull request #88): core-geometry-policies 6.63 / 5.70 min,
# 2.32 / 2.38 GiB; core-minimap-fit 8.46 / 8.42 min, 2.17 / 2.17 GiB. Both
# shards measured inside their derived figures, which stand (raise, never
# lower): the derived figures come from the slowest runner seen on these
# harnesses (16.57 min unsplit), and two runs cannot rule that runner out.
GEOMETRY_SPLIT_MEASURED_IN = (
    "derived from runs 42331071390, 41995267728 (per-harness verification times plus "
    "each run's whole build and start-up time; max of the two); measured on the split "
    "form by runs 42333476867, 42352722531 (6.63 min / 2.38 GiB and 8.46 min / "
    "2.17 GiB max, inside the figures)"
)
# core-minimap-binning (bin-minimap-markers-per-row) has no CI run yet. Its
# figures are derived the way the geometry split's were: the local run of
# `make kani KANI_SHARD=core-minimap-binning` (2026-11-07: 6.31 min with its
# build, peak 11.64 GiB, 11 harnesses)
# scaled by the largest CI/local ratio of the minimap fit harnesses (the
# core-minimap-fit shard, 8.46 CI minutes for 5.3 local minutes of verification
# plus its build, about 1.6), plus a CI build; the peak is the local peak.
# Re-measure from the first dispatched run and raise, never lower.
MINIMAP_BINNING_CI_MINUTES = 12.0
MINIMAP_BINNING_CI_PEAK_GIB = 12.0
MINIMAP_BINNING_MEASURED_IN = (
    "derived (no CI run yet): local per-harness verification times of 2026-11-07 "
    "scaled by core-minimap-fit's CI/local ratio of about 1.6, plus a CI build; "
    "peak from the local run"
)
# core-minimap-fit became a required check on main (ruleset 23911547). PR #91's
# run 43105455931 and PR #92's run 43144457562 measured it at 10.03 and
# 10.93 min (2.19 / 2.32 GiB), inside its 12.9-min figure, which stands
# (raise, never lower), so no re-split. PR #92's job took 16m39s only because
# its `Install system dependencies` step (dnf) took 5m03s against 38 s in the
# same run's core-geometry-policies job: a slow mirror, outside the shard, that
# a split would not shorten. docs/next/formal-verification.md, "core-minimap-fit
# as a required check".
MINIMAP_FIT_MEASURED_IN = GEOMETRY_SPLIT_MEASURED_IN + (
    "; as a required check by runs 43105455931, 43144457562 (10.03 / 10.93 min, "
    "2.19 / 2.32 GiB, inside the figures)"
)
SETTLE_EVIDENCE_MEASURED_IN = (
    "; with the settle-evidence harnesses by runs 42169248534, 42172535348 (max of the two)"
)
SHARDS: dict[str, Shard] = {
    "widgets-geometry": Shard(
        "gtk-lush-widgets",
        (
            "kani_proofs::no_input_panics",
            "kani_proofs::whole_pixel_band_",
            "kani_proofs::slice_lies_",
            "kani_proofs::slice_covers_",
            "kani_proofs::slice_containment_",
            "kani_proofs::resting_bin_",
            "kani_proofs::requests_are_",
            "kani_proofs::request_lands_",
            "kani_proofs::request_landing_",
        ),
        gate="pull-request",
        ci_minutes=9.2,
        ci_peak_gib=1.8,
        measured_in=MEASURED_IN + PROOF_STRENGTH_RAISED_IN + CLOSE_PROTECTED_RAISED_IN,
    ),
    "widgets-slice-loop-rest": Shard(
        "gtk-lush-widgets",
        (
            "kani_proofs::slice_loop_rests_with_one_bin",
            "kani_proofs::slice_loop_rests_with_two_bins",
        ),
        gate="scheduled",
        ci_minutes=11.8,
        ci_peak_gib=1.7,
        measured_in=CLOSED_LOOP_MEASURED_IN,
    ),
    # The anchor model of extend-closed-loop-geometry-verification made the
    # three-bin rest and two-bin request harnesses too slow to share a job.
    "widgets-slice-loop-rest-three": Shard(
        "gtk-lush-widgets",
        ("kani_proofs::slice_loop_rests_with_three_bins",),
        gate="scheduled",
        ci_minutes=15.3,
        ci_peak_gib=1.7,
        measured_in=CLOSED_LOOP_MEASURED_IN + CLOSE_PROTECTED_FINAL_RAISED_IN,
    ),
    "widgets-slice-loop-requests": Shard(
        "gtk-lush-widgets",
        (
            "kani_proofs::slice_loop_honours_a_request_with_one_bin",
            "kani_proofs::slice_loop_learning_frame_",
            "kani_proofs::slice_loop_one_reconfiguring_",
            "kani_proofs::slice_loop_two_reconfiguring_",
            "kani_proofs::slice_loop_settle_evidence_",
        ),
        gate="scheduled",
        ci_minutes=14.0,
        ci_peak_gib=1.7,
        measured_in=CLOSED_LOOP_MEASURED_IN + PROOF_STRENGTH_RAISED_IN + SETTLE_EVIDENCE_MEASURED_IN + CLOSE_PROTECTED_FINAL_RAISED_IN,
    ),
    "widgets-slice-loop-requests-two": Shard(
        "gtk-lush-widgets",
        ("kani_proofs::slice_loop_honours_a_request_with_two_bins",),
        gate="scheduled",
        ci_minutes=15.3,
        ci_peak_gib=1.7,
        measured_in=CLOSED_LOOP_MEASURED_IN + PROOF_STRENGTH_RAISED_IN + CLOSE_PROTECTED_FINAL_RAISED_IN,
    ),
    "widgets-slice-loop-pairs": Shard(
        "gtk-lush-widgets",
        (
            "kani_proofs::slice_loop_two_simultaneous_",
            "kani_proofs::slice_bin_allocation_",
            "kani_proofs::slice_bin_decision_",
        ),
        gate="scheduled",
        ci_minutes=16.1,
        ci_peak_gib=2.0,
        measured_in=CLOSED_LOOP_MEASURED_IN + PROOF_STRENGTH_RAISED_IN,
    ),
    "widgets-slice-loop-unbounded": Shard(
        "gtk-lush-widgets",
        (
            "kani_proofs::slice_bin_rests_",
            "kani_proofs::slice_bin_honours_",
            "kani_proofs::slice_loop_viewport_resize_",
            "kani_proofs::slice_loop_forwarding_a_published_",
            "kani_proofs::slice_loop_request_coinciding_",
        ),
        gate="scheduled",
        ci_minutes=14.4,
        ci_peak_gib=1.7,
        measured_in=CLOSED_LOOP_MEASURED_IN,
    ),
    "core-journal-and-write": Shard(
        "lushtext-core",
        (
            "services::draft_service::kani_proofs::journal_invariants_hold_under_crashes",
            "services::draft_service::kani_proofs::journal_set_aside_",
            "services::draft_service::kani_proofs::journal_decisions_",
            "services::draft_service::set_aside_retention::kani_proofs::",
            "services::filesystem::write_protocol::kani_proofs::",
        ),
        gate="scheduled",
        ci_minutes=12.0,
        ci_peak_gib=2.6,
        measured_in=MULTI_WINDOW_MEASURED_IN + PROOF_STRENGTH_RAISED_IN,
    ),
    # verify-multi-window-draft-journal added the journal lane to the model,
    # which made the two L1 harnesses too slow to share the journal shard.
    "core-journal-liveness": Shard(
        "lushtext-core",
        ("services::draft_service::kani_proofs::a_dirty_editor_",),
        gate="scheduled",
        ci_minutes=15.2,
        ci_peak_gib=4.8,
        measured_in=MULTI_WINDOW_MEASURED_IN + PROOF_STRENGTH_RAISED_IN,
    ),
    # probe-long-line-toolkit-costs added the responsiveness-budget arithmetic
    # here: seconds of CBMC, too small for a shard of its own.
    "core-second-writer": Shard(
        "lushtext-core",
        (
            "services::draft_service::kani_proofs::a_second_writer_",
            "model::responsiveness_budget::kani_proofs::",
            "model::settings_write_batch::kani_proofs::",
        ),
        gate="scheduled",
        ci_minutes=7.4,
        ci_peak_gib=2.7,
        measured_in=MULTI_WINDOW_MEASURED_IN + RESPONSIVENESS_BUDGET_REMEASURED_IN + CLOSE_PROTECTED_RAISED_IN + CLOSE_PROTECTED_FINAL_RAISED_IN,
    ),
    # verify-multi-window-draft-journal: two windows of one process over the
    # journal machine, too large to share a job with the single-window harness.
    "core-multi-window": Shard(
        "lushtext-core",
        ("services::draft_service::kani_proofs::journal_invariants_hold_across_two_windows",),
        gate="scheduled",
        ci_minutes=8.9,
        ci_peak_gib=2.7,
        measured_in=MULTI_WINDOW_MEASURED_IN + PROOF_STRENGTH_RAISED_IN + CLOSE_PROTECTED_FINAL_RAISED_IN,
    ),
    "core-memory-policy": Shard(
        "lushtext-core",
        ("model::editor_memory::kani_proofs::",),
        gate="scheduled",
        ci_minutes=16.2,
        ci_peak_gib=9.0,
        measured_in=POLICY_MEASURED_IN,
    ),
    # A required check on main (ruleset 23911547), so the name stays with the
    # larger half when the shard is split. Every minimap fit harness that is not
    # one of core-minimap-fit's three is named here by its prefix; the marker
    # row binning harnesses have their own shard, core-minimap-binning.
    "core-geometry-policies": Shard(
        "lushtext-core",
        (
            "ui::editor_page::minimap::policy::kani_proofs::native_slider_containment_",
            "ui::editor_page::minimap::policy::kani_proofs::native_slider_estimate_",
            "ui::editor_page::minimap::policy::kani_proofs::native_slider_fit_",
            "ui::editor_page::minimap::policy::kani_proofs::projected_",
            "ui::editor_page::minimap::policy::kani_proofs::skipped_line_",
            "ui::window::geometry::policy::kani_proofs::",
            "ui::sidebar::width_preset::kani_proofs::",
            "ui::markdown_preview::policy::kani_proofs::",
        ),
        gate="pull-request",
        ci_minutes=7.5,
        ci_peak_gib=2.4,
        measured_in=GEOMETRY_SPLIT_MEASURED_IN,
    ),
    # Split from core-geometry-policies (GEOMETRY_SPLIT_MEASURED_IN): the
    # minimum-height expansion and the whole-pixel native-slider containment,
    # the three widest minimap fits.
    # Also a required check on main (ruleset 23911547): keep the name.
    "core-minimap-fit": Shard(
        "lushtext-core",
        (
            "ui::editor_page::minimap::policy::kani_proofs::min_height_expansion_",
            "ui::editor_page::minimap::policy::kani_proofs::native_slider_stays_",
        ),
        gate="pull-request",
        ci_minutes=12.9,
        ci_peak_gib=2.4,
        measured_in=MINIMAP_FIT_MEASURED_IN,
    ),
    # bin-minimap-markers-per-row D8: the integer marker row binning (row
    # groups, the per-kind index walker, the run builder, its growth and merge),
    # which replaced fit_marker_bounds and its three core-geometry-policies
    # harnesses. Split off rather than added to core-geometry-policies, which
    # would then exceed the pull-request margin (D3: split first). Scheduled
    # until a dispatched CI run measures it; the figures are derived (below).
    "core-minimap-binning": Shard(
        "lushtext-core",
        ("ui::editor_page::minimap::policy::kani_proofs::marker_rows_",),
        gate="scheduled",
        ci_minutes=MINIMAP_BINNING_CI_MINUTES,
        ci_peak_gib=MINIMAP_BINNING_CI_PEAK_GIB,
        measured_in=MINIMAP_BINNING_MEASURED_IN,
    ),
    "core-post-open-settle": Shard(
        "lushtext-core",
        (
            "model::buffer_replacement::kani_proofs::",
            "model::highlight_chunking::kani_proofs::",
            "ui::editor_page::load::policy::kani_proofs::",
        ),
        gate="scheduled",
        ci_minutes=17.2,
        ci_peak_gib=1.8,
        measured_in=CLOSE_PROTECTED_MEASURED_IN + CLOSE_PROTECTED_FINAL_RAISED_IN,
    ),
    # bound-lists-by-budget-not-count: the list budget both workspace
    # workflows rely on (admission B1-B4, turn continuation T1-T2), so it
    # gates pull requests.
    "core-list-budget": Shard(
        "lushtext-core",
        ("model::list_budget::kani_proofs::",),
        gate="pull-request",
        ci_minutes=LIST_BUDGET_MINUTES,
        ci_peak_gib=LIST_BUDGET_PEAK_GIB,
        measured_in=LIST_BUDGET_MEASURED_IN,
    ),
    "core-document-scale": Shard(
        "lushtext-core",
        (
            "model::document_scale::kani_proofs::",
            "model::reanalysis_pacing::kani_proofs::",
            "model::paragraph_boundary::kani_proofs::boundary_newline_",
            "model::paragraph_boundary::kani_proofs::boundary_line_length_",
            "model::paragraph_boundary::kani_proofs::boundary_break_",
        ),
        gate="scheduled",
        ci_minutes=10.2,
        ci_peak_gib=2.7,
        measured_in=DOCUMENT_SCALE_MEASURED_IN + CLOSE_PROTECTED_RAISED_IN,
    ),
    # Split by the fit order: the two slice-choice harnesses and the
    # delimiter-rule agreement take about 13 of the boundary group's 18 local
    # minutes (symbolic `from_utf8` over 8 bytes).
    "core-paragraph-boundary": Shard(
        "lushtext-core",
        (
            "model::paragraph_boundary::kani_proofs::boundary_agrees_",
            "model::paragraph_boundary::kani_proofs::boundary_slice_end_",
        ),
        gate="scheduled",
        ci_minutes=21.9,
        ci_peak_gib=3.8,
        measured_in=DOCUMENT_SCALE_MEASURED_IN,
    ),
    "core-paragraph-profile": Shard(
        "lushtext-core",
        (
            "model::paragraph_boundary::kani_proofs::profile_",
            "model::paragraph_profile_watch::kani_proofs::watch_delete_",
        ),
        gate="scheduled",
        ci_minutes=14.7,
        ci_peak_gib=8.3,
        measured_in=DOCUMENT_SCALE_MEASURED_IN,
    ),
    # Split by the fit order: each inductive watch step takes 11-13 local
    # minutes and about 10 GiB (CaDiCaL), so together they measured 38 local
    # minutes, beyond the 25-minute margin; one harness per shard.
    "core-profile-watch-step": Shard(
        "lushtext-core",
        ("model::paragraph_profile_watch::kani_proofs::watch_step_",),
        gate="scheduled",
        ci_minutes=19.4,
        ci_peak_gib=10.4,
        measured_in=DOCUMENT_SCALE_MEASURED_IN,
    ),
    "core-profile-watch-reads": Shard(
        "lushtext-core",
        ("model::paragraph_profile_watch::kani_proofs::watch_reads_",),
        gate="scheduled",
        ci_minutes=18.4,
        ci_peak_gib=10.4,
        measured_in=DOCUMENT_SCALE_MEASURED_IN,
    ),
    "core-profile-watch-unsound": Shard(
        "lushtext-core",
        ("model::paragraph_profile_watch::kani_proofs::watch_fast_path_",),
        gate="scheduled",
        ci_minutes=19.5,
        ci_peak_gib=10.4,
        measured_in=DOCUMENT_SCALE_MEASURED_IN,
    ),
    "core-shell-geometry": Shard(
        "lushtext-core",
        ("ui::window::geometry::kani_proofs::shell_loop_",),
        gate="scheduled",
        ci_minutes=10.9,
        ci_peak_gib=2.6,
        measured_in=CLOSED_LOOP_MEASURED_IN,
    ),    # Split by design D12's rule (the whole group is over the 15-minute
    # pull-request margin): the density bound (H1, H1w, H1c, H1x) gates pull
    # requests, the fold shape, UTF-8, column and slicing harnesses are scheduled.
    "core-protected-fold": Shard(
        "lushtext-core",
        (
            "model::protected_fold::kani_proofs::density_bound_",
            "model::protected_fold::kani_proofs::an_over_budget_window_",
            "model::protected_fold::kani_proofs::production_thresholds_",
        ),
        gate="pull-request",
        ci_minutes=12.4,
        ci_peak_gib=1.7,
        measured_in=CLOSE_PROTECTED_MEASURED_IN + CLOSE_PROTECTED_FINAL_RAISED_IN,
    ),
    # The scheduled rest is split again: the fold-shape harnesses alone took
    # 12 of the 17.3 local minutes the combined group did.
    "core-protected-install": Shard(
        "lushtext-core",
        # Its four harnesses by name: the module path alone would also own the
        # step harnesses of core-protected-install-step.
        (
            "model::protected_install::kani_proofs::protected_clear_plan_",
            "model::protected_install::kani_proofs::settle_budget_derivation_",
            "model::protected_install::kani_proofs::unrounded_tail_end_",
            "model::protected_install::kani_proofs::one_paragraph_fold_",
        ),
        gate="scheduled",
        ci_minutes=8.7,
        ci_peak_gib=8.0,
        measured_in=CLOSE_PROTECTED_MEASURED_IN,
    ),
    # The one-step induction over the planner (K1-K6, K8, K9), apart from
    # core-protected-install so neither shard's figure hides the other's, and
    # split by the fit order: the five harnesses took about 16 local minutes
    # together, over the 15-minute local budget; the visible step alone takes
    # about 6.
    "core-protected-install-step": Shard(
        "lushtext-core",
        (
            "model::protected_install::kani_proofs::step_initial_",
            "model::protected_install::kani_proofs::step_head_",
            "model::protected_install::kani_proofs::step_hidden_tail_",
        ),
        gate="scheduled",
        ci_minutes=17.9,
        ci_peak_gib=4.5,
        measured_in=CLOSE_PROTECTED_STEP_MEASURED_IN,
    ),
    "core-protected-install-step-visible": Shard(
        "lushtext-core",
        ("model::protected_install::kani_proofs::step_visible_",),
        gate="scheduled",
        ci_minutes=11.9,
        ci_peak_gib=4.6,
        measured_in=CLOSE_PROTECTED_STEP_MEASURED_IN,
    ),
    "core-protected-fold-shape": Shard(
        "lushtext-core",
        ("model::protected_fold::kani_proofs::folds_hide_",),
        gate="scheduled",
        ci_minutes=22.8,
        ci_peak_gib=1.7,
        measured_in=CLOSE_PROTECTED_MEASURED_IN,
    ),
    "core-protected-fold-mapping": Shard(
        "lushtext-core",
        (
            "model::protected_fold::kani_proofs::fold_byte_offset_",
            "model::protected_fold::kani_proofs::fold_location_",
            "model::protected_fold::kani_proofs::fold_plan_",
        ),
        gate="scheduled",
        ci_minutes=11.7,
        ci_peak_gib=1.7,
        measured_in=CLOSE_PROTECTED_MEASURED_IN,
    ),
    "core-protected-view": Shard(
        "lushtext-core",
        (
            "ui::editor_page::protected_view::policy::kani_proofs::no_mutating_",
            "ui::editor_page::protected_view::policy::kani_proofs::protected_view_entry_",
            "ui::editor_page::protected_view::policy::kani_proofs::entry_",
            "ui::editor_page::protected_view::policy::kani_proofs::find_batch",
        ),
        gate="scheduled",
        ci_minutes=3.6,
        ci_peak_gib=3.4,
        measured_in=CLOSE_PROTECTED_MEASURED_IN,
    ),
    "core-protected-view-navigation": Shard(
        "lushtext-core",
        ("ui::editor_page::protected_view::policy::kani_proofs::navigation_target_",),
        gate="scheduled",
        ci_minutes=2.6,
        ci_peak_gib=1.7,
        measured_in=CLOSE_PROTECTED_MEASURED_IN,
    ),
}


# The Kani mutation oracle (`scripts/proof-strength.py`, openspec/changes/
# archive/2026-10-10-measure-proof-strength-with-mutation). Each Kani-checked
# module maps to the ordered harnesses that check it: `proof-strength.py`
# applies one mutant of the module at a time and runs these harnesses cheapest
# first, stopping at the first that fails. `seconds` is the harness's measured
# local verification time on the unmutated tree (Kani 0.68.0 / CBMC 6.11.0,
# `proof-strength.py baseline`), which orders the list and sets the per-harness
# timeout. `tier` is `ci` for a harness cheap enough that a module's whole ci
# list fits a CI shard per mutant, `local` for the expensive closed-loop and
# journal harnesses. `functions`, when set, scopes the oracle to the functions
# the harnesses were written for (the rest of a large mixed module is counted as
# outside the oracle, not as survivors); an empty tuple means the whole module.
# `check` keeps this table in step with the harnesses: every oracle harness
# exists, every harness is an oracle or is named in NOT_ORACLE_HARNESSES, and
# every module a harness file imports from is an oracle or in NOT_ORACLE_MODULES.
TIERS = ("ci", "local")


@dataclass(frozen=True)
class OracleHarness:
    name: str
    seconds: float
    tier: str


@dataclass(frozen=True)
class Oracle:
    package: str
    harnesses: tuple[OracleHarness, ...]
    functions: tuple[str, ...] = ()

    def ordered(self, tier: str = "all") -> list[OracleHarness]:
        chosen = [h for h in self.harnesses if tier == "all" or h.tier == tier]
        return sorted(chosen, key=lambda h: h.seconds)


def _h(name: str, seconds: float, tier: str) -> OracleHarness:
    return OracleHarness(name, seconds, tier)


_WIDGETS = "gtk-lush-widgets"
_CORE = "lushtext-core"
_W = "kani_proofs::"
_SLICE_LOOP = (
    _h(_W + "slice_loop_rests_with_one_bin", 86.57, "local"),
    _h(_W + "slice_loop_rests_with_two_bins", 455.1, "local"),
    _h(_W + "slice_loop_rests_with_three_bins", 605.99, "local"),
    _h(_W + "slice_loop_honours_a_request_with_one_bin", 191.56, "local"),
    _h(_W + "slice_loop_honours_a_request_with_two_bins", 642.97, "local"),
    _h(_W + "slice_loop_learning_frame_request_beyond_the_inset_is_honoured", 55.55, "local"),
    _h(_W + "slice_loop_learning_frame_request_within_the_inset_is_erased", 22.82, "local"),
    _h(_W + "slice_loop_one_reconfiguring_allocation_leaves_the_outer_alone", 20.97, "local"),
    _h(_W + "slice_loop_two_reconfiguring_allocations_can_move_the_outer", 40.16, "local"),
    _h(_W + "slice_loop_two_simultaneous_requests_do_not_add_up", 539.45, "local"),
    _h(_W + "slice_loop_viewport_resize_leaves_the_outer_alone", 74.74, "local"),
    _h(_W + "slice_loop_forwarding_a_published_anchor_settle_moves_the_outer", 95.6, "local"),
    _h(_W + "slice_loop_request_coinciding_with_a_resize_is_erased", 57.38, "local"),
    _h(_W + "slice_bin_allocation_touches_only_its_own_bin", 43.54, "local"),
    _h(_W + "slice_bin_decision_depends_only_on_its_own_inputs", 119.15, "local"),
    _h(_W + "slice_bin_rests_from_any_origin", 48.02, "local"),
    _h(_W + "slice_bin_rests_after_any_outer_jump", 80.36, "local"),
    _h(_W + "slice_bin_honours_a_request_from_any_origin", 341.54, "local"),
    _h(_W + "slice_loop_settle_evidence_waits_for_the_reslice", 190.38, "local"),
    _h(_W + "slice_loop_settle_evidence_without_the_reslice_is_stale", 88.4, "local"),
)
_JOURNAL = "services::draft_service::kani_proofs::"
_RETENTION = "services::draft_service::set_aside_retention::kani_proofs::"
_WRITE = "services::filesystem::write_protocol::kani_proofs::"
_MEMORY = "model::editor_memory::kani_proofs::"
_SHELL = "ui::window::geometry::policy::kani_proofs::"
_SHELL_LOOP = "ui::window::geometry::kani_proofs::"
_PRESET = "ui::sidebar::width_preset::kani_proofs::"
_MINIMAP = "ui::editor_page::minimap::policy::kani_proofs::"
_PREVIEW = "ui::markdown_preview::policy::kani_proofs::"
_BUDGET = "model::responsiveness_budget::kani_proofs::"
_WRITE_BATCH = "model::settings_write_batch::kani_proofs::"
# document-scale-policy local seconds: the verification times of `make kani` on
# this workstation (Kani 0.68.0, 2026-10-20), cheapest first per module.
PROFILE_CARRY_SECONDS = 13.64
PROFILE_SPEC_SECONDS = 30.19
PROFILE_SLICING_SECONDS = 132.52
WATCH_DELETE_SECONDS = 1.0
WATCH_READS_SECONDS = 651.83
WATCH_STEP_SECONDS = 672.86
WATCH_UNSOUND_SECONDS = 778.55
_LIST_BUDGET = "model::list_budget::kani_proofs::"
_SCALE = "model::document_scale::kani_proofs::"
_PACED = "model::buffer_replacement::kani_proofs::"
_REANALYSIS = "model::reanalysis_pacing::kani_proofs::"
_CHUNKED = "model::highlight_chunking::kani_proofs::"
_SETTLE = "ui::editor_page::load::policy::kani_proofs::"
_BOUNDARY = "model::paragraph_boundary::kani_proofs::"
_WATCH = "model::paragraph_profile_watch::kani_proofs::"
_FOLD = "model::protected_fold::kani_proofs::"
_PV = "ui::editor_page::protected_view::policy::kani_proofs::"
_PI = "model::protected_install::kani_proofs::"
_SHELL_POLICY_HARNESSES = (
    _h(_SHELL + "focus_mode_renders_no_secondary_surface", 0.11, "ci"),
    _h(_SHELL + "layout_never_renders_an_unrequested_surface", 0.2, "ci"),
    _h(_SHELL + "compact_layout_renders_at_most_one_surface", 0.18, "ci"),
    _h(_SHELL + "wide_layout_renders_every_requested_surface", 0.25, "ci"),
    _h(_SHELL + "sheet_presentation_matches_the_breakpoint", 0.23, "ci"),
    _h(_SHELL + "breakpoint_is_monotone_and_bounded", 0.14, "ci"),
    _h(_SHELL + "pane_shares_are_positive", 0.56, "ci"),
    _h(_SHELL + "shell_policy_never_panics", 0.38, "ci"),
    _h(_SHELL + "split_view_restore_plan_matches_the_pre_plan_restore", 4.52, "ci"),
)
ORACLES: dict[str, Oracle] = {
    "crates/gtk-lush/widgets/src/slice_geometry.rs": Oracle(
        _WIDGETS,
        (
            _h(_W + "no_input_panics", 0.71, "ci"),
            _h(_W + "whole_pixel_band_stays_inside_the_widget", 0.65, "ci"),
            _h(_W + "slice_containment_fails_for_general_f64", 1.79, "ci"),
            _h(_W + "whole_pixel_band_keeps_a_whole_pixel_slice", 3.86, "ci"),
            _h(_W + "slice_lies_inside_content_on_whole_pixels", 55.42, "ci"),
            _h(_W + "slice_covers_the_visible_intersection_on_whole_pixels", 243.49, "ci"),
            *_SLICE_LOOP,
        ),
    ),
    "crates/gtk-lush/widgets/src/scroll_request.rs": Oracle(
        _WIDGETS,
        (
            _h(_W + "no_input_panics", 0.71, "ci"),
            _h(_W + "requests_are_at_least_epsilon", 0.11, "ci"),
            _h(_W + "request_landing_fails_for_general_f64", 0.18, "ci"),
            _h(_W + "resting_bin_never_requests", 0.47, "ci"),
            _h(_W + "request_lands_exactly_on_whole_pixels", 7.29, "ci"),
            _h(_W + "requests_are_classified_by_the_documented_rules", 53.51, "ci"),
            *_SLICE_LOOP,
        ),
    ),
    "crates/lushtext-core/src/services/draft_service/journal_core.rs": Oracle(
        _CORE,
        (
            _h(_JOURNAL + "journal_decisions_follow_their_documented_rules", 0.35, "ci"),
            _h(_JOURNAL + "journal_set_aside_keeps_every_body_it_reports_kept", 4.13, "local"),
            _h(_JOURNAL + "journal_set_aside_stamp_only_naming_loses_a_newer_body", 4.01, "local"),
            _h(_JOURNAL + "a_dirty_editor_becomes_clean_without_faults", 292.41, "local"),
            _h(_JOURNAL + "a_dirty_editor_may_need_seven_steps", 262.12, "local"),
            _h(_JOURNAL + "journal_invariants_hold_across_two_windows", 260.12, "local"),
            _h(_JOURNAL + "journal_invariants_hold_under_crashes", 309.72, "local"),
        ),
    ),
    "crates/lushtext-core/src/services/draft_service/set_aside_retention.rs": Oracle(
        _CORE,
        (
            _h(_RETENTION + "set_aside_retention_bound_and_notice_never_plan_a_deletion", 0.19, "ci"),
            _h(_RETENTION + "set_aside_retention_deleting_every_body_breaks_r2", 1.28, "ci"),
            _h(_RETENTION + "set_aside_retention_deletes_only_confirmed_bodies", 2.6, "ci"),
            _h(_RETENTION + "set_aside_retention_notice_follows_the_bound_and_its_rate_limit", 12.73, "ci"),
        ),
    ),
    "crates/lushtext-core/src/services/filesystem/write_protocol.rs": Oracle(
        _CORE,
        (
            _h(_WRITE + "a_move_removes_its_source_only_after_the_copy_is_durable", 0.07, "ci"),
            _h(_WRITE + "a_completed_rename_synced_every_directory_it_mutated", 0.08, "ci"),
            _h(_WRITE + "skipping_the_temp_sync_tears_the_destination", 3.43, "ci"),
            _h(_WRITE + "a_crash_never_tears_the_destination", 3.65, "ci"),
            _h(_WRITE + "every_classification_describes_the_destination", 5.55, "ci"),
        ),
    ),
    "crates/lushtext-core/src/model/editor_memory.rs": Oracle(
        _CORE,
        (
            _h(_MEMORY + "estimate_is_bookkeeping_when_evicted_and_floored_by_file_size_otherwise", 0.07, "local"),
            _h(_MEMORY + "ledger_crossing_flag_is_exact", 0.5, "local"),
            _h(_MEMORY + "within_budget_selects_nothing", 35.87, "local"),
            _h(_MEMORY + "budget_stops_at_the_lower_watermark", 39.39, "local"),
            _h(_MEMORY + "ledger_totals_match_a_recomputation", 40.33, "local"),
            _h(_MEMORY + "budget_never_selects_protected_or_bookkeeping_pages", 42.11, "local"),
            _h(_MEMORY + "budget_selects_least_recently_used_first", 51.83, "local"),
            _h(_MEMORY + "budget_outcome_matches_the_projected_total", 144.6, "local"),
        ),
    ),
    "crates/lushtext-core/src/ui/window/geometry/policy.rs": Oracle(
        _CORE,
        (
            *_SHELL_POLICY_HARNESSES,
            _h(_SHELL_LOOP + "shell_loop_layout_is_stable_under_its_own_compact_slot", 0.66, "local"),
            _h(_SHELL_LOOP + "shell_loop_allocation_never_persists", 11.99, "local"),
            _h(_SHELL_LOOP + "shell_loop_workspace_collapses_with_its_breakpoint", 28.2, "local"),
            _h(_SHELL_LOOP + "shell_loop_preserves_requested_visibility", 25.07, "local"),
            _h(_SHELL_LOOP + "shell_loop_layout_agrees_with_policy_at_rest", 30.66, "local"),
            _h(_SHELL_LOOP + "shell_loop_settles_at_a_stable_width", 49.13, "local"),
            _h(_SHELL_LOOP + "shell_loop_sweep_does_not_flap", 78.26, "local"),
        ),
    ),
    "crates/lushtext-core/src/ui/sidebar/width_preset.rs": Oracle(
        _CORE,
        (
            _h(_PRESET + "percent_is_the_hint_fraction", 0.05, "ci"),
            _h(_PRESET + "index_round_trips", 0.05, "ci"),
            _h(_PRESET + "fraction_round_trips", 0.05, "ci"),
            _h(_PRESET + "clamp_matches_the_spec_formula_and_bounds", 0.36, "ci"),
            _h(_PRESET + "clamp_is_monotone_in_window_width", 0.26, "ci"),
            _h(_PRESET + "from_fraction_picks_the_nearest_preset", 9.77, "ci"),
            *_SHELL_POLICY_HARNESSES,
        ),
    ),
    "crates/lushtext-core/src/ui/editor_page/minimap/policy.rs": Oracle(
        _CORE,
        (
            _h(_MINIMAP + "native_slider_fit_never_panics", 0.29, "ci"),
            _h(_MINIMAP + "marker_rows_groups_partition_the_band_one_row", 0.43, "ci"),
            _h(_MINIMAP + "marker_rows_groups_partition_the_band_two_rows", 0.85, "ci"),
            _h(_MINIMAP + "min_height_expansion_panics_on_an_inverted_band", 1.31, "ci"),
            _h(_MINIMAP + "marker_rows_walk_is_sound_complete_and_bounded_one_row", 1.49, "ci"),
            _h(_MINIMAP + "skipped_line_crosses_no_threshold", 1.64, "ci"),
            _h(_MINIMAP + "marker_rows_groups_partition_the_band_three_rows", 1.83, "ci"),
            _h(_MINIMAP + "projected_fit_never_panics", 1.83, "ci"),
            _h(_MINIMAP + "marker_rows_runs_stay_in_the_band_at_the_minimum_height_one_row", 1.84, "ci"),
            _h(_MINIMAP + "marker_rows_merge_keeps_runs_disjoint_and_ordered", 3.94, "ci"),
            _h(_MINIMAP + "projected_fit_rejects_a_span_outside_the_band", 3.96, "ci"),
            _h(_MINIMAP + "marker_rows_runs_stay_in_the_band_at_the_minimum_height_two_rows", 4.29, "ci"),
            _h(_MINIMAP + "native_slider_estimate_never_panics", 4.67, "ci"),
            _h(_MINIMAP + "projected_containment_fails_for_general_f64", 5.55, "ci"),
            _h(_MINIMAP + "native_slider_containment_fails_for_general_f64", 7.12, "ci"),
            _h(_MINIMAP + "marker_rows_expansion_stays_in_the_band_on_whole_pixels", 28.47, "ci"),
            _h(_MINIMAP + "skipped_line_crosses_no_threshold_per_character", 30.29, "ci"),
            _h(_MINIMAP + "projected_bounds_stay_in_content_on_whole_pixels", 44.21, "ci"),
            _h(_MINIMAP + "marker_rows_walk_is_sound_complete_and_bounded_two_rows", 61.6, "ci"),
            _h(_MINIMAP + "min_height_expansion_reaches_the_minimum_on_small_whole_pixels", 101.64, "ci"),
            _h(_MINIMAP + "min_height_expansion_never_panics_inside_its_band", 102.5, "ci"),
            _h(_MINIMAP + "marker_rows_walk_is_sound_complete_and_bounded_three_rows", 108.19, "ci"),
            _h(_MINIMAP + "native_slider_stays_in_the_source_map_on_whole_pixels", 116.0, "ci"),
            _h(_MINIMAP + "marker_rows_runs_stay_in_the_band_at_the_minimum_height_three_rows", 142.19, "ci"),
        ),
        functions=(
            "expanded_run",
            "expanded_to_min_height",
            "fit_native_slider_to_source_map_bounds",
            "fit_projected_bounds",
            "mark_row_groups",
            "marker_row_groups",
            "marker_row_runs",
            "native_slider_estimate_from_inputs",
            "paragraph_may_cross",
            "push_expanded_run",
        ),
    ),
    "crates/lushtext-core/src/ui/markdown_preview/policy.rs": Oracle(
        _CORE,
        (
            _h(_PREVIEW + "preview_width_is_not_always_a_third", 0.05, "ci"),
            _h(_PREVIEW + "preview_width_respects_the_floor", 0.05, "ci"),
            _h(_PREVIEW + "preview_width_is_at_most_a_third_above_three_sp", 0.07, "ci"),
            _h(_PREVIEW + "preview_width_is_the_floor_below_three_sp", 0.05, "ci"),
            _h(_PREVIEW + "preview_width_keeps_an_in_band_preference", 0.06, "ci"),
            _h(_PREVIEW + "preview_width_is_monotone_in_preference", 0.15, "ci"),
        ),
        functions=("clamped_preview_width",),
    ),
    "crates/lushtext-core/src/model/responsiveness_budget.rs": Oracle(
        _CORE,
        (
            _h(_BUDGET + "classification_is_sound_and_complete", 0.22, "ci"),
            _h(_BUDGET + "classification_is_monotone_in_the_lateness", 0.19, "ci"),
            _h(_BUDGET + "buckets_are_monotone_and_in_range", 0.31, "ci"),
            _h(_BUDGET + "percentile_is_bounded_and_monotone", 16.09, "ci"),
            _h(_BUDGET + "percentile_refuses_an_unsorted_slice", 7.41, "ci"),
        ),
        functions=("classify_dispatch", "histogram_bucket", "histogram_bucket_floor_us", "nearest_rank_percentile"),
    ),
    "crates/lushtext-core/src/model/settings_write_batch.rs": Oracle(
        _CORE,
        (_h(_WRITE_BATCH + "a_flush_never_drops_the_latest_value_or_re_emits_a_persisted_one", 1.38, "ci"),),
        functions=("record", "observe_persisted", "acknowledge"),
    ),
    "crates/lushtext-core/src/model/list_budget.rs": Oracle(
        _CORE,
        (
            _h(_LIST_BUDGET + "every_turn_makes_progress", 0.02, "ci"),
            _h(_LIST_BUDGET + "rows_within_a_zero_charge_bounds_nothing", 0.02, "ci"),
            _h(_LIST_BUDGET + "the_retained_items_are_a_prefix", 0.06, "ci"),
            _h(_LIST_BUDGET + "a_turn_stops_at_the_first_unit_after_the_slice", 0.08, "ci"),
            _h(_LIST_BUDGET + "a_budget_one_byte_below_n_times_w_refuses_the_last_item", 0.1, "ci"),
            _h(_LIST_BUDGET + "the_charge_never_exceeds_the_limit", 0.17, "ci"),
            _h(_LIST_BUDGET + "a_budget_of_n_times_w_loses_nothing", 1.07, "ci"),
            _h(_LIST_BUDGET + "rows_within_agrees_with_the_budget", 1.17, "ci"),
            _h(_LIST_BUDGET + "rows_within_is_the_largest_fitting_prefix", 1.93, "ci"),
        ),
        functions=("admit", "turn_continues", "rows_within"),
    ),
    "crates/lushtext-core/src/model/document_scale.rs": Oracle(
        _CORE,
        (
            _h(_SCALE + "scale_save_without_join_reenables_a_feature", 0.09, "ci"),
            _h(_SCALE + "scale_byte_tiers_match_the_tier_reference", 0.13, "ci"),
            _h(_SCALE + "scale_protected_view_is_upward_closed", 0.18, "ci"),
            _h(_SCALE + "scale_is_monotone", 0.49, "ci"),
            _h(_SCALE + "scale_is_monotone_in_the_language_threshold", 0.27, "ci"),
            _h(_SCALE + "scale_join_is_a_meet_on_presentation_features", 0.54, "ci"),
            _h(_SCALE + "scale_of_a_profile_join_is_the_join_of_scales", 0.77, "ci"),
            _h(_SCALE + "scale_edit_reanalysis_reduction_is_monotone_and_survives_the_join", 0.6, "ci"),
        ),
    ),
    "crates/lushtext-core/src/model/reanalysis_pacing.rs": Oracle(
        _CORE,
        (
            _h(_REANALYSIS + "the_start_session_is_in_bounds", 0.02, "ci"),
            _h(_REANALYSIS + "a_session_continues_only_within_its_allowance", 0.5, "ci"),
            _h(_REANALYSIS + "a_session_that_never_merges_downgrades_within_the_step_bound", 0.2, "ci"),
        ),
    ),
    "crates/lushtext-core/src/model/buffer_replacement.rs": Oracle(
        _CORE,
        (
            _h(_PACED + "the_paced_budget_follows_batch_density", 0.02, "ci"),
            _h(_PACED + "paced_slices_without_the_visible_budget_exceed_v_slice", 1.12, "ci"),
            _h(_PACED + "paced_slices_cover_the_paragraphs_exactly", 247.11, "ci"),
            _h(_PACED + "paced_slices_hold_both_budgets_unless_one_paragraph", 41.31, "ci"),
        ),
        functions=("admit", "for_profile", "batch_exceeds_slice"),
    ),
    "crates/lushtext-core/src/model/highlight_chunking.rs": Oracle(
        _CORE,
        (
            _h(_CHUNKED + "a_chunk_waits_at_most_once_and_never_when_analysed", 0.02, "ci"),
            _h(_CHUNKED + "a_chunked_language_is_gated_above_its_attach_threshold_only_by_the_scale", 0.03, "ci"),
            _h(_CHUNKED + "the_install_order_is_chunked_exactly_where_stated", 0.04, "ci"),
            _h(_CHUNKED + "without_the_paragraph_bound_a_slice_exceeds_the_cut", 0.03, "ci"),
            _h(_CHUNKED + "a_highlighted_chunk_follows_the_attach_and_the_analysis", 0.06, "ci"),
            _h(_CHUNKED + "highlighted_slices_never_exceed_the_cut", 3.49, "ci"),
            _h(_CHUNKED + "a_chunked_install_finishes_even_without_coverage_or_a_language", 0.44, "ci"),
        ),
    ),
    "crates/lushtext-core/src/ui/editor_page/load/policy.rs": Oracle(
        _CORE,
        (
            _h(_SETTLE + "settled_needs_every_clause", 0.11, "ci"),
            _h(_SETTLE + "restyle_ranges_cover_the_document_exactly_once", 12.42, "ci"),
            _h(_SETTLE + "restyle_ranges_hold_v_slice_unless_one_paragraph", 253.74, "ci"),
            _h(_SETTLE + "restyle_ranges_take_the_visible_paragraphs_first_then_alternate", 14.31, "ci"),
        ),
        functions=("next_restyle_range", "settle_stage", "new", "is_done"),
    ),
    "crates/lushtext-core/src/model/paragraph_boundary.rs": Oracle(
        _CORE,
        (
            _h(_BOUNDARY + "profile_slices_without_carry_disagree", PROFILE_CARRY_SECONDS, "ci"),
            _h(_BOUNDARY + "profile_matches_the_specification", PROFILE_SPEC_SECONDS, "ci"),
            _h(_BOUNDARY + "profile_is_independent_of_slicing", PROFILE_SLICING_SECONDS, "ci"),
            _h(_BOUNDARY + "boundary_break_is_a_scalar_boundary_after_a_delimiter", 96.25, "local"),
            _h(_BOUNDARY + "boundary_line_length_excludes_its_delimiter", 96.42, "local"),
            _h(_BOUNDARY + "boundary_slice_end_is_safe_and_advances", 203.01, "local"),
            _h(_BOUNDARY + "boundary_agrees_with_the_reference_rule", 258.74, "local"),
            _h(_BOUNDARY + "boundary_slice_end_is_the_reference_choice", 293.39, "local"),
        ),
    ),
    "crates/lushtext-core/src/model/protected_fold.rs": Oracle(
        _CORE,
        (
            _h(_FOLD + "production_thresholds_satisfy_the_preconditions", 0.03, "ci"),
            _h(_FOLD + "fold_location_is_total_and_round_trips", 0.43, "ci"),
            _h(_FOLD + "an_over_budget_window_fits_once_dense", 1.92, "ci"),
            _h(_FOLD + "fold_byte_offset_never_splits_a_scalar", 32.29, "ci"),
            _h(_FOLD + "density_bound_fails_without_the_density_fold", 0.56, "ci"),
            _h(_FOLD + "density_bound_holds_for_every_window_r1", 37.35, "ci"),
            _h(_FOLD + "density_bound_holds_for_every_window_r2", 59.76, "ci"),
            _h(_FOLD + "density_bound_holds_for_every_window_r3", 301.33, "local"),
            _h(_FOLD + "folds_hide_only_tails_and_only_where_needed_r1", 62.05, "ci"),
            _h(_FOLD + "folds_hide_only_tails_and_only_where_needed_r2", 70.42, "local"),
            _h(_FOLD + "folds_hide_only_tails_and_only_where_needed_r3", 597.46, "local"),
            _h(_FOLD + "fold_plan_is_independent_of_install_slicing_r1", 36.29, "ci"),
            _h(_FOLD + "fold_plan_is_independent_of_install_slicing_r2", 54.16, "ci"),
            _h(_FOLD + "fold_plan_is_independent_of_install_slicing_r3", 361.16, "local"),
        ),
    ),
    "crates/lushtext-core/src/model/protected_install.rs": Oracle(
        _CORE,
        (
            _h(_PI + "settle_budget_derivation_fits_and_never_overflows", 0.28, "ci"),
            _h(_PI + "one_paragraph_fold_shows_its_head_cap", 1.46, "ci"),
            _h(_PI + "protected_clear_plan_covers_the_buffer_once", 7.12, "ci"),
            _h(_PI + "step_initial_cursor_is_well_formed", 65.83, "local"),
            _h(_PI + "step_head_ends_at_the_fold_point", 109.58, "local"),
            _h(_PI + "step_hidden_tail_slices_are_safe_and_bounded", 131.77, "local"),
            _h(_PI + "unrounded_tail_end_splits_a_scalar", 249.29, "local"),
            _h(_PI + "step_hidden_tail_end_continues_into_a_visible_slice", 262.25, "local"),
            _h(_PI + "step_visible_slices_hold_whole_unfolded_paragraphs", 334.72, "local"),
        ),
    ),
    "crates/lushtext-core/src/ui/editor_page/protected_view/policy.rs": Oracle(
        _CORE,
        (
            _h(_PV + "no_mutating_action_is_admitted", 0.11, "ci"),
            _h(_PV + "find_batch_exceeds_the_byte_bound_without_record_admission", 0.12, "ci"),
            _h(_PV + "protected_view_entry_is_monotone", 0.84, "ci"),
            _h(_PV + "entry_is_not_monotone_if_eviction_reload_re_derives", 1.11, "ci"),
            _h(_PV + "entry_undo_after_load_entry_breaks_monotonicity", 1.17, "ci"),
            _h(_PV + "find_batches_are_bounded_and_partition_results", 49.09, "ci"),
            _h(_PV + "navigation_target_reaches_every_column", 784.45, "local"),
        ),
    ),
    "crates/lushtext-core/src/model/paragraph_profile_watch.rs": Oracle(
        _CORE,
        (
            _h(_WATCH + "watch_delete_between_cr_and_lf_preserves_the_invariant", WATCH_DELETE_SECONDS, "local"),
            _h(_WATCH + "watch_reads_are_bounded", WATCH_READS_SECONDS, "local"),
            _h(_WATCH + "watch_step_preserves_the_invariant", WATCH_STEP_SECONDS, "local"),
            _h(_WATCH + "watch_fast_path_without_a_carried_bound_is_unsound", WATCH_UNSOUND_SECONDS, "local"),
        ),
    ),
}

# Harnesses deliberately outside every oracle list, each with its reason.
NOT_ORACLE_HARNESSES: dict[str, str] = {
    _BOUNDARY + "boundary_newline_only_slicer_disagrees": (
        "pins the retired '\\n'-only slicer, a transcription inside the harness file, "
        "not a property of the shipped boundary function"
    ),
    _JOURNAL + "a_second_writer_breaks_the_journal_invariants": (
        "pins axiom A6 (one writer per data directory), not a property of journal_core; "
        "journal_invariants_hold_under_crashes says more, and it costs minutes per mutant"
    ),
    _SHELL_LOOP + "shell_loop_layout_setter_flaps": (
        "pins the retired pre-fix properties setter, a model inside the harness file, "
        "not a property of the shipped policy"
    ),
    _SHELL_LOOP + "shell_loop_open_button_breakpoint_uncollapsed_the_workspace": (
        "pins the retired pre-fix Open-button breakpoint, a model inside the harness "
        "file, not a property of the shipped policy"
    ),
}

# Modules a harness file imports from that are not oracles, each with its reason.
NOT_ORACLE_MODULES: dict[str, str] = {
    "crates/lushtext-core/src/model/draft.rs": (
        "the journal harnesses import only the DraftManifestCompleteness enum from it; "
        "no harness checks its behaviour"
    ),
}

HARNESS_START_RE = re.compile(r"^Checking harness (\S+?)\.\.\.$")
HARNESS_VERDICT_RE = re.compile(r"^VERIFICATION:- (\S+)")
HARNESS_TIME_RE = re.compile(r"^Verification Time: ([0-9.]+)s$")

PROOF_RE = re.compile(r"#\[kani::proof\][^\n]*\n(?:\s*#\[[^\n]*\n)*\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)")


def module_path(source_root: Path, path: Path) -> str:
    """The module path of a source file, relative to its crate root."""
    parts = list(path.relative_to(source_root).with_suffix("").parts)
    if parts[-1] in ("mod", "lib"):
        parts.pop()
    return "::".join(parts)


def discover() -> tuple[dict[str, list[str]], list[Path]]:
    """Every harness in the tree, as `package -> [kani names]`, plus the files
    holding harnesses outside every listed package (which no shard can run)."""
    found: dict[str, list[str]] = {package: [] for package in PACKAGES}
    unowned = []
    for path in sorted(CRATES.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        if "kani::proof" not in text or not PROOF_RE.search(text):
            continue
        owner = next(
            ((package, root) for package, root in PACKAGES.items() if path.is_relative_to(root)),
            None,
        )
        if owner is None:
            unowned.append(path)
            continue
        package, root = owner
        prefix = module_path(root, path)
        for match in PROOF_RE.finditer(text):
            found[package].append(f"{prefix}::{match.group(1)}" if prefix else match.group(1))
    return found, unowned


def kani_version() -> str:
    """The pinned Kani version, from the Makefile."""
    match = KANI_VERSION_RE.search(MAKEFILE.read_text(encoding="utf-8"))
    if match is None:
        raise SystemExit("kani-shards: no `KANI_VERSION ?=` line in the Makefile")
    return match.group(1)


def check(found: dict[str, list[str]], unowned: list[Path]) -> list[str]:
    """Every harness matches exactly one shard of its own package."""
    problems = [
        f"{path.relative_to(REPO_ROOT)} holds Kani harnesses outside every package in PACKAGES"
        for path in unowned
    ]
    for package, names in found.items():
        for name in names:
            owners = [shard_name for shard_name, shard in SHARDS.items() if shard.owns(package, name)]
            if len(owners) != 1:
                problems.append(f"{package} harness {name} matches shards {owners or 'none'}")
    for shard_name, shard in SHARDS.items():
        for prefix in shard.filters:
            if not any(prefix in name for name in found.get(shard.package, [])):
                problems.append(f"shard {shard_name} prefix {prefix} matches no harness")
    return problems


USE_RE = re.compile(r"^(?P<indent>[ \t]*)use[ \t]+(?P<path>(?:crate|super)(?:::\w+)*)", re.M)


def module_file(directory: Path, name: str) -> Path | None:
    """The file of child module `name` of a module whose children live in `directory`."""
    for candidate in (directory / f"{name}.rs", directory / name / "mod.rs"):
        if candidate.is_file():
            return candidate
    return None


def children_directory(module: Path) -> Path:
    """Where the child modules of the module file `module` live."""
    if module.name in ("mod.rs", "lib.rs", "main.rs"):
        return module.parent
    return module.parent / module.stem


def harness_parent(path: Path) -> Path | None:
    """The module file that declares the harness file `path`."""
    directory = path.parent
    names = ("lib.rs", "main.rs") if directory.name == "src" else ("mod.rs",)
    for name in names:
        if (directory / name).is_file():
            return directory / name
    sibling = directory.parent / f"{directory.name}.rs"
    return sibling if sibling.is_file() else None


def imported_modules(path: Path, source_root: Path) -> set[Path]:
    """The module files a harness file imports from, through top-level `use super::`
    and any `use crate::` statement: the longest path prefix naming a module file.
    An inline module's `use super::` names the harness file itself and is skipped."""
    text = path.read_text(encoding="utf-8")
    modules: set[Path] = set()
    for match in USE_RE.finditer(text):
        segments = match.group("path").split("::")
        if segments[0] == "super":
            if match.group("indent"):
                continue
            current = harness_parent(path)
        else:
            current = source_root / "lib.rs"
        if current is None:
            continue
        for segment in segments[1:]:
            child = module_file(children_directory(current), segment)
            if child is None:
                break
            current = child
        modules.add(current)
    return modules


def oracle_problems(
    found: dict[str, list[str]],
    oracles: dict[str, Oracle],
    not_harnesses: dict[str, str],
    not_modules: dict[str, str],
    root: Path = REPO_ROOT,
) -> list[str]:
    """The oracle table matches the harnesses: every oracle module exists, every
    oracle harness is exactly one discovered harness of its package, every
    discovered harness is an oracle or named with a reason, and every module a
    harness file imports from is an oracle or named with a reason."""
    problems = []
    used: set[tuple[str, str]] = set()
    for module, oracle in oracles.items():
        if not (root / module).is_file():
            problems.append(f"oracle module {module} does not exist")
        if not oracle.harnesses:
            problems.append(f"oracle module {module} lists no harness")
        names = [h.name for h in oracle.harnesses]
        if len(set(names)) != len(names):
            problems.append(f"oracle module {module} lists a harness twice")
        for harness in oracle.harnesses:
            if harness.tier not in TIERS:
                problems.append(f"oracle harness {harness.name} tier {harness.tier!r} is not one of {TIERS}")
            matches = [n for n in found.get(oracle.package, []) if n == harness.name]
            if len(matches) != 1:
                problems.append(
                    f"oracle harness {harness.name} of {module} resolves to {len(matches)} "
                    f"{oracle.package} harnesses, not exactly one"
                )
            if harness.name in not_harnesses:
                problems.append(f"harness {harness.name} is both an oracle and NOT_ORACLE_HARNESSES")
            used.add((oracle.package, harness.name))
    for name in not_harnesses:
        if not any(name in names for names in found.values()):
            problems.append(f"NOT_ORACLE_HARNESSES names {name}, which is no harness")
    for package, names in found.items():
        for name in names:
            if (package, name) not in used and name not in not_harnesses:
                problems.append(
                    f"{package} harness {name} is in no ORACLES list and not in NOT_ORACLE_HARNESSES"
                )
    for module in not_modules:
        if module in oracles:
            problems.append(f"module {module} is both an oracle and NOT_ORACLE_MODULES")
    for package, source_root in PACKAGES.items():
        source_root = root / source_root.relative_to(REPO_ROOT)
        for path in sorted(source_root.rglob("kani_proofs.rs")):
            for module in sorted(imported_modules(path, source_root)):
                relative = str(module.relative_to(root))
                if relative not in oracles and relative not in not_modules:
                    problems.append(
                        f"{path.relative_to(root)} imports from {relative}, which is neither in "
                        "ORACLES nor in NOT_ORACLE_MODULES"
                    )
    return problems


def harness_tree_problems(root: Path = REPO_ROOT) -> list[str]:
    """A file under a `kani_proofs/` directory is a child of the harness module
    `kani_proofs.rs` beside that directory, which `make check-workflow-boundaries`
    (rule 9) requires to be declared `#[cfg(kani)] mod kani_proofs;`. The
    mutation scope excludes both forms by name; this keeps the directory form
    tied to a gated module file rather than a `kani_proofs/mod.rs` that rule 9
    would never see."""
    problems = []
    crates = root / "crates"
    for directory in sorted(p for p in crates.rglob("kani_proofs") if p.is_dir()):
        relative = directory.relative_to(root)
        if (directory / "mod.rs").exists():
            problems.append(f"{relative}/mod.rs: a harness module must be the file kani_proofs.rs, not mod.rs")
        if not (directory.parent / "kani_proofs.rs").is_file():
            problems.append(
                f"{relative}/ holds harness code but no {relative.parent}/kani_proofs.rs declares it"
            )
    return problems


def pending_problem(shard_name: str, measured_in: str, changes_dir: Path) -> str | None:
    """A record that still holds a local figure names the active change that
    owes its dispatch (close-protected-install-proof-strength, design D7).

    The table must hold some figure before a branch can push and dispatch
    kani.yml, so a local figure is allowed while it is pending. A written
    obligation alone did not hold (long-line-protected-view's record said it
    "must not merge on it" and merged), so the owner is checked: once the
    owning change is archived, or if it never existed, the record fails, and
    the commit that archives a change still owing a figure fails its hook.
    """
    if not PENDING_MARK_RE.search(measured_in):
        return None
    owner = PENDING_OWNER_RE.search(measured_in)
    if owner is None:
        return (
            f"shard {shard_name} holds a local figure without an owner; write "
            "'PENDING DISPATCH (owed by <active openspec change>)' or record two dispatched runs"
        )
    change = owner.group(1)
    if change == "archive" or not (changes_dir / change).is_dir():
        return (
            f"shard {shard_name} is still pending dispatch, owed by {change}, which is not an "
            f"active change under {changes_dir.name}/; record two dispatched kani.yml runs"
        )
    return None


def budget_problems(shards: dict[str, Shard], changes_dir: Path = OPENSPEC_CHANGES) -> list[str]:
    """Every shard has a runner measurement inside the margins for its gate,
    and a local figure is owed by an active change."""
    problems = []
    for shard_name, shard in shards.items():
        if shard.gate not in GATES:
            problems.append(f"shard {shard_name} gate {shard.gate!r} is not one of {GATES}")
        if shard.ci_minutes is None or shard.ci_peak_gib is None or not shard.measured_in:
            problems.append(
                f"shard {shard_name} has no recorded CI runner measurement "
                "(ci_minutes, ci_peak_gib, measured_in); dispatch kani.yml and record it"
            )
            continue
        if problem := pending_problem(shard_name, shard.measured_in, changes_dir):
            problems.append(problem)
        if shard.ci_minutes > MAX_SHARD_MINUTES:
            problems.append(
                f"shard {shard_name} takes {shard.ci_minutes} min on the runner, over the "
                f"{MAX_SHARD_MINUTES:g}-minute margin; split it"
            )
        if shard.ci_peak_gib > MAX_SHARD_PEAK_GIB:
            problems.append(
                f"shard {shard_name} peaks at {shard.ci_peak_gib} GiB on the runner, over the "
                f"{MAX_SHARD_PEAK_GIB:g} GiB margin; split it"
            )
        if shard.gate == "pull-request" and shard.ci_minutes > MAX_PULL_REQUEST_SHARD_MINUTES:
            problems.append(
                f"shard {shard_name} is gated pull-request but takes {shard.ci_minutes} min, over "
                f"the {MAX_PULL_REQUEST_SHARD_MINUTES:g}-minute pull-request margin; gate it scheduled"
            )
    return problems


class HarnessLog:
    """Parses Kani's streamed output into one record per harness: the name
    from `Checking harness NAME...`, then that harness's verdict and its own
    `Verification Time:` line."""

    def __init__(self) -> None:
        self.harnesses: list[dict[str, object]] = []

    def feed(self, line: str) -> None:
        line = line.strip()
        if match := HARNESS_START_RE.match(line):
            self.harnesses.append({"name": match.group(1), "verdict": None, "seconds": None})
        elif self.harnesses and (match := HARNESS_VERDICT_RE.match(line)):
            self.harnesses[-1]["verdict"] = match.group(1)
        elif self.harnesses and (match := HARNESS_TIME_RE.match(line)):
            self.harnesses[-1]["seconds"] = float(match.group(1))


def run_measured(command: list[str]) -> tuple[int, float, int, HarnessLog]:
    """Runs `command`, streaming and parsing its output. Returns the exit
    status, the wall time in seconds, and the peak resident set in KiB of the
    largest process in the child's tree: on Linux the child's `wait4` rusage
    folds in every descendant it waited for (cargo-kani waits on kani-driver,
    which waits on CBMC), so the figure is CBMC's peak and is per shard, not
    cumulative across shards like `RUSAGE_CHILDREN`."""
    log = HarnessLog()
    start = time.monotonic()
    child = subprocess.Popen(
        command, cwd=REPO_ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace"
    )
    assert child.stdout is not None
    for line in child.stdout:
        sys.stdout.write(line)
        log.feed(line)
    sys.stdout.flush()
    _, wait_status, usage = os.wait4(child.pid, 0)
    return os.waitstatus_to_exitcode(wait_status), time.monotonic() - start, usage.ru_maxrss, log


def shard_record(shard: str, status: int, seconds: float, peak_kib: int, log: HarnessLog) -> dict[str, object]:
    return {
        "shard": shard,
        "package": SHARDS[shard].package,
        "status": status,
        "wall_seconds": round(seconds, 1),
        "wall_minutes": round(seconds / 60, 2),
        "peak_rss_kib": peak_kib,
        "peak_rss_gib": round(peak_kib / (1024 * 1024), 2),
        "harnesses": log.harnesses,
    }


def summary_markdown(records: list[dict[str, object]]) -> str:
    lines = [
        "| Shard | Status | Wall time (min) | Peak RSS (GiB) | Harnesses |",
        "|---|---|---|---|---|",
    ]
    for record in records:
        lines.append(
            f"| `{record['shard']}` | {record['status']} | {record['wall_minutes']} "
            f"| {record['peak_rss_gib']} | {len(record['harnesses'])} |"
        )
    lines += ["", "| Harness | Verdict | Verification time (s) |", "|---|---|---|"]
    for record in records:
        for harness in record["harnesses"]:
            lines.append(f"| `{harness['name']}` | {harness['verdict']} | {harness['seconds']} |")
    return "\n".join(lines) + "\n"


def shard_command(shards: dict[str, Shard], shard: str, target_dir: str) -> list[str]:
    """The `cargo kani` command for one shard: its package, its harness
    filters, and its own `kani_flags`, which no other shard receives."""
    record = shards[shard]
    command = ["cargo", "kani", "-p", record.package, "--target-dir", target_dir, *record.kani_flags]
    for prefix in record.filters:
        command += ["--harness", prefix]
    return command


def run(shard_names: list[str], target_dir: str, measure: str | None = None) -> int:
    records: list[dict[str, object]] = []
    result = 0
    for shard in shard_names:
        command = shard_command(SHARDS, shard, target_dir)
        print(f"Running Kani shard {shard}: {' '.join(command)}", flush=True)
        if measure is None:
            status = subprocess.run(command, cwd=REPO_ROOT, check=False).returncode
        else:
            record = shard_record(shard, *run_measured(command))
            records.append(record)
            status = record["status"]
            print(
                f"Kani shard {shard}: {record['wall_minutes']} min, peak RSS {record['peak_rss_gib']} GiB, "
                f"{len(record['harnesses'])} harnesses",
                flush=True,
            )
        if status != 0:
            print(f"Kani shard {shard} failed with status {status}", file=sys.stderr)
            result = status
            break
    if measure is not None:
        Path(measure).write_text(
            json.dumps({"kani_version": kani_version(), "shards": records}, indent=2) + "\n", encoding="utf-8"
        )
        if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(summary, "a", encoding="utf-8") as handle:
                handle.write(summary_markdown(records))
    return result


def oracle_self_test() -> None:
    """The oracle table check and the harness-tree check, over a scratch tree."""
    import tempfile

    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        core = root / "crates/lushtext-core/src"
        (root / "crates/gtk-lush/widgets/src").mkdir(parents=True)
        (core / "m").mkdir(parents=True)
        (core / "lib.rs").write_text("pub mod m;\npub mod other;\n")
        (core / "m.rs").write_text("#[cfg(kani)]\nmod kani_proofs;\npub fn f() {}\n")
        (core / "other.rs").write_text("pub struct T;\n")
        (core / "m/kani_proofs.rs").write_text(
            "use super::f;\nuse crate::other::T;\nmod inner {\n    use super::helper;\n}\n"
            "#[kani::proof]\nfn a() {}\n#[kani::proof]\nfn b() {}\n"
        )
        found = {"gtk-lush-widgets": [], "lushtext-core": ["m::kani_proofs::a", "m::kani_proofs::b"]}
        oracles = {"crates/lushtext-core/src/m.rs": Oracle("lushtext-core", (_h("m::kani_proofs::a", 1.0, "ci"),))}
        not_harnesses = {"m::kani_proofs::b": "reason"}
        not_modules = {"crates/lushtext-core/src/other.rs": "reason"}
        assert imported_modules(core / "m/kani_proofs.rs", core) == {core / "m.rs", core / "other.rs"}
        good = oracle_problems(found, oracles, not_harnesses, not_modules, root)
        assert good == [], good
        misspelt = {
            "crates/lushtext-core/src/m.rs": Oracle("lushtext-core", (_h("m::kani_proofs::aa", 1.0, "ci"),))
        }
        failing = {
            "misspelt harness": (misspelt, not_harnesses, not_modules),
            "missing module": ({**oracles, "crates/lushtext-core/src/gone.rs": oracles["crates/lushtext-core/src/m.rs"]}, not_harnesses, not_modules),
            "empty oracle": ({"crates/lushtext-core/src/m.rs": Oracle("lushtext-core", ())}, {**not_harnesses, "m::kani_proofs::a": "r"}, not_modules),
            "bad tier": ({"crates/lushtext-core/src/m.rs": Oracle("lushtext-core", (_h("m::kani_proofs::a", 1.0, "nightly"),))}, not_harnesses, not_modules),
            "unlisted harness": (oracles, {}, not_modules),
            "unlisted imported module": (oracles, not_harnesses, {}),
            "stale not-oracle harness": (oracles, {**not_harnesses, "m::kani_proofs::gone": "r"}, not_modules),
        }
        for label, (o, h, m) in failing.items():
            assert oracle_problems(found, o, h, m, root), f"oracle self-test {label} should fail"
        ordered = Oracle("p", (_h("x", 9.0, "local"), _h("y", 1.0, "ci"), _h("z", 3.0, "ci"))).ordered
        assert [h.name for h in ordered()] == ["y", "z", "x"]
        assert [h.name for h in ordered("ci")] == ["y", "z"]

        # A `kani_proofs/` directory is a child of a sibling `kani_proofs.rs`,
        # never a `kani_proofs/mod.rs`.
        assert harness_tree_problems(root) == []
        (core / "m/kani_proofs").mkdir()
        (core / "m/kani_proofs/model.rs").write_text("")
        assert harness_tree_problems(root) == []
        (core / "m/kani_proofs/mod.rs").write_text("")
        assert len(harness_tree_problems(root)) == 1
        (core / "m/kani_proofs/mod.rs").unlink()
        (core / "m/kani_proofs.rs").unlink()
        assert len(harness_tree_problems(root)) == 1


def self_test() -> None:
    root = Path("/crate/src")
    assert module_path(root, root / "kani_proofs.rs") == "kani_proofs"
    assert module_path(root, root / "outer/inner/mod.rs") == "outer::inner"
    text = "#[kani::proof]\n#[kani::should_panic]\n#[kani::unwind(4)]\nfn one() {}\n#[kani::proof]\npub fn two() {}\n"
    assert [m.group(1) for m in PROOF_RE.finditer(text)] == ["one", "two"]
    assert KANI_VERSION_RE.search("X ?= 1\nKANI_VERSION ?= 0.68.0\n").group(1) == "0.68.0"

    # Kani's streamed output, abridged: one record per harness, with its own
    # verdict and time, and nothing picked up from lines between harnesses.
    log = HarnessLog()
    for line in (
        "Checking harness kani_proofs::no_input_panics...\n",
        "VERIFICATION:- SUCCESSFUL\n",
        "Verification Time: 0.4419652s\n",
        "Checking harness services::x::kani_proofs::a_second_writer_loses...\n",
        "Check 1: foo\n",
        "VERIFICATION:- SUCCESSFUL (encountered one or more panics as expected)\n",
        "Verification Time: 500.8s\n",
        "Complete - 2 successfully verified harnesses, 0 failures, 2 total.\n",
    ):
        log.feed(line)
    assert log.harnesses == [
        {"name": "kani_proofs::no_input_panics", "verdict": "SUCCESSFUL", "seconds": 0.4419652},
        {"name": "services::x::kani_proofs::a_second_writer_loses", "verdict": "SUCCESSFUL", "seconds": 500.8},
    ], log.harnesses

    # The JSON shape the workflow uploads, and its summary table.
    record = shard_record("widgets-geometry", 0, 90.0, 3 * 1024 * 1024, log)
    assert json.loads(json.dumps(record)) == {
        "shard": "widgets-geometry",
        "package": "gtk-lush-widgets",
        "status": 0,
        "wall_seconds": 90.0,
        "wall_minutes": 1.5,
        "peak_rss_kib": 3 * 1024 * 1024,
        "peak_rss_gib": 3.0,
        "harnesses": log.harnesses,
    }
    table = summary_markdown([record])
    assert "| `widgets-geometry` | 0 | 1.5 | 3.0 | 2 |" in table, table
    assert "| `kani_proofs::no_input_panics` | SUCCESSFUL | 0.4419652 |" in table, table

    # Budget rules: an unmeasured shard, one over 25 minutes, one over 12 GiB,
    # and a pull-request shard over 15 minutes each fail; a measured shard
    # inside every margin passes.
    good = Shard("p", ("f",), "pull-request", 14.0, 11.5, "run 1")
    assert budget_problems({"ok": good}) == [], budget_problems({"ok": good})
    assert budget_problems({"s": replace(good, gate="scheduled", ci_minutes=24.9)}) == []
    cases = {
        "unmeasured": replace(good, ci_minutes=None, ci_peak_gib=None, measured_in=""),
        "no-run-id": replace(good, measured_in=""),
        "slow": replace(good, gate="scheduled", ci_minutes=25.5),
        "memory": replace(good, ci_peak_gib=12.5),
        "slow-pull-request": replace(good, ci_minutes=15.5),
        "unknown-gate": replace(good, gate="nightly"),
    }
    for label, shard in cases.items():
        assert budget_problems({label: shard}), f"budget self-test {label} should fail"

    # A local figure: pending with no owner fails, owed by an archived or an
    # absent change fails, owed by an active change passes.
    with tempfile.TemporaryDirectory() as temporary:
        changes = Path(temporary)
        (changes / "open-change").mkdir()
        (changes / "archive/2026-01-01-done-change").mkdir(parents=True)
        pending = {
            "no-owner": "PENDING DISPATCH: local make kani run",
            "provisional": "provisional: local make kani 10.6 min",
            "archived": "PENDING DISPATCH (owed by done-change): local run",
            "absent": "PENDING DISPATCH (owed by never-created): local run",
            "archive-dir": "PENDING DISPATCH (owed by archive): local run",
        }
        for label, measured_in in pending.items():
            assert budget_problems({label: replace(good, measured_in=measured_in)}, changes), (
                f"pending self-test {label} should fail"
            )
        owed = replace(good, measured_in="run 1; PENDING DISPATCH (owed by open-change): local run")
        assert budget_problems({"owed": owed}, changes) == [], budget_problems({"owed": owed}, changes)

    oracle_self_test()

    # Per-shard Kani flags reach their own shard's command and no other.
    flagged = {
        "plain": good,
        "contracts": replace(good, filters=("g",), kani_flags=("-Z", "loop-contracts")),
    }
    plain = shard_command(flagged, "plain", "target/kani")
    contracts = shard_command(flagged, "contracts", "target/kani")
    assert "loop-contracts" not in plain and "-Z" not in plain, plain
    assert contracts[:8] == ["cargo", "kani", "-p", "p", "--target-dir", "target/kani", "-Z", "loop-contracts"], contracts
    assert contracts[-2:] == ["--harness", "g"], contracts


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["list", "check", "github-outputs", "run", "oracle"])
    parser.add_argument("shard", nargs="?", default="all")
    parser.add_argument("--target-dir", default="target/kani")
    parser.add_argument("--measure", metavar="JSON", help="run: record per-shard budgets to this JSON file")
    parser.add_argument("--tier", choices=["ci", "all"], default="all", help="oracle: ci-tier harnesses only")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    if args.command == "oracle":
        if args.shard not in ORACLES:
            print(f"unknown oracle module {args.shard}; known: {', '.join(ORACLES)}", file=sys.stderr)
            return 2
        for harness in ORACLES[args.shard].ordered(args.tier):
            print(harness.name)
        return 0
    found, unowned = discover()
    if args.command == "list":
        for shard_name, shard in SHARDS.items():
            names = [n for n in found[shard.package] if shard.owns(shard.package, n)]
            budget = (
                "unmeasured"
                if shard.ci_minutes is None
                else f"{shard.ci_minutes} min, {shard.ci_peak_gib} GiB on the runner"
            )
            print(f"{shard_name} ({shard.package}, {shard.gate}, {budget}): {len(names)} harnesses")
            for name in names:
                print(f"  {name}")
        return 0
    problems = check(found, unowned)
    # Budgets and the proof-strength oracle table gate the table (`check`) and
    # the CI matrix (`github-outputs`), not `run`: a shard over a margin must
    # stay runnable so it can be re-measured, and the oracle table does not
    # decide what a shard verifies.
    if args.command in ("check", "github-outputs"):
        problems += budget_problems(SHARDS)
        problems += oracle_problems(found, ORACLES, NOT_ORACLE_HARNESSES, NOT_ORACLE_MODULES)
        problems += harness_tree_problems()
    if problems:
        for problem in problems:
            print(f"kani-shards: {problem}", file=sys.stderr)
        return 1
    if args.command == "check":
        total = sum(len(names) for names in found.values())
        print(f"kani shard table passed: {total} harnesses, each in exactly one of {len(SHARDS)} shards")
        print(
            f"kani oracle table passed: {len(ORACLES)} modules, "
            f"{len(NOT_ORACLE_HARNESSES)} harnesses and {len(NOT_ORACLE_MODULES)} imported modules named as no oracle"
        )
        return 0
    if args.command == "github-outputs":
        print(f"shards={json.dumps(list(SHARDS))}")
        pr_shards = [shard_name for shard_name, shard in SHARDS.items() if shard.gate == "pull-request"]
        print(f"pr-shards={json.dumps(pr_shards)}")
        print(f"kani-version={kani_version()}")
        return 0
    if args.shard == "all":
        return run(list(SHARDS), args.target_dir, args.measure)
    if args.shard not in SHARDS:
        print(f"unknown shard {args.shard}; known: {', '.join(SHARDS)}", file=sys.stderr)
        return 2
    return run([args.shard], args.target_dir, args.measure)


if __name__ == "__main__":
    sys.exit(main())
