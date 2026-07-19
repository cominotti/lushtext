#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_SMOKE_ARTIFACT_DIR:-build/smoke/performance}"
FILTERS="${LUSHTEXT_PERFORMANCE_SMOKE_FILTER:-file_index_search palette_pipeline_hardening_100000 file_index_rebuild end_to_end_boundedness quality_gap_scale content_search_smoke search_interactive_policies markdown_render_planning save_admission_policy editor_memory_policy json_persistence editor_file_io transient_file_load workspace_watch_pressure replace_preview_generation replace_undo_workflows recovery_performance}"
SAMPLE_SIZE="${LUSHTEXT_PERFORMANCE_SMOKE_SAMPLE_SIZE:-10}"
MEASUREMENT_TIME="${LUSHTEXT_PERFORMANCE_SMOKE_MEASUREMENT_TIME:-1}"
WARM_UP_TIME="${LUSHTEXT_PERFORMANCE_SMOKE_WARM_UP_TIME:-1}"

usage() {
    cat <<'EOF'
Usage: scripts/run-performance-smoke.sh [--artifact-dir DIR] [--filter BENCH]

Run small Criterion smoke passes with coarse timing artifacts. This is distinct
from full benchmark reports and is intended as a quick sanity check. The default
filter set covers file indexing, command-palette search/source construction,
workspace-wide content search, persistence, bounded transient editor loads,
cleanup/tree planning, and replace/undo workflows.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact-dir)
            [[ $# -lt 2 ]] && smoke_fail "--artifact-dir requires a value"
            ARTIFACT_DIR="$2"
            shift 2
            ;;
        --filter)
            [[ $# -lt 2 ]] && smoke_fail "--filter requires a value"
            FILTERS="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            smoke_fail "unknown argument: $1"
            ;;
    esac
done

smoke_require_command cargo

ARTIFACT_DIR="$(smoke_artifact_dir "$ARTIFACT_DIR")"
smoke_write_environment_report "$ARTIFACT_DIR/environment.txt"
{
    echo "filters=$FILTERS"
    echo "sample_size=$SAMPLE_SIZE"
    echo "warm_up_time=$WARM_UP_TIME"
    echo "measurement_time=$MEASUREMENT_TIME"
    echo "profile=bench"
    echo "cargo=$(cargo --version 2>/dev/null || true)"
    echo "rustc=$(rustc --version 2>/dev/null || true)"
    echo "host=$(rustc -vV 2>/dev/null | awk -F': ' '/^host:/ { print $2 }' || true)"
} >"$ARTIFACT_DIR/config.txt"

cat >"$ARTIFACT_DIR/fixtures.txt" <<'EOF'
file_index_search: generated command-palette indexes at representative file counts
palette_pipeline_hardening_100000: generated 100,000-file indexes with varied hit rates, Unicode names, repeated equal-score names, bounded/reference limits, cancellation, and rapid latest-query replacement
file_index_rebuild: generated common 10k-file workspace, 1k missing roots, 1k/10k directory-only forests, and a 10k long-path tree that approaches both file-index byte policies with direct build/installed high water
end_to_end_boundedness: generated one flat 10,000-entry directory, file/note source budget and cancellation fixtures, active/latest coordinator pressure, canonical top-one exclusion, a 2,048-row cleanup page, a 10,000-row middle reconciliation, and large replacement policy input
content_search_smoke: generated 200-file trees plus one 10k-line file
search_interactive_policies: generated 1,000-query latest-wins ownership, 10,000-result retirement counters, and a 260-event mixed non-match turn-budget proof
markdown_render_planning: generated 10,000-paragraph complete and dense single-block limited plans plus rapid detached-generation ownership pressure
save_admission_policy: generated eight compact ordinary save requests under the shared byte budget
editor_memory_policy: generated 1k/10k/100k scalar tab sets plus one-record incremental edit evidence
json_persistence: generated workspace/session JSON save and load fixtures
editor_file_io: generated text files for load, save, and Save As-equivalent explicit-path writes
transient_file_load: generated scalar admission bursts, stale queues, an exclusive near-limit request, Unicode slice planning, and one headless chunked-install responsiveness fixture
workspace_watch_pressure: generated duplicate/access-noise/deep Unicode event batches, varied producer/consumer rates, and cap-plus-one full-refresh promotion
quality_gap_scale: generated a 10,000-row Notes browser source, 10,000-row metadata-dominated note scoring with large bodies, a 4 MiB local-history preview, raw watcher ingress, a 10,000-row terminal cache rebuild, 10,000 rapid per-store scan requests, a 2.23 MiB sliced minimap analysis, and headless draft/session/pre-admitted-disposal/minimap/main-loop ownership fixtures
replace_preview_generation: generated 1k and 10k in-memory match sets plus a 10k-row half-checked worker-side selection handoff
replace_undo_workflows: generated disposable files for Replace All and undo restore, including an accepted 10 MiB short-line file with 10,000 spread replacements and direct construction ownership evidence
recovery_performance: generated malformed metadata, pending migration ledgers, duplicate bookmark sidecars, many local-history lineages, and first-dirty autosave persistence batches
EOF

cat >"$ARTIFACT_DIR/thresholds.txt" <<'EOF'
This smoke lane is a coarse regression tripwire, not the release benchmark gate.

Coarse smoke thresholds:
- first-window readiness: target under 5s on a developer workstation; investigate over 10s
- representative small/medium file open: target under 500ms; investigate over 2s
- command-palette searches: target interactive-scale, not multi-second
- command-palette bounded ranking: retained candidates must stay at or below the requested per-source limit, bounded results must equal the full-sort reference, and runtime ownership must stay at one active plus one pending latest query
- file-index construction: complete ownership must stay at or below 128 MiB, installed output at or below 64 MiB, and common, missing-root, directory-heavy, and near-policy long-path fixtures must return a complete or typed deterministic usable partial result
- end-to-end source construction: directory retention must stay within 100,000 rows, note admission within 10,000 entries and 64 MiB searchable text, deterministic note cancellation must stop at 256 admitted rows, and file/note coordinators must retain only one active plus one latest request
- cleanup/tree completion: directory pages must retain at most 2,048 rows, broad reconciliation plans must stay plain until GTK applies at most 256 changed rows per turn, and widget evidence must prove main-loop progress, supersession, disposal, and readiness completion
- workspace/content search: must complete every generated fixture without stalling
- interactive search policy: active worker groups must stay at one, pending queries at one, whole-result clones at zero, retirement turns at or below 250 rows/cache entries, and every received event variant must share the 250-event GTK-turn budget
- Markdown planning and disposal: retained events must stay at or below 256 per projection batch, dense oversized blocks must publish an explicit limited terminal, ordinary detached ownership must stay at two generations, and only one latest deferred render may survive pressure
- save admission: active payload weight must stay within the shared byte budget except for one explicit exclusive overweight request; queued requests retain scalar metadata only
- editor memory: ordinary below-threshold edits must touch one record and perform zero full scans
- Replace preview generation and selection: 10k generated matches and a 10k-row half-checked worker selection should stay sub-second on a developer workstation; preview selection must avoid whole-payload clones on GTK
- sliced buffer replacement: a synchronous first delete or insert signal may supersede the active generation without a borrow conflict; exactly one latest body remains, editability/saveability are restored, and projection suspension clears
- persistence, editor file I/O, Replace All, and undo restore: must complete every smoke sample successfully; dense-line construction must retain no more edit records than accepted replacements while reporting source lines, retained edit bytes, output bytes, and undo bytes directly
- transient_file_load: admitted payload weight must stay within the scalar shared budget except for one exclusive request; the headless Unicode fixture must make main-loop progress between slices and release its permit after final editor residency is published
- workspace_watch_pressure: retained unique paths must stay at or below 1,024, GTK consumption must stay at one bounded notice per poll, and cap overflow must promote to one conservative full refresh
- quality_gap_scale: draft repair must reach a complete multi-page inventory before cleanup authority and preserve every body across two startups; session restore must create at most four pages per GTK turn with two file-plan permits and one terminal projection publication; weighted disposal must reject capacity immediately, retain only compact retry requests, pre-admit document-sized GTK owners, prove their nested final destruction off GTK, and drain within two workers, eight reserved drop slots, and 128 MiB ordinary retained weight; minimap analysis must inspect at most 32,768 characters per GTK turn and reject stale generations; note scoring must retain at most 500 rows while optimized and final-query reference results agree; Notes query ownership must remain one active plus one latest, preview ownership must retain one accepted payload and install in 256 KiB UTF-8-safe slices, raw watcher ingress must stay capped, per-store scans must stay at one active plus one weak latest request with mirror capture only at admission, and terminal cache operations must stay at or below eight times old-plus-new rows
- recovery_performance: malformed metadata, pending migration, duplicate sidecar, local-history lineage, and first-dirty autosave fixtures must complete every smoke sample successfully; investigate multi-second recovery timings before shipping startup or close-flow reliability changes

Use make bench-report or make bench-report-full for enforceable release analysis.
EOF

cat >"$ARTIFACT_DIR/recovery-fixtures.txt" <<'EOF'
recovery_performance fixture report:
- malformed_metadata/startup_and_sidecar_diagnostics:
  fixture_count=15 malformed metadata files per iteration
  metadata_classes=session.json,drafts/manifest.json,migration-ledger.json,bookmark sidecars
  metadata_size=small bounded JSON fragments; 12 bookmark sidecars plus 3 top-level metadata files
  expected_quarantine_or_preservation_count=15
  expected_repaired_count=0
- pending_migrations/reconcile/10:
  fixture_count=10 ledger entries, 20 incomplete kind states
  metadata_size=one generated migration-ledger.json
  expected_completed_or_deferred_count=20
- pending_migrations/reconcile/100:
  fixture_count=100 ledger entries, 200 incomplete kind states
  metadata_size=one generated migration-ledger.json
  expected_completed_or_deferred_count=200
- duplicate_sidecars/bookmark_merge:
  fixture_count=2 bookmark sidecars for one old/new file identity pair
  metadata_size=two small bookmark JSON documents
  expected_merged_target_count=1
- local_history_many_lineages/move_tree/24:
  fixture_count=24 source lineages plus 6 duplicate target lineages
  metadata_size=30 small index/snapshot pairs
  expected_completed_or_deferred_count=24 source lineages
- local_history_many_lineages/reconcile_bounded/24:
  fixture_count=24 mismatched lineage directories
  metadata_size=24 small index/snapshot pairs
  expected_reconciled_count=12 lineages with deferred_work=true
- local_history_many_lineages/move_tree/120:
  fixture_count=120 source lineages plus 30 duplicate target lineages
  metadata_size=150 small index/snapshot pairs
  expected_completed_or_deferred_count=120 source lineages
- local_history_many_lineages/reconcile_bounded/120:
  fixture_count=120 mismatched lineage directories
  metadata_size=120 small index/snapshot pairs
  expected_reconciled_count=60 lineages with deferred_work=true
- first_dirty_autosave/persist_manifest_batch:
  fixture_count=20 draft files
  metadata_size=20 drafts at 4 KiB each plus one generated manifest
  expected_saved_manifest_entries=20
EOF

cd "$REPO_ROOT"

: >"$ARTIFACT_DIR/summary.txt"
for filter in $FILTERS; do
    safe_filter="$(printf '%s' "$filter" | tr -c '[:alnum:]_.-' '_')"
    log_path="$ARTIFACT_DIR/criterion-$safe_filter.log"
    echo "Running performance smoke filter '$filter'..."
    if ! cargo bench -p lushtext-core --bench benchmarks -- "$filter" \
        --sample-size "$SAMPLE_SIZE" \
        --warm-up-time "$WARM_UP_TIME" \
        --measurement-time "$MEASUREMENT_TIME" \
        >"$log_path" 2>&1; then
        tail -n 120 "$log_path" >&2 || true
        smoke_fail "performance smoke failed for filter '$filter'. Artifacts: $ARTIFACT_DIR"
    fi
    {
        echo "## $filter"
        grep -E "^(Benchmarking|Analyzing|[[:space:]]*time:)|[[:alnum:]-]+-evidence" "$log_path" || true
        echo
    } >>"$ARTIFACT_DIR/summary.txt"
done

case " $FILTERS " in
    *" transient_file_load "*)
        widget_log="$ARTIFACT_DIR/widget-transient-file-load.log"
        echo "Running headless transient file-load responsiveness proof..."
        if ! scripts/run-widget-tests.sh --headless -- \
            editor_page::test_large_unicode_load_installs_in_exact_bounded_slices \
            >"$widget_log" 2>&1; then
            tail -n 120 "$widget_log" >&2 || true
            smoke_fail "headless transient file-load proof failed. Artifacts: $ARTIFACT_DIR"
        fi
        {
            echo "## transient_file_load_headless"
            grep -E "transient-load-runtime-evidence|test result:" "$widget_log" || true
            echo
        } >>"$ARTIFACT_DIR/summary.txt"
        ;;
esac

case " $FILTERS " in
    *" quality_gap_scale "*)
        widget_log="$ARTIFACT_DIR/widget-quality-gap-scale.log"
        : >"$widget_log"
        echo "Running headless quality-gap responsiveness and ownership proofs..."
        for widget_filter in \
            window::test_notes_browser_caps_large_result_sets_with_refine_notice \
            window::test_local_history_preview_supersedes_reads_and_unicode_install_slices \
            window::test_bounded_session_restore_preserves_order_selection_and_one_terminal_projection \
            window::test_session_restore_cancellation_clears_pending_permits_source_and_projection_deferral \
            workspace_section::test_workspace_scan_admission_bounds_multiple_sections_and_keeps_gtk_live \
            workspace_section::test_slow_directory_refresh_churn_keeps_one_active_and_one_weak_latest_request \
            workspace_section::test_large_reconciliation_is_batched_supersedable_and_preserves_state \
            plain_disposal::test_aggregate_disposal_pressure_returns_immediately_and_keeps_gtk_alive \
            editor_page::test_minimap_long_line_warning_scan_slices_large_many_short_buffer \
            editor_page::test_minimap_mid_scan_edit_cancels_stale_generation_and_publishes_latest
        do
            if ! scripts/run-widget-tests.sh --headless -- "$widget_filter" \
                >>"$widget_log" 2>&1; then
                tail -n 160 "$widget_log" >&2 || true
                smoke_fail "headless quality-gap proof failed for '$widget_filter'. Artifacts: $ARTIFACT_DIR"
            fi
        done
        {
            echo "## quality_gap_scale_headless"
            grep -E "notes-browser-runtime-evidence|local-history-preview-runtime-evidence|session-restore-(bound|cancellation)-evidence|workspace-scan-(aggregate|flight)-evidence|workspace-cache-runtime-evidence|plain-disposal-pressure-evidence|minimap-(analysis|cancellation)-evidence|test result:" "$widget_log" || true
            echo
        } >>"$ARTIFACT_DIR/summary.txt"

        draft_log="$ARTIFACT_DIR/integration-draft-repair-closeout.log"
        echo "Running two-startup multi-page draft-repair survival proof..."
        if ! cargo test -p lushtext --test integration \
            draft::multi_start_manifest_repair_preserves_every_body_through_all_cleanup_pages \
            -- --nocapture >"$draft_log" 2>&1; then
            tail -n 160 "$draft_log" >&2 || true
            smoke_fail "draft-repair closeout proof failed. Artifacts: $ARTIFACT_DIR"
        fi
        {
            echo "## draft_repair_closeout"
            grep -E "draft-repair-closeout-evidence|test result:" "$draft_log" || true
            echo
        } >>"$ARTIFACT_DIR/summary.txt"
        ;;
esac

case " $FILTERS " in
    *" search_interactive_policies "*)
        unit_log="$ARTIFACT_DIR/unit-search-event-budget.log"
        echo "Running exact mixed search-event turn-budget proof..."
        if ! cargo test -p lushtext-core ui::search_panel::runtime::tests::mixed_non_match_events_share_one_budget --lib -- --nocapture \
            >"$unit_log" 2>&1; then
            tail -n 120 "$unit_log" >&2 || true
            smoke_fail "mixed search-event budget proof failed. Artifacts: $ARTIFACT_DIR"
        fi
        {
            echo "## search_event_budget"
            grep -E "search-event-budget-evidence|test result:" "$unit_log" || true
            echo
        } >>"$ARTIFACT_DIR/summary.txt"
        ;;
esac

case " $FILTERS " in
    *" markdown_render_planning "*)
        widget_log="$ARTIFACT_DIR/widget-markdown-retirement-pressure.log"
        echo "Running headless Markdown retirement-pressure proof..."
        if ! scripts/run-widget-tests.sh --headless -- \
            markdown_preview::test_rapid_rerenders_cap_detached_generations_and_keep_latest_work \
            --nocapture >"$widget_log" 2>&1; then
            tail -n 120 "$widget_log" >&2 || true
            smoke_fail "Markdown retirement-pressure proof failed. Artifacts: $ARTIFACT_DIR"
        fi
        {
            echo "## markdown_retirement_pressure"
            grep -E "markdown-retirement-bound-evidence|test result:" "$widget_log" || true
            echo
        } >>"$ARTIFACT_DIR/summary.txt"
        ;;
esac

case " $FILTERS " in
    *" end_to_end_boundedness "*)
        widget_log="$ARTIFACT_DIR/widget-buffer-reentrant-replacement.log"
        echo "Running headless reentrant buffer-replacement proofs..."
        if ! scripts/run-widget-tests.sh --headless -- synchronous_ --nocapture \
            >"$widget_log" 2>&1; then
            tail -n 120 "$widget_log" >&2 || true
            smoke_fail "reentrant buffer-replacement proof failed. Artifacts: $ARTIFACT_DIR"
        fi
        {
            echo "## buffer_replacement_reentrancy"
            grep -E "buffer-replacement-reentrant-evidence|test result:" "$widget_log" || true
            echo
        } >>"$ARTIFACT_DIR/summary.txt"
        ;;
esac

echo "PASS: performance smoke completed for filters '$FILTERS'. Artifacts: $ARTIFACT_DIR"
