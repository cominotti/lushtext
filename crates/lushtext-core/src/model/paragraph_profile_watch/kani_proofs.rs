// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the edit-time paragraph watch: [`plan_insert`],
//! [`plan_delete`] and [`complete`], as an **inductive step** against
//! recomputation from scratch.
//!
//! Instead of replaying a bounded edit sequence from one start document, the
//! harnesses take an arbitrary buffer and an arbitrary watch state related to
//! it by the invariant below, apply **one** edit, and check that the invariant
//! holds again. The invariant holds after a load (`ProfileWatchState::from_profile`
//! sets the high-waters to the loaded profile, with no fast step yet), so by
//! induction it holds after **every** edit sequence of any length. (The
//! design's bounded replay of three edits from a 4-symbol start did not fit:
//! one replayed edit already took 14 minutes and 9 GiB.)
//!
//! The invariant, with `H_l`/`H_w` the high-waters of the recomputed longest
//! paragraph and worst window over the session so far:
//! - `longest_paragraph_chars == H_l` and `H_l >=` the buffer's longest paragraph;
//! - `density_bound >=` the buffer's worst window;
//! - `worst_batch_chars <= H_w` and `H_w >=` the buffer's worst window;
//! - `worst_batch_chars == H_w` while no step took the fast path;
//! - `worst_batch_chars > v_batch` exactly when `H_w > v_batch` (the decision
//!   the scale reads).
//!
//! Domain: a buffer of at most [`DOC_SYMBOLS`] symbols from {`a`, `\r`, `\n`,
//! U+2029} with **any** line structure `GtkTextBuffer` can reach (ledger A42:
//! a line starts only right after a delimiter character, and an edit history
//! can leave a delimiter inside a line, so every subset of after-delimiter
//! offsets is allowed, a superset of the reachable ones); one insertion of at
//! most 2 symbols at any offset or deletion of any range of at most 2 symbols;
//! `r_batch` of 1 or 2; `v_batch`, the state and the high-waters at most
//! [`VALUE_BOUND`] (every length and window of such a buffer is smaller). The
//! edit follows A42: an insertion adds line starts only after the delimiters
//! of its own text and shifts the later ones; a deletion removes exactly the
//! line starts in `(start, end]`. A line's paragraph length is its characters
//! less its trailing delimiter, the quantity `paragraph_chars_from_line`
//! derives from `chars_in_line` (proved by the boundary harnesses). The model
//! trusts A42 as the editor-memory proof trusts its `BTreeMap`; the probe pins
//! it. Arrays are fixed-capacity with symbolic lengths, never `String`s or
//! growing `Vec`s, so loops run per symbol (`unwind(8)`).
//!
//! Deleting the text between `\r` and `\n` needs three symbols, outside that
//! domain, so `watch_delete_between_cr_and_lf_preserves_the_invariant` proves
//! the same step for that one deletion from the buffer `\r`, any symbol,
//! `\n` ([`Doc::cr_any_lf`]), with any line structure after its delimiters
//! and any state satisfying the invariant.
//!
//! Compiled only under `cfg(kani)` (`make kani`).

use super::{
    DeleteFacts, InsertFacts, ProfileWatchState, WatchPlan, complete, plan_delete, plan_insert,
};
use crate::model::paragraph_boundary::{
    PROFILE_THRESHOLD_COUNT, ParagraphLengthFold, ProfileConfig, ProfileThresholds,
};

/// Most symbols in the buffer before the edit.
const DOC_SYMBOLS: usize = 2;
/// Model capacity: the buffer plus one insertion of 2 (which also holds the
/// three-symbol buffer of [`Doc::cr_any_lf`]).
const CAPACITY: usize = DOC_SYMBOLS + 2;
/// Bound on `v_batch`, the state's values and the high-waters.
const VALUE_BOUND: u64 = 16;
/// Most lines a plan can name here (`2 · r_batch` with `r_batch` at most 2,
/// plus slack the read-bound assertion checks).
const MAX_READS: usize = 8;

const SYMBOL_A: u8 = 0;
const SYMBOL_CR: u8 = 1;
const SYMBOL_LF: u8 = 2;
const SYMBOL_PS: u8 = 3;

/// A model buffer: symbols, and `starts[i]` for "a line starts at offset
/// `i`" (`starts[0]` is always true).
#[derive(Clone, Copy)]
struct Doc {
    symbols: [u8; CAPACITY],
    starts: [bool; CAPACITY + 1],
    len: usize,
}

fn is_delimiter(symbol: u8) -> bool {
    matches!(symbol, SYMBOL_CR | SYMBOL_LF | SYMBOL_PS)
}

/// Symbols in the reference-rule delimiter starting at `i` of `symbols[..len]`
/// (0 when none starts there); used to split an inserted run alone.
fn delimiter_at(symbols: &[u8], len: usize, i: usize) -> usize {
    match symbols[i] {
        SYMBOL_LF if i > 0 && symbols[i - 1] == SYMBOL_CR => 0,
        SYMBOL_LF | SYMBOL_PS => 1,
        SYMBOL_CR if i + 1 < len && symbols[i + 1] == SYMBOL_LF => 2,
        SYMBOL_CR => 1,
        _ => 0,
    }
}

impl Doc {
    /// Any buffer `GtkTextBuffer` can reach, and more (see the module docs).
    fn any() -> Self {
        let mut doc = Self {
            symbols: [SYMBOL_A; CAPACITY],
            starts: [false; CAPACITY + 1],
            len: kani::any(),
        };
        kani::assume(doc.len <= DOC_SYMBOLS);
        doc.starts[0] = true;
        let mut i = 0;
        while i < DOC_SYMBOLS {
            let symbol: u8 = kani::any();
            kani::assume(symbol <= SYMBOL_PS);
            doc.symbols[i] = symbol;
            let start: bool = kani::any();
            kani::assume(!start || (i < doc.len && is_delimiter(symbol)));
            doc.starts[i + 1] = start;
            i += 1;
        }
        doc
    }

    /// A three-symbol buffer `\r`, any symbol, `\n`, with any line structure
    /// after its delimiters: the shape the design's line-span delete finding
    /// is about, one symbol beyond [`DOC_SYMBOLS`].
    fn cr_any_lf() -> Self {
        let mut doc = Self {
            symbols: [SYMBOL_A; CAPACITY],
            starts: [false; CAPACITY + 1],
            len: 3,
        };
        doc.symbols[0] = SYMBOL_CR;
        doc.symbols[1] = any_symbol();
        doc.symbols[2] = SYMBOL_LF;
        doc.starts[0] = true;
        let mut i = 0;
        while i < 3 {
            let start: bool = kani::any();
            kani::assume(!start || is_delimiter(doc.symbols[i]));
            doc.starts[i + 1] = start;
            i += 1;
        }
        doc
    }

    fn line_count(&self) -> u64 {
        let mut count = 0;
        let mut i = 0;
        while i <= self.len {
            if self.starts[i] {
                count += 1;
            }
            i += 1;
        }
        count
    }

    /// The line an iter at `offset` is on: line starts in `(0, offset]`.
    fn line_of(&self, offset: usize) -> u64 {
        let mut line = 0;
        let mut i = 1;
        while i <= offset {
            if self.starts[i] {
                line += 1;
            }
            i += 1;
        }
        line
    }

    /// Paragraph lengths line by line, and the line count.
    fn lines(&self) -> ([u64; CAPACITY + 1], usize) {
        let mut lengths = [0_u64; CAPACITY + 1];
        let mut count = 0;
        let mut line_start = 0;
        let mut i = 1;
        while i <= self.len + 1 {
            if i == self.len + 1 || self.starts[i] {
                let end = if i == self.len + 1 { self.len } else { i };
                let last = i == self.len + 1;
                let chars = end - line_start;
                let delimiter = if last || chars == 0 {
                    0
                } else if chars >= 2
                    && self.symbols[end - 2] == SYMBOL_CR
                    && self.symbols[end - 1] == SYMBOL_LF
                {
                    2
                } else if is_delimiter(self.symbols[end - 1]) {
                    1
                } else {
                    0
                };
                lengths[count] = (chars - delimiter) as u64;
                count += 1;
                line_start = end;
            }
            i += 1;
        }
        (lengths, count)
    }

    /// The recomputed policy view: longest paragraph and worst window.
    fn recompute(&self, config: ProfileConfig) -> (u64, u64) {
        let (lengths, count) = self.lines();
        let mut fold = ParagraphLengthFold::new(config);
        let mut line = 0;
        while line < count {
            fold.push(0, lengths[line]);
            line += 1;
        }
        (fold.longest_paragraph_chars(), fold.worst_batch_chars())
    }

    fn insert(&self, at: usize, inserted: [u8; 2], count: usize) -> Self {
        let mut next = Self {
            symbols: [SYMBOL_A; CAPACITY],
            starts: [false; CAPACITY + 1],
            len: self.len + count,
        };
        let mut k = 0;
        while k < next.len {
            next.symbols[k] = if k < at {
                self.symbols[k]
            } else if k < at + count {
                inserted[k - at]
            } else {
                self.symbols[k - count]
            };
            k += 1;
        }
        let mut i = 0;
        while i <= self.len {
            if self.starts[i] {
                next.starts[if i > at { i + count } else { i }] = true;
            }
            i += 1;
        }
        let mut j = 0;
        while j < count {
            let delimiter = delimiter_at(&inserted, count, j);
            if delimiter > 0 {
                next.starts[at + j + delimiter] = true;
            }
            j += 1;
        }
        next
    }

    fn delete(&self, from: usize, count: usize) -> Self {
        let to = from + count;
        let mut next = Self {
            symbols: [SYMBOL_A; CAPACITY],
            starts: [false; CAPACITY + 1],
            len: self.len - count,
        };
        let mut k = 0;
        while k < next.len {
            next.symbols[k] = if k < from {
                self.symbols[k]
            } else {
                self.symbols[k + count]
            };
            k += 1;
        }
        let mut i = 0;
        while i <= self.len {
            if self.starts[i] && !(from < i && i <= to) {
                next.starts[if i > to { i - count } else { i }] = true;
            }
            i += 1;
        }
        next
    }
}

fn symbol_utf8(symbol: u8) -> (usize, [u8; 3]) {
    match symbol {
        SYMBOL_CR => (1, [b'\r', 0, 0]),
        SYMBOL_LF => (1, [b'\n', 0, 0]),
        SYMBOL_PS => (3, [0xE2, 0x80, 0xA9]),
        _ => (1, [b'a', 0, 0]),
    }
}

fn any_symbol() -> u8 {
    let symbol: u8 = kani::any();
    kani::assume(symbol <= SYMBOL_PS);
    symbol
}

fn any_value() -> u64 {
    let value: u64 = kani::any();
    kani::assume(value <= VALUE_BOUND);
    value
}

fn any_config() -> ProfileConfig {
    let r_batch: usize = kani::any();
    kani::assume((1..=2).contains(&r_batch));
    ProfileConfig {
        thresholds: ProfileThresholds([u64::MAX; PROFILE_THRESHOLD_COUNT]),
        offset_cap: 0,
        r_batch,
        v_batch: any_value(),
    }
}

/// Read the plan's lines from the model into a fixed buffer: the reads, how
/// many there were, and whether every named line existed.
fn reads_for(plan: &WatchPlan<'_>, doc: &Doc) -> ([u64; MAX_READS], usize, bool) {
    let (lengths, count) = doc.lines();
    let mut reads = [0_u64; MAX_READS];
    let mut n = 0;
    let mut in_range = true;
    for line in plan.lines_to_read() {
        let line = usize::try_from(line).unwrap();
        in_range &= line < count && n < MAX_READS;
        if line < count && n < MAX_READS {
            reads[n] = lengths[line];
        }
        n += 1;
    }
    (reads, n, in_range)
}

/// The invariant of the module docs.
struct Invariant {
    state: ProfileWatchState,
    high_longest: u64,
    high_worst: u64,
    no_fast_step: bool,
}

impl Invariant {
    fn holds(&self, doc: &Doc, config: ProfileConfig) -> bool {
        let (longest, worst) = doc.recompute(config);
        self.state.longest_paragraph_chars == self.high_longest
            && self.high_longest >= longest
            && self.state.density_bound >= worst
            && self.state.worst_batch_chars <= self.high_worst
            && self.high_worst >= worst
            && (!self.no_fast_step || self.state.worst_batch_chars == self.high_worst)
            && (self.state.worst_batch_chars > config.v_batch) == (self.high_worst > config.v_batch)
    }
}

/// What one step observed: the three checked properties, and the shapes the
/// general harnesses cover ([`cover_step_shapes`]).
#[derive(Default)]
struct Step {
    invariant_holds: bool,
    reads_in_range: bool,
    reads_bounded: bool,
    lf_typed_after_cr: bool,
    insertion_between_cr_and_lf: bool,
    deleted_lf_of_crlf: bool,
    fast: bool,
    window: bool,
    fast_after_fast: bool,
    density_crossing: bool,
    raised_only_density: bool,
}

/// The step shapes the general harnesses must reach. Kept out of [`step_on`]
/// so the fixed-shape deletion harness, which reaches none of the insertion
/// shapes, does not report them unsatisfied.
fn cover_step_shapes(observed: &Step) {
    kani::cover!(observed.lf_typed_after_cr, "an LF typed after a CR");
    kani::cover!(
        observed.insertion_between_cr_and_lf,
        "an insertion between CR and LF"
    );
    kani::cover!(observed.deleted_lf_of_crlf, "deleting the LF of a CRLF");
    kani::cover!(observed.fast, "a fast-path step");
    kani::cover!(observed.window, "a window-path step");
    kani::cover!(
        observed.fast_after_fast,
        "a fast step after an earlier fast step"
    );
    kani::cover!(observed.density_crossing, "a density crossing");
    kani::cover!(
        observed.raised_only_density,
        "a step raising only the density window"
    );
}

/// One edit from any buffer and any state satisfying the invariant;
/// `uncarried` resets the density bound to the batch sum first (the retired
/// draft rule, expressed through the core's input).
fn step(uncarried: bool) -> Step {
    step_on(Doc::any(), uncarried, None)
}

/// [`step`] from a given buffer; `delete` forces the edit to delete that
/// `(offset, count)` instead of choosing any edit.
fn step_on(doc: Doc, uncarried: bool, delete: Option<(usize, usize)>) -> Step {
    let config = any_config();
    let mut state = ProfileWatchState {
        longest_paragraph_chars: any_value(),
        worst_batch_chars: any_value(),
        density_bound: any_value(),
    };
    let before = Invariant {
        state,
        high_longest: any_value(),
        high_worst: any_value(),
        no_fast_step: kani::any(),
    };
    kani::assume(before.holds(&doc, config));
    if uncarried {
        state.density_bound = state.worst_batch_chars;
    }

    let at: usize = delete.map_or_else(kani::any, |(at, _)| at);
    let count: usize = delete.map_or_else(kani::any, |(_, count)| count);
    kani::assume((1..=2).contains(&count) && at <= doc.len);
    // Outlives the plan, which borrows the inserted run.
    let mut run_bytes = [0_u8; 6];
    let mut observed = Step::default();
    let next;
    let plan;
    if delete.is_none() && kani::any() {
        let inserted = [any_symbol(), any_symbol()];
        next = doc.insert(at, inserted, count);
        let mut run_len = 0;
        let mut s = 0;
        while s < count {
            let (width, encoded) = symbol_utf8(inserted[s]);
            let mut k = 0;
            while k < width {
                run_bytes[run_len + k] = encoded[k];
                k += 1;
            }
            run_len += width;
            s += 1;
        }
        // SAFETY: whole UTF-8 encodings concatenated, valid by construction.
        let run = unsafe { std::str::from_utf8_unchecked(&run_bytes[..run_len]) };
        let after_cr = at > 0 && doc.symbols[at - 1] == SYMBOL_CR;
        observed.lf_typed_after_cr = after_cr && inserted[0] == SYMBOL_LF;
        observed.insertion_between_cr_and_lf =
            after_cr && at < doc.len && doc.symbols[at] == SYMBOL_LF;
        plan = plan_insert(
            state,
            config,
            &InsertFacts {
                run,
                line_count_before: doc.line_count(),
                line_count_after: next.line_count(),
                start_line: next.line_of(at),
                end_line: next.line_of(at + count),
                between_cr_and_lf: after_cr && at < doc.len && doc.symbols[at] == SYMBOL_LF,
            },
        );
    } else {
        kani::assume(count <= doc.len - at);
        next = doc.delete(at, count);
        observed.deleted_lf_of_crlf = count == 1
            && doc.symbols[at] == SYMBOL_LF
            && at > 0
            && doc.symbols[at - 1] == SYMBOL_CR;
        plan = plan_delete(
            config,
            DeleteFacts {
                line_count_before: doc.line_count(),
                line_count_after: next.line_count(),
                line: next.line_of(at),
            },
        );
    }

    let (reads, read_count, reads_in_range) = reads_for(&plan, &next);
    let fast = matches!(plan, WatchPlan::FastPath { .. });
    let reads_bounded = match plan {
        WatchPlan::FastPath { .. } => read_count == 1,
        WatchPlan::ReadWindow(_) => read_count <= 2 * config.r_batch + 2,
        WatchPlan::Nothing => read_count == 0,
    };
    let read_count = read_count.min(MAX_READS);
    let after_state = complete(state, config, &plan, &reads[..read_count]);
    let (longest, worst) = next.recompute(config);
    let after = Invariant {
        state: after_state,
        high_longest: before.high_longest.max(longest),
        high_worst: before.high_worst.max(worst),
        no_fast_step: before.no_fast_step && !fast,
    };
    let window = matches!(plan, WatchPlan::ReadWindow(_));
    Step {
        invariant_holds: after.holds(&next, config),
        reads_in_range,
        reads_bounded,
        fast,
        window,
        fast_after_fast: fast && !before.no_fast_step,
        density_crossing: before.state.worst_batch_chars <= config.v_batch
            && after_state.worst_batch_chars > config.v_batch,
        raised_only_density: window
            && after_state.longest_paragraph_chars == before.state.longest_paragraph_chars
            && after_state.worst_batch_chars > before.state.worst_batch_chars,
        ..observed
    }
}

/// One edit preserves the invariant: the watched longest paragraph equals the
/// high-water of the recomputed one, the carried density bound stays above
/// the buffer's worst window, the batch sum never exceeds the recomputed
/// high-water and equals it while no step took the fast path, and its
/// `v_batch` decision is exact. By induction, for every edit sequence.
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn watch_step_preserves_the_invariant() {
    let observed = step(false);
    cover_step_shapes(&observed);
    assert!(observed.invariant_holds);
}

/// Every plan names lines the buffer has, at most `2 · r_batch + 2` of them,
/// and a fast-path plan names one.
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn watch_reads_are_bounded() {
    let observed = step(false);
    cover_step_shapes(&observed);
    assert!(observed.reads_in_range);
    assert!(observed.reads_bounded);
}

/// Resetting the fast-path bound to the batch sum before a step (the draft
/// rule, never carried) breaks the invariant: after an earlier fast step the
/// batch sum lags the buffer, and a fast step crosses `v_batch` unseen.
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
#[kani::should_panic]
fn watch_fast_path_without_a_carried_bound_is_unsound() {
    let observed = step(true);
    cover_step_shapes(&observed);
    assert!(observed.invariant_holds);
}

/// Deleting the text between `\r` and `\n` preserves the invariant and reads
/// a bounded window. That buffer needs three symbols, one more than the
/// inductive step's [`DOC_SYMBOLS`], so it has its own harness over a fixed
/// shape (`\r`, any symbol, `\n`, any line structure after delimiters, any
/// state satisfying the invariant): the case of the design's withdrawn
/// line-span delete finding, which the general step cannot reach.
#[kani::proof]
#[kani::unwind(8)]
#[kani::solver(cadical)]
fn watch_delete_between_cr_and_lf_preserves_the_invariant() {
    let doc = Doc::cr_any_lf();
    kani::cover!(
        !is_delimiter(doc.symbols[1]),
        "deleting a non-delimiter between CR and LF"
    );
    let observed = step_on(doc, false, Some((1, 1)));
    kani::cover!(observed.window, "a window-path step");
    assert!(observed.invariant_holds);
    assert!(observed.reads_in_range);
    assert!(observed.reads_bounded);
}
