// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy — the Protected View workflow's `policy.rs`
//! (`long-line-protected-view`, design D2, D3, D5, D6, D7 and D12).
//!
//! Every decision Protected View takes without touching GTK lives here, so the
//! integrity and responsiveness preconditions can be unit-tested, mutated, and
//! Kani-proved (`policy/kani_proofs.rs`, shard `core-protected-view`):
//!
//! - **Caret-free interaction (D3).** [`viewport_step`] turns a `move-cursor`
//!   request into a viewport movement; [`press_disposition`] decides what a
//!   capture-phase press or drag does; the closed [`ProtectedAction`] enum names
//!   every toolkit binding and LushText operation that could move a mark or
//!   change the buffer, [`toolkit_effect`] records what each would do if its
//!   default handler ran, and [`action_disposition`] decides whether it may.
//!   Only [`ActionDisposition::ViewportSafe`] admits a default handler.
//! - **The session (D2).** [`ProtectedSession::on_event`] is the only
//!   transition of the per-editor Protected View state; `execution.rs` stores
//!   the value and only ever replaces it with `on_event`'s result.
//! - **Navigation (D7).** [`navigation_target`] classifies every navigation
//!   into a scroll or an inspector window.
//! - **Line Inspector (D6).** [`inspector_window`],
//!   [`inspector_window_count`] and [`inspector_window_containing`] tile a
//!   paragraph into windows of at most `n_inspect` characters, loop-free.
//! - **Worker find (D5).** [`admit_find_record`] bounds each result record and
//!   [`next_find_batch`] plans one GTK turn's publication from admitted records
//!   only, under [`FindBatchLimits::PRODUCTION`].
//!
//! The module is integer-only and carries the whole-pixel `forbid` at ceiling 0
//! (`.agents/rules/workflow-convention.md`). It has no `gtk4`, `glib`, `gio`,
//! `libadwaita` or `sourceview5` import: the toolkit enums it mirrors
//! ([`MovementStep`], [`PressButton`], [`PointerDevice`]) are converted by the
//! executor.
//!
//! The responsiveness budgets are read from `model/responsiveness_budget.rs`
//! (`B_INTERACT_US`, `B_SETTLE_US`) and never restated here: the find-batch and
//! copy-chunk constants below are counts and bytes whose publication the
//! verdict (task 9.4) measures against `B_SETTLE_US`.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

use core::num::NonZeroU64;

use crate::model::document_scale::L_PROTECT;
use crate::model::protected_fold::HEAD_CAP;

#[cfg(kani)]
mod kani_proofs;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// The most characters one Line Inspector window holds (C0's `N_inspect`
/// bound, design D6), owned by the fold policy and re-exported here so the
/// workflow narrates in its own vocabulary. It is small enough that the
/// inspector's caret, IM and accessibility paths stay within the
/// `L_protect_stall` basis.
pub(crate) use crate::model::protected_fold::N_INSPECT;

/// [`N_INSPECT`] as the non-zero width the window functions take.
pub(crate) const N_INSPECT_WIDTH: NonZeroU64 = match NonZeroU64::new(N_INSPECT) {
    Some(width) => width,
    None => panic!("N_INSPECT must be at least one character"),
};

const fn min_u64(a: u64, b: u64) -> u64 {
    if a < b { a } else { b }
}

// An inspector window never shows more than a head may (so it is never a more
// expensive layout than the main view) and stays within C0's stall basis.
const _: () = assert!(N_INSPECT <= min_u64(HEAD_CAP, L_PROTECT));

/// The most find results one GTK turn publishes (design D5; initial value,
/// confirmed by the verdict).
pub(crate) const FIND_RESULTS_PER_TURN: usize = 256;
/// The most record bytes one GTK turn publishes (design D5; initial value).
pub(crate) const FIND_BYTES_PER_TURN: u64 = 64 * 1024;
/// The total result cap of one find request (design D5); the counter reports
/// "capped" when it is reached.
pub(crate) const FIND_RESULT_CAP: u64 = 10_000;
/// The most retained bytes one admitted find record carries: its preview
/// excerpt, cut by [`admit_find_record`]. The record's fixed fields are bounded
/// by the per-turn count instead.
pub(crate) const MAX_FIND_RECORD_BYTES: u64 = 512;

// A single admitted record always fits one turn, so the planner's
// first-record-always progress rule never breaks the byte bound.
const _: () = assert!(MAX_FIND_RECORD_BYTES <= FIND_BYTES_PER_TURN);

/// The most bytes one whole-line clipboard write copies per GTK dispatch
/// (design D6). At least one UTF-8 scalar, so a chunk end can always be moved
/// back to a character boundary without becoming empty.
pub const COPY_CHUNK_BYTES: usize = 64 * 1024;

/// Characters an edit entry's source capture copies per GTK dispatch (design
/// D4): A66 measured 0.4–1.1 ms per 64 Ki characters at every depth of a
/// two-million-character paragraph when the iterator advances in steps below
/// GtkTextIter's linear-scan limit.
pub(crate) const SOURCE_CAPTURE_CHUNK_CHARS: i32 = 64 * 1024;

/// The `forward_chars` step of that capture: below GtkTextIter's
/// `MAX_LINEAR_SCAN` (150), so the iterator's byte and character offsets stay
/// current instead of being re-derived from the segment start (A66).
pub(crate) const SOURCE_CAPTURE_STEP_CHARS: i32 = 128;

const _: () = assert!(COPY_CHUNK_BYTES >= 4);

// ---------------------------------------------------------------------------
// Notices
// ---------------------------------------------------------------------------

/// Why a Protected View notice is published. Each text is bounded and contains
/// no document text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtectedNotice {
    /// An edit, selection or caret request was pre-empted.
    EditRefused,
    /// In-tab Replace was requested (`win.begin-replace`, the replace actions).
    ReplaceUnavailable,
    /// Invisible-character drawing was requested while folds exist.
    InvisiblesUnavailable,
    /// Printing was requested.
    PrintRefused,
    /// A bookmark toggle was requested: there is no caret line.
    BookmarkUnavailable,
    /// Line-ending normalization was requested.
    LineEndingsUnavailable,
    /// Save As of an edited document whose decode does not round-trip.
    SaveAsRefusedInexactDecode,
}

/// The text of a Protected View notice.
#[must_use]
pub(crate) const fn notice_text(notice: ProtectedNotice) -> &'static str {
    match notice {
        ProtectedNotice::EditRefused => {
            "This document is view-only because it contains very long lines. \
             Use Inspect Line (Ctrl+Alt+I) to read or copy a line."
        }
        ProtectedNotice::ReplaceUnavailable => {
            "Replace is not available in this view-only document with very long lines."
        }
        ProtectedNotice::InvisiblesUnavailable => {
            "Invisible characters are not shown in this view-only document with very long lines."
        }
        ProtectedNotice::PrintRefused => {
            "Printing view-only documents with very long lines is not supported."
        }
        ProtectedNotice::BookmarkUnavailable => {
            "Bookmarks cannot be added or removed in this view-only document with very long lines."
        }
        ProtectedNotice::LineEndingsUnavailable => {
            "Line endings cannot be changed in this view-only document with very long lines."
        }
        ProtectedNotice::SaveAsRefusedInexactDecode => {
            "Save As is not available for this edited view-only document with very long \
             lines: its encoding cannot write its text back exactly."
        }
    }
}

/// What Copy Whole Line says once the line is on the clipboard: its 1-based
/// number and length. Bounded: numbers only.
#[must_use]
pub(crate) fn whole_line_copied_text(paragraph: u64, chars: u64) -> String {
    let line = paragraph.saturating_add(1);
    if chars == 1 {
        format!("Line {line} copied (1 character)")
    } else {
        format!("Line {line} copied ({chars} characters)")
    }
}

/// The shortcut of Inspect Line (`win.inspect-line`), named in the alert,
/// the notice and the view's accessible description.
pub(crate) const INSPECT_LINE_SHORTCUT_LABEL: &str = "Ctrl+Alt+I";

/// The quiet interval of the pre-emption notice, in microseconds: a
/// pre-empted request publishes the notice at most once per interval, unless
/// a different notice is due. A time window, not a size, so it is not subject
/// to the power-of-two rule; 4 s is long enough that ten Ctrl+V presses read
/// as one message.
pub(crate) const NOTICE_QUIET_US: i64 = 4_000_000;

/// Notice reasons, one rate-limit slot each.
const NOTICE_REASONS: usize = 7;

/// When each notice reason was last published (monotonic µs): the rate limit
/// is per reason (review S5), so another reason in between never lets one
/// through again inside its quiet interval.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NoticeTimes([Option<i64>; NOTICE_REASONS]);

const fn notice_slot(notice: ProtectedNotice) -> usize {
    match notice {
        ProtectedNotice::EditRefused => 0,
        ProtectedNotice::ReplaceUnavailable => 1,
        ProtectedNotice::InvisiblesUnavailable => 2,
        ProtectedNotice::PrintRefused => 3,
        ProtectedNotice::BookmarkUnavailable => 4,
        ProtectedNotice::LineEndingsUnavailable => 5,
        ProtectedNotice::SaveAsRefusedInexactDecode => 6,
    }
}

impl NoticeTimes {
    /// When `notice` was last published.
    #[must_use]
    pub(crate) const fn last(&self, notice: ProtectedNotice) -> Option<i64> {
        self.0[notice_slot(notice)]
    }

    /// Record `notice` published at `now_us`.
    pub(crate) const fn record(&mut self, notice: ProtectedNotice, now_us: i64) {
        self.0[notice_slot(notice)] = Some(now_us);
    }
}

/// Whether a pre-emption notice is published now (`now_us`, monotonic), given
/// when that same reason was last published: at most once per
/// [`NOTICE_QUIET_US`] per reason. A clock that went backwards publishes.
#[must_use]
pub(crate) const fn notice_due(last_of_this_reason: Option<i64>, now_us: i64) -> bool {
    match last_of_this_reason {
        None => true,
        Some(at) => now_us < at || now_us.saturating_sub(at) >= NOTICE_QUIET_US,
    }
}

// ---------------------------------------------------------------------------
// The Protected View alert
// ---------------------------------------------------------------------------

/// What the one Protected View inline alert must say and offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProtectedAlertFacts {
    pub(crate) folded_paragraphs: u64,
    /// The entry undo is available now (an edit entered Protected View, undo
    /// history is on, and the entry edit is still the newest step).
    pub(crate) entry_undo_available: bool,
    /// The document is a recovered draft: `Some(has_backing_file)`. The alert
    /// then merges with the restored-draft alert and keeps its actions.
    pub(crate) restored_draft: Option<bool>,
}

/// The alert's primary button, as the executor wires it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtectedAlertPrimary {
    /// Open the Line Inspector at the viewport.
    InspectLine,
    /// Run the entry undo.
    EntryUndo,
    /// The restored-draft alert's Discard (its secondary is Save).
    DiscardDraft,
}

/// The alert's copy. Bounded: no document text, numbers only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProtectedAlertCopy {
    pub(crate) title: &'static str,
    pub(crate) body: String,
    pub(crate) primary_label: &'static str,
    pub(crate) primary: ProtectedAlertPrimary,
    pub(crate) secondary_label: Option<&'static str>,
}

/// Compose the one Protected View alert (spec "Protected View is announced by
/// one bounded inline alert"). The alert has two action buttons at most, so a
/// recovered draft keeps Discard and Save and names Inspect Line by its
/// shortcut, and an available entry undo takes the primary button. The copy
/// never promises to undo "the last keystroke": the entry undo removes the
/// whole user action that made a line too long (design D2).
#[must_use]
pub(crate) fn protected_alert(facts: ProtectedAlertFacts) -> ProtectedAlertCopy {
    let lines = match facts.folded_paragraphs {
        0 => "Long lines are shortened.".to_owned(),
        1 => "1 long line is shortened.".to_owned(),
        count => format!("{count} long lines are shortened."),
    };
    let inspect =
        format!("Use Inspect Line ({INSPECT_LINE_SHORTCUT_LABEL}) to read or copy a whole line.");
    match facts.restored_draft {
        Some(has_backing_file) => ProtectedAlertCopy {
            title: "Draft Restored, View-Only",
            body: format!(
                "Unsaved changes from a previous session have been restored. \
                 The document has very long lines, so it is view-only. {lines} {inspect}"
            ),
            primary_label: "_Discard…",
            primary: ProtectedAlertPrimary::DiscardDraft,
            secondary_label: Some(if has_backing_file {
                "_Save…"
            } else {
                "Save _As…"
            }),
        },
        None if facts.entry_undo_available => ProtectedAlertCopy {
            title: "View-Only: Very Long Lines",
            body: format!(
                "An edit made a line too long to edit, so the document is view-only. \
                 Undo Edit removes that edit. {lines} {inspect}"
            ),
            primary_label: "_Undo Edit",
            primary: ProtectedAlertPrimary::EntryUndo,
            secondary_label: None,
        },
        None => ProtectedAlertCopy {
            title: "View-Only: Very Long Lines",
            body: format!(
                "This document has very long lines, so it is view-only. {lines} {inspect}"
            ),
            primary_label: "_Inspect Line",
            primary: ProtectedAlertPrimary::InspectLine,
            secondary_label: None,
        },
    }
}

// ---------------------------------------------------------------------------
// Viewport steps (D3 step 2)
// ---------------------------------------------------------------------------

/// GTK's `GtkMovementStep`, mirrored so this module stays GTK-free; the
/// executor converts one to the other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub(crate) enum MovementStep {
    LogicalPositions,
    VisualPositions,
    Words,
    DisplayLines,
    DisplayLineEnds,
    Paragraphs,
    ParagraphEnds,
    Pages,
    BufferEnds,
    HorizontalPages,
}

/// Which scrollbar a viewport movement drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewportAxis {
    Horizontal,
    Vertical,
}

/// How far a viewport movement goes. Steps and pages are signed counts of the
/// adjustment's own step and page increments; the executor multiplies once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewportMotion {
    Steps(i32),
    Pages(i32),
    Start,
    End,
}

/// One viewport movement replacing a caret movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ViewportMove {
    pub(crate) axis: ViewportAxis,
    pub(crate) motion: ViewportMotion,
}

/// Map one `move-cursor` emission to a viewport movement (design D3 step 2).
///
/// The executor's `move-cursor` handler calls this, applies the result to the
/// view's adjustments, and stops the emission, so the insert mark never moves,
/// with or without `gtk-keynav-use-caret` (ledger A61). `extend_selection`
/// (Shift) is accepted and ignored: no selection exists in Protected View, so
/// Shift+Down scrolls exactly as Down does. A zero count moves nothing.
#[must_use]
pub(crate) const fn viewport_step(
    step: MovementStep,
    count: i32,
    extend_selection: bool,
) -> Option<ViewportMove> {
    let _ = extend_selection;
    if count == 0 {
        return None;
    }
    let edge = if count < 0 {
        ViewportMotion::Start
    } else {
        ViewportMotion::End
    };
    let (axis, motion) = match step {
        MovementStep::LogicalPositions | MovementStep::VisualPositions | MovementStep::Words => {
            (ViewportAxis::Horizontal, ViewportMotion::Steps(count))
        }
        MovementStep::DisplayLines | MovementStep::Paragraphs => {
            (ViewportAxis::Vertical, ViewportMotion::Steps(count))
        }
        MovementStep::DisplayLineEnds | MovementStep::ParagraphEnds => {
            (ViewportAxis::Horizontal, edge)
        }
        MovementStep::Pages => (ViewportAxis::Vertical, ViewportMotion::Pages(count)),
        MovementStep::HorizontalPages => (ViewportAxis::Horizontal, ViewportMotion::Pages(count)),
        MovementStep::BufferEnds => (ViewportAxis::Vertical, edge),
    };
    Some(ViewportMove { axis, motion })
}

// ---------------------------------------------------------------------------
// Capture-phase presses (D3 step 3)
// ---------------------------------------------------------------------------

/// Which capture gesture saw the sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureGesture {
    /// The capture-phase `GtkGestureClick`.
    Click,
    /// The capture-phase `GtkGestureDrag`.
    Drag,
}

/// The pressed button, as GDK numbers it (1, 2, 3; anything else is `Other`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PressButton {
    Primary,
    Middle,
    Secondary,
    Other,
}

/// The source device class of the sequence (`GdkInputSource`, reduced).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PointerDevice {
    Mouse,
    Touchpad,
    Touchscreen,
    Pen,
    Other,
}

/// What the press landed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PressHit {
    /// A fold marker in the Protected View gutter, for this paragraph.
    FoldMarker { paragraph: u64 },
    /// Any other gutter cell.
    Gutter,
    /// The text area.
    Text,
}

/// One capture-phase press or drag start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PressInput {
    pub(crate) gesture: CaptureGesture,
    pub(crate) button: PressButton,
    /// GTK's `n_press` (1 single, 2 double, 3 triple click).
    pub(crate) n_press: i32,
    pub(crate) hit: PressHit,
    pub(crate) device: PointerDevice,
}

/// What the executor does with a capture-phase sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PressDisposition {
    /// Claim, and open the Line Inspector on this paragraph.
    OpenInspector { paragraph: u64 },
    /// Claim, and open the Protected View menu.
    OpenMenu,
    /// Claim and do nothing: GtkTextView's own gestures never see it, so no
    /// mark moves, no selection starts, nothing is pasted and no drag begins.
    Claim,
    /// Leave unclaimed so the scrolled window's touch pan takes it.
    Unclaimed,
}

/// Decide a capture-phase press or drag (design D3 step 3; ledger A59).
///
/// A single primary click on a fold marker opens the inspector; a single
/// secondary click opens the Protected View menu; a touchscreen drag that does
/// not start on a marker stays unclaimed so the scrolled window pans; every
/// other sequence is claimed and does nothing. A touchscreen tap is a `Click`
/// and is claimed: the executor claims a touch click only once the gesture
/// recognises it, so a pan starting at the same point stays with the scrolled
/// window (A59's touch half is not probe-pinned; the lane covers it).
#[must_use]
pub(crate) const fn press_disposition(input: PressInput) -> PressDisposition {
    match (input.gesture, input.hit) {
        (CaptureGesture::Drag, PressHit::FoldMarker { .. }) => PressDisposition::Claim,
        (CaptureGesture::Drag, _) => {
            if matches!(input.device, PointerDevice::Touchscreen) {
                PressDisposition::Unclaimed
            } else {
                PressDisposition::Claim
            }
        }
        (CaptureGesture::Click, hit) => {
            if input.n_press != 1 {
                return PressDisposition::Claim;
            }
            match (input.button, hit) {
                (PressButton::Primary, PressHit::FoldMarker { paragraph }) => {
                    PressDisposition::OpenInspector { paragraph }
                }
                (PressButton::Secondary, _) => PressDisposition::OpenMenu,
                _ => PressDisposition::Claim,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Actions (D3 step 5)
// ---------------------------------------------------------------------------

/// Every toolkit binding, gesture input and LushText operation that Protected
/// View must classify (design D3 step 5; spec "Protected View offers no caret,
/// selection or edit paths").
///
/// Closed on purpose: the widget test of task 4.7 walks the view's class
/// shortcut controllers and maps every bound signal and action name through
/// [`ProtectedAction::from_toolkit_name`], so a binding a toolkit update adds
/// fails that test instead of reaching the buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub(crate) enum ProtectedAction {
    // GtkTextView keybinding signals.
    MoveCursor,
    MoveViewport,
    /// `move-focus` (Ctrl+Tab and its variants): moves keyboard focus out
    /// of the view, touching no mark and no text.
    MoveFocus,
    SelectAll,
    ToggleCursorVisible,
    CutClipboard,
    CopyClipboard,
    PasteClipboard,
    Backspace,
    DeleteFromCursor,
    InsertAtCursor,
    ToggleOverwrite,
    SetAnchor,
    ExtendSelection,
    InsertEmoji,
    // GtkTextView class actions reached by bindings or the default menu.
    DeleteSelection,
    /// `text.clear`, bound to the Clear key.
    ClearText,
    MenuPopup,
    TextUndo,
    TextRedo,
    // GtkSourceView keybinding signals and actions. `change-number` is bound
    // to Ctrl+Shift+A and Ctrl+Shift+X; GtkTextView's Ctrl+Shift+A (unselect,
    // `select-all` FALSE) shadows the first, so it is reached by Ctrl+Shift+X
    // (ledger A64).
    ChangeNumber,
    ChangeCase,
    JoinLines,
    MoveToMatchingBracket,
    MoveLines,
    MoveWords,
    ShowCompletion,
    PushSnippet,
    // Input that is not a binding.
    ImCommit,
    MiddleClickPaste,
    DndDrop,
    // LushText operations.
    SearchReplace,
    SearchReplaceAll,
    NormalizeLineEndings,
    ToggleBookmarkAtCursor,
    CycleInvisibleCharacters,
}

/// Toolkit signal and action names and the action each reaches. Signal names
/// are GtkTextView's and GtkSourceView's keybinding signals; dotted names are
/// class actions. One action may have several names (a signal and the action
/// the default menu uses).
const TOOLKIT_NAMES: [(&str, ProtectedAction); 34] = [
    ("move-cursor", ProtectedAction::MoveCursor),
    ("move-viewport", ProtectedAction::MoveViewport),
    ("move-focus", ProtectedAction::MoveFocus),
    ("text.clear", ProtectedAction::ClearText),
    ("select-all", ProtectedAction::SelectAll),
    ("selection.select-all", ProtectedAction::SelectAll),
    (
        "toggle-cursor-visible",
        ProtectedAction::ToggleCursorVisible,
    ),
    ("cut-clipboard", ProtectedAction::CutClipboard),
    ("clipboard.cut", ProtectedAction::CutClipboard),
    ("copy-clipboard", ProtectedAction::CopyClipboard),
    ("clipboard.copy", ProtectedAction::CopyClipboard),
    ("paste-clipboard", ProtectedAction::PasteClipboard),
    ("clipboard.paste", ProtectedAction::PasteClipboard),
    ("backspace", ProtectedAction::Backspace),
    ("delete-from-cursor", ProtectedAction::DeleteFromCursor),
    ("insert-at-cursor", ProtectedAction::InsertAtCursor),
    ("toggle-overwrite", ProtectedAction::ToggleOverwrite),
    ("set-anchor", ProtectedAction::SetAnchor),
    ("extend-selection", ProtectedAction::ExtendSelection),
    ("insert-emoji", ProtectedAction::InsertEmoji),
    ("misc.insert-emoji", ProtectedAction::InsertEmoji),
    ("selection.delete", ProtectedAction::DeleteSelection),
    ("menu.popup", ProtectedAction::MenuPopup),
    ("text.undo", ProtectedAction::TextUndo),
    ("text.redo", ProtectedAction::TextRedo),
    ("change-number", ProtectedAction::ChangeNumber),
    ("change-case", ProtectedAction::ChangeCase),
    ("source.change-case", ProtectedAction::ChangeCase),
    ("join-lines", ProtectedAction::JoinLines),
    (
        "move-to-matching-bracket",
        ProtectedAction::MoveToMatchingBracket,
    ),
    ("move-lines", ProtectedAction::MoveLines),
    ("move-words", ProtectedAction::MoveWords),
    ("show-completion", ProtectedAction::ShowCompletion),
    ("push-snippet", ProtectedAction::PushSnippet),
];

impl ProtectedAction {
    /// The action a toolkit keybinding signal or class action name reaches, or
    /// `None` for a name Protected View has not classified (which the binding
    /// enumeration test treats as a failure).
    #[must_use]
    pub(crate) fn from_toolkit_name(name: &str) -> Option<Self> {
        TOOLKIT_NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, action)| *action)
    }
}

/// Whether Protected View classifies the toolkit keybinding signal or class
/// action `name` (the class-shortcut enumeration test walks the view's
/// shortcut controllers through this; an unclassified binding fails it).
#[must_use]
pub fn classifies_toolkit_binding(name: &str) -> bool {
    ProtectedAction::from_toolkit_name(name).is_some()
}

/// What an action's default handler would do to the view if it ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolkitEffect {
    /// Scrolls the viewport, or moves keyboard focus out of the view; touches
    /// no mark and no text.
    ViewportOnly,
    /// Reads the selection (none exists in Protected View).
    ReadsSelection,
    /// Changes view state that is neither text nor a mark (overwrite mode,
    /// invisible-character drawing).
    ChangesViewState,
    /// Turns the caret back on.
    ShowsCaret,
    /// Opens GtkTextView's default context menu, which offers Select All.
    OpensDefaultMenu,
    /// Moves the insert or selection-bound mark, or adds a mark at the cursor.
    MovesMark,
    /// Inserts, deletes or replaces buffer text.
    MutatesBuffer,
}

impl ToolkitEffect {
    /// Whether the effect changes the buffer's text.
    #[must_use]
    pub(crate) const fn mutates_buffer(self) -> bool {
        matches!(self, Self::MutatesBuffer)
    }

    /// Whether the effect moves (or adds) a mark.
    #[must_use]
    pub(crate) const fn moves_mark(self) -> bool {
        matches!(self, Self::MovesMark)
    }
}

/// What each action's default handler would do (design D3; the toolkit
/// sources cited in the spec). Conservative: an action that can lead to a
/// buffer change through a popup (completion, emoji chooser) is recorded as
/// mutating.
#[must_use]
pub(crate) const fn toolkit_effect(action: ProtectedAction) -> ToolkitEffect {
    use ProtectedAction as A;
    match action {
        A::MoveViewport | A::MoveFocus => ToolkitEffect::ViewportOnly,
        A::CopyClipboard => ToolkitEffect::ReadsSelection,
        A::ToggleOverwrite | A::CycleInvisibleCharacters => ToolkitEffect::ChangesViewState,
        A::ToggleCursorVisible => ToolkitEffect::ShowsCaret,
        A::MenuPopup => ToolkitEffect::OpensDefaultMenu,
        A::MoveCursor
        | A::SelectAll
        | A::SetAnchor
        | A::ExtendSelection
        | A::MoveToMatchingBracket
        | A::ToggleBookmarkAtCursor => ToolkitEffect::MovesMark,
        A::CutClipboard
        | A::PasteClipboard
        | A::Backspace
        | A::DeleteFromCursor
        | A::InsertAtCursor
        | A::InsertEmoji
        | A::DeleteSelection
        | A::ClearText
        | A::TextUndo
        | A::TextRedo
        | A::ChangeNumber
        | A::ChangeCase
        | A::JoinLines
        | A::MoveLines
        | A::MoveWords
        | A::ShowCompletion
        | A::PushSnippet
        | A::ImCommit
        | A::MiddleClickPaste
        | A::DndDrop
        | A::SearchReplace
        | A::SearchReplaceAll
        | A::NormalizeLineEndings => ToolkitEffect::MutatesBuffer,
    }
}

/// What Protected View does with an action before its default handler runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActionDisposition {
    /// The default handler may run: it only scrolls. The only admitting
    /// disposition.
    ViewportSafe,
    /// Stop the default handler (stopped signal or claimed key) and publish
    /// [`preemption_notice`], if any.
    Preempt,
    /// Stop the default handler and open the Protected View menu instead.
    RouteToMenu,
    /// Stop the default handler and run LushText's own bounded entry undo
    /// (design D2). Never admits the default undo handler.
    RouteToEntryUndo,
}

/// Decide an action (design D3 step 5, D2's entry undo).
///
/// `entry_undo_available` is the session fact "entered by an edit, undo history
/// on, and the entry edit is still the newest undo step" (see
/// [`ProtectedSession::entry_undo_available`]). Only `TextUndo` routes to the
/// entry undo, and only while it is available; otherwise it is pre-empted like
/// every other non-viewport action. Proved by H3 for the whole enum.
#[must_use]
pub(crate) const fn action_disposition(
    action: ProtectedAction,
    entry_undo_available: bool,
) -> ActionDisposition {
    match toolkit_effect(action) {
        ToolkitEffect::ViewportOnly => ActionDisposition::ViewportSafe,
        ToolkitEffect::OpensDefaultMenu => ActionDisposition::RouteToMenu,
        _ => {
            if matches!(action, ProtectedAction::TextUndo) && entry_undo_available {
                ActionDisposition::RouteToEntryUndo
            } else {
                ActionDisposition::Preempt
            }
        }
    }
}

/// The notice a pre-empted action publishes through the rate-limited lane, or
/// `None` when the executor replaces it silently (`move-cursor` becomes a
/// [`viewport_step`]; a viewport-safe action is not pre-empted at all).
#[must_use]
pub(crate) const fn preemption_notice(action: ProtectedAction) -> Option<ProtectedNotice> {
    match action {
        ProtectedAction::MoveCursor
        | ProtectedAction::MoveViewport
        | ProtectedAction::MoveFocus => None,
        ProtectedAction::SearchReplace | ProtectedAction::SearchReplaceAll => {
            Some(ProtectedNotice::ReplaceUnavailable)
        }
        ProtectedAction::CycleInvisibleCharacters => Some(ProtectedNotice::InvisiblesUnavailable),
        ProtectedAction::ToggleBookmarkAtCursor => Some(ProtectedNotice::BookmarkUnavailable),
        ProtectedAction::NormalizeLineEndings => Some(ProtectedNotice::LineEndingsUnavailable),
        _ => Some(ProtectedNotice::EditRefused),
    }
}

// ---------------------------------------------------------------------------
// Session (D2)
// ---------------------------------------------------------------------------

/// The whole-content installs that keep (or start) Protected View
/// monotonically. A reload from disk is not one of them: it is
/// [`SessionEvent::ReloadFromDisk`], which re-derives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub(crate) enum WholeContentInstall {
    InitialLoad,
    EvictionReload,
    DraftRecovery,
    SetAsideOpen,
    LocalHistoryRestore,
    UndoRestore,
}

/// One event the per-editor Protected View session reacts to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub(crate) enum SessionEvent {
    /// A whole-content install; `protected` is the installed body's
    /// `DocumentScale::protected_view`.
    Installed {
        kind: WholeContentInstall,
        protected: bool,
    },
    /// An external-change reload, an explicit reload, or a reopen with another
    /// encoding (both read the file from disk again); `protected` is the new
    /// content's decision. The executor milestone confirms the reopen mapping.
    ReloadFromDisk { protected: bool },
    /// C1's watch reported that an edit newly set `protected_view`.
    EditGrowth,
    /// The entry undo was requested; `is_entry_step` holds when the newest undo
    /// step is still the entry edit and undo history is on.
    EntryUndo { is_entry_step: bool },
    /// A preference changed (word wrap, invisibles, the minimap, ...).
    PreferenceChanged,
    /// The editor's tab was selected or deselected.
    TabSwitched,
}

/// The per-editor Protected View state (design D2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProtectedSession {
    /// Whether the editor is in Protected View.
    pub(crate) protected: bool,
    /// Whether the current protected session was entered through an edit (the
    /// only kind that may offer the entry undo).
    pub(crate) entered_by_edit: bool,
    /// The protected-session generation: advances on every whole-content
    /// install, reload from disk, entry and exit, so a stale ticket
    /// ([`super::seams::ProtectedViewTicket`]) is refused.
    pub(crate) generation: u64,
}

impl ProtectedSession {
    /// A new editor: editable, generation zero.
    pub(crate) const EDITABLE: Self = Self {
        protected: false,
        entered_by_edit: false,
        generation: 0,
    };

    /// Whether the entry undo may be offered, given whether the newest undo step
    /// is still the entry edit and undo history is on.
    #[must_use]
    pub(crate) const fn entry_undo_available(self, is_entry_step: bool) -> bool {
        self.protected && self.entered_by_edit && is_entry_step
    }

    /// The session after `event` (design D2). `execution.rs` stores the value
    /// and only ever replaces it with this function's result.
    ///
    /// Monotone: a protected session stays protected through every event except
    /// [`SessionEvent::ReloadFromDisk`] with `protected == false`, and
    /// [`SessionEvent::EntryUndo`] with `is_entry_step` on a session entered by
    /// an edit. Every whole-content install, every reload from disk, every
    /// entry and every exit advances the generation; nothing lowers it.
    ///
    /// Proved domain (H3m): every start generation up to `u64::MAX − 6` and
    /// every sequence of up to six events. The generation saturates at
    /// `u64::MAX`, which no editor reaches (one advance per install).
    #[must_use]
    pub(crate) const fn on_event(self, event: SessionEvent) -> Self {
        let next_generation = self.generation.saturating_add(1);
        match event {
            SessionEvent::Installed { protected, .. } => Self {
                protected: self.protected || protected,
                entered_by_edit: false,
                generation: next_generation,
            },
            SessionEvent::ReloadFromDisk { protected } => Self {
                protected,
                entered_by_edit: false,
                generation: next_generation,
            },
            SessionEvent::EditGrowth => {
                if self.protected {
                    self
                } else {
                    Self {
                        protected: true,
                        entered_by_edit: true,
                        generation: next_generation,
                    }
                }
            }
            SessionEvent::EntryUndo { is_entry_step } => {
                if self.entry_undo_available(is_entry_step) {
                    Self {
                        protected: false,
                        entered_by_edit: false,
                        generation: next_generation,
                    }
                } else {
                    self
                }
            }
            SessionEvent::PreferenceChanged | SessionEvent::TabSwitched => self,
        }
    }
}

// ---------------------------------------------------------------------------
// Line Inspector windows (D6)
// ---------------------------------------------------------------------------

/// One inspector window: the half-open column range `[start, end)` of a
/// paragraph, in characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InspectorWindow {
    pub(crate) start: u64,
    pub(crate) end: u64,
}

impl InspectorWindow {
    /// The window's length in characters.
    #[must_use]
    pub(crate) const fn len(self) -> u64 {
        self.end - self.start
    }

    /// Whether `column` lies in the window.
    #[must_use]
    pub(crate) const fn contains(self, column: u64) -> bool {
        self.start <= column && column < self.end
    }
}

/// How many inspector windows tile a paragraph of `paragraph_chars`
/// characters: `ceil(len / n_inspect)`, zero for an empty paragraph.
#[must_use]
pub(crate) const fn inspector_window_count(paragraph_chars: u64, n_inspect: NonZeroU64) -> u64 {
    paragraph_chars.div_ceil(n_inspect.get())
}

/// Window `index` of a paragraph, or `None` past the last.
///
/// The windows tile `[0, len)` in order: window 0 starts at column 0, each
/// later window starts where the previous ended, every window holds between 1
/// and `n_inspect` characters, and the last ends at `len`. Loop-free.
///
/// Proved domain (H4): every `u64` length ≥ 1, `n_inspect` from 1 to
/// [`N_INSPECT`], every index.
#[must_use]
pub(crate) const fn inspector_window(
    paragraph_chars: u64,
    n_inspect: NonZeroU64,
    index: u64,
) -> Option<InspectorWindow> {
    if index >= inspector_window_count(paragraph_chars, n_inspect) {
        return None;
    }
    // index < ceil(len / n), so index · n < len: no overflow.
    let start = index * n_inspect.get();
    let remaining = paragraph_chars - start;
    let width = min_u64(remaining, n_inspect.get());
    Some(InspectorWindow {
        start,
        end: start + width,
    })
}

/// The index and window containing `column`, or `None` when the column is not
/// below the paragraph length. Loop-free; proved with [`inspector_window`]
/// (H4).
#[must_use]
pub(crate) const fn inspector_window_containing(
    paragraph_chars: u64,
    n_inspect: NonZeroU64,
    column: u64,
) -> Option<(u64, InspectorWindow)> {
    if column >= paragraph_chars {
        return None;
    }
    let index = column / n_inspect.get();
    match inspector_window(paragraph_chars, n_inspect, index) {
        Some(window) => Some((index, window)),
        None => None,
    }
}

// ---------------------------------------------------------------------------
// Navigation (D7)
// ---------------------------------------------------------------------------

/// A paragraph's visible extent: its head and its whole length, in characters.
///
/// The executor milestone builds it from `model/protected_fold.rs`'s
/// `FoldRange { head_chars, tail_chars }` (length = head + tail) or, for an
/// unfolded paragraph, from its length alone. Plain integers keep this module
/// independent of that one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParagraphExtent {
    head_chars: u64,
    paragraph_chars: u64,
}

impl ParagraphExtent {
    /// A paragraph showing all its characters.
    #[must_use]
    pub(crate) const fn unfolded(paragraph_chars: u64) -> Self {
        Self {
            head_chars: paragraph_chars,
            paragraph_chars,
        }
    }

    /// A paragraph whose first `head_chars` of `paragraph_chars` characters are
    /// visible, or `None` when the head is longer than the paragraph.
    #[must_use]
    pub(crate) const fn new(head_chars: u64, paragraph_chars: u64) -> Option<Self> {
        if head_chars > paragraph_chars {
            None
        } else {
            Some(Self {
                head_chars,
                paragraph_chars,
            })
        }
    }

    /// Visible characters at the paragraph's start.
    #[must_use]
    pub(crate) const fn head_chars(self) -> u64 {
        self.head_chars
    }

    /// The paragraph's length in characters, delimiter excluded.
    #[must_use]
    pub(crate) const fn paragraph_chars(self) -> u64 {
        self.paragraph_chars
    }

    /// Whether a tail is hidden.
    #[must_use]
    pub(crate) const fn is_folded(self) -> bool {
        self.head_chars < self.paragraph_chars
    }

    /// Whether `column` is shown in the main view: every column of an unfolded
    /// paragraph (its end included), and the head's columns of a folded one.
    #[must_use]
    pub(crate) const fn column_is_visible(self, column: u64) -> bool {
        !self.is_folded() || column < self.head_chars
    }
}

/// Where a navigation into a protected document goes (design D7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavigationTarget {
    /// Scroll the `protected-nav` mark to the paragraph.
    ScrollTo { paragraph: u64 },
    /// Scroll to the paragraph and open the Line Inspector on this window.
    Inspect {
        paragraph: u64,
        window_index: u64,
        window: InspectorWindow,
    },
}

/// Classify a navigation to `column` of `paragraph` (design D7).
///
/// Every navigation entry point calls it: restore position, bookmark
/// activation, local-history targets, workspace-search hits and protected find
/// results. A visible column scrolls; a hidden one opens the inspector on the
/// window containing it, and a column past a folded paragraph's end opens the
/// last window (a stored offset can outlive the content it pointed into).
///
/// Proved domain (H4n): every extent, every `u64` column, `n_inspect` from 1 to
/// [`N_INSPECT`].
#[must_use]
pub(crate) const fn navigation_target(
    paragraph: u64,
    column: u64,
    extent: ParagraphExtent,
    n_inspect: NonZeroU64,
) -> NavigationTarget {
    if extent.column_is_visible(column) {
        return NavigationTarget::ScrollTo { paragraph };
    }
    // Folded, so head < len and len ≥ 1: the clamped column is below len.
    let last = extent.paragraph_chars - 1;
    let target = min_u64(column, last);
    match inspector_window_containing(extent.paragraph_chars, n_inspect, target) {
        Some((window_index, window)) => NavigationTarget::Inspect {
            paragraph,
            window_index,
            window,
        },
        None => NavigationTarget::ScrollTo { paragraph },
    }
}

// ---------------------------------------------------------------------------
// Worker find batches (D5)
// ---------------------------------------------------------------------------

/// The per-turn and total limits of find publication, validated as a unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FindBatchLimits {
    results_per_turn: usize,
    bytes_per_turn: u64,
    result_cap: u64,
    max_record_bytes: u64,
}

impl FindBatchLimits {
    /// The production limits (design D5).
    pub(crate) const PRODUCTION: Self = match Self::new(
        FIND_RESULTS_PER_TURN,
        FIND_BYTES_PER_TURN,
        FIND_RESULT_CAP,
        MAX_FIND_RECORD_BYTES,
    ) {
        Some(limits) => limits,
        None => panic!("the production find limits break FindBatchLimits::new"),
    };

    /// Limits, or `None` unless every cap is at least one and one admitted
    /// record fits one turn (`max_record_bytes ≤ bytes_per_turn`).
    #[must_use]
    pub(crate) const fn new(
        results_per_turn: usize,
        bytes_per_turn: u64,
        result_cap: u64,
        max_record_bytes: u64,
    ) -> Option<Self> {
        if results_per_turn == 0
            || bytes_per_turn == 0
            || result_cap == 0
            || max_record_bytes > bytes_per_turn
        {
            return None;
        }
        Some(Self {
            results_per_turn,
            bytes_per_turn,
            result_cap,
            max_record_bytes,
        })
    }

    #[must_use]
    pub(crate) const fn results_per_turn(self) -> usize {
        self.results_per_turn
    }

    #[must_use]
    pub(crate) const fn bytes_per_turn(self) -> u64 {
        self.bytes_per_turn
    }

    #[must_use]
    pub(crate) const fn result_cap(self) -> u64 {
        self.result_cap
    }

    #[must_use]
    pub(crate) const fn max_record_bytes(self) -> u64 {
        self.max_record_bytes
    }
}

/// One match as the worker finds it: paragraph, column and length in
/// characters, and the byte length of the preview excerpt it would like to
/// carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub(crate) struct FindMatch {
    pub(crate) paragraph: u64,
    pub(crate) column: u64,
    pub(crate) match_chars: u64,
    pub(crate) preview_bytes: u64,
}

/// A match admitted for publication: its preview is cut to the limits' record
/// bound. Only [`admit_find_record`] builds one, so [`next_find_batch`] never
/// sees a record larger than one turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AdmittedRecord {
    paragraph: u64,
    column: u64,
    match_chars: u64,
    bytes: u64,
}

impl AdmittedRecord {
    #[must_use]
    pub(crate) const fn paragraph(self) -> u64 {
        self.paragraph
    }

    #[must_use]
    pub(crate) const fn column(self) -> u64 {
        self.column
    }

    #[must_use]
    pub(crate) const fn match_chars(self) -> u64 {
        self.match_chars
    }

    /// The preview bytes the record may carry: the worker cuts its excerpt to
    /// the last character boundary at or before this length.
    #[must_use]
    pub(crate) const fn bytes(self) -> u64 {
        self.bytes
    }
}

/// Admit one match for publication, cutting its preview to
/// `limits.max_record_bytes()`. Every match is admitted: the bound is on what
/// a record carries, never on which results exist.
#[must_use]
pub(crate) const fn admit_find_record(found: FindMatch, limits: FindBatchLimits) -> AdmittedRecord {
    AdmittedRecord {
        paragraph: found.paragraph,
        column: found.column,
        match_chars: found.match_chars,
        bytes: min_u64(found.preview_bytes, limits.max_record_bytes),
    }
}

/// One GTK turn's publication: the next `count` pending records, carrying
/// `bytes` preview bytes. `count == 0` means the request is finished (nothing
/// pending, or the result cap reached).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FindBatch {
    pub(crate) count: usize,
    pub(crate) bytes: u64,
}

/// Plan the next GTK turn's batch from the front of `pending`, given that
/// `delivered` records were already published (design D5).
///
/// A batch takes at least one record while any remain under the cap, at most
/// `results_per_turn`, at most `result_cap − delivered`, and stops before a
/// record that would carry the batch over `bytes_per_turn`. The first record
/// is always taken, for progress; admission is what keeps it within one turn.
/// Batches taken in turn deliver the first `min(n, result_cap)` records exactly
/// once and in order, in at most `n` turns.
///
/// Proved domain (H4f): up to six admitted records, `results_per_turn` 1–3,
/// `bytes_per_turn` 1–6, every record bound up to it, every result cap. The
/// loop is bounded by `results_per_turn`.
#[must_use]
pub(crate) fn next_find_batch(
    pending: &[AdmittedRecord],
    delivered: u64,
    limits: FindBatchLimits,
) -> FindBatch {
    let remaining = limits.result_cap.saturating_sub(delivered);
    let mut count = 0usize;
    let mut bytes = 0u64;
    for record in pending.iter().take(limits.results_per_turn) {
        if count as u64 >= remaining {
            break;
        }
        let total = bytes.saturating_add(record.bytes);
        if count > 0 && total > limits.bytes_per_turn {
            break;
        }
        count += 1;
        bytes = total;
    }
    FindBatch { count, bytes }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn width(n: u64) -> NonZeroU64 {
        match NonZeroU64::new(n) {
            Some(width) => width,
            None => panic!("zero width"),
        }
    }

    // --- viewport_step -----------------------------------------------------

    #[test]
    fn viewport_step_maps_every_movement_to_a_viewport_motion() {
        use MovementStep as S;
        let cases = [
            (
                S::LogicalPositions,
                1,
                ViewportAxis::Horizontal,
                ViewportMotion::Steps(1),
            ),
            (
                S::VisualPositions,
                -2,
                ViewportAxis::Horizontal,
                ViewportMotion::Steps(-2),
            ),
            (
                S::Words,
                3,
                ViewportAxis::Horizontal,
                ViewportMotion::Steps(3),
            ),
            (
                S::DisplayLines,
                1,
                ViewportAxis::Vertical,
                ViewportMotion::Steps(1),
            ),
            (
                S::Paragraphs,
                -1,
                ViewportAxis::Vertical,
                ViewportMotion::Steps(-1),
            ),
            (
                S::DisplayLineEnds,
                -1,
                ViewportAxis::Horizontal,
                ViewportMotion::Start,
            ),
            (
                S::ParagraphEnds,
                1,
                ViewportAxis::Horizontal,
                ViewportMotion::End,
            ),
            (
                S::Pages,
                2,
                ViewportAxis::Vertical,
                ViewportMotion::Pages(2),
            ),
            (
                S::HorizontalPages,
                -1,
                ViewportAxis::Horizontal,
                ViewportMotion::Pages(-1),
            ),
            (
                S::BufferEnds,
                -1,
                ViewportAxis::Vertical,
                ViewportMotion::Start,
            ),
            (
                S::BufferEnds,
                1,
                ViewportAxis::Vertical,
                ViewportMotion::End,
            ),
        ];
        for (step, count, axis, motion) in cases {
            assert_eq!(
                viewport_step(step, count, false),
                Some(ViewportMove { axis, motion }),
                "{step:?} x {count}"
            );
        }
    }

    #[test]
    fn viewport_step_ignores_shift_and_zero_counts() {
        assert_eq!(
            viewport_step(MovementStep::DisplayLines, 1, true),
            viewport_step(MovementStep::DisplayLines, 1, false)
        );
        assert_eq!(viewport_step(MovementStep::Pages, 0, false), None);
        assert_eq!(viewport_step(MovementStep::BufferEnds, 0, true), None);
    }

    // --- press_disposition -------------------------------------------------

    fn press(
        gesture: CaptureGesture,
        button: PressButton,
        n_press: i32,
        hit: PressHit,
        device: PointerDevice,
    ) -> PressDisposition {
        press_disposition(PressInput {
            gesture,
            button,
            n_press,
            hit,
            device,
        })
    }

    #[test]
    fn press_on_a_fold_marker_opens_the_inspector_once() {
        use CaptureGesture::*;
        let marker = PressHit::FoldMarker { paragraph: 7 };
        assert_eq!(
            press(Click, PressButton::Primary, 1, marker, PointerDevice::Mouse),
            PressDisposition::OpenInspector { paragraph: 7 }
        );
        assert_eq!(
            press(
                Click,
                PressButton::Primary,
                1,
                marker,
                PointerDevice::Touchscreen
            ),
            PressDisposition::OpenInspector { paragraph: 7 }
        );
        assert_eq!(
            press(Click, PressButton::Primary, 2, marker, PointerDevice::Mouse),
            PressDisposition::Claim
        );
        assert_eq!(
            press(Click, PressButton::Middle, 1, marker, PointerDevice::Mouse),
            PressDisposition::Claim
        );
        assert_eq!(
            press(
                Drag,
                PressButton::Primary,
                1,
                marker,
                PointerDevice::Touchscreen
            ),
            PressDisposition::Claim
        );
    }

    #[test]
    fn secondary_click_opens_the_menu_and_other_clicks_are_claimed() {
        use CaptureGesture::*;
        assert_eq!(
            press(
                Click,
                PressButton::Secondary,
                1,
                PressHit::Text,
                PointerDevice::Mouse
            ),
            PressDisposition::OpenMenu
        );
        assert_eq!(
            press(
                Click,
                PressButton::Secondary,
                1,
                PressHit::FoldMarker { paragraph: 1 },
                PointerDevice::Mouse
            ),
            PressDisposition::OpenMenu
        );
        assert_eq!(
            press(
                Click,
                PressButton::Secondary,
                2,
                PressHit::Text,
                PointerDevice::Mouse
            ),
            PressDisposition::Claim
        );
        for n_press in [1, 2, 3] {
            for button in [
                PressButton::Primary,
                PressButton::Middle,
                PressButton::Other,
            ] {
                assert_eq!(
                    press(Click, button, n_press, PressHit::Text, PointerDevice::Mouse),
                    PressDisposition::Claim,
                    "{button:?} x {n_press}"
                );
            }
        }
        assert_eq!(
            press(
                Click,
                PressButton::Primary,
                1,
                PressHit::Gutter,
                PointerDevice::Mouse
            ),
            PressDisposition::Claim
        );
    }

    #[test]
    fn only_a_touchscreen_drag_off_a_marker_stays_unclaimed() {
        use CaptureGesture::*;
        for hit in [PressHit::Text, PressHit::Gutter] {
            assert_eq!(
                press(
                    Drag,
                    PressButton::Primary,
                    1,
                    hit,
                    PointerDevice::Touchscreen
                ),
                PressDisposition::Unclaimed
            );
            for device in [
                PointerDevice::Mouse,
                PointerDevice::Touchpad,
                PointerDevice::Pen,
                PointerDevice::Other,
            ] {
                assert_eq!(
                    press(Drag, PressButton::Primary, 1, hit, device),
                    PressDisposition::Claim
                );
            }
        }
        assert_eq!(
            press(
                Click,
                PressButton::Primary,
                1,
                PressHit::Text,
                PointerDevice::Touchscreen
            ),
            PressDisposition::Claim
        );
    }

    // --- actions -----------------------------------------------------------

    const ALL_ACTIONS: [ProtectedAction; 36] = {
        use ProtectedAction::*;
        [
            MoveCursor,
            MoveViewport,
            MoveFocus,
            ClearText,
            SelectAll,
            ToggleCursorVisible,
            CutClipboard,
            CopyClipboard,
            PasteClipboard,
            Backspace,
            DeleteFromCursor,
            InsertAtCursor,
            ToggleOverwrite,
            SetAnchor,
            ExtendSelection,
            InsertEmoji,
            DeleteSelection,
            MenuPopup,
            TextUndo,
            TextRedo,
            ChangeNumber,
            ChangeCase,
            JoinLines,
            MoveToMatchingBracket,
            MoveLines,
            MoveWords,
            ShowCompletion,
            PushSnippet,
            ImCommit,
            MiddleClickPaste,
            DndDrop,
            SearchReplace,
            SearchReplaceAll,
            NormalizeLineEndings,
            ToggleBookmarkAtCursor,
            CycleInvisibleCharacters,
        ]
    };

    #[test]
    fn only_viewport_actions_are_admitted() {
        for action in ALL_ACTIONS {
            for entry_undo in [false, true] {
                let disposition = action_disposition(action, entry_undo);
                let effect = toolkit_effect(action);
                assert_eq!(
                    disposition == ActionDisposition::ViewportSafe,
                    effect == ToolkitEffect::ViewportOnly,
                    "{action:?}"
                );
                if effect.mutates_buffer() || effect.moves_mark() {
                    assert_ne!(disposition, ActionDisposition::ViewportSafe, "{action:?}");
                }
            }
        }
        assert_eq!(
            action_disposition(ProtectedAction::MoveViewport, false),
            ActionDisposition::ViewportSafe
        );
    }

    #[test]
    fn menu_bindings_route_to_the_protected_menu() {
        assert_eq!(
            action_disposition(ProtectedAction::MenuPopup, false),
            ActionDisposition::RouteToMenu
        );
        assert_eq!(
            action_disposition(ProtectedAction::MenuPopup, true),
            ActionDisposition::RouteToMenu
        );
    }

    #[test]
    fn undo_routes_to_the_entry_undo_only_while_it_is_available() {
        assert_eq!(
            action_disposition(ProtectedAction::TextUndo, true),
            ActionDisposition::RouteToEntryUndo
        );
        assert_eq!(
            action_disposition(ProtectedAction::TextUndo, false),
            ActionDisposition::Preempt
        );
        for action in ALL_ACTIONS {
            if action != ProtectedAction::TextUndo {
                assert_ne!(
                    action_disposition(action, true),
                    ActionDisposition::RouteToEntryUndo
                );
            }
        }
        assert_eq!(
            action_disposition(ProtectedAction::TextRedo, true),
            ActionDisposition::Preempt
        );
    }

    #[test]
    fn the_named_mutating_actions_are_recorded_as_mutating() {
        use ProtectedAction::*;
        for action in [
            InsertAtCursor,
            DeleteFromCursor,
            Backspace,
            CutClipboard,
            PasteClipboard,
            MiddleClickPaste,
            MoveLines,
            MoveWords,
            JoinLines,
            ChangeCase,
            ChangeNumber,
            InsertEmoji,
            PushSnippet,
            TextUndo,
            TextRedo,
            DndDrop,
            SearchReplace,
            SearchReplaceAll,
            NormalizeLineEndings,
            ImCommit,
        ] {
            assert_eq!(
                toolkit_effect(action),
                ToolkitEffect::MutatesBuffer,
                "{action:?}"
            );
        }
        for action in [
            SelectAll,
            SetAnchor,
            ExtendSelection,
            MoveToMatchingBracket,
            MoveCursor,
        ] {
            assert_eq!(
                toolkit_effect(action),
                ToolkitEffect::MovesMark,
                "{action:?}"
            );
        }
        assert_eq!(
            toolkit_effect(ToggleCursorVisible),
            ToolkitEffect::ShowsCaret
        );
        assert_eq!(toolkit_effect(CopyClipboard), ToolkitEffect::ReadsSelection);
        assert_eq!(
            toolkit_effect(ToggleOverwrite),
            ToolkitEffect::ChangesViewState
        );
    }

    #[test]
    fn toolkit_names_resolve_including_change_number_on_ctrl_shift_x() {
        assert_eq!(
            ProtectedAction::from_toolkit_name("change-number"),
            Some(ProtectedAction::ChangeNumber)
        );
        assert_eq!(
            ProtectedAction::from_toolkit_name("select-all"),
            Some(ProtectedAction::SelectAll)
        );
        assert_eq!(
            ProtectedAction::from_toolkit_name("menu.popup"),
            Some(ProtectedAction::MenuPopup)
        );
        assert_eq!(
            ProtectedAction::from_toolkit_name("source.change-case"),
            Some(ProtectedAction::ChangeCase)
        );
        assert_eq!(ProtectedAction::from_toolkit_name("no-such-binding"), None);
        for (name, action) in TOOLKIT_NAMES {
            assert_eq!(
                ProtectedAction::from_toolkit_name(name),
                Some(action),
                "{name}"
            );
        }
        let mut names: Vec<_> = TOOLKIT_NAMES.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), TOOLKIT_NAMES.len(), "duplicate toolkit name");
    }

    #[test]
    fn preemption_notices_name_the_right_reason() {
        assert_eq!(preemption_notice(ProtectedAction::MoveCursor), None);
        assert_eq!(preemption_notice(ProtectedAction::MoveViewport), None);
        assert_eq!(
            preemption_notice(ProtectedAction::SearchReplace),
            Some(ProtectedNotice::ReplaceUnavailable)
        );
        assert_eq!(
            preemption_notice(ProtectedAction::SearchReplaceAll),
            Some(ProtectedNotice::ReplaceUnavailable)
        );
        assert_eq!(
            preemption_notice(ProtectedAction::CycleInvisibleCharacters),
            Some(ProtectedNotice::InvisiblesUnavailable)
        );
        assert_eq!(
            preemption_notice(ProtectedAction::PasteClipboard),
            Some(ProtectedNotice::EditRefused)
        );
    }

    #[test]
    fn notice_texts_are_bounded_and_distinct() {
        let notices = [
            ProtectedNotice::EditRefused,
            ProtectedNotice::ReplaceUnavailable,
            ProtectedNotice::InvisiblesUnavailable,
            ProtectedNotice::PrintRefused,
            ProtectedNotice::BookmarkUnavailable,
            ProtectedNotice::LineEndingsUnavailable,
            ProtectedNotice::SaveAsRefusedInexactDecode,
        ];
        for (i, a) in notices.iter().enumerate() {
            let text = notice_text(*a);
            assert!(!text.is_empty() && text.len() <= 160, "{a:?}");
            assert!(text.contains("very long lines"), "{a:?}");
            for b in &notices[i + 1..] {
                assert_ne!(text, notice_text(*b));
            }
        }
        assert!(notice_text(ProtectedNotice::EditRefused).contains("Inspect Line"));
        assert!(notice_text(ProtectedNotice::EditRefused).contains(INSPECT_LINE_SHORTCUT_LABEL));
    }

    #[test]
    fn copy_whole_line_names_the_line_and_its_length() {
        assert_eq!(
            whole_line_copied_text(1, 50_007),
            "Line 2 copied (50007 characters)"
        );
        assert_eq!(whole_line_copied_text(0, 1), "Line 1 copied (1 character)");
        assert_eq!(
            whole_line_copied_text(u64::MAX, 0),
            format!("Line {} copied (0 characters)", u64::MAX)
        );
    }

    #[test]
    fn the_notice_is_rate_limited_per_reason() {
        let edit = ProtectedNotice::EditRefused;
        let replace = ProtectedNotice::ReplaceUnavailable;
        let mut times = NoticeTimes::default();
        assert!(notice_due(times.last(edit), 0));
        times.record(edit, 1_000);
        assert!(!notice_due(times.last(edit), 1_001));
        assert!(!notice_due(times.last(edit), 1_000 + NOTICE_QUIET_US - 1));
        assert!(notice_due(times.last(edit), 1_000 + NOTICE_QUIET_US));
        // Another reason has its own slot ...
        assert!(notice_due(times.last(replace), 1_001));
        times.record(replace, 2);
        // ... and publishing it does not let this one through again inside
        // its quiet interval (review S5).
        assert!(!notice_due(times.last(edit), 3_000));
        // A clock that went backwards never silences the notice for good.
        assert!(notice_due(Some(10), 5));
        assert!(notice_due(Some(i64::MIN), i64::MAX));
        // Every reason has a distinct slot.
        let all = [
            ProtectedNotice::EditRefused,
            ProtectedNotice::ReplaceUnavailable,
            ProtectedNotice::InvisiblesUnavailable,
            ProtectedNotice::PrintRefused,
            ProtectedNotice::BookmarkUnavailable,
            ProtectedNotice::LineEndingsUnavailable,
            ProtectedNotice::SaveAsRefusedInexactDecode,
        ];
        let mut slots: Vec<usize> = all.iter().map(|notice| notice_slot(*notice)).collect();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), NOTICE_REASONS);
    }

    #[test]
    fn the_alert_offers_one_bounded_action_set() {
        let plain = protected_alert(ProtectedAlertFacts {
            folded_paragraphs: 3,
            entry_undo_available: false,
            restored_draft: None,
        });
        assert_eq!(plain.primary, ProtectedAlertPrimary::InspectLine);
        assert!(plain.body.contains("3 long lines"));
        assert!(plain.body.contains(INSPECT_LINE_SHORTCUT_LABEL));
        assert_eq!(plain.secondary_label, None);

        let undo = protected_alert(ProtectedAlertFacts {
            folded_paragraphs: 1,
            entry_undo_available: true,
            restored_draft: None,
        });
        assert_eq!(undo.primary, ProtectedAlertPrimary::EntryUndo);
        assert!(undo.body.contains("1 long line is"));
        assert!(!undo.body.to_lowercase().contains("keystroke"));

        for has_backing_file in [true, false] {
            let draft = protected_alert(ProtectedAlertFacts {
                folded_paragraphs: 2,
                entry_undo_available: true,
                restored_draft: Some(has_backing_file),
            });
            assert_eq!(draft.primary, ProtectedAlertPrimary::DiscardDraft);
            assert_eq!(
                draft.secondary_label,
                Some(if has_backing_file {
                    "_Save…"
                } else {
                    "Save _As…"
                })
            );
            assert!(draft.body.contains("restored"));
            assert!(draft.body.contains(INSPECT_LINE_SHORTCUT_LABEL));
        }
        for copy in [plain, undo] {
            assert!(copy.body.chars().count() <= 320, "{copy:?}");
        }
    }

    // --- session -----------------------------------------------------------

    const PROTECTED_ON_LOAD: ProtectedSession = ProtectedSession {
        protected: true,
        entered_by_edit: false,
        generation: 1,
    };

    #[test]
    fn installs_are_monotone_and_advance_the_generation() {
        let kinds = [
            WholeContentInstall::InitialLoad,
            WholeContentInstall::EvictionReload,
            WholeContentInstall::DraftRecovery,
            WholeContentInstall::SetAsideOpen,
            WholeContentInstall::LocalHistoryRestore,
            WholeContentInstall::UndoRestore,
        ];
        for kind in kinds {
            let kept = PROTECTED_ON_LOAD.on_event(SessionEvent::Installed {
                kind,
                protected: false,
            });
            assert!(kept.protected, "{kind:?}");
            assert_eq!(kept.generation, 2);
            let entered = ProtectedSession::EDITABLE.on_event(SessionEvent::Installed {
                kind,
                protected: true,
            });
            assert_eq!(
                entered,
                ProtectedSession {
                    protected: true,
                    entered_by_edit: false,
                    generation: 1
                }
            );
            let stays_editable = ProtectedSession::EDITABLE.on_event(SessionEvent::Installed {
                kind,
                protected: false,
            });
            assert!(!stays_editable.protected);
            assert_eq!(stays_editable.generation, 1);
        }
    }

    #[test]
    fn reload_from_disk_re_derives() {
        let shrunk = PROTECTED_ON_LOAD.on_event(SessionEvent::ReloadFromDisk { protected: false });
        assert_eq!(
            shrunk,
            ProtectedSession {
                protected: false,
                entered_by_edit: false,
                generation: 2
            }
        );
        let still = PROTECTED_ON_LOAD.on_event(SessionEvent::ReloadFromDisk { protected: true });
        assert_eq!(
            still,
            ProtectedSession {
                protected: true,
                entered_by_edit: false,
                generation: 2
            }
        );
    }

    #[test]
    fn edit_growth_enters_once_with_the_entry_undo() {
        let entered = ProtectedSession::EDITABLE.on_event(SessionEvent::EditGrowth);
        assert_eq!(
            entered,
            ProtectedSession {
                protected: true,
                entered_by_edit: true,
                generation: 1
            }
        );
        assert!(entered.entry_undo_available(true));
        assert!(!entered.entry_undo_available(false));
        assert_eq!(entered.on_event(SessionEvent::EditGrowth), entered);
        assert_eq!(
            PROTECTED_ON_LOAD.on_event(SessionEvent::EditGrowth),
            PROTECTED_ON_LOAD
        );
    }

    #[test]
    fn entry_undo_exits_only_an_edit_entry_at_the_entry_step() {
        let entered = ProtectedSession::EDITABLE.on_event(SessionEvent::EditGrowth);
        let undone = entered.on_event(SessionEvent::EntryUndo {
            is_entry_step: true,
        });
        assert_eq!(
            undone,
            ProtectedSession {
                protected: false,
                entered_by_edit: false,
                generation: 2
            }
        );
        assert_eq!(
            entered.on_event(SessionEvent::EntryUndo {
                is_entry_step: false
            }),
            entered
        );
        assert_eq!(
            PROTECTED_ON_LOAD.on_event(SessionEvent::EntryUndo {
                is_entry_step: true
            }),
            PROTECTED_ON_LOAD
        );
        assert!(!PROTECTED_ON_LOAD.entry_undo_available(true));
        assert_eq!(
            ProtectedSession::EDITABLE.on_event(SessionEvent::EntryUndo {
                is_entry_step: true
            }),
            ProtectedSession::EDITABLE
        );
        // An install over an edit entry withdraws the entry undo.
        let reinstalled = entered.on_event(SessionEvent::Installed {
            kind: WholeContentInstall::DraftRecovery,
            protected: false,
        });
        assert!(reinstalled.protected && !reinstalled.entered_by_edit);
        assert!(!reinstalled.entry_undo_available(true));
    }

    #[test]
    fn preferences_and_tab_switches_change_nothing() {
        let entered = ProtectedSession::EDITABLE.on_event(SessionEvent::EditGrowth);
        for session in [ProtectedSession::EDITABLE, PROTECTED_ON_LOAD, entered] {
            assert_eq!(session.on_event(SessionEvent::PreferenceChanged), session);
            assert_eq!(session.on_event(SessionEvent::TabSwitched), session);
        }
    }

    #[test]
    fn generation_saturates_at_the_top() {
        let top = ProtectedSession {
            protected: true,
            entered_by_edit: false,
            generation: u64::MAX,
        };
        let next = top.on_event(SessionEvent::ReloadFromDisk { protected: true });
        assert_eq!(next.generation, u64::MAX);
    }

    // --- inspector windows -------------------------------------------------

    #[test]
    fn inspector_windows_tile_the_paragraph() {
        for (len, n) in [
            (1, 1),
            (1, 500),
            (499, 500),
            (500, 500),
            (501, 500),
            (1_900_000, 500),
            (7, 3),
        ] {
            let n = width(n);
            let count = inspector_window_count(len, n);
            assert!(count >= 1);
            let mut expected_start = 0;
            for index in 0..count {
                let window = inspector_window(len, n, index).expect("window below the count");
                assert_eq!(window.start, expected_start);
                assert!(window.len() >= 1 && window.len() <= n.get());
                expected_start = window.end;
            }
            assert_eq!(expected_start, len);
            assert_eq!(inspector_window(len, n, count), None);
        }
        assert_eq!(inspector_window_count(0, width(500)), 0);
        assert_eq!(inspector_window(0, width(500), 0), None);
    }

    #[test]
    fn inspector_window_counts_round_up() {
        assert_eq!(inspector_window_count(500, width(500)), 1);
        assert_eq!(inspector_window_count(501, width(500)), 2);
        assert_eq!(inspector_window_count(1_900_000, N_INSPECT_WIDTH), 3_800);
        assert_eq!(inspector_window_count(u64::MAX, width(1)), u64::MAX);
        let last = inspector_window(u64::MAX, width(500), u64::MAX / 500).expect("last window");
        assert_eq!(last.end, u64::MAX);
    }

    #[test]
    fn inspector_window_containing_finds_the_column() {
        let n = N_INSPECT_WIDTH;
        let (index, window) = inspector_window_containing(1_900_000, n, 950_000).expect("in range");
        assert_eq!(index, 1_900);
        assert_eq!(
            window,
            InspectorWindow {
                start: 950_000,
                end: 950_500
            }
        );
        assert!(window.contains(950_000));
        assert!(!window.contains(950_500));
        assert_eq!(
            inspector_window_containing(1_000, n, 999),
            Some((
                1,
                InspectorWindow {
                    start: 500,
                    end: 1_000
                }
            ))
        );
        assert_eq!(inspector_window_containing(1_000, n, 1_000), None);
        assert_eq!(inspector_window_containing(0, n, 0), None);
    }

    // --- navigation --------------------------------------------------------

    proptest::proptest! {
        /// The inspector tiling over the whole `u64` range (design D12's H4):
        /// the window at any index below the count holds 1 to `n` characters,
        /// starts at `index · n`, abuts the next window, the last ends at the
        /// length, and the window containing any column holds it. CBMC could
        /// not finish this statement in 20 minutes at a symbolic or at the
        /// production width (64-bit division), so a property test over the
        /// full range and the exhaustive small-width test below carry it.
        #[test]
        fn inspector_windows_tile_over_the_whole_range(
            len in 1..=u64::MAX,
            n_raw in proptest::prop_oneof![
                proptest::strategy::Just(N_INSPECT),
                1..=N_INSPECT,
            ],
            index_seed in proptest::prelude::any::<u64>(),
            column_seed in proptest::prelude::any::<u64>(),
        ) {
            let n = NonZeroU64::new(n_raw).expect("non-zero width");
            let count = inspector_window_count(len, n);
            proptest::prop_assert!(count >= 1);
            proptest::prop_assert_eq!(inspector_window(len, n, count), None);
            let index = index_seed % count;
            let window = inspector_window(len, n, index).expect("window below count");
            proptest::prop_assert!(window.len() >= 1 && window.len() <= n_raw);
            proptest::prop_assert_eq!(
                u128::from(window.start),
                u128::from(index) * u128::from(n_raw)
            );
            if index + 1 < count {
                let next = inspector_window(len, n, index + 1).expect("next window");
                proptest::prop_assert_eq!(next.start, window.end);
            } else {
                proptest::prop_assert_eq!(window.end, len);
            }
            let column = column_seed % len;
            let (found_index, found) =
                inspector_window_containing(len, n, column).expect("column below length");
            proptest::prop_assert!(found.contains(column));
            proptest::prop_assert_eq!(inspector_window(len, n, found_index), Some(found));
        }
    }

    /// The part of the tiling statement Kani's H4 leaves to tests: every width
    /// other than the production one, exhaustively over small paragraphs.
    #[test]
    fn inspector_windows_tile_for_every_small_width() {
        for n_raw in 1..=7u64 {
            let n = NonZeroU64::new(n_raw).expect("non-zero width");
            for len in 1..=40u64 {
                let count = inspector_window_count(len, n);
                assert!(count >= 1);
                assert_eq!(inspector_window(len, n, count), None);
                let mut expected_start = 0;
                for index in 0..count {
                    let window = inspector_window(len, n, index).expect("window below count");
                    assert_eq!(window.start, expected_start);
                    assert!(window.len() >= 1 && window.len() <= n_raw);
                    expected_start = window.end;
                }
                assert_eq!(expected_start, len);
                for column in 0..len {
                    let (index, window) =
                        inspector_window_containing(len, n, column).expect("column below length");
                    assert!(window.contains(column));
                    assert_eq!(inspector_window(len, n, index), Some(window));
                }
                assert_eq!(inspector_window_containing(len, n, len), None);
            }
        }
    }

    /// The part of the navigation statement Kani's H4n leaves to tests: every
    /// width other than the production one, exhaustively over small paragraphs.
    #[test]
    fn navigation_targets_hold_for_every_small_width() {
        for n_raw in 1..=7u64 {
            let n = NonZeroU64::new(n_raw).expect("non-zero width");
            for len in 0..=20u64 {
                for head in 0..=len {
                    let extent = ParagraphExtent::new(head, len).expect("head within length");
                    for column in 0..=len + 2 {
                        let visible = head == len || column < head;
                        match navigation_target(9, column, extent, n) {
                            NavigationTarget::ScrollTo { paragraph } => {
                                assert_eq!(paragraph, 9);
                                assert!(visible, "n={n_raw} len={len} head={head} column={column}");
                            }
                            NavigationTarget::Inspect {
                                paragraph,
                                window_index,
                                window,
                            } => {
                                assert_eq!(paragraph, 9);
                                assert!(!visible);
                                let clamped = column.min(len - 1);
                                assert!(window.contains(clamped));
                                assert_eq!(inspector_window(len, n, window_index), Some(window));
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn navigation_scrolls_to_visible_columns() {
        let n = N_INSPECT_WIDTH;
        let unfolded = ParagraphExtent::unfolded(600);
        for column in [0, 599, 600, 10_000] {
            assert_eq!(
                navigation_target(3, column, unfolded, n),
                NavigationTarget::ScrollTo { paragraph: 3 }
            );
        }
        let folded = ParagraphExtent::new(15_000, 1_900_000).expect("valid");
        assert_eq!(
            navigation_target(4, 14_999, folded, n),
            NavigationTarget::ScrollTo { paragraph: 4 }
        );
    }

    #[test]
    fn navigation_into_a_hidden_tail_opens_the_containing_window() {
        let n = N_INSPECT_WIDTH;
        let folded = ParagraphExtent::new(15_000, 1_900_000).expect("valid");
        assert_eq!(
            navigation_target(4, 15_000, folded, n),
            NavigationTarget::Inspect {
                paragraph: 4,
                window_index: 30,
                window: InspectorWindow {
                    start: 15_000,
                    end: 15_500
                },
            }
        );
        assert_eq!(
            navigation_target(4, 950_000, folded, n),
            NavigationTarget::Inspect {
                paragraph: 4,
                window_index: 1_900,
                window: InspectorWindow {
                    start: 950_000,
                    end: 950_500
                },
            }
        );
        // The paragraph end and anything past it open the last window.
        let last = NavigationTarget::Inspect {
            paragraph: 4,
            window_index: 3_799,
            window: InspectorWindow {
                start: 1_899_500,
                end: 1_900_000,
            },
        };
        assert_eq!(navigation_target(4, 1_900_000, folded, n), last);
        assert_eq!(navigation_target(4, u64::MAX, folded, n), last);
    }

    #[test]
    fn paragraph_extents_validate_and_classify() {
        assert_eq!(ParagraphExtent::new(11, 10), None);
        let empty_head = ParagraphExtent::new(0, 10).expect("valid");
        assert!(empty_head.is_folded() && !empty_head.column_is_visible(0));
        let whole = ParagraphExtent::new(10, 10).expect("valid");
        assert!(!whole.is_folded());
        assert_eq!(whole, ParagraphExtent::unfolded(10));
        assert_eq!((whole.head_chars(), whole.paragraph_chars()), (10, 10));
        let empty = ParagraphExtent::unfolded(0);
        assert!(!empty.is_folded() && empty.column_is_visible(0));
    }

    // --- find batches ------------------------------------------------------

    fn limits(per_turn: usize, bytes: u64, cap: u64, record: u64) -> FindBatchLimits {
        FindBatchLimits::new(per_turn, bytes, cap, record).expect("valid limits")
    }

    fn admitted(preview_bytes: &[u64], limits: FindBatchLimits) -> Vec<AdmittedRecord> {
        preview_bytes
            .iter()
            .enumerate()
            .map(|(i, &preview_bytes)| {
                admit_find_record(
                    FindMatch {
                        paragraph: i as u64,
                        column: 2 * i as u64,
                        match_chars: 3,
                        preview_bytes,
                    },
                    limits,
                )
            })
            .collect()
    }

    #[test]
    fn find_limits_validate() {
        assert_eq!(FindBatchLimits::new(0, 1, 1, 1), None);
        assert_eq!(FindBatchLimits::new(1, 0, 1, 0), None);
        assert_eq!(FindBatchLimits::new(1, 1, 0, 1), None);
        assert_eq!(FindBatchLimits::new(1, 4, 1, 5), None);
        let production = FindBatchLimits::PRODUCTION;
        assert_eq!(production.results_per_turn(), FIND_RESULTS_PER_TURN);
        assert_eq!(production.bytes_per_turn(), FIND_BYTES_PER_TURN);
        assert_eq!(production.result_cap(), FIND_RESULT_CAP);
        assert_eq!(production.max_record_bytes(), MAX_FIND_RECORD_BYTES);
    }

    #[test]
    fn admission_cuts_only_the_preview() {
        let limits = limits(4, 10, 100, 6);
        let record = admit_find_record(
            FindMatch {
                paragraph: 9,
                column: 1_000_000,
                match_chars: 12,
                preview_bytes: 900,
            },
            limits,
        );
        assert_eq!(
            (
                record.paragraph(),
                record.column(),
                record.match_chars(),
                record.bytes()
            ),
            (9, 1_000_000, 12, 6)
        );
        let small = admit_find_record(
            FindMatch {
                paragraph: 0,
                column: 0,
                match_chars: 1,
                preview_bytes: 5,
            },
            limits,
        );
        assert_eq!(small.bytes(), 5);
    }

    #[test]
    fn find_batches_respect_the_count_byte_and_total_caps() {
        let limits = limits(3, 10, 5, 6);
        let records = admitted(&[6, 6, 2, 2, 2, 2, 2], limits);
        let mut delivered = 0u64;
        let mut position = 0usize;
        let mut batches = Vec::new();
        loop {
            let batch = next_find_batch(&records[position..], delivered, limits);
            if batch.count == 0 {
                break;
            }
            assert!(batch.count <= 3 && batch.bytes <= 10);
            batches.push(batch);
            position += batch.count;
            delivered += batch.count as u64;
        }
        assert_eq!(
            batches,
            vec![
                FindBatch { count: 1, bytes: 6 },
                FindBatch {
                    count: 3,
                    bytes: 10
                },
                FindBatch { count: 1, bytes: 2 },
            ]
        );
        assert_eq!(delivered, 5);
    }

    #[test]
    fn find_batches_finish_on_an_empty_queue_or_a_reached_cap() {
        let limits = limits(3, 10, 2, 6);
        assert_eq!(
            next_find_batch(&[], 0, limits),
            FindBatch { count: 0, bytes: 0 }
        );
        let records = admitted(&[1, 1, 1], limits);
        assert_eq!(
            next_find_batch(&records, 2, limits),
            FindBatch { count: 0, bytes: 0 }
        );
        assert_eq!(
            next_find_batch(&records, 5, limits),
            FindBatch { count: 0, bytes: 0 }
        );
        assert_eq!(
            next_find_batch(&records, 1, limits),
            FindBatch { count: 1, bytes: 1 }
        );
    }

    #[test]
    fn production_find_batches_publish_at_most_the_declared_turn() {
        let limits = FindBatchLimits::PRODUCTION;
        let records = admitted(&vec![10_000; 1_000], limits);
        let batch = next_find_batch(&records, 0, limits);
        assert_eq!(
            batch.count,
            (FIND_BYTES_PER_TURN / MAX_FIND_RECORD_BYTES) as usize
        );
        assert!(batch.bytes <= FIND_BYTES_PER_TURN);
        let tiny = admitted(&vec![0; 1_000], limits);
        assert_eq!(
            next_find_batch(&tiny, 0, limits).count,
            FIND_RESULTS_PER_TURN
        );
        assert_eq!(next_find_batch(&tiny, FIND_RESULT_CAP - 3, limits).count, 3);
    }

    #[test]
    fn copy_chunk_holds_at_least_one_scalar() {
        const { assert!(COPY_CHUNK_BYTES >= 4) };
        assert_eq!(N_INSPECT_WIDTH.get(), N_INSPECT);
        const { assert!(N_INSPECT <= L_PROTECT) };
    }
}
