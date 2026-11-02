// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: seam value objects — the Protected View workflow's ticket, facts and
//! freshness predicate (design D2), in the `*Ticket` + `*Facts` +
//! `*_is_current` shape.
//!
//! [`ProtectedViewTicket`] is built once when a protected session starts and
//! carried by every later stage that leaves the GTK dispatch (worker find,
//! inspector window reads, whole-line copy chunks, the edit-growth capture):
//! the editor's weak identity plus the protected-session generation.
//! [`ProtectedViewFacts`] is what the editor holds when such a stage resumes:
//! its live session, the entry reason, the fold summary and the source state.
//! [`protected_view_is_current`] validates the two as a unit, so a completion
//! that outlived an install, a reload, an exit or the editor itself publishes
//! nothing.
//!
//! The GTK half (the weak reference) stays in the ticket; the predicate and the
//! facts are plain values. Nothing here is Kani-checked: the session transition
//! that moves the generation is, in `policy.rs`.

use std::sync::{Arc, OnceLock};

use super::policy::{
    InspectorWindow, N_INSPECT_WIDTH, ProtectedSession, inspector_window_containing,
};
use crate::model::paragraph_boundary::next_paragraph_break;
use crate::ui::editor_page::LushtextEditorPage;
use crate::ui::plain_disposal::DisposalOwned;

/// Why the editor is in Protected View.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtectedViewReason {
    /// A whole-content install whose body has a paragraph over
    /// `L_PROTECT_EDIT` (C1's `ReductionReason::LongParagraph`).
    LongParagraph,
    /// An edit grew a paragraph over `L_PROTECT_EDIT` (the only reason that
    /// may offer the entry undo).
    EditGrowth,
}

/// The fold plan's summary: what the alert, the automation snapshot and the
/// accessible description report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FoldSummary {
    /// Paragraphs with a hidden tail.
    pub(crate) folded_paragraphs: u64,
    /// Characters hidden across all tails.
    pub(crate) hidden_chars: u64,
}

/// Where the immutable text source stands (design D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProtectedSourceState {
    /// The installed body, retained at the install's terminal.
    Retained,
    /// An edit-growth entry whose one capture has not run yet.
    CapturePending,
    /// An edit-growth entry whose capture completed.
    Captured,
    /// Released (close, eviction, reload, a replacing install).
    Released,
}

/// The editor's Protected View state when a stage resumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProtectedViewFacts {
    pub(crate) session: ProtectedSession,
    pub(crate) reason: ProtectedViewReason,
    pub(crate) folds: FoldSummary,
    pub(crate) source: ProtectedSourceState,
}

/// The dispatch-time expectation of a Protected View stage.
#[derive(Clone, Debug)]
pub(crate) struct ProtectedViewTicket {
    editor: glib::WeakRef<LushtextEditorPage>,
    generation: u64,
}

impl ProtectedViewTicket {
    /// The ticket for the editor's current session, or `None` when the session
    /// is not protected (no Protected View stage may start then).
    #[must_use]
    pub(crate) fn new(
        editor: glib::WeakRef<LushtextEditorPage>,
        session: ProtectedSession,
    ) -> Option<Self> {
        session.protected.then_some(Self {
            editor,
            generation: session.generation,
        })
    }

    /// The editor, while it is alive.
    #[must_use]
    pub(crate) fn editor(&self) -> Option<LushtextEditorPage> {
        self.editor.upgrade()
    }

    /// The protected-session generation the stage was dispatched under.
    #[must_use]
    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }
}

/// Whether a resumed stage may still publish: the editor is still in the very
/// protected session the ticket was dispatched under.
///
/// The generation advances on every install, reload, entry and exit
/// (`ProtectedSession::on_event`), so an equal generation on a protected
/// session means nothing replaced the content or left Protected View since
/// dispatch. Whether the editor itself is alive is the ticket's
/// [`ProtectedViewTicket::editor`], checked before the facts can be read.
#[must_use]
pub(crate) const fn protected_view_is_current(
    ticket: &ProtectedViewTicket,
    facts: &ProtectedViewFacts,
) -> bool {
    facts.session.protected && facts.session.generation == ticket.generation
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::editor_page::protected_view::policy::SessionEvent;

    fn facts(session: ProtectedSession) -> ProtectedViewFacts {
        ProtectedViewFacts {
            session,
            reason: ProtectedViewReason::EditGrowth,
            folds: FoldSummary::default(),
            source: ProtectedSourceState::CapturePending,
        }
    }

    #[test]
    fn no_ticket_for_an_editable_session() {
        assert!(
            ProtectedViewTicket::new(glib::WeakRef::new(), ProtectedSession::EDITABLE).is_none()
        );
    }

    #[test]
    fn a_ticket_is_current_only_in_its_own_protected_session() {
        let entered = ProtectedSession::EDITABLE.on_event(SessionEvent::EditGrowth);
        let ticket = ProtectedViewTicket::new(glib::WeakRef::new(), entered).expect("protected");
        assert_eq!(ticket.generation(), entered.generation);
        assert!(ticket.editor().is_none());
        assert!(protected_view_is_current(&ticket, &facts(entered)));

        let reinstalled = entered.on_event(SessionEvent::ReloadFromDisk { protected: true });
        assert!(!protected_view_is_current(&ticket, &facts(reinstalled)));

        let exited = entered.on_event(SessionEvent::EntryUndo {
            is_entry_step: true,
        });
        assert!(!protected_view_is_current(&ticket, &facts(exited)));

        // A non-protected session at the ticket's generation is still stale.
        let forged = ProtectedSession {
            protected: false,
            ..entered
        };
        assert!(!protected_view_is_current(&ticket, &facts(forged)));
    }
}

// ---------------------------------------------------------------------------
// The text source (design D4, task 5.1)
// ---------------------------------------------------------------------------
//
// Find, the Line Inspector, whole-line copy and every save of a protected
// document read the source, never a GTK snapshot of the protected buffer (the
// chunked snapshot is quadratic inside one character segment, ledger A66). For
// a whole-content install it is the installed body, moved at the install's
// terminal instead of being disposed: no copy, and the body keeps its
// guaranteed hand-off to the disposal workers wherever its last reference
// drops. The buffer is immutable while protected, so the source equals
// `buffer.text(…, true)` for the life of the session. It crosses into the
// find and save workers, which is why it is a seam value object.

/// Paragraphs between two paragraph checkpoints of the source's index: the
/// worker resolves a paragraph by jumping to the checkpoint at or before it and
/// walking at most this many paragraph breaks. A cap on that walk, a power of
/// two (`.agents/rules/rust.md`).
pub(crate) const PARAGRAPH_CHECKPOINT_STRIDE: u64 = 1024;

/// Characters between two character checkpoints of one long paragraph: a
/// window read decodes at most this many characters before its window starts.
/// A cap on that walk, a power of two. A paragraph of at most this many bytes
/// has at most this many characters, so it needs no checkpoint.
pub(crate) const CHAR_CHECKPOINT_STRIDE: u64 = 64 * 1024;

/// One paragraph of the source, located: its byte range (delimiter excluded)
/// and its length in characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceParagraph {
    pub(crate) start_byte: usize,
    pub(crate) end_byte: usize,
    pub(crate) chars: u64,
}

/// A paragraph longer than [`CHAR_CHECKPOINT_STRIDE`] bytes: its length and the
/// byte offset of every `CHAR_CHECKPOINT_STRIDE`-th character.
#[derive(Debug)]
struct LongParagraph {
    chars: u64,
    checkpoints: Vec<usize>,
}

/// The paragraph index and character checkpoints of a source (design D6),
/// built once on a worker, the first time the Line Inspector or whole-line
/// copy needs it. Its size is a few machine words per
/// [`PARAGRAPH_CHECKPOINT_STRIDE`] paragraphs and per
/// [`CHAR_CHECKPOINT_STRIDE`] characters of long paragraphs, charged to the
/// live-editor estimate with the body.
#[derive(Debug)]
struct ProtectedTextIndex {
    /// Byte start of paragraph `k · PARAGRAPH_CHECKPOINT_STRIDE`.
    paragraph_starts: Vec<usize>,
    /// Paragraphs in the text (an empty text has one, as a GtkTextBuffer).
    paragraphs: u64,
    long: std::collections::BTreeMap<u64, LongParagraph>,
}

impl ProtectedTextIndex {
    fn build(text: &str) -> Self {
        let mut paragraph_starts = Vec::new();
        let mut long = std::collections::BTreeMap::new();
        let mut paragraph = 0u64;
        let mut start = 0usize;
        loop {
            if paragraph.is_multiple_of(PARAGRAPH_CHECKPOINT_STRIDE) {
                paragraph_starts.push(start);
            }
            let found = next_paragraph_break(text, start);
            let end = found.map_or(text.len(), |found| found.content_end);
            let content = &text[start..end];
            if u64::try_from(content.len()).unwrap_or(u64::MAX) > CHAR_CHECKPOINT_STRIDE {
                let stride = usize::try_from(CHAR_CHECKPOINT_STRIDE).unwrap_or(usize::MAX);
                let mut checkpoints = Vec::new();
                let mut chars = 0u64;
                for (index, (byte, _)) in content.char_indices().enumerate() {
                    if index % stride == 0 {
                        checkpoints.push(start + byte);
                    }
                    chars += 1;
                }
                long.insert(paragraph, LongParagraph { chars, checkpoints });
            }
            match found {
                Some(found) => {
                    start = found.next_start;
                    paragraph += 1;
                }
                None => break,
            }
        }
        Self {
            paragraph_starts,
            paragraphs: paragraph + 1,
            long,
        }
    }

    fn retained_bytes(&self) -> u64 {
        let word = std::mem::size_of::<usize>();
        let checkpoints: usize = self
            .long
            .values()
            .map(|long| long.checkpoints.len() * word + std::mem::size_of::<LongParagraph>() + 32)
            .sum();
        u64::try_from(self.paragraph_starts.len() * word + checkpoints).unwrap_or(u64::MAX)
    }

    fn paragraph(&self, text: &str, paragraph: u64) -> Option<SourceParagraph> {
        if paragraph >= self.paragraphs {
            return None;
        }
        let checkpoint = paragraph / PARAGRAPH_CHECKPOINT_STRIDE;
        let mut start = *self
            .paragraph_starts
            .get(usize::try_from(checkpoint).ok()?)?;
        for _ in 0..paragraph % PARAGRAPH_CHECKPOINT_STRIDE {
            start = next_paragraph_break(text, start)?.next_start;
        }
        let end = next_paragraph_break(text, start).map_or(text.len(), |found| found.content_end);
        let chars = match self.long.get(&paragraph) {
            Some(long) => long.chars,
            None => u64::try_from(text[start..end].chars().count()).unwrap_or(u64::MAX),
        };
        Some(SourceParagraph {
            start_byte: start,
            end_byte: end,
            chars,
        })
    }

    /// The byte offset of character `column` of `located` (its end when the
    /// column is the paragraph length), decoding at most
    /// `CHAR_CHECKPOINT_STRIDE` characters.
    fn byte_of_column(
        &self,
        text: &str,
        paragraph: u64,
        located: SourceParagraph,
        column: u64,
    ) -> usize {
        let (from, skip) = match self.long.get(&paragraph) {
            Some(long) => {
                let checkpoint =
                    usize::try_from(column / CHAR_CHECKPOINT_STRIDE).unwrap_or(usize::MAX);
                match long.checkpoints.get(checkpoint) {
                    Some(byte) => (*byte, column % CHAR_CHECKPOINT_STRIDE),
                    None => return located.end_byte,
                }
            }
            None => (located.start_byte, column),
        };
        let skip = usize::try_from(skip).unwrap_or(usize::MAX);
        text[from..located.end_byte]
            .char_indices()
            .nth(skip)
            .map_or(located.end_byte, |(byte, _)| from + byte)
    }
}

/// What one Line Inspector window read returns: the window's text (at most
/// `N_INSPECT` characters) and where it lies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InspectorWindowText {
    pub(crate) paragraph: u64,
    pub(crate) paragraph_chars: u64,
    pub(crate) window_index: u64,
    pub(crate) window: InspectorWindow,
    pub(crate) text: String,
}

/// The source's shared state: the body and its lazily built index.
struct SourceInner {
    body: DisposalOwned<String>,
    index: OnceLock<ProtectedTextIndex>,
}

/// The immutable text of a protected document, shareable with workers.
#[derive(Clone)]
pub(crate) struct ProtectedTextSource {
    inner: Arc<SourceInner>,
}

impl ProtectedTextSource {
    /// Retain an installed body as the session's source.
    pub(crate) fn retain(body: DisposalOwned<String>) -> Self {
        Self {
            inner: Arc::new(SourceInner {
                body: body.into_retained_current(),
                index: OnceLock::new(),
            }),
        }
    }

    /// The whole document text.
    pub(crate) fn text(&self) -> &str {
        self.inner.body.as_str()
    }

    /// Bytes the source retains, for the live-editor memory estimate: the
    /// body, and the paragraph index once a worker built it.
    pub(crate) fn retained_bytes(&self) -> u64 {
        let body = u64::try_from(self.inner.body.len()).unwrap_or(u64::MAX);
        let index = self
            .inner
            .index
            .get()
            .map_or(0, ProtectedTextIndex::retained_bytes);
        body.saturating_add(index)
    }

    /// Whether the paragraph index is built (and so charged).
    pub(crate) fn has_index(&self) -> bool {
        self.inner.index.get().is_some()
    }

    /// The index, built on first use. **Worker only**: building walks the
    /// whole text once.
    fn index(&self) -> &ProtectedTextIndex {
        self.inner
            .index
            .get_or_init(|| ProtectedTextIndex::build(self.text()))
    }

    /// Paragraphs in the document. Worker only.
    pub(crate) fn paragraph_count(&self) -> u64 {
        self.index().paragraphs
    }

    /// Locate one paragraph. Worker only.
    pub(crate) fn paragraph(&self, paragraph: u64) -> Option<SourceParagraph> {
        self.index().paragraph(self.text(), paragraph)
    }

    /// Read the inspector window containing `column` of `paragraph` (the last
    /// window for a column past the end, the first of an empty paragraph).
    /// Worker only.
    pub(crate) fn read_inspector_window(
        &self,
        paragraph: u64,
        column: u64,
    ) -> Option<InspectorWindowText> {
        let located = self.paragraph(paragraph)?;
        let (window_index, window) = if located.chars == 0 {
            (0, InspectorWindow { start: 0, end: 0 })
        } else {
            let column = column.min(located.chars - 1);
            inspector_window_containing(located.chars, N_INSPECT_WIDTH, column)?
        };
        let index = self.index();
        let from = index.byte_of_column(self.text(), paragraph, located, window.start);
        let width = usize::try_from(window.len()).unwrap_or(usize::MAX);
        let to = self.text()[from..located.end_byte]
            .char_indices()
            .nth(width)
            .map_or(located.end_byte, |(byte, _)| from + byte);
        Some(InspectorWindowText {
            paragraph,
            paragraph_chars: located.chars,
            window_index,
            window,
            text: self.text()[from..to].to_owned(),
        })
    }
}

impl std::fmt::Debug for ProtectedTextSource {
    // The size only: document text never reaches a debug log.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProtectedTextSource")
            .field("bytes", &self.retained_bytes())
            .finish()
    }
}

#[cfg(test)]
mod source_tests {
    use super::*;
    use crate::ui::editor_page::protected_view::policy::N_INSPECT;
    use crate::ui::plain_disposal::DisposalOwned;

    fn source(text: &str) -> ProtectedTextSource {
        ProtectedTextSource::retain(DisposalOwned::small_unreserved(text.to_owned()))
    }

    #[test]
    fn paragraphs_follow_gtk_delimiters() {
        let text = "a\nbb\r\ncc\rd\u{2029}é";
        let source = source(text);
        assert_eq!(source.paragraph_count(), 5);
        let p = source.paragraph(4).expect("last");
        assert_eq!(&text[p.start_byte..p.end_byte], "é");
        assert_eq!(p.chars, 1);
        assert!(source.paragraph(5).is_none());
        assert_eq!(source.paragraph(1).map(|p| p.chars), Some(2));
    }

    #[test]
    fn many_paragraphs_resolve_across_checkpoints() {
        let stride = usize::try_from(PARAGRAPH_CHECKPOINT_STRIDE).expect("fits");
        let lines: Vec<String> = (0..stride * 3 + 5).map(|i| format!("line {i}")).collect();
        let text = lines.join("\n");
        let source = source(&text);
        assert_eq!(
            source.paragraph_count(),
            u64::try_from(lines.len()).expect("fits")
        );
        for probe in [0, stride - 1, stride, 2 * stride + 7, lines.len() - 1] {
            let p = source
                .paragraph(u64::try_from(probe).expect("fits"))
                .expect("in range");
            assert_eq!(&text[p.start_byte..p.end_byte], lines[probe]);
        }
    }

    #[test]
    fn windows_in_a_long_multibyte_paragraph_match_a_naive_cut() {
        let long: String = (0..300_000u32)
            .map(|i| {
                char::from_u32(0x61 + i % 26)
                    .filter(|_| i % 7 != 0)
                    .unwrap_or('é')
            })
            .collect();
        let text = format!("head\n{long}\ntail");
        let source = source(&text);
        let chars: Vec<char> = long.chars().collect();
        let n = usize::try_from(N_INSPECT).expect("fits");
        let stride = CHAR_CHECKPOINT_STRIDE;
        for column in [
            0u64,
            499,
            500,
            2 * stride - 1,
            2 * stride,
            199_999,
            299_999,
            400_000,
        ] {
            let read = source.read_inspector_window(1, column).expect("window");
            let clamped = usize::try_from(column.min(299_999)).expect("fits");
            let start = clamped / n * n;
            let end = (start + n).min(chars.len());
            assert_eq!(
                read.text,
                chars[start..end].iter().collect::<String>(),
                "column {column}"
            );
            assert_eq!(read.paragraph_chars, 300_000);
            assert!(read.window.contains(u64::try_from(clamped).expect("fits")));
        }
        assert!(source.retained_bytes() > u64::try_from(text.len()).expect("fits"));
    }

    #[test]
    fn empty_paragraph_reads_an_empty_window() {
        let source = source("a\n\nb");
        let read = source.read_inspector_window(1, 0).expect("window");
        assert_eq!(read.text, "");
        assert_eq!(read.paragraph_chars, 0);
    }
}
