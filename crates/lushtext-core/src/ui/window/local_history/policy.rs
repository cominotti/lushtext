// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure decisions for the local-history workflow.
//!
//! Both stage orders' policy in one place, with no GTK anywhere: which snapshots
//! a user should be shown, how a preview body is installed, whether a capture is
//! still worth persisting, and the viewer geometry that keeps the browser
//! readable. The capture half's freshness tickets live here too, even though the
//! coordination that uses them is in `ui/editor_page/local_history.rs` — a
//! workflow owns one `policy.rs`, and this is it.
//!
//! Sizing and paragraph-boundary arithmetic is **called, not copied**:
//! `crate::model::buffer_replacement` owns it, and forking a shared limit would
//! let it drift while both copies still read as correct.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

use std::path::PathBuf;

use crate::model::buffer_replacement::{
    REPLACEMENT_INSERT_SLICE_BYTES, SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES,
};
use crate::model::local_history::{LocalHistorySnapshotMeta, LocalHistorySnapshotOrigin};
use crate::services::local_history_service::LocalHistoryAvailability;

// --- viewer geometry -------------------------------------------------------

/// Leave a visible gutter around the local-history viewer so it still reads as
/// a parent-owned secondary surface instead of another primary window.
pub(super) const VIEWER_PARENT_MARGIN_SP: i32 = 48;
/// Wide local-history browsing should use most of the parent width, in
/// thousandths of it.
pub(super) const VIEWER_WIDTH_PERMILLE: i32 = 900;
/// Wide local-history browsing should use most of the parent height, in
/// thousandths of it.
pub(super) const VIEWER_HEIGHT_PERMILLE: i32 = 880;
/// Wide local-history browsing should stay comfortably readable on desktops.
pub(super) const VIEWER_MIN_WIDTH_SP: i32 = 1080;
/// Wide local-history browsing should stop growing once it already feels like a viewer.
pub(super) const VIEWER_MAX_WIDTH_SP: i32 = 1680;
/// Wide local-history browsing should keep enough height for reading snapshot text.
pub(super) const VIEWER_MIN_HEIGHT_SP: i32 = 720;
/// Wide local-history browsing should stop growing once the preview has ample height.
pub(super) const VIEWER_MAX_HEIGHT_SP: i32 = 1080;
/// The snapshot list should stay readable without competing evenly with the preview.
pub(super) const VIEWER_MIN_SIDEBAR_WIDTH_SP: f64 = 260.0;
/// The snapshot list should behave like a browse rail, not a co-equal pane.
pub(super) const VIEWER_MAX_SIDEBAR_WIDTH_SP: f64 = 340.0;
/// Compact empty-history width mirrors the Notes empty browser so status pages
/// have a readable line length instead of collapsing to their natural text size.
pub(super) const EMPTY_WIDTH_SP: i32 = 640;
/// Compact empty-history height fits the normal status-page icon, title, and
/// description without introducing a scrollbar.
pub(super) const EMPTY_HEIGHT_SP: i32 = 480;

/// Maximum preview body installed synchronously in one GTK turn.
pub(super) const PREVIEW_DIRECT_THRESHOLD_BYTES: usize = SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES;
/// Maximum UTF-8 bytes inserted by one scheduled preview slice.
pub(super) const PREVIEW_INSTALL_SLICE_BYTES: usize = REPLACEMENT_INSERT_SLICE_BYTES;
/// Conservative future worker-drop ownership for a browseable history body.
pub(super) const PREVIEW_RESERVATION_BYTES: u64 = 64 * 1024 * 1024;

/// Clamp one dialog axis so the viewer uses most of the parent window without
/// outgrowing it on either small or large desktops.
///
/// Whole pixels throughout: the share is `target_permille` thousandths of the
/// parent axis, rounded half up.
#[must_use]
pub(super) fn parent_relative_dialog_axis_size(
    parent_axis: i32,
    target_permille: i32,
    min_axis: i32,
    max_axis: i32,
) -> i32 {
    let parent_axis = parent_axis.max(1);
    let bounded_parent = parent_axis.saturating_sub(VIEWER_PARENT_MARGIN_SP).max(1);
    let proportional = (i64::from(parent_axis) * i64::from(target_permille) + 500).div_euclid(1000);
    let proportional = i32::try_from(proportional).unwrap_or(i32::MAX);
    proportional.clamp(min_axis, max_axis).min(bounded_parent)
}

/// Resolve one current-vs-default axis without forcing callers to repeat the
/// same width/height fallback logic.
///
/// The second guard is `> 0`, not `> 1`. Both produce identical results for every
/// input — a default of exactly 1 yields 1 either way — but `> 1` makes the
/// comparison *equivalent under mutation*, so no test can distinguish it from
/// `>= 1` and a real off-by-one here would be invisible. `> 0` states the actual
/// question ("is this a usable size?") and is detectable.
#[must_use]
pub(super) const fn current_window_dimension(current_axis: i32, default_axis: i32) -> i32 {
    if current_axis > 0 {
        current_axis
    } else if default_axis > 0 {
        default_axis
    } else {
        1
    }
}

/// Size the populated local-history browser like a large viewer while keeping
/// the dialog visibly smaller than its parent window.
#[must_use]
pub(super) fn viewer_dialog_size(parent_width: i32, parent_height: i32) -> (i32, i32) {
    (
        parent_relative_dialog_axis_size(
            parent_width,
            VIEWER_WIDTH_PERMILLE,
            VIEWER_MIN_WIDTH_SP,
            VIEWER_MAX_WIDTH_SP,
        ),
        parent_relative_dialog_axis_size(
            parent_height,
            VIEWER_HEIGHT_PERMILLE,
            VIEWER_MIN_HEIGHT_SP,
            VIEWER_MAX_HEIGHT_SP,
        ),
    )
}

// --- which snapshots the user sees -----------------------------------------

/// Whether one snapshot is a legacy empty baseline row.
#[must_use]
pub(super) fn is_empty_baseline_snapshot(meta: &LocalHistorySnapshotMeta) -> bool {
    meta.origin == LocalHistorySnapshotOrigin::Baseline && meta.byte_len == 0
}

/// Whether a legacy empty baseline should be hidden from the browser.
///
/// Deliberately conservative: it hides only when there is *evidence of the old
/// bug* — at least two empty baselines alongside at least two real periodic
/// snapshots. One empty baseline may be genuine, and a lineage with no real
/// content must still show something rather than an empty list.
#[must_use]
pub(super) fn should_hide_legacy_empty_baseline(
    meta: &LocalHistorySnapshotMeta,
    empty_baseline_count: usize,
    non_empty_periodic_count: usize,
) -> bool {
    is_empty_baseline_snapshot(meta) && empty_baseline_count >= 2 && non_empty_periodic_count >= 2
}

/// Hide legacy empty baseline rows that were repeatedly created by the older
/// draft-restore workflow while leaving the stored history untouched on disk.
#[must_use]
pub(super) fn filter_visible_snapshots(
    snapshots: Vec<LocalHistorySnapshotMeta>,
) -> Vec<LocalHistorySnapshotMeta> {
    let empty_baseline_count = snapshots
        .iter()
        .filter(|meta| is_empty_baseline_snapshot(meta))
        .count();
    let non_empty_periodic_count = snapshots
        .iter()
        .filter(|meta| meta.origin == LocalHistorySnapshotOrigin::Periodic && meta.byte_len > 0)
        .count();

    snapshots
        .into_iter()
        .filter(|meta| {
            !should_hide_legacy_empty_baseline(meta, empty_baseline_count, non_empty_periodic_count)
        })
        .collect()
}

// --- how a preview body is installed ---------------------------------------

/// How the browser installs one loaded snapshot body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PreviewInstallPlan {
    /// The snapshot held no text; show the empty-snapshot status page.
    Empty,
    /// Small enough to set in one GTK turn.
    Direct,
    /// Install through bounded paragraph-aligned slices.
    Sliced,
}

/// Decide how a loaded preview body reaches the read-only text view.
///
/// The empty case is separated from the small case because it is a different
/// *user-visible* state, not just a faster path: an empty snapshot gets an
/// explanatory status page, and its Copy action stays disabled while Restore
/// does not.
#[must_use]
pub(super) const fn preview_install_plan(body_len: usize) -> PreviewInstallPlan {
    if body_len == 0 {
        PreviewInstallPlan::Empty
    } else if body_len <= PREVIEW_DIRECT_THRESHOLD_BYTES {
        PreviewInstallPlan::Direct
    } else {
        PreviewInstallPlan::Sliced
    }
}

/// Whether a sliced preview install has reached the end of its body.
#[must_use]
pub(super) const fn preview_install_is_complete(offset: usize, body_len: usize) -> bool {
    offset >= body_len
}

// --- capture freshness -----------------------------------------------------

/// Identity captured before a baseline snapshot's worker starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BaselineCaptureTicket {
    pub(crate) editor_generation: u64,
    pub(crate) path_generation: u64,
    pub(crate) clean_baseline_generation: u64,
    pub(crate) path: PathBuf,
}

/// Live editor state observed when a baseline capture reports back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BaselineCaptureFacts {
    pub(crate) editor_generation: u64,
    pub(crate) path_generation: u64,
    pub(crate) clean_baseline_generation: u64,
    pub(crate) path: Option<PathBuf>,
    pub(crate) modified: bool,
    pub(crate) baseline_slot_empty: bool,
}

/// Whether a failed baseline's text may be returned to its original cycle.
///
/// `baseline_slot_empty` is the subtle one: a newer clean baseline may already
/// have filled the slot, and overwriting it with the failed older text would
/// hand a later capture the wrong "last clean" content.
#[must_use]
pub(crate) fn baseline_capture_is_current(
    ticket: &BaselineCaptureTicket,
    facts: &BaselineCaptureFacts,
) -> bool {
    facts.editor_generation == ticket.editor_generation
        && facts.path_generation == ticket.path_generation
        && facts.clean_baseline_generation == ticket.clean_baseline_generation
        && facts.path.as_ref() == Some(&ticket.path)
        && facts.modified
        && facts.baseline_slot_empty
}

/// Identity captured before a periodic snapshot's worker starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PeriodicCaptureTicket {
    pub(crate) editor_generation: u64,
    pub(crate) path_generation: u64,
    pub(crate) periodic_generation: u32,
    pub(crate) edit_generation: u64,
    pub(crate) path: PathBuf,
}

/// Live editor state observed when a periodic capture is about to persist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PeriodicCaptureFacts {
    pub(crate) editor_generation: u64,
    pub(crate) path_generation: u64,
    pub(crate) periodic_generation: u32,
    pub(crate) edit_generation: u64,
    pub(crate) path: Option<PathBuf>,
    pub(crate) modified: bool,
    pub(crate) availability: LocalHistoryAvailability,
}

/// Whether a captured periodic snapshot still describes its editor.
#[must_use]
pub(crate) fn periodic_capture_is_current(
    ticket: &PeriodicCaptureTicket,
    facts: &PeriodicCaptureFacts,
) -> bool {
    facts.editor_generation == ticket.editor_generation
        && facts.path_generation == ticket.path_generation
        && facts.periodic_generation == ticket.periodic_generation
        && facts.edit_generation == ticket.edit_generation
        && facts.path.as_ref() == Some(&ticket.path)
        && facts.modified
        && facts.availability.allows_automatic_capture()
}

/// Whether a finished capture should arm the next periodic timer.
///
/// The suppressed check is what stops a save from re-arming the timer it just
/// cancelled, and the path check stops an untitled tab from spinning one.
#[must_use]
pub(crate) const fn should_reschedule_periodic_capture(
    modified: bool,
    has_path: bool,
    automatic_capture_suppressed: bool,
) -> bool {
    modified && has_path && !automatic_capture_suppressed
}

// --- presentation ----------------------------------------------------------

/// Format one snapshot's origin and size for a row subtitle.
#[must_use]
pub(super) fn format_snapshot_meta(origin: LocalHistorySnapshotOrigin, byte_len: u64) -> String {
    if byte_len == 0 {
        format!("{} · Empty file", origin.label())
    } else {
        format!("{} · {}", origin.label(), format_bytes(byte_len))
    }
}

/// Format a byte count for a browse row.
///
/// One decimal place of a KiB or MiB, rounded half to even exactly as `{:.1}`
/// rounds the exact quotient, formed in integer tenths so no float is involved.
/// Identical to the `f64` form it replaced (`{:.1}` of `byte_len as f64 / unit`)
/// for every size below 2^53 bytes (8 PiB); above that the old form rounded the
/// size itself to `f64` first, and this one stays exact.
#[must_use]
pub(super) fn format_bytes(byte_len: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;

    if byte_len >= MIB {
        format!("{} MB", one_decimal_place(byte_len, MIB))
    } else if byte_len >= KIB {
        format!("{} KB", one_decimal_place(byte_len, KIB))
    } else {
        format!("{byte_len} B")
    }
}

/// `byte_len / unit` to one decimal place, ties to even.
fn one_decimal_place(byte_len: u64, unit: u64) -> String {
    let unit = u128::from(unit);
    let scaled = u128::from(byte_len) * 10;
    let tenths = scaled / unit;
    let twice_remainder = (scaled % unit) * 2;
    let tenths = if twice_remainder > unit || (twice_remainder == unit && tenths % 2 == 1) {
        tenths + 1
    } else {
        tenths
    };
    format!("{}.{}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(origin: LocalHistorySnapshotOrigin, byte_len: u64) -> LocalHistorySnapshotMeta {
        LocalHistorySnapshotMeta {
            snapshot_id: format!("{origin:?}-{byte_len}"),
            origin,
            byte_len,
            captured_at_millis: 0,
            content_hash: String::new(),
        }
    }

    #[test]
    fn viewer_size_stays_inside_its_parent_and_its_readable_band() {
        // A small parent is bounded by the gutter, not by the readable minimum.
        let (width, height) = viewer_dialog_size(800, 600);
        assert!(width <= 800 - VIEWER_PARENT_MARGIN_SP);
        assert!(height <= 600 - VIEWER_PARENT_MARGIN_SP);

        // A large parent is bounded by the maximum, so the viewer stops growing.
        let (width, height) = viewer_dialog_size(6000, 4000);
        assert_eq!(width, VIEWER_MAX_WIDTH_SP);
        assert_eq!(height, VIEWER_MAX_HEIGHT_SP);

        // A degenerate parent must still produce a positive size.
        let (width, height) = viewer_dialog_size(0, -5);
        assert!(width >= 1);
        assert!(height >= 1);
    }

    #[test]
    fn window_dimension_falls_back_only_when_unmapped() {
        assert_eq!(current_window_dimension(1280, 900), 1280);
        assert_eq!(current_window_dimension(0, 900), 900);
        assert_eq!(current_window_dimension(-1, 900), 900);
        assert_eq!(current_window_dimension(0, 0), 1);
        // The default's own guard, not just the current axis's: a `>=` here would
        // let a degenerate default of 1 be returned as itself, which is the same
        // unusable size the final arm exists to replace.
        assert_eq!(current_window_dimension(0, 1), 1);
        assert_eq!(current_window_dimension(0, 2), 2);
        assert_eq!(current_window_dimension(0, -5), 1);
        // A current axis of exactly 1 is a real mapped size, not a fallback.
        assert_eq!(current_window_dimension(1, 900), 1);
    }

    #[test]
    fn viewer_size_is_actually_proportional_between_its_clamps() {
        // The extremes are covered above, but they hold for *any* fraction. This
        // is the case where the fraction itself decides, so it is what pins the
        // 0.9 / 0.88 policy rather than the clamps.
        let parent = 1600;
        let (width, _) = viewer_dialog_size(parent, 4000);
        assert_eq!(width, 1440, "90% of 1600, inside both clamps");

        let parent = 1000;
        let (_, height) = viewer_dialog_size(6000, parent);
        // 88% of 1000 is 880, above the 720 minimum, and the gutter bound is 952.
        assert_eq!(height, 880);

        // And the raw axis helper, so a fraction change cannot hide behind a clamp.
        assert_eq!(parent_relative_dialog_axis_size(1000, 500, 1, 100_000), 500);
        assert_eq!(parent_relative_dialog_axis_size(1000, 250, 1, 100_000), 250);
        // Half up: 90% of 1005 is 904.5.
        assert_eq!(parent_relative_dialog_axis_size(1005, 900, 1, 100_000), 905);
    }

    #[test]
    fn preview_reservation_is_the_documented_conservative_ceiling() {
        // Pinned concretely: every other assertion compares the reservation
        // against itself, so the value could change without a test noticing. It
        // is the disposal weight a browseable snapshot body is charged before its
        // real size is known, so it must comfortably exceed the largest snapshot
        // the browser will load.
        assert_eq!(PREVIEW_RESERVATION_BYTES, 64 * 1024 * 1024);
        assert_eq!(PREVIEW_RESERVATION_BYTES, 0x0400_0000);
        // And the two install bounds are the cross-cutting values, not copies.
        assert_eq!(
            PREVIEW_DIRECT_THRESHOLD_BYTES,
            SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES
        );
        assert_eq!(PREVIEW_INSTALL_SLICE_BYTES, REPLACEMENT_INSERT_SLICE_BYTES);
    }

    #[test]
    fn legacy_empty_baselines_hide_only_with_evidence_of_the_old_bug() {
        let empty_baseline = meta(LocalHistorySnapshotOrigin::Baseline, 0);

        // Both thresholds must be met.
        assert!(should_hide_legacy_empty_baseline(&empty_baseline, 2, 2));
        assert!(!should_hide_legacy_empty_baseline(&empty_baseline, 1, 2));
        assert!(!should_hide_legacy_empty_baseline(&empty_baseline, 2, 1));

        // A non-empty baseline and a periodic snapshot are never hidden.
        assert!(!should_hide_legacy_empty_baseline(
            &meta(LocalHistorySnapshotOrigin::Baseline, 10),
            9,
            9
        ));
        assert!(!should_hide_legacy_empty_baseline(
            &meta(LocalHistorySnapshotOrigin::Periodic, 0),
            9,
            9
        ));
    }

    #[test]
    fn the_periodic_count_requires_a_periodic_snapshot_with_real_content() {
        // Pins both halves of the counting predicate. Two *empty* periodic
        // snapshots are not evidence of the old bug, so they must not license
        // hiding the empty baselines: an `||` here would count any periodic
        // snapshot, and a `>=` would count a zero-length one.
        let snapshots = vec![
            meta(LocalHistorySnapshotOrigin::Baseline, 0),
            meta(LocalHistorySnapshotOrigin::Baseline, 0),
            meta(LocalHistorySnapshotOrigin::Periodic, 0),
            meta(LocalHistorySnapshotOrigin::Periodic, 0),
        ];
        assert_eq!(
            filter_visible_snapshots(snapshots).len(),
            4,
            "empty periodic snapshots are not evidence of the legacy bug"
        );

        // Two non-empty baselines are also not periodic snapshots, so they do not
        // license hiding either — the `origin` half of the predicate.
        let snapshots = vec![
            meta(LocalHistorySnapshotOrigin::Baseline, 0),
            meta(LocalHistorySnapshotOrigin::Baseline, 0),
            meta(LocalHistorySnapshotOrigin::Baseline, 10),
            meta(LocalHistorySnapshotOrigin::Baseline, 20),
        ];
        assert_eq!(filter_visible_snapshots(snapshots).len(), 4);

        // And one byte of periodic content on each is enough, which is the
        // boundary `>` versus `>=` moves.
        let snapshots = vec![
            meta(LocalHistorySnapshotOrigin::Baseline, 0),
            meta(LocalHistorySnapshotOrigin::Baseline, 0),
            meta(LocalHistorySnapshotOrigin::Periodic, 1),
            meta(LocalHistorySnapshotOrigin::Periodic, 1),
        ];
        assert_eq!(filter_visible_snapshots(snapshots).len(), 2);
    }

    #[test]
    fn filtering_keeps_a_lineage_that_has_nothing_else_to_show() {
        // Two empty baselines but only one real periodic: nothing is hidden,
        // because an empty browser is worse than a redundant row.
        let snapshots = vec![
            meta(LocalHistorySnapshotOrigin::Baseline, 0),
            meta(LocalHistorySnapshotOrigin::Baseline, 0),
            meta(LocalHistorySnapshotOrigin::Periodic, 12),
        ];
        assert_eq!(filter_visible_snapshots(snapshots).len(), 3);

        let snapshots = vec![
            meta(LocalHistorySnapshotOrigin::Baseline, 0),
            meta(LocalHistorySnapshotOrigin::Baseline, 0),
            meta(LocalHistorySnapshotOrigin::Periodic, 12),
            meta(LocalHistorySnapshotOrigin::Periodic, 34),
        ];
        let visible = filter_visible_snapshots(snapshots);
        assert_eq!(visible.len(), 2);
        assert!(visible.iter().all(|meta| meta.byte_len > 0));
    }

    #[test]
    fn preview_install_separates_empty_from_merely_small() {
        assert_eq!(preview_install_plan(0), PreviewInstallPlan::Empty);
        assert_eq!(preview_install_plan(1), PreviewInstallPlan::Direct);
        assert_eq!(
            preview_install_plan(PREVIEW_DIRECT_THRESHOLD_BYTES),
            PreviewInstallPlan::Direct
        );
        assert_eq!(
            preview_install_plan(PREVIEW_DIRECT_THRESHOLD_BYTES + 1),
            PreviewInstallPlan::Sliced
        );
    }

    #[test]
    fn preview_install_completes_at_or_past_the_body_end() {
        assert!(!preview_install_is_complete(0, 10));
        assert!(!preview_install_is_complete(9, 10));
        assert!(preview_install_is_complete(10, 10));
        assert!(preview_install_is_complete(0, 0));
    }

    #[test]
    fn snapshot_meta_names_an_empty_file_rather_than_zero_bytes() {
        assert_eq!(
            format_snapshot_meta(LocalHistorySnapshotOrigin::Baseline, 0),
            format!(
                "{} · Empty file",
                LocalHistorySnapshotOrigin::Baseline.label()
            )
        );
        assert!(
            format_snapshot_meta(LocalHistorySnapshotOrigin::Periodic, 2048).contains("2.0 KB")
        );
    }

    #[test]
    fn byte_formatting_switches_units_at_the_binary_boundaries() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1024 * 1024 - 1), "1024.0 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn byte_formatting_rounds_ties_to_even_like_the_float_form() {
        // 1280 B is exactly 1.25 KiB: `{:.1}` gave "1.2", so must this.
        assert_eq!(format_bytes(1280), "1.2 KB");
        // 1.35 KiB is not a tie (1382.4 B); 1.375 KiB = 1408 B rounds up.
        assert_eq!(format_bytes(1408), "1.4 KB");
        // 1.75 KiB = 1792 B is a tie that rounds up to the even 1.8.
        assert_eq!(format_bytes(1792), "1.8 KB");
        // 1.25 MiB is a MiB tie.
        assert_eq!(format_bytes(1_310_720), "1.2 MB");
        assert_eq!(format_bytes(u64::MAX), "17592186044416.0 MB");
    }

    /// The exact value the replaced `f64` form formatted, formed without float
    /// arithmetic: `byte_len / 2^shift` is a dyadic rational, so its decimal
    /// expansion (`remainder * 5^shift` over `10^shift`) is finite, and parsing it
    /// yields the very `f64` that `byte_len as f64 / 2^shift as f64` produced for
    /// every `byte_len` below 2^53. Formatting that value with the same `{:.1}`
    /// reproduces the replaced implementation.
    fn legacy_one_decimal_place(byte_len: u64, shift: u32, digits: usize) -> String {
        let whole = byte_len >> shift;
        let remainder = u128::from(byte_len & ((1u64 << shift) - 1));
        let fraction = remainder * 5u128.pow(shift);
        let exact: f64 = format!("{whole}.{fraction:0>digits$}")
            .parse()
            .unwrap_or_else(|_| unreachable!("a decimal expansion always parses"));
        format!("{exact:.1}")
    }

    fn legacy_format_bytes(byte_len: u64) -> String {
        if byte_len >= 1 << 20 {
            format!("{} MB", legacy_one_decimal_place(byte_len, 20, 20))
        } else if byte_len >= 1 << 10 {
            format!("{} KB", legacy_one_decimal_place(byte_len, 10, 10))
        } else {
            format!("{byte_len} B")
        }
    }

    #[test]
    fn integer_byte_formatting_matches_the_float_form_densely() {
        // Every size up to 2 MiB, which covers each KiB tie, then the first
        // 65,536 MiB ties (a tenth-of-a-MiB tie is an odd multiple of 256 KiB)
        // and their neighbours.
        for byte_len in 0..=2 * 1024 * 1024 {
            assert_eq!(
                format_bytes(byte_len),
                legacy_format_bytes(byte_len),
                "{byte_len} B"
            );
        }
        for tie in ((1u64 << 18)..).step_by(1 << 19).take(1 << 16) {
            for byte_len in [tie - 1, tie, tie + 1] {
                assert_eq!(
                    format_bytes(byte_len),
                    legacy_format_bytes(byte_len),
                    "{byte_len} B"
                );
            }
        }
    }

    proptest::proptest! {
        #[test]
        fn integer_byte_formatting_matches_the_float_form_below_2_53(
            byte_len in 0u64..(1u64 << 53),
        ) {
            proptest::prop_assert_eq!(format_bytes(byte_len), legacy_format_bytes(byte_len));
        }
    }

    fn periodic_ticket() -> PeriodicCaptureTicket {
        PeriodicCaptureTicket {
            editor_generation: 3,
            path_generation: 5,
            periodic_generation: 7,
            edit_generation: 11,
            path: PathBuf::from("/workspace/current.md"),
        }
    }

    fn periodic_facts() -> PeriodicCaptureFacts {
        let ticket = periodic_ticket();
        PeriodicCaptureFacts {
            editor_generation: ticket.editor_generation,
            path_generation: ticket.path_generation,
            periodic_generation: ticket.periodic_generation,
            edit_generation: ticket.edit_generation,
            path: Some(ticket.path),
            modified: true,
            availability: LocalHistoryAvailability::Full,
        }
    }

    #[test]
    fn periodic_capture_accepts_only_the_unchanged_live_editor() {
        assert!(periodic_capture_is_current(
            &periodic_ticket(),
            &periodic_facts()
        ));
    }

    #[test]
    fn periodic_capture_rejects_close_edit_timer_and_identity_changes() {
        let ticket = periodic_ticket();
        for mutate in [
            |facts: &mut PeriodicCaptureFacts| facts.editor_generation += 1,
            |facts: &mut PeriodicCaptureFacts| facts.path_generation += 1,
            |facts: &mut PeriodicCaptureFacts| facts.periodic_generation += 1,
            |facts: &mut PeriodicCaptureFacts| facts.edit_generation += 1,
        ] {
            let mut changed = periodic_facts();
            mutate(&mut changed);
            assert!(!periodic_capture_is_current(&ticket, &changed));
        }

        let mut renamed = periodic_facts();
        renamed.path = Some(PathBuf::from("/workspace/renamed.md"));
        assert!(!periodic_capture_is_current(&ticket, &renamed));

        let mut save_as = periodic_facts();
        save_as.path = Some(PathBuf::from("/elsewhere/saved-as.md"));
        assert!(!periodic_capture_is_current(&ticket, &save_as));
    }

    #[test]
    fn periodic_capture_rejects_clean_or_no_longer_full_history_state() {
        let ticket = periodic_ticket();
        let mut clean = periodic_facts();
        clean.modified = false;
        assert!(!periodic_capture_is_current(&ticket, &clean));

        for availability in [
            LocalHistoryAvailability::SaveOnly,
            LocalHistoryAvailability::Unavailable,
        ] {
            let mut limited = periodic_facts();
            limited.availability = availability;
            assert!(!periodic_capture_is_current(&ticket, &limited));
        }
    }

    #[test]
    fn modified_file_backed_editors_reschedule_without_tight_retry() {
        assert!(should_reschedule_periodic_capture(true, true, false));
        assert!(!should_reschedule_periodic_capture(false, true, false));
        assert!(!should_reschedule_periodic_capture(true, false, false));
        assert!(!should_reschedule_periodic_capture(true, true, true));
    }

    #[test]
    fn failed_baseline_returns_only_to_its_original_cycle() {
        let ticket = BaselineCaptureTicket {
            editor_generation: 3,
            path_generation: 5,
            clean_baseline_generation: 7,
            path: PathBuf::from("/workspace/current.md"),
        };
        let facts = BaselineCaptureFacts {
            editor_generation: 3,
            path_generation: 5,
            clean_baseline_generation: 7,
            path: Some(ticket.path.clone()),
            modified: true,
            baseline_slot_empty: true,
        };
        assert!(baseline_capture_is_current(&ticket, &facts));

        for mutate in [
            |facts: &mut BaselineCaptureFacts| facts.editor_generation += 1,
            |facts: &mut BaselineCaptureFacts| facts.path_generation += 1,
            |facts: &mut BaselineCaptureFacts| facts.clean_baseline_generation += 1,
        ] {
            let mut stale = facts.clone();
            mutate(&mut stale);
            assert!(!baseline_capture_is_current(&ticket, &stale));
        }

        let mut renamed = facts.clone();
        renamed.path = Some(PathBuf::from("/workspace/renamed.md"));
        assert!(!baseline_capture_is_current(&ticket, &renamed));

        let mut newer_baseline = facts;
        newer_baseline.baseline_slot_empty = false;
        assert!(!baseline_capture_is_current(&ticket, &newer_baseline));
    }
}
