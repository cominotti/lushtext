// SPDX-License-Identifier: GPL-3.0-or-later

//! The pure half of held typing (`keep-keystrokes-during-install`): what
//! happens to text the user types while a workflow owns the document's buffer.
//!
//! **Cross-cutting pure policy, no role.** Its GTK half is
//! `ui/editor_page/held_input.rs`, called from the buffer-level edit lock's
//! handlers (`ui/editor_page/edit_lock.rs`), which the document load, the
//! whole-buffer replacement and the save share.
//!
//! - [`HeldText`] is the held buffer: whole-or-nothing appends under
//!   [`HELD_TEXT_CAP_BYTES`], and Backspace on what was held.
//! - [`insert_disposition`] decides whether one insertion is held, refused,
//!   or passes (design D2).
//! - [`HeldEpisode`] folds the content disposition each lock release reports
//!   (design D5), and [`settle_verdict`] decides, when nothing owns the buffer
//!   any more, whether the held text is inserted into the document or rescued
//!   into a new Untitled tab (design D4, D5). Held text is never discarded.
//! - [`close_disposition`] decides what a close does with held text: flush it
//!   into the document first, or refuse the close while a workflow still owns
//!   the buffer (design D10).

/// The held buffer's cap: 4 KiB of UTF-8.
///
/// Kind: **presentation cap** (per held episode), a power of two (`.agents/rules/rust.md`).
/// The longest hold is one settle (the responsiveness lane's settle cap is
/// 10 s); a fast typist at 20 characters per second types 200 bytes of ASCII,
/// 800 of four-byte UTF-8, in that time, so the cap is five times the worst
/// typing. It also keeps the one replay insert small against `B_SETTLE`
/// (design D6, D8). An insertion that does not fit whole is refused whole.
pub const HELD_TEXT_CAP_BYTES: usize = 4 * 1024;

/// What a held insertion did to the held buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldOutcome {
    /// The text was appended whole.
    Held,
    /// The text did not fit whole and was not appended.
    Overflow,
}

/// The text held for one editor, in the order it was typed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HeldText {
    text: String,
}

impl HeldText {
    /// Append `typed` whole, or not at all when it would exceed
    /// [`HELD_TEXT_CAP_BYTES`].
    pub fn hold(&mut self, typed: &str) -> HoldOutcome {
        if self.text.len().saturating_add(typed.len()) > HELD_TEXT_CAP_BYTES {
            return HoldOutcome::Overflow;
        }
        self.text.push_str(typed);
        HoldOutcome::Held
    }

    /// Remove the last held character (Backspace on held text). Returns
    /// whether there was one.
    pub fn backspace(&mut self) -> bool {
        self.text.pop().is_some()
    }

    /// Whether nothing is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Characters held.
    #[must_use]
    pub fn char_count(&self) -> u64 {
        u64::try_from(self.text.chars().count()).unwrap_or(u64::MAX)
    }

    /// Bytes held: what a user action's start records, so an overflow can
    /// truncate back to it (paste atomicity).
    #[must_use]
    pub fn len_bytes(&self) -> usize {
        self.text.len()
    }

    /// Drop everything held after `len` bytes: the part of one user action
    /// that was held before a later segment of it overflowed. `len` is a
    /// length this buffer had earlier, so it is a character boundary; any
    /// other length is ignored.
    pub fn truncate_to(&mut self, len: usize) {
        if self.text.is_char_boundary(len) {
            self.text.truncate(len);
        }
    }

    /// The held text, for the input method's surrounding text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Apply an input method's `delete-surrounding` to the held text, which
    /// sits just before the caret: `offset` characters from the caret
    /// (negative is before it), `n_chars` long. Applied only when the range
    /// falls inside the held text; returns whether it did.
    pub fn delete_surrounding(&mut self, offset: i32, n_chars: i32) -> bool {
        let Ok(n_chars) = usize::try_from(n_chars) else {
            return false;
        };
        let Some(back) = offset.checked_neg().and_then(|n| usize::try_from(n).ok()) else {
            return false;
        };
        if n_chars == 0 || n_chars > back {
            return false;
        }
        let held = self.text.chars().count();
        if back > held {
            return false;
        }
        let first = held - back;
        let start = self.byte_at_char(first);
        let end = self.byte_at_char(first + n_chars);
        self.text.replace_range(start..end, "");
        true
    }

    fn byte_at_char(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map_or(self.text.len(), |(index, _)| index)
    }

    /// Take the held text, leaving the buffer empty.
    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.text)
    }
}

/// What one edit-lock release left in the buffer (design D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum ContentDisposition {
    /// A whole document was published (a completed install or replacement).
    WholeContent,
    /// The content is as it was before the lock (a save, a replacement
    /// cancelled before its first mutation).
    Unchanged,
    /// The buffer was left partial or emptied (a superseded or cancelled
    /// install, a cancelled replacement that had cleared text).
    Unpublished,
}

/// Where a replay inserts the held text (design D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayAnchor {
    /// At the caret the document is shown with: a release published a whole
    /// document, whose terminal placed the caret.
    Caret,
    /// Where the caret was when the first character was held: every release
    /// left the content as it was (a save), so that position still names the
    /// place the user typed at, whatever a later click moved the caret to.
    FirstHold,
}

/// Whether the episode's buffer is still a document held text may go into,
/// and where in it the text goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeldEpisode {
    broken: bool,
    republished: bool,
}

impl HeldEpisode {
    /// A new episode: the document the user typed into was whole.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            broken: false,
            republished: false,
        }
    }

    /// Fold one lock release into the episode.
    #[must_use]
    pub const fn after_release(self, release: ContentDisposition) -> Self {
        match release {
            ContentDisposition::WholeContent => Self {
                broken: false,
                republished: true,
            },
            ContentDisposition::Unchanged => self,
            ContentDisposition::Unpublished => Self {
                broken: true,
                republished: self.republished,
            },
        }
    }

    /// Where the replay goes: the published document's caret after a whole
    /// document was published, otherwise the first hold's position.
    #[must_use]
    pub const fn anchor(self) -> ReplayAnchor {
        if self.republished {
            ReplayAnchor::Caret
        } else {
            ReplayAnchor::FirstHold
        }
    }

    /// Whether a release left the buffer partial or emptied with no whole
    /// document published after it.
    #[must_use]
    pub const fn is_broken(self) -> bool {
        self.broken
    }
}

/// The facts one insertion is decided on (design D2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one independent fact the decision reads; an enum would encode combinations"
)]
pub struct InsertFacts {
    /// The owning workflow's own edit (`edit_lock::with_own_edit`).
    pub own_edit: bool,
    /// A workflow holds the edit lock.
    pub locked: bool,
    /// The editor holds typed input: locked, a load in flight, or held text
    /// waiting for its replay.
    pub held: bool,
    /// The insertion is inside a user action.
    pub user_action: bool,
    /// The insertion is at the caret (the insert mark).
    pub at_caret: bool,
    /// The insertion's user action already had a deletion refused.
    pub tainted: bool,
}

/// What happens to one insertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertDisposition {
    /// The insertion reaches the buffer.
    Pass,
    /// The insertion is appended to the held text and stopped.
    Hold,
    /// The insertion is stopped and not kept. `notify`: whether the user is
    /// told (a user's own edit).
    Refuse {
        /// Whether a status warning is due.
        notify: bool,
    },
}

/// Decide one insertion (design D2): held when it is typed at the caret, in a
/// user action that deleted nothing, while the editor holds input; refused
/// when the lock refuses it today, or when it is a user's edit the held
/// window cannot keep; otherwise it passes.
#[must_use]
pub const fn insert_disposition(facts: InsertFacts) -> InsertDisposition {
    if facts.own_edit || !facts.held {
        return InsertDisposition::Pass;
    }
    if facts.user_action {
        if facts.at_caret && !facts.tainted {
            InsertDisposition::Hold
        } else {
            InsertDisposition::Refuse { notify: true }
        }
    } else if facts.locked {
        InsertDisposition::Refuse { notify: false }
    } else {
        InsertDisposition::Pass
    }
}

/// The facts one deletion (or undo/redo) is decided on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct RemovalFacts {
    /// The owning workflow's own edit.
    pub own_edit: bool,
    /// A workflow holds the edit lock.
    pub locked: bool,
    /// The editor holds typed input.
    pub held: bool,
    /// The edit is a user's: inside a user action, or an undo/redo request.
    pub user_edit: bool,
}

/// Decide one deletion, undo or redo (design D3): refused while locked, as the
/// lock refuses it, and refused while held when it is a user's edit, which the
/// install would undo anyway. A user's refused edit is reported.
#[must_use]
pub const fn removal_disposition(facts: RemovalFacts) -> InsertDisposition {
    if facts.own_edit {
        return InsertDisposition::Pass;
    }
    if facts.held && facts.user_edit {
        return InsertDisposition::Refuse { notify: true };
    }
    if facts.locked {
        return InsertDisposition::Refuse { notify: false };
    }
    InsertDisposition::Pass
}

/// The document's state at a settle dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one independent fact the verdict reads"
)]
pub struct SettleFacts {
    /// A workflow holds the edit lock.
    pub locked: bool,
    /// A load is in flight.
    pub loading: bool,
    /// An input-method preedit is in progress (and has not outlived its
    /// deadline, after which the adapter resets the input method).
    pub preedit: bool,
    /// A draft restore for this editor is still queued or reading its body:
    /// the recovered text, not the file, is the document the user typed into.
    pub restore_pending: bool,
    /// The episode's buffer was left partial or emptied (`HeldEpisode`).
    pub broken: bool,
    /// The latest load failed or was cancelled and left no document.
    pub load_failed: bool,
    /// The installation is incomplete (a cancelled install's cleared buffer).
    pub installation_incomplete: bool,
    /// The tab's content was evicted from memory.
    pub evicted: bool,
    /// Protected View, or the view is not editable.
    pub view_only: bool,
}

/// Why the buffer holds no document the held text may go into: what the
/// rescue notice names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoDocumentCause {
    /// The tab's content was unloaded from memory.
    Evicted,
    /// The load failed or was cancelled, or its installation is incomplete.
    NotLoaded,
    /// A workflow left the buffer partial or emptied and nothing published a
    /// whole document after it.
    Replaced,
}

/// What a settle dispatch does with the held text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettleVerdict {
    /// A workflow still owns the buffer, a load is in flight, a draft restore
    /// is pending, or a composition is in progress: decide again later.
    Wait,
    /// Insert the held text into the document.
    Replay,
    /// The document is view-only: rescue the text into a new Untitled tab.
    RescueViewOnly,
    /// The buffer holds no whole document: rescue the text into a new
    /// Untitled tab.
    RescueNoDocument(NoDocumentCause),
}

/// Decide a settle dispatch (design D4, D5). No verdict discards the text.
#[must_use]
pub const fn settle_verdict(facts: SettleFacts) -> SettleVerdict {
    if facts.locked || facts.loading || facts.preedit || facts.restore_pending {
        SettleVerdict::Wait
    } else if facts.view_only {
        SettleVerdict::RescueViewOnly
    } else if facts.evicted {
        SettleVerdict::RescueNoDocument(NoDocumentCause::Evicted)
    } else if facts.load_failed || facts.installation_incomplete {
        SettleVerdict::RescueNoDocument(NoDocumentCause::NotLoaded)
    } else if facts.broken {
        SettleVerdict::RescueNoDocument(NoDocumentCause::Replaced)
    } else {
        SettleVerdict::Replay
    }
}

/// What a close request does with an editor's held text (design D10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseDisposition {
    /// Nothing is held: the close goes on as before.
    Nothing,
    /// Text is held and no workflow owns the buffer (the gap between a
    /// release and its settle dispatch, or a composition): settle it now, so
    /// the close sees a modified document and its Save writes the text.
    Flush,
    /// A workflow still owns the buffer: refuse the close, as a save in
    /// flight refuses it.
    Busy,
}

/// Decide what a close does with held text. `workflow_owns`: the edit lock
/// is held, a load is in flight, or a replacement or install still suspends
/// the buffer's projections.
#[must_use]
pub const fn close_disposition(held: bool, workflow_owns: bool) -> CloseDisposition {
    if !held {
        CloseDisposition::Nothing
    } else if workflow_owns {
        CloseDisposition::Busy
    } else {
        CloseDisposition::Flush
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_appends_whole_or_not_at_all() {
        let mut held = HeldText::default();
        assert_eq!(held.hold("abc"), HoldOutcome::Held);
        assert_eq!(held.char_count(), 3);
        let filler = "x".repeat(HELD_TEXT_CAP_BYTES - 3);
        assert_eq!(held.hold(&filler), HoldOutcome::Held);
        assert_eq!(held.hold("y"), HoldOutcome::Overflow);
        assert_eq!(
            held.char_count(),
            u64::try_from(HELD_TEXT_CAP_BYTES).expect("the cap fits u64")
        );
        assert!(held.take().starts_with("abc"));
        assert!(held.is_empty());
    }

    #[test]
    fn an_insertion_over_the_cap_is_refused_whole() {
        let mut held = HeldText::default();
        assert_eq!(held.hold("kept"), HoldOutcome::Held);
        let paste = "é".repeat(HELD_TEXT_CAP_BYTES / 2);
        assert_eq!(held.hold(&paste), HoldOutcome::Overflow);
        assert_eq!(held.take(), "kept");
    }

    #[test]
    fn backspace_removes_one_character() {
        let mut held = HeldText::default();
        let _ = held.hold("aé");
        assert!(held.backspace());
        assert_eq!(held.char_count(), 1);
        assert!(held.backspace());
        assert!(!held.backspace());
    }

    #[test]
    fn the_cap_is_a_power_of_two() {
        assert!(HELD_TEXT_CAP_BYTES.is_power_of_two());
    }

    #[test]
    fn an_unpublished_release_breaks_until_a_whole_document() {
        let episode = HeldEpisode::new();
        assert!(!episode.is_broken());
        let broken = episode.after_release(ContentDisposition::Unpublished);
        assert!(broken.is_broken());
        assert!(
            broken
                .after_release(ContentDisposition::Unchanged)
                .is_broken()
        );
        assert!(
            !broken
                .after_release(ContentDisposition::WholeContent)
                .is_broken()
        );
        assert!(
            !episode
                .after_release(ContentDisposition::Unchanged)
                .is_broken()
        );
    }

    fn bools() -> [bool; 2] {
        [false, true]
    }

    #[test]
    fn insert_disposition_over_every_combination() {
        for own_edit in bools() {
            for locked in bools() {
                for held in bools() {
                    for user_action in bools() {
                        for at_caret in bools() {
                            for tainted in bools() {
                                let facts = InsertFacts {
                                    own_edit,
                                    locked,
                                    // A lock always holds input.
                                    held: held || locked,
                                    user_action,
                                    at_caret,
                                    tainted,
                                };
                                let got = insert_disposition(facts);
                                if own_edit || !facts.held {
                                    assert_eq!(got, InsertDisposition::Pass, "{facts:?}");
                                } else if user_action && at_caret && !tainted {
                                    assert_eq!(got, InsertDisposition::Hold, "{facts:?}");
                                } else if user_action {
                                    assert_eq!(
                                        got,
                                        InsertDisposition::Refuse { notify: true },
                                        "{facts:?}"
                                    );
                                } else if locked {
                                    // What the lock refused before this change.
                                    assert_eq!(
                                        got,
                                        InsertDisposition::Refuse { notify: false },
                                        "{facts:?}"
                                    );
                                } else {
                                    // A programmatic edit while a load plans
                                    // passes, as before.
                                    assert_eq!(got, InsertDisposition::Pass, "{facts:?}");
                                }
                                // Never weaker than the lock: a locked, foreign
                                // edit never reaches the buffer.
                                if locked && !own_edit {
                                    assert_ne!(got, InsertDisposition::Pass, "{facts:?}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn removal_disposition_over_every_combination() {
        for own_edit in bools() {
            for locked in bools() {
                for held in bools() {
                    for user_edit in bools() {
                        let facts = RemovalFacts {
                            own_edit,
                            locked,
                            held: held || locked,
                            user_edit,
                        };
                        let got = removal_disposition(facts);
                        if own_edit {
                            assert_eq!(got, InsertDisposition::Pass);
                        } else if facts.held && user_edit {
                            assert_eq!(got, InsertDisposition::Refuse { notify: true });
                        } else if locked {
                            assert_eq!(got, InsertDisposition::Refuse { notify: false });
                        } else {
                            assert_eq!(got, InsertDisposition::Pass);
                        }
                        assert_ne!(got, InsertDisposition::Hold, "a removal is never held");
                    }
                }
            }
        }
    }

    #[test]
    fn settle_verdict_over_every_combination() {
        for bits in 0u16..512 {
            let bit = |n: u16| bits & (1 << n) != 0;
            let facts = SettleFacts {
                locked: bit(0),
                loading: bit(1),
                preedit: bit(2),
                broken: bit(3),
                load_failed: bit(4),
                installation_incomplete: bit(5),
                evicted: bit(6),
                view_only: bit(7),
                restore_pending: bit(8),
            };
            let got = settle_verdict(facts);
            let waiting = facts.locked || facts.loading || facts.preedit || facts.restore_pending;
            let no_document =
                facts.broken || facts.load_failed || facts.installation_incomplete || facts.evicted;
            if waiting {
                assert_eq!(got, SettleVerdict::Wait, "{facts:?}");
            } else if facts.view_only {
                assert_eq!(got, SettleVerdict::RescueViewOnly, "{facts:?}");
            } else if no_document {
                assert!(
                    matches!(got, SettleVerdict::RescueNoDocument(_)),
                    "{facts:?}"
                );
                // The cause the notice names is the most specific one.
                let cause = if facts.evicted {
                    NoDocumentCause::Evicted
                } else if facts.load_failed || facts.installation_incomplete {
                    NoDocumentCause::NotLoaded
                } else {
                    NoDocumentCause::Replaced
                };
                assert_eq!(got, SettleVerdict::RescueNoDocument(cause), "{facts:?}");
            } else {
                assert_eq!(got, SettleVerdict::Replay, "{facts:?}");
            }
        }
    }

    #[test]
    fn close_disposition_flushes_or_refuses_held_text() {
        assert_eq!(close_disposition(false, false), CloseDisposition::Nothing);
        assert_eq!(close_disposition(false, true), CloseDisposition::Nothing);
        assert_eq!(close_disposition(true, false), CloseDisposition::Flush);
        assert_eq!(close_disposition(true, true), CloseDisposition::Busy);
    }

    #[test]
    fn the_anchor_is_the_first_hold_until_a_whole_document_is_published() {
        let episode = HeldEpisode::new();
        assert_eq!(episode.anchor(), ReplayAnchor::FirstHold);
        let saved = episode.after_release(ContentDisposition::Unchanged);
        assert_eq!(saved.anchor(), ReplayAnchor::FirstHold);
        let loaded = saved.after_release(ContentDisposition::WholeContent);
        assert_eq!(loaded.anchor(), ReplayAnchor::Caret);
        assert_eq!(
            loaded.after_release(ContentDisposition::Unchanged).anchor(),
            ReplayAnchor::Caret
        );
    }

    #[test]
    fn truncate_to_restores_an_earlier_length() {
        let mut held = HeldText::default();
        let _ = held.hold("ab");
        let mark = held.len_bytes();
        let _ = held.hold("éé");
        held.truncate_to(mark);
        assert_eq!(held.as_str(), "ab");
        // Not a boundary of this text: ignored.
        let _ = held.hold("é");
        held.truncate_to(3);
        assert_eq!(held.as_str(), "abé");
    }

    #[test]
    fn delete_surrounding_applies_only_inside_the_held_text() {
        let mut held = HeldText::default();
        let _ = held.hold("viet");
        // Telex: the input method deletes the last character and commits a
        // composed one.
        assert!(held.delete_surrounding(-1, 1));
        assert_eq!(held.as_str(), "vie");
        assert!(held.delete_surrounding(-3, 2));
        assert_eq!(held.as_str(), "e");
        // Reaching past the held text, after the caret, or empty: not applied.
        assert!(!held.delete_surrounding(-2, 1));
        assert!(!held.delete_surrounding(0, 1));
        assert!(!held.delete_surrounding(-1, 2));
        assert!(!held.delete_surrounding(-1, 0));
        assert!(!held.delete_surrounding(i32::MIN, 1));
        assert_eq!(held.as_str(), "e");
    }
}
