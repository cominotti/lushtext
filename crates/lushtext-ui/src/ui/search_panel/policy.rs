// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure decision logic for the workspace search panel workflow.
//!
//! Everything here is free of GTK-family imports so the default mutation scope
//! reaches it through the `ui/**/policy.rs` convention:
//!
//! * [`WorkspaceSearchFlight`] — single-flight ownership of the one active
//!   search plus one replaceable latest query.
//! * [`SearchRetirementSliceBudget`] — the per-turn ownership budget that lets
//!   one GTK retirement turn release at most its row budget.
//! * [`ReplacePreviewTicket`] plus [`ReplacePreviewFacts`] — the Replace All
//!   preview freshness seam, validated as one unit by
//!   [`ReplacePreviewTicket::is_current`].
//! * [`preview_reservation_weight`], [`completed_preview_reservation_weight`],
//!   and [`retained_byte_weight`] — the disposal weights the Replace All
//!   preview reserves up front and shrinks to once its outcome is known.
//! * [`plan_undo_reservation`] plus [`UndoReservationPlan`] — the undo-journal
//!   admission arithmetic that decides whether a reservation replaces guarded
//!   owners and what weight it credits back, and [`combined_undo_weight`],
//!   the installed weight of every journal a Replace All's admission retires.
//! * [`journal_generation_is_current`] — the freshness predicate every
//!   generation-guarded journal install, clear, disk save, and disk delete
//!   compares against.
//! * [`ReplaceApplyCounts`] — the last durable apply's observable counts.
//! * [`replace_admission_resume`] over [`ReplaceAdmissionFacts`] — whether a
//!   confirmed Replace All that waits for disposal capacity resumes, or ends
//!   as [`ReplaceWaitEnd::Cancelled`] or [`ReplaceWaitEnd::Superseded`].
//! * [`SEARCH_RESULTS_RETAINED_BYTES`] and [`SEARCH_MATCH_FIXED_CHARGE`] — the
//!   retained-byte budget that replaced the 16,384-match count cap
//!   (`bound-lists-by-budget-not-count` D3), and the wording that states what it
//!   cut ([`limited_count_label`], [`budget_reached_announcement`],
//!   [`search_limit_clause`]).

use std::path::PathBuf;
use std::sync::Arc;

use crate::ui::search_bar::policy::EditorReplacePrefill;
use lushtext_model::budgets::list_budget::grouped_thousands;
use lushtext_model::values::{FlightGeneration, ReplacePreviewGeneration, UndoBackupGeneration};
use lushtext_model::workspace::content_search::{
    ReplacePreviewBudget, ReplacePreviewOutcome, ReplaceSkipReason, ReplaceSkipReport,
    ReplaceSkippedFile, Replacement, SearchMatchId, SearchQuerySpec,
};
use lushtext_services::coordination::single_flight::SingleFlightCoordinator;

/// Bytes one generated preview row retains besides its charged source text.
///
/// A reserved preview row costs the replacement value itself plus its two
/// generation-scoped identity-map slots. Naming the composition keeps the
/// reservation estimate and the shrink-to measurement describing the same row.
///
/// Kind: derived; computed from the constants it names, not rounded.
const PREVIEW_ROW_RETAINED_BYTES: usize = std::mem::size_of::<Replacement>()
    .saturating_add(std::mem::size_of::<Option<usize>>())
    .saturating_add(std::mem::size_of::<SearchMatchId>());

/// Charge a byte count against the disposal lane's `u64` weight, saturating.
///
/// Disposal weights are `u64` while buffer and capacity measurements are
/// `usize`. Saturating keeps an implausibly large measurement admitting as the
/// heaviest possible payload instead of wrapping into a small one.
#[must_use]
pub fn retained_byte_weight(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

/// Weight one preview attempt reserves before its worker starts.
///
/// The attempt has produced nothing yet, so the reservation is the budget's
/// worst case: every charged source byte plus one retained row per budgeted row.
#[must_use]
pub fn preview_reservation_weight(budget: ReplacePreviewBudget) -> u64 {
    retained_byte_weight(budget.max_bytes).saturating_add(retained_byte_weight(
        budget.max_rows.saturating_mul(PREVIEW_ROW_RETAINED_BYTES),
    ))
}

/// Weight a completed preview attempt shrinks its reservation to.
///
/// Measured from what the outcome actually retains — charged source bytes plus
/// the two identity-map allocations' capacities — so the lane stops holding the
/// budgeted worst case once the real cost is known.
#[must_use]
pub fn completed_preview_reservation_weight(outcome: &ReplacePreviewOutcome) -> u64 {
    retained_byte_weight(outcome.charged_bytes)
        .saturating_add(retained_byte_weight(
            outcome
                .replacements
                .capacity()
                .saturating_mul(std::mem::size_of::<Replacement>()),
        ))
        .saturating_add(retained_byte_weight(
            outcome
                .match_to_preview
                .capacity()
                .saturating_mul(std::mem::size_of::<Option<usize>>()),
        ))
        .saturating_add(retained_byte_weight(
            outcome.ineligible_files.iter().fold(
                outcome
                    .ineligible_files
                    .capacity()
                    .saturating_mul(std::mem::size_of::<
                        lushtext_model::workspace::content_search::ReplacePreviewIneligibleFile,
                    >()),
                |total, file| total.saturating_add(file.path.as_os_str().len()),
            ),
        ))
}

/// How one undo-journal reservation relates to the guarded owners it displaces.
///
/// Undo admission is not a plain "reserve the ceiling" decision: the installed
/// journal and the transient worker input are both already charged against the
/// same retained-bytes ceiling, so a reservation that will replace them must
/// credit their weight back or it would double-count itself out of capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UndoReservationPlan {
    /// Nothing guarded is being displaced: reserve against the bare ceiling.
    Fresh,
    /// Guarded owners are being displaced, and this much weight is credited.
    Replacement {
        /// Combined weight of the installed journal and the transient input.
        replaced_weight: u64,
    },
}

/// Decide how an undo-journal reservation must be admitted.
///
/// `installed_weight` is the currently published journal's reservation weight,
/// when it holds one; `transient_input_weight` is the guarded input the caller
/// is about to hand to a worker, when there is one. Either being present means
/// the new reservation replaces a guarded owner.
#[must_use]
pub fn plan_undo_reservation(
    installed_weight: Option<u64>,
    transient_input_weight: Option<u64>,
) -> UndoReservationPlan {
    if installed_weight.is_none() && transient_input_weight.is_none() {
        return UndoReservationPlan::Fresh;
    }
    UndoReservationPlan::Replacement {
        replaced_weight: installed_weight
            .unwrap_or(0)
            .saturating_add(transient_input_weight.unwrap_or(0)),
    }
}

/// The combined reservation weight of several installed undo journals.
///
/// `None` only when no journal is installed, because `Some(0)` (a journal that
/// measures nothing) still makes the reservation a replacement
/// ([`plan_undo_reservation`]). A Replace All passes every window's journal,
/// since its admission retires all of them in the same turn.
#[must_use]
pub fn combined_undo_weight(weights: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    weights
        .into_iter()
        .flatten()
        .fold(None, |total: Option<u64>, weight| {
            Some(total.unwrap_or(0).saturating_add(weight))
        })
}

/// Whether a journal mutation still owns the generation it reserved.
///
/// Every generation-guarded journal step — the in-memory install, the in-memory
/// clear, the worker-side disk save, and the worker-side disk delete — compares
/// the live counter against the generation it reserved before it may proceed. A
/// mismatch means a newer Replace All or undo already superseded this one, so
/// the step must abandon its payload rather than resurrect stale journal state.
#[must_use]
pub const fn journal_generation_is_current(
    observed: UndoBackupGeneration,
    reserved: UndoBackupGeneration,
) -> bool {
    observed.get() == reserved.get()
}

/// Observable counts from the most recent durable Replace All apply.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReplaceApplyCounts {
    /// Matches actually rewritten on disk.
    pub replaced: u32,
    /// Files skipped because they were unsaved, saving, or externally changed.
    pub skipped: u32,
    /// Files whose replacement reported an error.
    pub errors: u32,
}

/// What a confirmed Replace All waiting for disposal capacity knows when the
/// capacity wakeup fires (`replace-all-confirm-disposal-lane` design D5).
///
/// Every field is a scalar read of live panel state at the resumption, so the
/// decision stays GTK-free and testable over every combination.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "pure policy fact bundle for Replace All admission"
)]
pub struct ReplaceAdmissionFacts {
    /// Whether this panel still holds the application's Replace All transaction.
    pub transaction_claimed: bool,
    /// Whether the parked journal freshness still matches the panel's journal
    /// generation (`ReplaceJournalFreshness::is_current`).
    pub freshness_current: bool,
    /// Whether the user asked the waiting run to stop.
    pub cancel_requested: bool,
    /// Whether the run's control (cancel flag and progress) still exists.
    pub run_control_present: bool,
}

/// Why a wait for capacity ended without starting the run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplaceWaitEnd {
    /// The user cancelled, or the run's control is gone: nothing was changed.
    Cancelled,
    /// The transaction or the journal generation is no longer this run's (the
    /// panel's own deferred persisted-undo startup load can install a journal
    /// during the wait): the run was superseded before any file changed.
    Superseded,
}

/// The decision for one resumption of a waiting Replace All.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplaceAdmissionDecision {
    /// Try the undo reservation again; the window recomputes the skip set first.
    Resume,
    /// End the wait without writing anything, for this reason.
    Abandon(ReplaceWaitEnd),
}

/// Decide whether a waiting Replace All may try its admission again.
///
/// A cancel, or a run control that no longer exists, ends the wait as
/// `Cancelled`; cancel wins over supersession because the user asked for it.
/// Otherwise a lost transaction or a stale journal generation ends it as
/// `Superseded`. Only a run that still owns all four facts resumes, so the
/// window never starts a run this did not clear.
#[must_use]
pub const fn replace_admission_resume(facts: ReplaceAdmissionFacts) -> ReplaceAdmissionDecision {
    if facts.cancel_requested || !facts.run_control_present {
        ReplaceAdmissionDecision::Abandon(ReplaceWaitEnd::Cancelled)
    } else if !facts.transaction_claimed || !facts.freshness_current {
        ReplaceAdmissionDecision::Abandon(ReplaceWaitEnd::Superseded)
    } else {
        ReplaceAdmissionDecision::Resume
    }
}

/// Compact latest query retained while one active search disconnects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSearchRequest {
    pub spec: SearchQuerySpec,
    pub folders: Arc<[PathBuf]>,
}

/// One request admitted to become the only active controller/walker group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSearchStart {
    pub generation: FlightGeneration,
    pub request: WorkspaceSearchRequest,
}

/// Result of submitting one latest query to the single-flight policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceSearchSubmission {
    Start(WorkspaceSearchStart),
    Supersede { active_generation: FlightGeneration },
}

/// Direct ownership counters for readiness and concurrency evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceSearchFlightSnapshot {
    pub active: usize,
    pub pending: usize,
    pub active_generation: Option<FlightGeneration>,
}

/// At most one active search plus one replaceable latest pending request.
///
/// A thin wrapper over the shared [`SingleFlightCoordinator`] that adapts the
/// generic submit/finish results into workspace-search evidence: workspace
/// search does not use the coordinator's cancellation token (the content-search
/// walker owns cancellation), and `submit` reports the superseded generation
/// rather than dropping it.
#[derive(Debug, Default)]
pub struct WorkspaceSearchFlight {
    coordinator: SingleFlightCoordinator<WorkspaceSearchRequest>,
}

impl WorkspaceSearchFlight {
    /// Start immediately when idle, otherwise replace the compact pending query.
    ///
    /// # Panics
    ///
    /// Panics only if the shared coordinator rejects a submission while
    /// reporting no active generation, which its one-active/one-latest contract
    /// makes impossible.
    pub fn submit(&mut self, request: WorkspaceSearchRequest) -> WorkspaceSearchSubmission {
        let active_generation = self.coordinator.active_generation();
        match self.coordinator.submit(request) {
            Some(start) => WorkspaceSearchSubmission::Start(WorkspaceSearchStart {
                generation: start.generation,
                request: start.request,
            }),
            None => WorkspaceSearchSubmission::Supersede {
                active_generation: active_generation
                    .expect("a superseded submission always has an active generation"),
            },
        }
    }

    /// Finish only the current generation and admit the retained latest query.
    pub fn finish(&mut self, generation: FlightGeneration) -> Option<WorkspaceSearchStart> {
        self.coordinator
            .finish(generation)
            .map(|start| WorkspaceSearchStart {
                generation: start.generation,
                request: start.request,
            })
    }

    /// Cancel pending ownership while the active generation drains externally.
    pub fn clear_pending(&mut self) {
        self.coordinator.clear_pending();
    }

    #[must_use]
    pub fn snapshot(&self) -> WorkspaceSearchFlightSnapshot {
        let snapshot = self.coordinator.snapshot();
        WorkspaceSearchFlightSnapshot {
            active: snapshot.active,
            pending: snapshot.pending,
            active_generation: self.coordinator.active_generation(),
        }
    }
}

/// Saturating counter that lets one GTK adapter turn release at most its row budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchRetirementSliceBudget {
    remaining: usize,
    retired: usize,
}

impl SearchRetirementSliceBudget {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            remaining: limit,
            retired: 0,
        }
    }

    /// Reserve bounded work from the next deterministic ownership category.
    pub fn take(&mut self, available: usize) -> usize {
        let count = self.remaining.min(available);
        self.remaining = self.remaining.saturating_sub(count);
        self.retired = self.retired.saturating_add(count);
        count
    }

    /// Charge one independently owned value before its caller releases it.
    pub fn take_one(&mut self) -> bool {
        self.take(1) == 1
    }

    #[must_use]
    pub fn exhausted(self) -> bool {
        self.remaining == 0
    }

    #[must_use]
    pub fn retired(self) -> usize {
        self.retired
    }
}

/// Identity of one Replace All preview attempt, captured when it is dispatched.
///
/// The preview seam is inverted twice: generation and replacement previews are
/// built on a worker, and the checked selection is partitioned on a second
/// worker. Both completions resume on GTK and must decide whether the panel they
/// return to still wants their result. Reifying `{generation, query_spec}` here
/// keeps that decision one validated value rather than a clause list repeated at
/// each resumption point, and makes the "same generation, different query" case
/// impossible to compare loosely.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplacePreviewTicket {
    generation: ReplacePreviewGeneration,
    query_spec: SearchQuerySpec,
}

/// Live preview state observed on GTK when a preview worker completion resumes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplacePreviewFacts {
    pub generation: ReplacePreviewGeneration,
    pub pending: bool,
    pub query_spec: SearchQuerySpec,
}

impl ReplacePreviewTicket {
    /// Capture the expectation for one preview attempt at its entry point.
    #[must_use]
    pub fn new(generation: ReplacePreviewGeneration, query_spec: SearchQuerySpec) -> Self {
        Self {
            generation,
            query_spec,
        }
    }

    #[must_use]
    pub const fn generation(&self) -> ReplacePreviewGeneration {
        self.generation
    }

    #[must_use]
    pub const fn query_spec(&self) -> &SearchQuerySpec {
        &self.query_spec
    }

    /// Adopt a newer attempt's expectation when a queued request is superseded,
    /// so the retained request keeps exactly one identity.
    ///
    /// Takes the newer ticket by value: the expectation is still constructed
    /// once, at `issue_preview_ticket`, and moves into the retained request
    /// rather than being re-split into its fields and copied across the seam.
    pub fn supersede(&mut self, newer: Self) {
        *self = newer;
    }

    /// Whether a completed preview result may still be published to the panel.
    ///
    /// All three clauses are part of one decision: the panel must still be on
    /// this generation, must still be waiting for a preview, and must still be
    /// showing the query the preview was generated for.
    #[must_use]
    pub fn is_current(&self, facts: &ReplacePreviewFacts) -> bool {
        self.generation == facts.generation && facts.pending && self.query_spec == facts.query_spec
    }

    /// Whether a retained request may still be dispatched to the worker.
    ///
    /// Re-dispatch deliberately checks only generation and pending state: the
    /// request has not produced a result yet, so the live query is re-read when
    /// its result comes back through [`Self::is_current`].
    #[must_use]
    pub fn may_dispatch(&self, facts: &ReplacePreviewFacts) -> bool {
        self.generation == facts.generation && facts.pending
    }
}

/// Bytes charged per retained match beyond its line text: the plain match
/// (`SearchMatch`, its `Arc<str>` header), its list item and the item's display
/// copy of the line (at most 500 bytes plus an ellipsis), its tree row and its
/// two navigation entries.
///
/// Kind: coverage window; rounded up to a power of two from the measured
/// per-match growth (`bound-lists-by-budget-not-count` task 2.4).
pub const SEARCH_MATCH_FIXED_CHARGE: u64 = 2 * 1024;
/// `main`'s worst case under the former 16,384-match cap: every match a
/// 4,096-byte line (`MAX_SEARCH_MATCH_LINE_BYTES`) in its own file whose path
/// is 4,096 bytes (Linux `PATH_MAX`), charged twice (canonical and display
/// path), plus the fixed charge.
///
/// Kind: derived; computed from the constants it names, not rounded.
pub const MAIN_SEARCH_WORST_RETENTION: u64 = MAIN_SEARCH_CAP_MATCHES
    * (lushtext_model::workspace::content_search::MAX_SEARCH_MATCH_LINE_BYTES as u64
        + 2 * LINUX_PATH_MAX_BYTES
        + SEARCH_MATCH_FIXED_CHARGE);
/// The match-count cap `main` applied before this budget (`RESULT_CAP`).
///
/// Kind: external fact; the retired cap the budget must not lose against.
pub const MAIN_SEARCH_CAP_MATCHES: u64 = 16 * 1024;
/// Linux `PATH_MAX`, bytes.
///
/// Kind: external fact; `<linux/limits.h>`.
pub const LINUX_PATH_MAX_BYTES: u64 = 4_096;
/// The retained-byte budget of one workspace search's results.
///
/// The power of two at or above [`MAIN_SEARCH_WORST_RETENTION`], so the
/// budget never shows less than `main` showed at `main`'s worst case (Kani B3
/// over `model::list_budget`). Typical matches (a 120-byte line, about 20 per
/// file) retain far more than 16,384 matches in it.
///
/// Kind: presentation cap; up from `MAIN_SEARCH_WORST_RETENTION`
/// (`bound-lists-by-budget-not-count` D3).
pub const SEARCH_RESULTS_RETAINED_BYTES: u64 = 256 * 1024 * 1024;
const _: () = assert!(SEARCH_RESULTS_RETAINED_BYTES >= MAIN_SEARCH_WORST_RETENTION);
// ... and the smallest such power of two (never rounded past it).
const _: () = assert!(
    SEARCH_RESULTS_RETAINED_BYTES.is_power_of_two()
        && SEARCH_RESULTS_RETAINED_BYTES / 2 < MAIN_SEARCH_WORST_RETENTION
);

/// Events a turn may receive while the results list holds fewer matches than
/// one full turn (256): the turn that reveals the results and the few after
/// it. The first rows make `GtkListView` create its row widgets, up to its
/// realized-row cap (A1), each with a setup, a bind and a first layout, so a
/// full first turn spent 13.8 ms applying and its frame 17–19 ms laying them
/// out (`bound-lists-by-budget-not-count` task 1.2 on the branch); small turns
/// spread that one-time cost (D4 b).
///
/// Kind: cap; a power of two, rounded down.
pub const REVEAL_TURN_MAX_EVENTS: usize = 32;

/// The count label while results stream or once they end, unlimited.
#[must_use]
pub fn count_label(matches: usize, files: usize) -> String {
    format!("{matches} results in {files} files")
}

/// The count label of a search its budget limited: what is shown, and how many
/// more were found (growing until the search ends).
#[must_use]
pub fn limited_count_label(shown: usize, files: usize, found: usize) -> String {
    let more = found.saturating_sub(shown);
    format!(
        "Showing {} matches in {} files; {} more not shown \u{2014} refine the search to see them",
        grouped_thousands(shown),
        grouped_thousands(files),
        grouped_thousands(more)
    )
}

/// The one polite announcement raised when a search reaches its budget.
#[must_use]
pub fn budget_reached_announcement(shown: usize) -> String {
    format!(
        "Search results limited: showing {} matches; more were found",
        grouped_thousands(shown)
    )
}

/// The Replace Preview summary clause after a limited search, or `None` when
/// the search retained every match it found.
#[must_use]
pub fn search_limit_clause(retained: usize, found: usize) -> Option<String> {
    let past = found.checked_sub(retained).filter(|past| *past > 0)?;
    Some(format!(
        "{} matches past the search limit are not included",
        grouped_thousands(past)
    ))
}

/// Heading of one reason's section in the Skipped Files dialog (design D7).
#[must_use]
pub const fn skip_reason_heading(reason: ReplaceSkipReason) -> &'static str {
    match reason {
        ReplaceSkipReason::UnsavedChanges => "Open with Unsaved Changes",
        ReplaceSkipReason::LineTooLong => "Matches on Very Long Lines",
        ReplaceSkipReason::InvalidPreview => "Matches the Preview Could Not Confirm",
        ReplaceSkipReason::Unreadable => "Could Not Be Read",
        ReplaceSkipReason::LargerThanOpenTier => "Too Large to Open",
        ReplaceSkipReason::NotUtf8 => "Not UTF-8 Text",
        ReplaceSkipReason::ChangedSinceSearch => "Changed Since the Search",
        ReplaceSkipReason::NotEnoughDiskSpace => "Not Enough Disk Space",
        ReplaceSkipReason::UndoJournalFailed => "Undo Could Not Be Recorded",
        ReplaceSkipReason::ChangedDuringReplace => "Changed During Replace All",
        ReplaceSkipReason::WriteFailed => "Could Not Be Written",
    }
}

/// Second line of one skipped-file row: its folder, the first skipped line,
/// and for a partly replaced file how many matches are left.
#[must_use]
pub fn skipped_file_subtitle(file: &ReplaceSkippedFile) -> String {
    let mut subtitle = file
        .path
        .parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_default();
    if let Some(line) = file.first_line {
        if !subtitle.is_empty() {
            subtitle.push_str(" · ");
        }
        subtitle.push_str(&format!(
            "line {}",
            grouped_thousands(usize::try_from(line).unwrap_or(usize::MAX))
        ));
    }
    if let Some(left) = file.matches_left {
        subtitle.push_str(&format!(
            " · partly replaced, {} {} left",
            grouped_thousands(left),
            if left == 1 { "match" } else { "matches" }
        ));
    }
    subtitle
}

/// Whether a skipped file's row offers Open and Replace in Editor: every reason
/// [`replace_in_editor_availability`] does not refuse. A row names no line
/// length, so a document that turns out to be protected is decided when it
/// opens (the Protected View notice).
#[must_use]
pub fn skipped_file_opens_in_editor(reason: ReplaceSkipReason) -> bool {
    replace_in_editor_availability(reason, None) != ReplaceInEditorAvailability::NotOpenable
}

/// The completion clause for skipped files (" · N skipped"), or `None`.
#[must_use]
pub fn skip_completion_clause(report: &ReplaceSkipReport) -> Option<String> {
    let total = report.total();
    (total > 0).then(|| format!(" · {} skipped", grouped_thousands(total)))
}

/// The Skipped Files dialog's summary line, with the files the list's byte
/// bound left out stated.
#[must_use]
pub fn skip_summary(report: &ReplaceSkipReport) -> String {
    let skipped = report.skipped_total();
    let partly = report.partly_replaced();
    let mut summary = format!(
        "{} {} not replaced",
        grouped_thousands(skipped),
        if skipped == 1 { "file" } else { "files" }
    );
    if partly > 0 {
        summary.push_str(&format!(", {} partly replaced", grouped_thousands(partly)));
    }
    if report.not_listed() > 0 {
        summary.push_str(&format!(
            "; showing {} of {}",
            grouped_thousands(report.files().len()),
            grouped_thousands(report.total())
        ));
    }
    summary
}

/// Section ranges of a finished skip report: one per reason that lists a file,
/// as `(reason, start, end)` positions into `files`, in list order.
#[must_use]
pub fn skip_sections(files: &[ReplaceSkippedFile]) -> Vec<(ReplaceSkipReason, u32, u32)> {
    let mut sections: Vec<(ReplaceSkipReason, u32, u32)> = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let position = u32::try_from(index).unwrap_or(u32::MAX);
        match sections.last_mut() {
            Some((reason, _, end)) if *reason == file.reason => *end = position.saturating_add(1),
            _ => sections.push((file.reason, position, position.saturating_add(1))),
        }
    }
    sections
}

/// Whether a skipped file can be finished with Replace in Editor (design D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceInEditorAvailability {
    /// The file opens editable; the in-tab bar can finish it.
    Offered,
    /// LushText cannot open the file (too large, or unreadable).
    NotOpenable,
    /// The file opens view-only in Protected View, where in-tab Replace is off.
    ProtectedView,
}

/// Decide whether Replace in Editor is offered for a file skipped for
/// `reason`. `longest_skipped_line_bytes` is the byte length of the longest
/// source line the skip names, when the skip names lines.
///
/// A line of more than `4 * L_PROTECT_EDIT` bytes certainly holds more than
/// `L_PROTECT_EDIT` characters (UTF-8 is at most four bytes a character), so
/// its document opens in Protected View.
#[must_use]
pub fn replace_in_editor_availability(
    reason: ReplaceSkipReason,
    longest_skipped_line_bytes: Option<u64>,
) -> ReplaceInEditorAvailability {
    match reason {
        ReplaceSkipReason::LargerThanOpenTier | ReplaceSkipReason::Unreadable => {
            ReplaceInEditorAvailability::NotOpenable
        }
        ReplaceSkipReason::LineTooLong
            if longest_skipped_line_bytes.is_some_and(|bytes| {
                bytes > lushtext_model::document::document_scale::L_PROTECT_EDIT.saturating_mul(4)
            }) =>
        {
            ReplaceInEditorAvailability::ProtectedView
        }
        _ => ReplaceInEditorAvailability::Offered,
    }
}

/// What one Replace All run asked for, captured from the preview it applied
/// and kept with its skip report (design D7), so Open and Replace in Editor
/// prefills the run's request, never the panel's live fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceRunIntent {
    /// The query and options the run searched with.
    pub query: SearchQuerySpec,
    /// The replacement template, in Rust `regex` syntax in regex mode.
    pub replacement: String,
}

/// Build the in-tab bar's prefill for `intent` (design D7): the query and
/// options unchanged; the replacement copied in literal mode, translated by
/// [`rust_replacement_to_gtksourceview`] in regex mode, and `None` when it
/// names a group the pattern lacks or the pattern does not compile.
#[must_use]
pub fn editor_replace_prefill(intent: &ReplaceRunIntent) -> EditorReplacePrefill {
    let options = &intent.query.options;
    let replacement = if options.regex {
        lushtext_services::search::regex_query::regex_capture_names(&intent.query.query).and_then(
            |names| {
                let name_refs: Vec<Option<&str>> = names.iter().map(Option::as_deref).collect();
                rust_replacement_to_gtksourceview(&intent.replacement, &name_refs)
            },
        )
    } else {
        Some(intent.replacement.clone())
    };
    EditorReplacePrefill {
        query: intent.query.query.clone(),
        replacement,
        regex: options.regex,
        case_sensitive: options.case_sensitive,
        whole_word: options.whole_word,
    }
}

/// Translate a Rust `regex` replacement template into GtkSourceView's (GRegex)
/// syntax, for prefilling the in-tab bar (design D7).
///
/// `capture_names[i]` is group `i`'s name (`regex::Regex::capture_names`), so
/// its length is the group count plus one. A reference follows
/// `Captures::expand`: `$` then the longest run of `[_0-9A-Za-z]`, or `${...}`;
/// all digits name a group by number, anything else by name. Every reference
/// becomes `\g<N>` or `\g<name>` (never `\N`, so `${1}0` stays group 1 then
/// `0`), `$$` becomes `$`, a `$` that starts no reference stays `$`, and a
/// backslash becomes `\\`. A reference to a group the pattern does not have
/// returns `None`, `${}` (the empty name) included: Rust expands it to
/// nothing, GtkSourceView would fail it.
#[must_use]
pub fn rust_replacement_to_gtksourceview(
    template: &str,
    capture_names: &[Option<&str>],
) -> Option<String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(position) = rest.find(['$', '\\']) {
        out.push_str(&rest[..position]);
        let tail = &rest[position..];
        if let Some(stripped) = tail.strip_prefix('\\') {
            out.push_str("\\\\");
            rest = stripped;
            continue;
        }
        let after = &tail[1..];
        if let Some(stripped) = after.strip_prefix('$') {
            out.push('$');
            rest = stripped;
            continue;
        }
        let (name, consumed) = if let Some(braced) = after.strip_prefix('{') {
            match braced.find('}') {
                Some(end) if end > 0 => (&braced[..end], end + 2),
                // `${}` names the empty group, which no pattern has: Rust
                // expands it to nothing.
                Some(_) => return None,
                None => ("", 0),
            }
        } else {
            let end = after
                .find(|c: char| !(c == '_' || c.is_ascii_alphanumeric()))
                .unwrap_or(after.len());
            (&after[..end], end)
        };
        if name.is_empty() {
            out.push('$');
            rest = after;
            continue;
        }
        let group = match name.parse::<usize>() {
            Ok(number) if number < capture_names.len() => number.to_string(),
            Err(_) if capture_names.iter().flatten().any(|known| *known == name) => {
                name.to_string()
            }
            // A missing group, by number or by name.
            Ok(_) | Err(_) => return None,
        };
        out.push_str("\\g<");
        out.push_str(&group);
        out.push('>');
        rest = &after[consumed..];
    }
    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod search_budget_wording_tests {
    use super::*;

    #[test]
    fn the_budget_covers_mains_worst_case() {
        assert_eq!(
            MAIN_SEARCH_WORST_RETENTION,
            16 * 1024 * (4_096 + 8_192 + 2_048)
        );
    }

    #[test]
    fn the_limited_label_states_shown_and_more() {
        assert_eq!(
            limited_count_label(245_301, 1_200, 353_530),
            "Showing 245,301 matches in 1,200 files; 108,229 more not shown \u{2014} refine the search to see them"
        );
        assert_eq!(count_label(12, 3), "12 results in 3 files");
        assert_eq!(grouped_thousands(0), "0");
        assert_eq!(grouped_thousands(999), "999");
        assert_eq!(grouped_thousands(1_000), "1,000");
        assert_eq!(grouped_thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn the_announcement_is_bounded_and_content_free() {
        let text = budget_reached_announcement(16 * 1024);
        assert_eq!(
            text,
            "Search results limited: showing 16,384 matches; more were found"
        );
        assert!(text.len() < 96);
    }

    #[test]
    fn the_preview_clause_only_follows_a_limited_search() {
        assert_eq!(search_limit_clause(100, 100), None);
        assert_eq!(search_limit_clause(100, 99), None);
        assert_eq!(
            search_limit_clause(16 * 1024, 40_000),
            Some("23,616 matches past the search limit are not included".to_string())
        );
    }
}

#[cfg(test)]
mod preview_ticket_tests {
    use super::*;
    use lushtext_model::workspace::content_search::ContentSearchOptions;

    fn spec(query: &str) -> SearchQuerySpec {
        SearchQuerySpec {
            query: query.to_string(),
            options: ContentSearchOptions::default(),
        }
    }

    fn facts(generation: u32, pending: bool, query: &str) -> ReplacePreviewFacts {
        ReplacePreviewFacts {
            generation: ReplacePreviewGeneration::new(generation),
            pending,
            query_spec: spec(query),
        }
    }

    #[test]
    fn matching_generation_pending_and_query_publishes() {
        let ticket = ReplacePreviewTicket::new(ReplacePreviewGeneration::new(7), spec("needle"));
        assert_eq!(ticket.generation(), ReplacePreviewGeneration::new(7));
        assert_eq!(ticket.query_spec(), &spec("needle"));
        assert!(ticket.is_current(&facts(7, true, "needle")));
        assert!(ticket.may_dispatch(&facts(7, true, "needle")));
    }

    #[test]
    fn each_stale_clause_rejects_the_completion() {
        let ticket = ReplacePreviewTicket::new(ReplacePreviewGeneration::new(7), spec("needle"));
        assert!(!ticket.is_current(&facts(8, true, "needle")));
        assert!(!ticket.is_current(&facts(7, false, "needle")));
        assert!(!ticket.is_current(&facts(7, true, "other")));
    }

    #[test]
    fn same_generation_with_changed_options_is_not_current() {
        let mut changed = spec("needle");
        changed.options.regex = true;
        let ticket = ReplacePreviewTicket::new(ReplacePreviewGeneration::new(3), spec("needle"));
        assert!(!ticket.is_current(&ReplacePreviewFacts {
            generation: ReplacePreviewGeneration::new(3),
            pending: true,
            query_spec: changed,
        }));
    }

    #[test]
    fn dispatch_ignores_query_drift_but_not_generation_or_pending() {
        let ticket = ReplacePreviewTicket::new(ReplacePreviewGeneration::new(2), spec("needle"));
        assert!(ticket.may_dispatch(&facts(2, true, "typed-more")));
        assert!(!ticket.may_dispatch(&facts(3, true, "needle")));
        assert!(!ticket.may_dispatch(&facts(2, false, "needle")));
    }

    #[test]
    fn superseding_a_retained_request_replaces_the_whole_expectation() {
        let mut ticket =
            ReplacePreviewTicket::new(ReplacePreviewGeneration::new(2), spec("needle"));
        ticket.supersede(ReplacePreviewTicket::new(
            ReplacePreviewGeneration::new(5),
            spec("latest"),
        ));
        assert_eq!(ticket.generation(), ReplacePreviewGeneration::new(5));
        assert!(ticket.is_current(&facts(5, true, "latest")));
        assert!(!ticket.is_current(&facts(2, true, "needle")));
    }
}

#[cfg(test)]
mod flight_tests {
    use super::*;
    use lushtext_model::workspace::content_search::ContentSearchOptions;

    fn request(query: &str) -> WorkspaceSearchRequest {
        WorkspaceSearchRequest {
            spec: SearchQuerySpec {
                query: query.to_string(),
                options: ContentSearchOptions::default(),
            },
            folders: Arc::from([PathBuf::from("/workspace")]),
        }
    }

    #[test]
    fn rapid_submissions_keep_one_active_and_only_latest_pending() {
        let mut flight = WorkspaceSearchFlight::default();
        let WorkspaceSearchSubmission::Start(first) = flight.submit(request("first")) else {
            panic!("first request should start");
        };
        for query in ["second", "third", "latest"] {
            assert_eq!(
                flight.submit(request(query)),
                WorkspaceSearchSubmission::Supersede {
                    active_generation: first.generation,
                }
            );
        }
        assert_eq!(
            flight.snapshot(),
            WorkspaceSearchFlightSnapshot {
                active: 1,
                pending: 1,
                active_generation: Some(first.generation),
            }
        );

        let next = flight
            .finish(first.generation)
            .expect("latest should start");
        assert_eq!(next.request.spec.query, "latest");
        assert_eq!(flight.snapshot().active, 1);
        assert_eq!(flight.snapshot().pending, 0);
    }

    #[test]
    fn stale_disconnect_cannot_finish_current_generation() {
        let mut flight = WorkspaceSearchFlight::default();
        let WorkspaceSearchSubmission::Start(first) = flight.submit(request("first")) else {
            panic!("first request should start");
        };
        flight.submit(request("latest"));
        assert!(
            flight
                .finish(FlightGeneration::new(
                    first.generation.get().wrapping_add(99)
                ))
                .is_none()
        );
        assert_eq!(flight.snapshot().active_generation, Some(first.generation));
    }

    #[test]
    fn panel_clear_drops_pending_but_waits_for_active_disconnect() {
        let mut flight = WorkspaceSearchFlight::default();
        flight.submit(request("first"));
        flight.submit(request("pending"));
        flight.clear_pending();
        assert_eq!(flight.snapshot().active, 1);
        assert_eq!(flight.snapshot().pending, 0);
    }

    #[test]
    fn active_and_pending_requests_share_immutable_scope_snapshots() {
        let shared =
            Arc::<[PathBuf]>::from([PathBuf::from("/workspace/a"), PathBuf::from("/workspace/b")]);
        let mut first = request("first");
        first.folders = Arc::clone(&shared);
        let mut latest = request("latest");
        latest.folders = Arc::clone(&shared);
        let changed = Arc::<[PathBuf]>::from([PathBuf::from("/workspace/changed")]);

        let mut flight = WorkspaceSearchFlight::default();
        let WorkspaceSearchSubmission::Start(active) = flight.submit(first) else {
            panic!("first request should start");
        };
        flight.submit(latest);
        let pending = flight
            .finish(active.generation)
            .expect("latest request should start");

        assert!(Arc::ptr_eq(&active.request.folders, &shared));
        assert!(Arc::ptr_eq(&pending.request.folders, &shared));
        assert!(!Arc::ptr_eq(&pending.request.folders, &changed));
        assert_eq!(pending.request.folders.as_ref(), shared.as_ref());
    }
}

#[cfg(test)]
mod retirement_budget_tests {
    use super::*;

    #[test]
    fn configured_result_cap_retires_over_bounded_slices() {
        let mut categories = [1usize, 10_000, 1, 10_000, 1];
        let mut slices = 0usize;
        while categories.iter().any(|count| *count > 0) {
            let mut budget = SearchRetirementSliceBudget::new(250);
            for count in &mut categories {
                let retired = budget.take(*count);
                *count = count.saturating_sub(retired);
            }
            assert!(budget.retired() <= 250);
            assert!(budget.retired() > 0);
            slices = slices.saturating_add(1);
        }
        assert!(slices > 1);
    }

    #[test]
    fn partially_consumed_budget_still_has_room_in_the_same_turn() {
        // A slice that has released some rows but not all of its budget must
        // report room left, or one bounded retirement turn would stop after its
        // first category and the disposer would crawl one item per GTK turn.
        let mut budget = SearchRetirementSliceBudget::new(250);
        assert!(
            !budget.exhausted(),
            "a fresh slice budget has its whole allowance available",
        );
        assert_eq!(budget.take(100), 100);
        assert!(
            !budget.exhausted(),
            "100 of 250 released leaves room for the next ownership category",
        );
        assert!(budget.take_one());
        assert!(
            !budget.exhausted(),
            "charging one more value must not exhaust a 250-row slice",
        );
        assert_eq!(budget.retired(), 101);
        assert_eq!(budget.take(usize::MAX), 149);
        assert!(
            budget.exhausted(),
            "the slice is exhausted only once its whole budget is spent",
        );
    }

    #[test]
    fn zero_budget_and_large_available_counts_saturate_safely() {
        let mut empty = SearchRetirementSliceBudget::new(0);
        assert_eq!(empty.take(usize::MAX), 0);
        assert!(!empty.take_one());
        assert!(empty.exhausted());

        let mut bounded = SearchRetirementSliceBudget::new(250);
        assert!(bounded.take_one());
        assert_eq!(bounded.retired(), 1);
        assert_eq!(bounded.take(usize::MAX), 249);
        assert_eq!(bounded.retired(), 250);
        assert!(bounded.exhausted());
    }
}

#[cfg(test)]
mod replace_weight_tests {
    use super::*;

    fn budget(max_bytes: usize, max_rows: usize) -> ReplacePreviewBudget {
        ReplacePreviewBudget {
            max_rows,
            max_bytes,
        }
    }

    #[test]
    fn retained_byte_weight_saturates_instead_of_wrapping() {
        assert_eq!(retained_byte_weight(0), 0);
        assert_eq!(retained_byte_weight(4_096), 4_096);
        assert_eq!(
            retained_byte_weight(usize::MAX),
            u64::try_from(usize::MAX).unwrap_or(u64::MAX)
        );
    }

    #[test]
    fn preview_reservation_charges_source_bytes_plus_one_row_each() {
        let row = retained_byte_weight(PREVIEW_ROW_RETAINED_BYTES);
        assert_eq!(preview_reservation_weight(budget(0, 0)), 0);
        assert_eq!(preview_reservation_weight(budget(1_000, 0)), 1_000);
        assert_eq!(
            preview_reservation_weight(budget(0, 4)),
            row.saturating_mul(4)
        );
        assert_eq!(
            preview_reservation_weight(budget(1_000, 4)),
            1_000 + row.saturating_mul(4),
        );
    }

    #[test]
    fn preview_reservation_saturates_on_an_implausible_budget() {
        assert_eq!(
            preview_reservation_weight(budget(usize::MAX, usize::MAX)),
            u64::MAX,
        );
    }

    fn empty_outcome(charged_bytes: usize) -> ReplacePreviewOutcome {
        ReplacePreviewOutcome {
            replacements: Vec::new(),
            match_to_preview: Vec::new(),
            omitted_eligible: 0,
            skipped: lushtext_model::workspace::content_search::ReplacePreviewSkipCounts::default(),
            charged_bytes,
            limiting_reason: None,
            ineligible_files: Vec::new(),
        }
    }

    #[test]
    fn completed_reservation_measures_real_retention_not_the_budget() {
        let mut outcome = empty_outcome(512);
        assert_eq!(completed_preview_reservation_weight(&outcome), 512);

        outcome.replacements.reserve_exact(2);
        outcome.match_to_preview.reserve_exact(3);
        let expected =
            512 + retained_byte_weight(
                outcome
                    .replacements
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Replacement>()),
            ) + retained_byte_weight(
                outcome
                    .match_to_preview
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Option<usize>>()),
            );
        assert_eq!(completed_preview_reservation_weight(&outcome), expected);
    }

    #[test]
    fn nothing_guarded_reserves_the_bare_ceiling() {
        assert_eq!(
            plan_undo_reservation(None, None),
            UndoReservationPlan::Fresh
        );
    }

    #[test]
    fn each_guarded_owner_alone_still_makes_it_a_replacement() {
        assert_eq!(
            plan_undo_reservation(Some(700), None),
            UndoReservationPlan::Replacement {
                replaced_weight: 700
            }
        );
        assert_eq!(
            plan_undo_reservation(None, Some(300)),
            UndoReservationPlan::Replacement {
                replaced_weight: 300
            }
        );
    }

    #[test]
    fn both_guarded_owners_credit_their_combined_weight() {
        assert_eq!(
            plan_undo_reservation(Some(700), Some(300)),
            UndoReservationPlan::Replacement {
                replaced_weight: 1_000
            }
        );
    }

    #[test]
    fn a_zero_weight_guarded_owner_is_still_a_replacement() {
        // `Some(0)` means "a guarded owner exists and measures nothing", which
        // is not the same admission decision as "no guarded owner exists".
        assert_eq!(
            plan_undo_reservation(Some(0), None),
            UndoReservationPlan::Replacement { replaced_weight: 0 }
        );
    }

    #[test]
    fn combined_replaced_weight_saturates() {
        assert_eq!(
            plan_undo_reservation(Some(u64::MAX), Some(1)),
            UndoReservationPlan::Replacement {
                replaced_weight: u64::MAX
            }
        );
    }

    #[test]
    fn combined_undo_weight_sums_only_installed_journals() {
        assert_eq!(combined_undo_weight([None, None]), None);
        assert_eq!(combined_undo_weight([None, Some(0)]), Some(0));
        assert_eq!(
            combined_undo_weight([Some(700), None, Some(300)]),
            Some(1_000)
        );
        assert_eq!(
            combined_undo_weight([Some(u64::MAX), Some(1)]),
            Some(u64::MAX)
        );
    }

    #[test]
    fn journal_generation_matches_only_its_own_reservation() {
        let g = UndoBackupGeneration::new;
        assert!(journal_generation_is_current(g(7), g(7)));
        assert!(!journal_generation_is_current(g(8), g(7)));
        assert!(!journal_generation_is_current(g(6), g(7)));
        assert!(journal_generation_is_current(
            UndoBackupGeneration::ZERO,
            UndoBackupGeneration::ZERO
        ));
        // The counter wraps, so the far side of a wrap must not be accepted.
        assert!(!journal_generation_is_current(
            UndoBackupGeneration::ZERO,
            g(u32::MAX)
        ));
        assert!(journal_generation_is_current(g(u32::MAX), g(u32::MAX)));
    }

    #[test]
    fn apply_counts_default_to_an_empty_result() {
        assert_eq!(
            ReplaceApplyCounts::default(),
            ReplaceApplyCounts {
                replaced: 0,
                skipped: 0,
                errors: 0,
            }
        );
    }
}

#[cfg(test)]
mod skip_report_tests {
    use super::*;
    use std::path::Path;

    fn report() -> ReplaceSkipReport {
        let mut report = ReplaceSkipReport::default();
        report.record_skipped(Path::new("/w/b.txt"), ReplaceSkipReason::NotUtf8, None);
        report.record_skipped(
            Path::new("/w/a.txt"),
            ReplaceSkipReason::UnsavedChanges,
            Some(3),
        );
        report.record_skipped(Path::new("/w/c.txt"), ReplaceSkipReason::NotUtf8, None);
        report.record_partly_replaced(Path::new("/w/d.txt"), ReplaceSkipReason::LineTooLong, 7, 2);
        report.finish();
        report
    }

    #[test]
    fn sections_follow_the_finished_reason_order() {
        let report = report();
        assert_eq!(
            skip_sections(report.files()),
            vec![
                (ReplaceSkipReason::UnsavedChanges, 0, 1),
                (ReplaceSkipReason::LineTooLong, 1, 2),
                (ReplaceSkipReason::NotUtf8, 2, 4),
            ]
        );
        assert!(skip_sections(&[]).is_empty());
    }

    #[test]
    fn summary_and_clause_count_every_file() {
        let report = report();
        assert_eq!(
            skip_summary(&report),
            "3 files not replaced, 1 partly replaced"
        );
        assert_eq!(
            skip_completion_clause(&report).as_deref(),
            Some(" · 4 skipped")
        );
        assert_eq!(skip_completion_clause(&ReplaceSkipReport::default()), None);
    }

    #[test]
    fn subtitles_name_the_folder_line_and_matches_left() {
        let report = report();
        let partly = report
            .files()
            .iter()
            .find(|file| file.matches_left.is_some())
            .expect("partly replaced row");
        assert_eq!(
            skipped_file_subtitle(partly),
            "/w · line 7 · partly replaced, 2 matches left"
        );
        let unsaved = &report.files()[0];
        assert_eq!(skipped_file_subtitle(unsaved), "/w · line 3");
    }

    #[test]
    fn only_a_file_too_large_or_unreadable_has_no_editor_action() {
        for reason in ReplaceSkipReason::ALL {
            assert_eq!(
                skipped_file_opens_in_editor(reason),
                !matches!(
                    reason,
                    ReplaceSkipReason::LargerThanOpenTier | ReplaceSkipReason::Unreadable
                )
            );
            assert!(!skip_reason_heading(reason).is_empty());
        }
    }
}

#[cfg(test)]
mod replace_in_editor_tests {
    use super::*;
    use lushtext_model::workspace::content_search::ContentSearchOptions;

    const NAMES: [Option<&str>; 3] = [None, None, Some("word")];

    #[test]
    fn references_become_named_or_numbered_gregex_groups() {
        let translate = |t| rust_replacement_to_gtksourceview(t, &NAMES);
        assert_eq!(translate("$1").as_deref(), Some("\\g<1>"));
        assert_eq!(translate("${1}0").as_deref(), Some("\\g<1>0"));
        assert_eq!(translate("$word!").as_deref(), Some("\\g<word>!"));
        assert_eq!(translate("${word}s").as_deref(), Some("\\g<word>s"));
        assert_eq!(translate("$0").as_deref(), Some("\\g<0>"));
    }

    #[test]
    fn dollars_and_backslashes_stay_literal() {
        let translate = |t| rust_replacement_to_gtksourceview(t, &NAMES);
        assert_eq!(translate("$$1").as_deref(), Some("$1"));
        assert_eq!(translate("cost: $").as_deref(), Some("cost: $"));
        assert_eq!(translate("$ 5").as_deref(), Some("$ 5"));
        assert_eq!(translate("${").as_deref(), Some("${"));
        assert_eq!(translate("a\\b").as_deref(), Some("a\\\\b"));
        assert_eq!(translate("plain").as_deref(), Some("plain"));
        assert_eq!(translate("é$1é").as_deref(), Some("é\\g<1>é"));
    }

    #[test]
    fn a_missing_group_returns_none() {
        assert_eq!(rust_replacement_to_gtksourceview("$3", &NAMES), None);
        assert_eq!(rust_replacement_to_gtksourceview("$1a", &NAMES), None);
        assert_eq!(
            rust_replacement_to_gtksourceview("${missing}", &NAMES),
            None
        );
        assert_eq!(rust_replacement_to_gtksourceview("${}", &NAMES), None);
    }

    fn intent(query: &str, replacement: &str, regex: bool) -> ReplaceRunIntent {
        ReplaceRunIntent {
            query: SearchQuerySpec::new(
                query,
                ContentSearchOptions {
                    regex,
                    case_sensitive: true,
                    whole_word: true,
                    ..ContentSearchOptions::default()
                },
            ),
            replacement: replacement.to_string(),
        }
    }

    #[test]
    fn the_prefill_carries_the_run_request() {
        let literal = editor_replace_prefill(&intent("a$1", "b\\$2", false));
        assert_eq!(literal.query, "a$1");
        assert_eq!(literal.replacement.as_deref(), Some("b\\$2"));
        assert!(!literal.regex && literal.case_sensitive && literal.whole_word);

        let regex = editor_replace_prefill(&intent(r"(\w+)-(?P<n>\d)", "$2${1}0$n", true));
        assert_eq!(regex.query, r"(\w+)-(?P<n>\d)");
        assert_eq!(regex.replacement.as_deref(), Some("\\g<2>\\g<1>0\\g<n>"));
        assert!(regex.regex);

        let missing = editor_replace_prefill(&intent(r"(\w+)", "$2", true));
        assert_eq!(missing.replacement, None);
        let invalid = editor_replace_prefill(&intent("(", "x", true));
        assert_eq!(invalid.replacement, None);
    }

    #[test]
    fn availability_follows_the_reason_and_the_protect_bound() {
        use ReplaceInEditorAvailability::{NotOpenable, Offered, ProtectedView};
        let limit = lushtext_model::document::document_scale::L_PROTECT_EDIT * 4;
        assert_eq!(
            replace_in_editor_availability(ReplaceSkipReason::LargerThanOpenTier, None),
            NotOpenable
        );
        assert_eq!(
            replace_in_editor_availability(ReplaceSkipReason::Unreadable, None),
            NotOpenable
        );
        assert_eq!(
            replace_in_editor_availability(ReplaceSkipReason::LineTooLong, Some(limit)),
            Offered
        );
        assert_eq!(
            replace_in_editor_availability(ReplaceSkipReason::LineTooLong, Some(limit + 1)),
            ProtectedView
        );
        for reason in ReplaceSkipReason::ALL {
            if !matches!(
                reason,
                ReplaceSkipReason::LargerThanOpenTier
                    | ReplaceSkipReason::Unreadable
                    | ReplaceSkipReason::LineTooLong
            ) {
                assert_eq!(
                    replace_in_editor_availability(reason, Some(u64::MAX)),
                    Offered
                );
            }
        }
    }

    /// Ledger A127, as GtkSourceView 5.20, 5.22 and 5.23 replaced
    /// `foo-bar baz-qux` for `(?P<first>\w+)-(\w+)` (the probe's
    /// `EXPANSIONS`): each GRegex replacement and the buffer it left.
    const A127_PATTERN: &str = r"(?P<first>\w+)-(\w+)";
    const A127_DOCUMENT: &str = "foo-bar baz-qux";
    const A127_OBSERVED: [(&str, &str); 4] = [
        (r"\g<2>-\g<1>", "bar-foo qux-baz"),
        (r"\g<first>!", "foo! baz!"),
        (r"\g<1>0", "foo0 baz0"),
        ("$1", "$1 $1"),
    ];

    #[test]
    fn the_gregex_model_matches_axiom_a127() {
        use lushtext_services::search::regex_query::{regex_capture_names, test_support};

        for (template, observed) in A127_OBSERVED {
            assert_eq!(
                test_support::gregex_replace_all(A127_PATTERN, A127_DOCUMENT, template),
                observed,
                "{template}"
            );
        }
        // And the translated Rust templates land on the same buffers.
        let names = regex_capture_names(A127_PATTERN).expect("pattern");
        let name_refs: Vec<Option<&str>> = names.iter().map(Option::as_deref).collect();
        for (rust, observed) in [
            ("$2-$1", "bar-foo qux-baz"),
            ("${first}!", "foo! baz!"),
            ("${1}0", "foo0 baz0"),
            ("$$1", "$1 $1"),
        ] {
            let translated =
                rust_replacement_to_gtksourceview(rust, &name_refs).expect("groups exist");
            assert_eq!(
                test_support::gregex_replace_all(A127_PATTERN, A127_DOCUMENT, &translated),
                observed,
                "{rust} -> {translated}"
            );
            assert_eq!(
                test_support::rust_replace_all(A127_PATTERN, A127_DOCUMENT, rust),
                observed,
                "{rust}"
            );
        }
    }

    const PROPERTY_PATTERN: &str = r"(\w+)-(?P<word>\w+)";
    const PROPERTY_TEXT: &str = "foo-bar";

    /// Pieces whose concatenation never forms a reference to a missing group:
    /// every reference is braced, and no bare `$` precedes a word character.
    const CLOSED_PIECES: [&str; 12] = [
        "${1}", "${0}", "${2}", "${word}", "$$", "\\", "a", "0", "é", " ", "{", "}",
    ];
    /// Any pieces, including bare `$`, unbraced and missing references.
    const OPEN_PIECES: [&str; 16] = [
        "$1",
        "$0",
        "$word",
        "$3",
        "${missing}",
        "$",
        "${",
        "$$",
        "\\",
        "a",
        "1",
        "_",
        "é",
        " ",
        "{",
        "}",
    ];

    proptest::proptest! {
        /// A template with no missing reference translates, and GtkSourceView
        /// expands the translation to exactly what Rust expands the template to.
        #[test]
        fn a_closed_template_expands_alike(
            pieces in proptest::collection::vec(proptest::sample::select(&CLOSED_PIECES[..]), 0..12),
        ) {
            use lushtext_services::search::regex_query::{regex_capture_names, test_support};

            let names = regex_capture_names(PROPERTY_PATTERN).expect("pattern");
            let name_refs: Vec<Option<&str>> = names.iter().map(Option::as_deref).collect();
            let template = pieces.concat();
            let translated = rust_replacement_to_gtksourceview(&template, &name_refs);
            proptest::prop_assert!(translated.is_some(), "{template}");
            let (gregex, rust) = test_support::expand_first_capture_pair(
                PROPERTY_PATTERN,
                PROPERTY_TEXT,
                translated.as_deref().unwrap_or_default(),
                &template,
            );
            proptest::prop_assert_eq!(gregex, rust, "{}", template);
        }

        /// Any template either translates to the same expansion, or names a
        /// group the pattern does not have, which Rust expands to nothing.
        #[test]
        fn any_template_expands_alike_or_is_refused(
            pieces in proptest::collection::vec(proptest::sample::select(&OPEN_PIECES[..]), 0..12),
        ) {
            use lushtext_services::search::regex_query::{regex_capture_names, test_support};

            let names = regex_capture_names(PROPERTY_PATTERN).expect("pattern");
            let name_refs: Vec<Option<&str>> = names.iter().map(Option::as_deref).collect();
            let template = pieces.concat();
            match rust_replacement_to_gtksourceview(&template, &name_refs) {
                Some(translated) => {
                    let (gregex, rust) = test_support::expand_first_capture_pair(
                        PROPERTY_PATTERN,
                        PROPERTY_TEXT,
                        &translated,
                        &template,
                    );
                    proptest::prop_assert_eq!(gregex, rust, "{} -> {}", template, translated);
                }
                None => {
                    // Refused only for a reference: a template without `$` has none.
                    proptest::prop_assert!(
                        template.contains('$'),
                        "a refused template names a group: {}", template
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod replace_admission_resume_tests {
    use super::*;

    /// Every combination of the four facts (design D5), with the decision the
    /// table expects: a cancel or a lost run control ends as `Cancelled`, a
    /// transaction or journal generation this run no longer holds ends as
    /// `Superseded`, and only all four clear resumes. Cancel wins over
    /// supersession, because the user asked for it.
    #[test]
    fn every_fact_combination_has_its_decision() {
        for bits in 0u8..16 {
            let facts = ReplaceAdmissionFacts {
                transaction_claimed: bits & 1 != 0,
                freshness_current: bits & 2 != 0,
                cancel_requested: bits & 4 != 0,
                run_control_present: bits & 8 != 0,
            };
            let expected = if facts.cancel_requested || !facts.run_control_present {
                ReplaceAdmissionDecision::Abandon(ReplaceWaitEnd::Cancelled)
            } else if !facts.transaction_claimed || !facts.freshness_current {
                ReplaceAdmissionDecision::Abandon(ReplaceWaitEnd::Superseded)
            } else {
                ReplaceAdmissionDecision::Resume
            };
            assert_eq!(replace_admission_resume(facts), expected, "{facts:?}");
        }
    }

    #[test]
    fn a_current_claimed_run_resumes() {
        assert_eq!(
            replace_admission_resume(ReplaceAdmissionFacts {
                transaction_claimed: true,
                freshness_current: true,
                cancel_requested: false,
                run_control_present: true,
            }),
            ReplaceAdmissionDecision::Resume
        );
    }

    #[test]
    fn a_stale_journal_generation_is_superseded_not_cancelled() {
        assert_eq!(
            replace_admission_resume(ReplaceAdmissionFacts {
                transaction_claimed: true,
                freshness_current: false,
                cancel_requested: false,
                run_control_present: true,
            }),
            ReplaceAdmissionDecision::Abandon(ReplaceWaitEnd::Superseded)
        );
    }

    #[test]
    fn a_requested_cancel_wins_over_a_lost_transaction() {
        assert_eq!(
            replace_admission_resume(ReplaceAdmissionFacts {
                transaction_claimed: false,
                freshness_current: false,
                cancel_requested: true,
                run_control_present: true,
            }),
            ReplaceAdmissionDecision::Abandon(ReplaceWaitEnd::Cancelled)
        );
    }
}
