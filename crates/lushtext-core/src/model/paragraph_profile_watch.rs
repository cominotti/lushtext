// SPDX-License-Identifier: GPL-3.0-or-later

//! The pure core of the edit-time paragraph watch.
//!
//! `ui/editor_page/document_scale.rs` connects `insert-text` and
//! `delete-range` after-handlers to the editor buffer. They gather the facts of
//! one edit ([`InsertFacts`], [`DeleteFacts`]), ask this core which buffer
//! lines to read ([`plan_insert`], [`plan_delete`] returning a [`WatchPlan`]),
//! read exactly those line lengths, and hand them back to [`complete`], which
//! returns the new [`ProfileWatchState`]. The adapter makes no decision of its
//! own, so the read bound is a property of this core: a plan names at most
//! `2 · r_batch` lines (within the specified `2 · r_batch + 2`), and a fast-path
//! plan names one.
//!
//! The state keeps the two components the scale policy reads, as session
//! high-waters: the longest paragraph and the worst `r_batch` window
//! (`worst_batch_chars`), plus `density_bound`, an upper bound on the current
//! document's worst window carried across fast-path steps. A fast step skips
//! the window recompute only while the line count is unchanged, the edit
//! touches one line, and `density_bound + inserted ≤ v_batch`; it then adds the
//! inserted characters to the bound (an uncarried bound is unsound across
//! keystrokes, design D5 of `document-scale-policy`). Every other step
//! recomputes the windows through the touched lines with the same
//! [`ParagraphLengthFold`] the load profile uses.
//!
//! **The lines are `GtkTextBuffer`'s, which depend on edit history** (ledger
//! A42): an insertion splits only its own text at Pango's paragraph
//! delimiters, never joining with or splitting around the characters beside
//! it (a `\n` typed after a buffer `\r` starts a new line rather than forming
//! a CRLF, and text typed between a `\r` and an `\n` stays inside that line),
//! and a deletion joins exactly the lines whose starts it removes. Text
//! inserted whole has Pango's lines, so a load's worker profile describes the
//! buffer it installs; afterwards the watch measures the buffer's own lines,
//! which are what GTK lays out.
//!
//! A deletion that leaves the line count unchanged stays inside one line, so
//! no paragraph grows and the watch does nothing. A deletion that removes a
//! line start joins two lines, which can raise the longest paragraph and the
//! density (later lines shift into the window), so it takes the window path.
//!
//! Kani proves (`paragraph_profile_watch/kani_proofs.rs`) an **inductive
//! step**: from any buffer of at most 2 symbols from `a`, `\r`, `\n` and
//! U+2029, with any line structure the A42 rule can reach (and more), and any
//! watch state related to it by the invariant below, one edit inserting at
//! most 2 symbols at any offset or deleting any range of at most 2, with
//! `r_batch` of 1 or 2 and `v_batch`, the state and the high-waters at most 16,
//! keeps the invariant: the watched longest paragraph equals the high-water of
//! the recomputed one, `density_bound` never falls below the recomputed worst
//! window, `worst_batch_chars` never exceeds the recomputed high-water and
//! equals it while no step took the fast path, and it exceeds `v_batch`
//! exactly when the high-water does. The invariant holds after a load
//! ([`ProfileWatchState::from_profile`]), so by induction it holds after every
//! edit sequence whose buffers stay in that domain; the read bound is proved
//! for the same step. (The design's bounded replay of 3 edits from a 4-symbol
//! start did not fit the lane: one replayed edit took 14 minutes and 9 GiB.)
//! Deleting the text between `\r` and `\n` needs a third symbol, so one more
//! harness proves the same step for that deletion over the buffer `\r`, any
//! symbol, `\n`, with any line structure and any related state.
//! Longer documents and edits repeat the same local situations; the widget
//! tests exercise production sizes.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

#[cfg(kani)]
mod kani_proofs;

use super::paragraph_boundary::{
    ParagraphLengthFold, ProfileConfig, ProfileSummary, next_paragraph_break,
};

/// What the `insert-text` after-handler knows about one insertion.
#[derive(Clone, Copy, Debug)]
pub struct InsertFacts<'a> {
    /// The inserted text.
    pub run: &'a str,
    /// Buffer line count before the insertion.
    pub line_count_before: u64,
    /// Buffer line count after the insertion.
    pub line_count_after: u64,
    /// Line of the iter at the run's start, after the insertion.
    pub start_line: u64,
    /// Line of the iter at the run's end, after the insertion.
    pub end_line: u64,
    /// Whether the run sits right after a buffer `\r` and right before a
    /// buffer `\n`. Text typed there turns a CRLF line ending into an LF one,
    /// so the line's paragraph grows by one more than the inserted characters
    /// (the `\r` becomes content): the fast path's carried bound would
    /// under-count it, so such an insertion takes the window path.
    pub between_cr_and_lf: bool,
}

/// What the `delete-range` handlers know about one deletion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeleteFacts {
    /// Buffer line count before the deletion (recorded by the before-handler).
    pub line_count_before: u64,
    /// Buffer line count after the deletion.
    pub line_count_after: u64,
    /// Line of the deletion point, after the deletion.
    pub line: u64,
}

/// The lines one edit must read, and what to do with them. Planning and
/// completing a step allocate nothing (a `Vec` per step also made the Kani
/// model's formula outgrow memory).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchPlan<'a> {
    /// Nothing changed that the watch tracks.
    Nothing,
    /// Read one line for the longest paragraph and add `inserted_chars` to
    /// the carried density bound.
    FastPath {
        /// The line the insertion touched.
        touched_line: u64,
        /// Characters the insertion added.
        inserted_chars: u64,
    },
    /// Recompute the windows through `start_line..=end_line`.
    ReadWindow(WindowPlan<'a>),
}

/// The lines of one window recompute: the buffer lines `first_line..=start_line`
/// and `end_line..=last_line` are read, and the lines strictly between
/// `start_line` and `end_line` lie inside the inserted `run`, whose paragraph
/// lengths the core measures itself ([`WindowPlan::inner_lengths`]).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowPlan<'a> {
    /// First line of the window range.
    pub first_line: u64,
    /// First touched line.
    pub start_line: u64,
    /// Last touched line.
    pub end_line: u64,
    /// Last line of the window range.
    pub last_line: u64,
    /// The inserted text (empty for a deletion).
    pub run: &'a str,
}

impl WindowPlan<'_> {
    /// Paragraph lengths of the lines strictly between the touched edges.
    ///
    /// `GtkTextBuffer` splits an inserted run at its own Pango paragraph
    /// delimiters and nowhere else (ledger A42), so the run's first segment
    /// joins the start line, its last segment joins the end line, and every
    /// segment between is a whole line of its own, measured here from the run
    /// text with the shared delimiter rule.
    pub fn inner_lengths(&self) -> impl Iterator<Item = u64> + '_ {
        let run = if self.end_line > self.start_line {
            self.run
        } else {
            ""
        };
        let mut cursor = 0;
        let mut line_start: Option<usize> = None;
        std::iter::from_fn(move || {
            while let Some(found) = next_paragraph_break(run, cursor) {
                cursor = found.next_start;
                let previous = line_start.replace(found.next_start);
                if let Some(start) = previous {
                    return Some(char_count(&run[start..found.content_end]));
                }
            }
            None
        })
    }
}

/// The buffer lines a plan reads, in order.
pub type LinesToRead =
    std::iter::Chain<std::ops::RangeInclusive<u64>, std::ops::RangeInclusive<u64>>;

impl WatchPlan<'_> {
    /// The buffer lines the adapter must read, in order; [`complete`] takes
    /// their paragraph lengths in the same order.
    pub fn lines_to_read(&self) -> LinesToRead {
        #[expect(
            clippy::reversed_empty_ranges,
            reason = "an intentionally empty range for the half that reads nothing"
        )]
        let empty = 1..=0;
        match self {
            Self::Nothing => empty.clone().chain(empty),
            Self::FastPath { touched_line, .. } => (*touched_line..=*touched_line).chain(empty),
            Self::ReadWindow(window) => {
                let tail_start = if window.end_line > window.start_line {
                    window.end_line
                } else {
                    window.start_line + 1
                };
                (window.first_line..=window.start_line).chain(tail_start..=window.last_line)
            }
        }
    }
}

/// The watch's live state for one document session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProfileWatchState {
    longest_paragraph_chars: u64,
    worst_batch_chars: u64,
    density_bound: u64,
}

impl ProfileWatchState {
    /// The state after a load or replacement whose worker profile is `summary`.
    #[must_use]
    pub const fn from_profile(summary: ProfileSummary) -> Self {
        Self {
            longest_paragraph_chars: summary.longest_paragraph_chars,
            worst_batch_chars: summary.worst_batch_chars,
            density_bound: summary.worst_batch_chars,
        }
    }

    /// Join a newly installed body's profile into the session high-waters.
    #[must_use]
    pub const fn absorb(self, summary: ProfileSummary) -> Self {
        Self {
            longest_paragraph_chars: max(
                self.longest_paragraph_chars,
                summary.longest_paragraph_chars,
            ),
            worst_batch_chars: max(self.worst_batch_chars, summary.worst_batch_chars),
            density_bound: max(self.density_bound, summary.worst_batch_chars),
        }
    }

    /// The session high-water of the longest paragraph.
    #[must_use]
    pub const fn longest_paragraph_chars(self) -> u64 {
        self.longest_paragraph_chars
    }

    /// The session high-water of the worst density window.
    #[must_use]
    pub const fn worst_batch_chars(self) -> u64 {
        self.worst_batch_chars
    }

    /// The scale-policy view of the state, for a document of `total_bytes`.
    #[must_use]
    pub const fn summary(self, total_bytes: u64) -> ProfileSummary {
        ProfileSummary {
            total_bytes,
            longest_paragraph_chars: self.longest_paragraph_chars,
            worst_batch_chars: self.worst_batch_chars,
        }
    }
}

const fn max(left: u64, right: u64) -> u64 {
    if left > right { left } else { right }
}

/// Characters in `text` (a scalar count, what `GtkTextIter` counts): its
/// UTF-8 lead bytes. A plain byte loop, so the Kani harnesses unwind it per
/// byte (the library's chunked `chars().count()` does not unwind).
fn char_count(text: &str) -> u64 {
    let leads = text.bytes().filter(|byte| byte & 0xC0 != 0x80).count();
    u64::try_from(leads).unwrap_or(u64::MAX)
}

/// The window range through `start_line..=end_line` in a buffer of
/// `line_count` lines: `r_batch − 1` lines on each side, so every window of
/// `r_batch` consecutive lines that contains a touched line lies inside.
fn window_range(
    config: ProfileConfig,
    start_line: u64,
    end_line: u64,
    line_count: u64,
) -> (u64, u64) {
    let reach = u64::try_from(config.window())
        .unwrap_or(u64::MAX)
        .saturating_sub(1);
    let last_index = line_count.saturating_sub(1);
    let end_line = if end_line > last_index {
        last_index
    } else {
        end_line
    };
    let start_line = if start_line > end_line {
        end_line
    } else {
        start_line
    };
    let last = end_line.saturating_add(reach);
    (
        start_line.saturating_sub(reach),
        if last > last_index { last_index } else { last },
    )
}

/// Plan the watch's work for one insertion.
#[must_use]
pub fn plan_insert<'a>(
    state: ProfileWatchState,
    config: ProfileConfig,
    facts: &InsertFacts<'a>,
) -> WatchPlan<'a> {
    if facts.run.is_empty() {
        return WatchPlan::Nothing;
    }
    let inserted_chars = char_count(facts.run);
    if facts.line_count_after == facts.line_count_before
        && facts.start_line == facts.end_line
        && !facts.between_cr_and_lf
        && state.density_bound.saturating_add(inserted_chars) <= config.v_batch
    {
        return WatchPlan::FastPath {
            touched_line: facts.start_line,
            inserted_chars,
        };
    }
    let (first_line, last_line) = window_range(
        config,
        facts.start_line,
        facts.end_line,
        facts.line_count_after,
    );
    WatchPlan::ReadWindow(WindowPlan {
        first_line,
        start_line: facts.start_line.clamp(first_line, last_line),
        end_line: facts.end_line.clamp(first_line, last_line),
        last_line,
        run: facts.run,
    })
}

/// Plan the watch's work for one deletion.
#[must_use]
pub fn plan_delete(config: ProfileConfig, facts: DeleteFacts) -> WatchPlan<'static> {
    if facts.line_count_after >= facts.line_count_before {
        // No line removed: every paragraph only shrank or kept its length.
        return WatchPlan::Nothing;
    }
    let (first_line, last_line) =
        window_range(config, facts.line, facts.line, facts.line_count_after);
    let line = facts.line.clamp(first_line, last_line);
    WatchPlan::ReadWindow(WindowPlan {
        first_line,
        start_line: line,
        end_line: line,
        last_line,
        run: "",
    })
}

/// Apply a plan's reads: `read_lengths` are the paragraph lengths of
/// [`WatchPlan::lines_to_read`], in that order.
#[must_use]
pub fn complete(
    state: ProfileWatchState,
    config: ProfileConfig,
    plan: &WatchPlan<'_>,
    read_lengths: &[u64],
) -> ProfileWatchState {
    match plan {
        WatchPlan::Nothing => state,
        WatchPlan::FastPath { inserted_chars, .. } => {
            let touched = read_lengths.first().copied().unwrap_or(0);
            ProfileWatchState {
                longest_paragraph_chars: max(state.longest_paragraph_chars, touched),
                worst_batch_chars: state.worst_batch_chars,
                density_bound: state.density_bound.saturating_add(*inserted_chars),
            }
        }
        WatchPlan::ReadWindow(window) => {
            let head =
                usize::try_from(window.start_line - window.first_line + 1).unwrap_or(usize::MAX);
            let head = head.min(read_lengths.len());
            let mut fold = ParagraphLengthFold::new(config);
            let mut longest = state.longest_paragraph_chars;
            let lengths = read_lengths[..head]
                .iter()
                .copied()
                .chain(window.inner_lengths())
                .chain(read_lengths[head..].iter().copied());
            for length in lengths {
                fold.push(0, length);
                longest = max(longest, length);
            }
            let window_worst = fold.worst_batch_chars();
            ProfileWatchState {
                longest_paragraph_chars: longest,
                worst_batch_chars: max(state.worst_batch_chars, window_worst),
                density_bound: max(state.density_bound, window_worst),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::paragraph_boundary::ProfileThresholds;

    fn config(r_batch: usize, v_batch: u64) -> ProfileConfig {
        ProfileConfig {
            thresholds: ProfileThresholds([u64::MAX; 6]),
            offset_cap: 0,
            r_batch,
            v_batch,
        }
    }

    /// A buffer with `GtkTextBuffer`'s history-dependent lines (ledger A42):
    /// characters plus the offsets where lines start.
    #[derive(Clone)]
    struct Buffer {
        chars: Vec<char>,
        line_starts: Vec<usize>,
    }

    fn breaks_of(text: &[char]) -> Vec<usize> {
        let text: String = text.iter().collect();
        let mut out = Vec::new();
        let mut cursor = 0;
        while let Some(found) = next_paragraph_break(&text, cursor) {
            out.push(text[..found.next_start].chars().count());
            cursor = found.next_start;
        }
        out
    }

    impl Buffer {
        fn new(text: &str) -> Self {
            let chars: Vec<char> = text.chars().collect();
            let mut line_starts = vec![0];
            line_starts.extend(breaks_of(&chars));
            Self { chars, line_starts }
        }

        fn lines(&self) -> Vec<u64> {
            let mut out = Vec::new();
            for (index, &start) in self.line_starts.iter().enumerate() {
                let end = self
                    .line_starts
                    .get(index + 1)
                    .copied()
                    .unwrap_or(self.chars.len());
                let line = &self.chars[start..end];
                let last = index + 1 == self.line_starts.len();
                let delimiter = match line {
                    _ if last => 0,
                    [.., '\r', '\n'] => 2,
                    [.., '\n' | '\r' | '\u{2029}'] => 1,
                    _ => 0,
                };
                out.push((line.len() - delimiter) as u64);
            }
            out
        }

        fn line_of(&self, offset: usize) -> u64 {
            (self
                .line_starts
                .iter()
                .filter(|&&start| start <= offset)
                .count()
                - 1) as u64
        }

        fn insert(&mut self, at: usize, run: &str) {
            let run: Vec<char> = run.chars().collect();
            let n = run.len();
            let mut starts: Vec<usize> = self
                .line_starts
                .iter()
                .map(|&start| if start > at { start + n } else { start })
                .collect();
            starts.extend(breaks_of(&run).into_iter().map(|offset| at + offset));
            starts.sort_unstable();
            starts.dedup();
            self.chars.splice(at..at, run);
            self.line_starts = starts;
        }

        fn delete(&mut self, from: usize, to: usize) {
            let n = to - from;
            self.line_starts = self
                .line_starts
                .iter()
                .filter(|&&start| !(from < start && start <= to))
                .map(|&start| if start > to { start - n } else { start })
                .collect();
            self.chars.drain(from..to);
        }

        fn worst(&self, r_batch: usize) -> u64 {
            let mut fold = ParagraphLengthFold::new(config(r_batch, 0));
            for length in self.lines() {
                fold.push(0, length);
            }
            fold.worst_batch_chars()
        }
    }

    fn reads(plan: &WatchPlan<'_>, buffer: &Buffer) -> Vec<u64> {
        let all = buffer.lines();
        plan.lines_to_read()
            .map(|line| all[usize::try_from(line).expect("line index fits usize")])
            .collect()
    }

    fn insert<'a>(
        state: ProfileWatchState,
        cfg: ProfileConfig,
        buffer: &mut Buffer,
        at: usize,
        run: &'a str,
    ) -> (ProfileWatchState, WatchPlan<'a>) {
        let line_count_before = buffer.line_starts.len() as u64;
        buffer.insert(at, run);
        let facts = InsertFacts {
            run,
            line_count_before,
            line_count_after: buffer.line_starts.len() as u64,
            start_line: buffer.line_of(at),
            end_line: buffer.line_of(at + run.chars().count()),
            between_cr_and_lf: at > 0
                && buffer.chars.get(at - 1) == Some(&'\r')
                && buffer.chars.get(at + run.chars().count()) == Some(&'\n'),
        };
        let plan = plan_insert(state, cfg, &facts);
        (complete(state, cfg, &plan, &reads(&plan, buffer)), plan)
    }

    fn delete(
        state: ProfileWatchState,
        cfg: ProfileConfig,
        buffer: &mut Buffer,
        from: usize,
        to: usize,
    ) -> (ProfileWatchState, WatchPlan<'static>) {
        let line_count_before = buffer.line_starts.len() as u64;
        buffer.delete(from, to);
        let facts = DeleteFacts {
            line_count_before,
            line_count_after: buffer.line_starts.len() as u64,
            line: buffer.line_of(from),
        };
        let plan = plan_delete(cfg, facts);
        (complete(state, cfg, &plan, &reads(&plan, buffer)), plan)
    }

    fn start(buffer: &Buffer, r_batch: usize) -> ProfileWatchState {
        ProfileWatchState::from_profile(ProfileSummary {
            total_bytes: 0,
            longest_paragraph_chars: buffer.lines().into_iter().max().unwrap_or(0),
            worst_batch_chars: buffer.worst(r_batch),
        })
    }

    #[test]
    fn the_model_matches_the_probed_buffer() {
        // The GTK 4.22 observations ledger A42 pins.
        let mut b = Buffer::new("aa\rX\nbb");
        b.delete(3, 4);
        assert_eq!(b.lines(), vec![2, 0, 2]);
        let mut b = Buffer::new("aa\r\nbb");
        b.insert(3, "Y");
        assert_eq!(b.lines(), vec![4, 2]);
        let mut b = Buffer::new("ab\rcd");
        b.insert(3, "\nxy");
        assert_eq!(b.lines(), vec![2, 0, 4]);
        let mut b = Buffer::new("aa\r\nbb");
        b.delete(3, 4);
        assert_eq!(b.lines(), vec![5]);
        let mut b = Buffer::new("aa\r\nbb");
        b.delete(2, 3);
        assert_eq!(b.lines(), vec![2, 2]);
    }

    #[test]
    fn two_fast_keystrokes_crossing_v_batch_are_seen() {
        // V_batch 10, worst window 5: the draft's uncarried bound
        // (`worst + inserted`) lets two 3-character keystrokes through while
        // the real window reaches 11.
        let cfg = config(1, 10);
        let mut buffer = Buffer::new("aaaaa");
        let state = start(&buffer, 1);
        let (state, plan) = insert(state, cfg, &mut buffer, 0, "bbb");
        assert!(matches!(plan, WatchPlan::FastPath { .. }));
        let (state, plan) = insert(state, cfg, &mut buffer, 0, "ccc");
        assert!(
            matches!(plan, WatchPlan::ReadWindow(_)),
            "carried bound 8 + 3 > 10"
        );
        assert_eq!(state.worst_batch_chars(), 11);
        assert_eq!(buffer.worst(1), 11);

        // The retired draft rule, transcribed: a fast step when
        // `worst_batch_chars + inserted <= v_batch`, never carried.
        let draft_takes_fast_path = |worst: u64, inserted: u64| worst + inserted <= 10;
        assert!(draft_takes_fast_path(5, 3) && draft_takes_fast_path(5, 3));
    }

    #[test]
    fn text_between_cr_and_lf_stays_in_its_line() {
        // Deleting the text between `\r` and `\n` removes no line start, so
        // the buffer keeps "aa\r" and "\n" as separate lines: nothing grows.
        let cfg = config(2, 100);
        let mut buffer = Buffer::new("aa\rX\nbb");
        let state = start(&buffer, 2);
        let (next, plan) = delete(state, cfg, &mut buffer, 3, 4);
        assert_eq!(plan, WatchPlan::Nothing);
        assert_eq!(next, state);
        // Inserting between `\r` and `\n` adds no line but turns the CRLF
        // ending into LF, so "aa\rY" grows by two for one typed character:
        // the window path measures it.
        let mut buffer = Buffer::new("aa\r\nbb");
        let state = start(&buffer, 2);
        let (state, plan) = insert(state, cfg, &mut buffer, 3, "Y");
        assert!(matches!(plan, WatchPlan::ReadWindow(_)));
        assert_eq!(state.longest_paragraph_chars(), 4);
    }

    #[test]
    fn a_run_with_inner_lines_is_measured_from_its_text() {
        let cfg = config(2, 1_000);
        let mut buffer = Buffer::new("ab\ncd");
        let state = start(&buffer, 2);
        let (state, plan) = insert(state, cfg, &mut buffer, 1, "1\n22222\n333\r");
        let WatchPlan::ReadWindow(window) = &plan else {
            panic!("multi-line insertion takes the window path");
        };
        assert_eq!(window.inner_lengths().collect::<Vec<_>>(), vec![5, 3]);
        assert_eq!(state.longest_paragraph_chars(), 5);
        assert_eq!(state.worst_batch_chars(), buffer.worst(2));
        assert!(plan.lines_to_read().count() <= 4);
    }

    #[test]
    fn an_lf_typed_after_a_cr_starts_a_new_line() {
        let cfg = config(2, 1_000);
        let mut buffer = Buffer::new("ab\rcd");
        let state = start(&buffer, 2);
        let (state, plan) = insert(state, cfg, &mut buffer, 3, "\nxy");
        assert!(matches!(plan, WatchPlan::ReadWindow(_)));
        assert_eq!(buffer.lines(), vec![2, 0, 4]);
        assert_eq!(state.worst_batch_chars(), buffer.worst(2));
        assert_eq!(state.longest_paragraph_chars(), 4);
    }

    #[test]
    fn a_merge_raises_the_longest_paragraph() {
        let cfg = config(2, 1_000);
        let mut buffer = Buffer::new("aaa\nbbbb");
        let state = start(&buffer, 2);
        let (state, _) = delete(state, cfg, &mut buffer, 3, 4);
        assert_eq!(state.longest_paragraph_chars(), 7);
        // Deleting the `\n` of a CRLF removes a line start too: the lines join
        // with an embedded `\r`.
        let mut buffer = Buffer::new("aa\r\nbb");
        let state = start(&buffer, 2);
        let (state, plan) = delete(state, cfg, &mut buffer, 3, 4);
        assert!(matches!(plan, WatchPlan::ReadWindow(_)));
        assert_eq!(state.longest_paragraph_chars(), 5);
    }

    #[test]
    fn plans_read_at_most_two_r_batch_lines() {
        let cfg = config(3, 0);
        let mut buffer = Buffer::new("a\nb\nc\nd\ne\nf\ng\nh\ni");
        let state = start(&buffer, 3);
        let (_, plan) = insert(state, cfg, &mut buffer.clone(), 8, "x\ny\nz");
        assert!(plan.lines_to_read().count() <= 2 * 3);
        let (_, plan) = delete(state, cfg, &mut buffer, 7, 9);
        assert!(plan.lines_to_read().count() <= 2 * 3);
    }

    /// Every buffer of up to three symbols with every reachable-or-not line
    /// structure, every state the invariant admits, and every one-symbol or
    /// two-symbol edit: the watch keeps the invariant the Kani step proves.
    #[test]
    fn exhaustive_small_step_keeps_the_invariant() {
        let symbols = ['a', '\r', '\n', '\u{2029}'];
        let is_delimiter = |c: char| matches!(c, '\r' | '\n' | '\u{2029}');
        let mut texts: Vec<Vec<char>> = vec![Vec::new()];
        for len in 1..=3 {
            let mut next = Vec::new();
            for text in texts.iter().filter(|t| t.len() == len - 1) {
                for &s in &symbols {
                    let mut t = text.clone();
                    t.push(s);
                    next.push(t);
                }
            }
            texts.extend(next);
        }
        let runs: Vec<String> = {
            let mut out = Vec::new();
            for &a in &symbols {
                out.push(a.to_string());
                for &b in &symbols {
                    out.push([a, b].iter().collect());
                }
            }
            out
        };
        for chars in &texts {
            let candidates: Vec<usize> = (1..=chars.len())
                .filter(|&i| is_delimiter(chars[i - 1]))
                .collect();
            for mask in 0..(1u32 << candidates.len()) {
                let mut line_starts = vec![0];
                for (bit, &offset) in candidates.iter().enumerate() {
                    if mask & (1 << bit) != 0 {
                        line_starts.push(offset);
                    }
                }
                let buffer = Buffer {
                    chars: chars.clone(),
                    line_starts,
                };
                for r_batch in 1..=2 {
                    let longest = buffer.lines().into_iter().max().unwrap_or(0);
                    let worst = buffer.worst(r_batch);
                    for v_batch in 0..=5u64 {
                        let cfg = config(r_batch, v_batch);
                        for hl in longest..=longest + 1 {
                            for hw in worst..=worst + 1 {
                                for sw in 0..=hw {
                                    for bound in worst..=worst + 1 {
                                        for no_fast in [false, true] {
                                            if no_fast && sw != hw {
                                                continue;
                                            }
                                            if (sw > v_batch) != (hw > v_batch) {
                                                continue;
                                            }
                                            let state = ProfileWatchState {
                                                longest_paragraph_chars: hl,
                                                worst_batch_chars: sw,
                                                density_bound: bound,
                                            };
                                            check_edits(
                                                &buffer, state, cfg, hl, hw, no_fast, &runs,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn check_edits(
        buffer: &Buffer,
        state: ProfileWatchState,
        cfg: ProfileConfig,
        hl: u64,
        hw: u64,
        no_fast: bool,
        runs: &[String],
    ) {
        let mut cases: Vec<(Buffer, WatchPlan<'_>, String)> = Vec::new();
        for at in 0..=buffer.chars.len() {
            for run in runs {
                let mut next = buffer.clone();
                let (_, plan) = insert(state, cfg, &mut next, at, run);
                cases.push((next, plan, format!("insert {run:?} at {at}")));
            }
            for count in 1..=2 {
                if at + count <= buffer.chars.len() {
                    let mut next = buffer.clone();
                    let (_, plan) = delete(state, cfg, &mut next, at, at + count);
                    cases.push((next, plan, format!("delete {at}..{}", at + count)));
                }
            }
        }
        for (next, plan, label) in cases {
            let after = complete(state, cfg, &plan, &reads(&plan, &next));
            let longest = next.lines().into_iter().max().unwrap_or(0);
            let worst = next.worst(cfg.r_batch);
            let hl2 = hl.max(longest);
            let hw2 = hw.max(worst);
            let fast = matches!(plan, WatchPlan::FastPath { .. });
            let no_fast2 = no_fast && !fast;
            let ok = after.longest_paragraph_chars == hl2
                && after.density_bound >= worst
                && after.worst_batch_chars <= hw2
                && (!no_fast2 || after.worst_batch_chars == hw2)
                && (after.worst_batch_chars > cfg.v_batch) == (hw2 > cfg.v_batch);
            assert!(
                ok,
                "{label} on {:?} starts {:?} state {state:?} hl {hl} hw {hw} no_fast {no_fast} cfg r{} v{} -> {after:?}, longest {longest} worst {worst}, plan {plan:?}",
                buffer.chars, buffer.line_starts, cfg.r_batch, cfg.v_batch
            );
        }
    }
}
