// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the Protected View policy (design D12, harnesses H3–H4x;
//! shard `core-protected-view`, gate `scheduled`).
//!
//! - **H3** `no_mutating_action_is_admitted`: the action-admission table over
//!   the whole closed [`ProtectedAction`] enum, loop-free.
//! - **H3m** `protected_view_entry_is_monotone`: the session transition over
//!   every start generation up to `u64::MAX − 6` and every sequence of up to six
//!   events.
//! - **H3x**, **H3u** (`should_panic`): the H3m check over two stubs of the
//!   transition — eviction reload re-deriving like a reload from disk, and the
//!   entry undo accepted on a session entered on load — so the two guards it
//!   depends on are pinned.
//! - **H4n** `navigation_target_reaches_every_column`: every `u64` length and
//!   column at the production width `N_INSPECT`, loop-free (about 13 minutes;
//!   other widths are an exhaustive unit test). **H4** (the inspector tiling)
//!   is not a harness: CBMC did not finish it in 20 minutes at a symbolic or
//!   at the production width, because each check is a 64-bit division, so
//!   `policy.rs`'s property test `inspector_windows_tile_over_the_whole_range`
//!   and the exhaustive `inspector_windows_tile_for_every_small_width` carry
//!   it.
//! - **H4f** `find_batches_are_bounded_and_partition_results`: up to six
//!   admitted records, per-turn count caps 1–3, byte caps 1–6, any record bound
//!   up to the byte cap, any result cap.
//! - **H4x** (`should_panic`): one record built without [`admit_find_record`]
//!   and over the byte cap breaks the H4f byte bound.
//!
//! Every model is allocation-free (fixed arrays). Completeness of the action
//! table against the toolkit's bindings is the widget enumeration test's job
//! (task 4.7), not these harnesses'.
//!
//! Compiled only under `cfg(kani)` (`make kani`).

use super::{
    ActionDisposition, AdmittedRecord, FindBatchLimits, FindMatch, N_INSPECT_WIDTH,
    NavigationTarget, ParagraphExtent, ProtectedAction, ProtectedSession, SessionEvent,
    ToolkitEffect, WholeContentInstall, action_disposition, admit_find_record, navigation_target,
    next_find_batch, toolkit_effect,
};

// ---------------------------------------------------------------------------
// H3: action admission
// ---------------------------------------------------------------------------

/// The actions the spec names as mutating, written independently of
/// `toolkit_effect` (spec "No buffer-mutating action is admitted").
const NAMED_MUTATING: [ProtectedAction; 20] = [
    ProtectedAction::InsertAtCursor,
    ProtectedAction::DeleteFromCursor,
    ProtectedAction::Backspace,
    ProtectedAction::CutClipboard,
    ProtectedAction::PasteClipboard,
    ProtectedAction::MiddleClickPaste,
    ProtectedAction::MoveLines,
    ProtectedAction::MoveWords,
    ProtectedAction::JoinLines,
    ProtectedAction::ChangeCase,
    ProtectedAction::ChangeNumber,
    ProtectedAction::InsertEmoji,
    ProtectedAction::PushSnippet,
    ProtectedAction::TextUndo,
    ProtectedAction::TextRedo,
    ProtectedAction::DndDrop,
    ProtectedAction::SearchReplace,
    ProtectedAction::SearchReplaceAll,
    ProtectedAction::NormalizeLineEndings,
    ProtectedAction::ImCommit,
];

/// H3. No action whose default handler would mutate the buffer or move a mark
/// is admitted (classified `ViewportSafe`, the only admitting disposition);
/// only a viewport-only action is admitted; the entry undo is reached only from
/// `TextUndo` while it is available; and every independently named mutating
/// action is recorded as mutating and not admitted.
///
/// Domain: the whole closed enum, both entry-undo availabilities; loop-free
/// (the named list is indexed symbolically).
#[kani::proof]
fn no_mutating_action_is_admitted() {
    let action: ProtectedAction = kani::any();
    let entry_undo_available: bool = kani::any();
    let disposition = action_disposition(action, entry_undo_available);
    let effect = toolkit_effect(action);

    if effect.mutates_buffer() || effect.moves_mark() {
        assert!(disposition != ActionDisposition::ViewportSafe);
    }
    if disposition == ActionDisposition::ViewportSafe {
        assert!(effect == ToolkitEffect::ViewportOnly);
    }
    if disposition == ActionDisposition::RouteToEntryUndo {
        assert!(action == ProtectedAction::TextUndo && entry_undo_available);
    }
    kani::cover!(
        disposition == ActionDisposition::ViewportSafe,
        "viewport-safe"
    );
    kani::cover!(disposition == ActionDisposition::RouteToMenu, "menu");
    kani::cover!(
        disposition == ActionDisposition::RouteToEntryUndo,
        "entry undo"
    );
    kani::cover!(
        disposition == ActionDisposition::Preempt && effect.moves_mark(),
        "pre-empted mark move"
    );

    let index: usize = kani::any();
    kani::assume(index < NAMED_MUTATING.len());
    let named = NAMED_MUTATING[index];
    assert!(toolkit_effect(named) == ToolkitEffect::MutatesBuffer);
    assert!(action_disposition(named, entry_undo_available) != ActionDisposition::ViewportSafe);
}

// ---------------------------------------------------------------------------
// H3m, H3x, H3u: session monotonicity
// ---------------------------------------------------------------------------

const MAX_EVENTS: usize = 6;

/// The H3m property, over any transition `step`.
///
/// From a protected session (entered on load or by an edit) at any generation
/// up to `u64::MAX − 6`, apply up to six arbitrary events. Whenever the session
/// is protected before an event, it stays protected unless the event is a
/// reload from disk whose new decision is editable, or the entry undo at the
/// entry step of a session entered by an edit. The generation never decreases,
/// and advances on every whole-content install, every reload from disk and
/// every exit.
fn check_monotone(step: fn(ProtectedSession, SessionEvent) -> ProtectedSession) {
    let generation: u64 = kani::any();
    kani::assume(generation <= u64::MAX - MAX_EVENTS as u64);
    let mut session = ProtectedSession {
        protected: true,
        entered_by_edit: kani::any(),
        generation,
    };
    let count: usize = kani::any();
    kani::assume(count <= MAX_EVENTS);
    let mut index = 0;
    while index < count {
        let event: SessionEvent = kani::any();
        let next = step(session, event);
        assert!(next.generation >= session.generation);
        if matches!(
            event,
            SessionEvent::Installed { .. } | SessionEvent::ReloadFromDisk { .. }
        ) {
            assert!(next.generation > session.generation);
        }
        if session.protected && !next.protected {
            let allowed = matches!(event, SessionEvent::ReloadFromDisk { protected: false })
                || (matches!(
                    event,
                    SessionEvent::EntryUndo {
                        is_entry_step: true
                    }
                ) && session.entered_by_edit);
            assert!(allowed);
            assert!(next.generation > session.generation);
            kani::cover!(
                matches!(event, SessionEvent::ReloadFromDisk { .. }),
                "exit by reload"
            );
            kani::cover!(
                matches!(event, SessionEvent::EntryUndo { .. }),
                "exit by entry undo"
            );
        }
        kani::cover!(
            !session.protected && next.protected && next.entered_by_edit,
            "re-entry by edit growth"
        );
        kani::cover!(
            session.protected
                && matches!(
                    event,
                    SessionEvent::Installed {
                        kind: WholeContentInstall::EvictionReload,
                        protected: false
                    }
                ),
            "eviction reload of a protected session"
        );
        session = next;
        index += 1;
    }
}

/// H3m. Protected View entry is monotone for the session.
///
/// Domain: start generation ≤ `u64::MAX − 6`, either entry kind, up to six
/// arbitrary events (`#[kani::unwind(7)]`).
#[kani::proof]
#[kani::unwind(7)]
fn protected_view_entry_is_monotone() {
    check_monotone(ProtectedSession::on_event);
}

/// The transition with eviction reload re-deriving like a reload from disk.
fn eviction_reload_re_derives(session: ProtectedSession, event: SessionEvent) -> ProtectedSession {
    match event {
        SessionEvent::Installed {
            kind: WholeContentInstall::EvictionReload,
            protected,
        } => session.on_event(SessionEvent::ReloadFromDisk { protected }),
        _ => session.on_event(event),
    }
}

/// H3x. Re-deriving on eviction reload breaks monotonicity: Kani finds an
/// eviction reload of an editable-looking body that leaves Protected View
/// without a reload from disk.
#[kani::proof]
#[kani::unwind(7)]
#[kani::should_panic]
fn entry_is_not_monotone_if_eviction_reload_re_derives() {
    check_monotone(eviction_reload_re_derives);
}

/// The transition with the `entered_by_edit` guard removed from the entry undo.
fn entry_undo_ignores_the_entry_kind(
    session: ProtectedSession,
    event: SessionEvent,
) -> ProtectedSession {
    match event {
        SessionEvent::EntryUndo {
            is_entry_step: true,
        } if session.protected => ProtectedSession {
            protected: false,
            entered_by_edit: false,
            generation: session.generation.saturating_add(1),
        },
        _ => session.on_event(event),
    }
}

/// H3u. Accepting the entry undo on a session entered on load breaks
/// monotonicity, which pins the `entered_by_edit` guard.
#[kani::proof]
#[kani::unwind(7)]
#[kani::should_panic]
fn entry_undo_after_load_entry_breaks_monotonicity() {
    check_monotone(entry_undo_ignores_the_entry_kind);
}

// ---------------------------------------------------------------------------
// H4n: navigation
// ---------------------------------------------------------------------------

/// H4n. `navigation_target` scrolls exactly for visible columns, and otherwise
/// opens the inspector on the tiling window that holds the column clamped to
/// the paragraph's last character: it starts at `index · n`, holds 1 to `n`
/// characters, is full unless it ends the paragraph, and contains the column.
/// Those properties determine the window uniquely.
///
/// H4n checks the window `navigation_target` returns by its defining
/// properties and does not re-derive the tiling arithmetic (the tiling itself
/// is `policy.rs`'s property test `inspector_windows_tile_over_the_whole_range`
/// and its exhaustive small-width test). Re-deriving it (the first form) made CBMC bit-blast four
/// 64-bit divisions by a symbolic width and ran past 71 minutes; comparing
/// against a second `inspector_window_containing` call (the second form) asked
/// it to prove two dividers equivalent and timed out at 20 minutes. This form
/// keeps the one division inside `navigation_target` and checks the result by
/// multiplication by the constant width.
///
/// Domain: every head ≤ length in `u64`, every `u64` column and paragraph,
/// the production width `N_INSPECT` (the only width the executor passes; other
/// widths are covered by the unit tests); loop-free.
#[kani::proof]
fn navigation_target_reaches_every_column() {
    let head: u64 = kani::any();
    let len: u64 = kani::any();
    kani::assume(head <= len);
    let extent = ParagraphExtent::new(head, len).unwrap();
    let paragraph: u64 = kani::any();
    let column: u64 = kani::any();
    let n = N_INSPECT_WIDTH;
    let target = navigation_target(paragraph, column, extent, n);
    let visible = head == len || column < head;
    match target {
        NavigationTarget::ScrollTo { paragraph: p } => {
            assert!(p == paragraph);
            assert!(visible);
            kani::cover!(head < len, "scroll into a folded paragraph's head");
        }
        NavigationTarget::Inspect {
            paragraph: p,
            window_index,
            window,
        } => {
            assert!(p == paragraph);
            assert!(!visible);
            // Folded, so len ≥ 1 and the clamped column is below len. The
            // window's defining properties, by multiplication only: comparing
            // with a second `inspector_window_containing` call made CBMC prove
            // two 64-bit dividers equivalent and did not finish in 20 minutes.
            let clamped = if column < len { column } else { len - 1 };
            assert!(window.contains(clamped));
            assert!(u128::from(window.start) == u128::from(window_index) * u128::from(n.get()));
            assert!(window.start < window.end && window.end <= len);
            assert!(window.len() <= n.get());
            assert!(window.end == len || window.len() == n.get());
            kani::cover!(column < len, "inspect a hidden column");
            kani::cover!(column >= len, "inspect past the end");
        }
    }
}

// ---------------------------------------------------------------------------
// H4f, H4x: find batches
// ---------------------------------------------------------------------------

const MAX_RECORDS: usize = 6;

fn any_limits() -> FindBatchLimits {
    let results_per_turn: usize = kani::any();
    kani::assume(results_per_turn >= 1 && results_per_turn <= 3);
    let bytes_per_turn: u64 = kani::any();
    kani::assume(bytes_per_turn >= 1 && bytes_per_turn <= 6);
    let max_record_bytes: u64 = kani::any();
    kani::assume(max_record_bytes <= bytes_per_turn);
    let result_cap: u64 = kani::any();
    kani::assume(result_cap >= 1);
    FindBatchLimits::new(
        results_per_turn,
        bytes_per_turn,
        result_cap,
        max_record_bytes,
    )
    .unwrap()
}

/// H4f. Batches deliver 1 to `results_per_turn` records and at most
/// `bytes_per_turn` bytes while any remain under the cap, together the first
/// `min(n, result_cap)` admitted records exactly once and in order (contiguous
/// prefixes of the queue), in at most `n` turns, and then report finished.
///
/// Domain: up to six records of any match and any preview size, admitted by
/// the production `admit_find_record`; per-turn count caps 1–3, byte caps 1–6,
/// any record bound up to the byte cap, any result cap (`#[kani::unwind(7)]`).
#[kani::proof]
#[kani::unwind(7)]
fn find_batches_are_bounded_and_partition_results() {
    let limits = any_limits();
    let n: usize = kani::any();
    kani::assume(n <= MAX_RECORDS);
    let found: [FindMatch; MAX_RECORDS] = kani::any();
    let records: [AdmittedRecord; MAX_RECORDS] = [
        admit_find_record(found[0], limits),
        admit_find_record(found[1], limits),
        admit_find_record(found[2], limits),
        admit_find_record(found[3], limits),
        admit_find_record(found[4], limits),
        admit_find_record(found[5], limits),
    ];
    let expected = if (n as u64) < limits.result_cap() {
        n
    } else {
        limits.result_cap() as usize
    };

    let mut position = 0usize;
    let mut delivered = 0u64;
    let mut turns = 0usize;
    while turns < MAX_RECORDS && position < expected {
        let batch = next_find_batch(&records[position..n], delivered, limits);
        assert!(batch.count >= 1 && batch.count <= limits.results_per_turn());
        assert!(batch.bytes <= limits.bytes_per_turn());
        let mut sum = 0u64;
        let mut i = 0;
        while i < batch.count {
            sum += records[position + i].bytes();
            i += 1;
        }
        assert!(sum == batch.bytes);
        kani::cover!(batch.count >= 2, "multi-record batch");
        kani::cover!(
            batch.count < limits.results_per_turn() && position + batch.count < expected,
            "batch cut by bytes"
        );
        position += batch.count;
        delivered += batch.count as u64;
        turns += 1;
    }
    assert!(position == expected);
    assert!(turns <= n);
    let done = next_find_batch(&records[position..n], delivered, limits);
    assert!(done.count == 0 && done.bytes == 0);
    kani::cover!(expected < n, "result cap reached");
}

/// H4x. A record that did not pass `admit_find_record` and is larger than the
/// per-turn byte cap breaks the byte bound: the planner takes the first record
/// unconditionally, so admission is what keeps a batch within one turn.
#[kani::proof]
#[kani::unwind(2)]
#[kani::should_panic]
fn find_batch_exceeds_the_byte_bound_without_record_admission() {
    let limits = any_limits();
    let bytes: u64 = kani::any();
    kani::assume(bytes > limits.bytes_per_turn());
    let record = AdmittedRecord {
        paragraph: kani::any(),
        column: kani::any(),
        match_chars: kani::any(),
        bytes,
    };
    let records = [record];
    let batch = next_find_batch(&records, 0, limits);
    assert!(batch.bytes <= limits.bytes_per_turn());
}
