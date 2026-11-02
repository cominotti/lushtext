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
//!   any more, whether the held text is inserted or discarded (design D4, D5).

/// The held buffer's cap: 4 KiB of UTF-8.
///
/// Kind: **cap** (per held episode), a power of two (`.agents/rules/rust.md`).
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

/// Whether the episode's buffer is still a document held text may go into.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeldEpisode {
    broken: bool,
}

impl HeldEpisode {
    /// A new episode: the document the user typed into was whole.
    #[must_use]
    pub const fn new() -> Self {
        Self { broken: false }
    }

    /// Fold one lock release into the episode.
    #[must_use]
    pub const fn after_release(self, release: ContentDisposition) -> Self {
        match release {
            ContentDisposition::WholeContent => Self { broken: false },
            ContentDisposition::Unchanged => self,
            ContentDisposition::Unpublished => Self { broken: true },
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
    /// An input-method preedit is in progress.
    pub preedit: bool,
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

/// What a settle dispatch does with the held text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettleVerdict {
    /// A workflow still owns the buffer, a load is in flight, or a
    /// composition is in progress: decide again later.
    Wait,
    /// Insert the held text at the caret.
    Replay,
    /// The document is view-only: discard with the Protected View notice.
    DiscardViewOnly,
    /// The buffer holds no whole document: discard with a warning.
    DiscardNoDocument,
}

/// Decide a settle dispatch (design D4, D5).
#[must_use]
pub const fn settle_verdict(facts: SettleFacts) -> SettleVerdict {
    if facts.locked || facts.loading || facts.preedit {
        SettleVerdict::Wait
    } else if facts.view_only {
        SettleVerdict::DiscardViewOnly
    } else if facts.broken || facts.load_failed || facts.installation_incomplete || facts.evicted {
        SettleVerdict::DiscardNoDocument
    } else {
        SettleVerdict::Replay
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
        for bits in 0u16..256 {
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
            };
            let got = settle_verdict(facts);
            let waiting = facts.locked || facts.loading || facts.preedit;
            let no_document =
                facts.broken || facts.load_failed || facts.installation_incomplete || facts.evicted;
            if waiting {
                assert_eq!(got, SettleVerdict::Wait, "{facts:?}");
            } else if facts.view_only {
                assert_eq!(got, SettleVerdict::DiscardViewOnly, "{facts:?}");
            } else if no_document {
                assert_eq!(got, SettleVerdict::DiscardNoDocument, "{facts:?}");
            } else {
                assert_eq!(got, SettleVerdict::Replay, "{facts:?}");
            }
        }
    }
}
