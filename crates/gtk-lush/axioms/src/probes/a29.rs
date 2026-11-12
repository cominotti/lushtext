// SPDX-License-Identifier: MIT OR Apache-2.0

//! A29: the GtkSourceView context engine analyses a line **atomically**, in
//! one call, with a fixed **2000 ms** cut-off. A line whose analysis runs past
//! it makes the engine log a `GtkSourceView-CRITICAL` ("Highlighting a single
//! line took too much time, syntax highlighting will be disabled") from
//! inside that one call, and turn highlighting **off for the whole buffer**:
//! a short line added afterwards gets no highlighting. The engine also
//! **reads hidden text**: on a short line, highlight tags land inside a range
//! tagged `invisible`.
//!
//! Whether a given line reaches 2000 ms depends on the runner's speed, so the
//! probe doubles a minified-markup line until the cut-off fires, up to a
//! length cap and a 60 s wall budget ([`next_attempt_fits`]: an attempt the
//! earlier ones predict, with a margin, would end past the budget is not
//! started); reaching
//! either without the cut-off is `FixtureInvalid`, never `Violated`. The line
//! length that reached it and every attempt's duration are recorded values.
//!
//! The 2000 ms deadline covers only `analyze_line`'s segment search for the
//! one line (`gtksourcecontextengine.c`), not the call: a line whose analysis
//! finishes under it is then tagged by `ensure_highlighted`, so a call
//! without the cut-off can run far past 2000 ms (6.57 s at 32,768
//! characters in the GNOME 51 SDK), while a cut-off call returns at about
//! 2000 ms because the disabled engine applies no tags.
//!
//! The probe runs for a few seconds (the cut-off itself is 2000 ms), far
//! below the pull-request probe job's timeout, so it is not marked slow.
//!
//! The document scale policy (C1) turns syntax off before the engine sees a
//! giant paragraph; folding (C2) cannot hide text from it.

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{
    LogCapture, ParagraphKind, SourceViewFixture, invisible_tag, offset, paragraph, time_us,
};
use crate::{AxiomId, Observation};

/// The line length the doubling starts at.
pub const FIRST_LENGTH: usize = 4_096;
/// The longest line the doubling tries before calling the fixture invalid.
pub const LENGTH_CAP: usize = 1 << 24;
/// The text of the cut-off's critical message.
pub const CUT_OFF_MESSAGE: &str = "Highlighting a single line took too much time";
/// The short line the hidden-text check highlights: a visible element, then
/// an element inside the hidden range.
pub const HIDDEN_LINE: (&str, &str) = ("<b class=\"v\">visible</b>", "<i class=\"h\">hidden</i>");

/// The wall time, in microseconds, the doubling's attempts may take in all.
/// An attempt is not started when the attempts so far predict it would end
/// past this budget; the probe is then `FixtureInvalid`, never `Violated`.
pub const WALL_BUDGET_US: u128 = 60_000_000;
/// The least growth factor an attempt is predicted to take over the one
/// before: analysis of this markup grows about quadratically with the line,
/// so doubling the line costs about four times as much.
pub const MIN_PREDICTED_GROWTH: u128 = 4;

/// The margin on the predicted growth, as a fraction: growth can steepen
/// from one doubling to the next (the nightly SDK's last attempt grew 4.95x
/// after 3.96x), so the prediction is a quarter above the larger of
/// [`MIN_PREDICTED_GROWTH`] and the last ratio.
pub const GROWTH_MARGIN: (u128, u128) = (5, 4);

/// Whether the doubling may start another attempt after attempts that took
/// `previous_us` microseconds each, in order: it may unless the time so far,
/// plus the last attempt times the larger of [`MIN_PREDICTED_GROWTH`] and
/// the last growth ratio, with [`GROWTH_MARGIN`], passes [`WALL_BUDGET_US`].
#[must_use]
pub fn next_attempt_fits(previous_us: &[u128]) -> bool {
    let Some((&last, earlier)) = previous_us.split_last() else {
        return true;
    };
    let by_minimum = last.saturating_mul(MIN_PREDICTED_GROWTH);
    let by_ratio = earlier
        .last()
        .filter(|&&before| before > 0)
        .map_or(0, |&before| last.saturating_mul(last) / before);
    let so_far = previous_us
        .iter()
        .fold(0u128, |total, &micros| total.saturating_add(micros));
    let (numerator, denominator) = GROWTH_MARGIN;
    let predicted = by_minimum.max(by_ratio).saturating_mul(numerator) / denominator;
    so_far.saturating_add(predicted) <= WALL_BUDGET_US
}

/// The highlighting tags (every tag but `except`) at `offset` in `buffer`.
#[must_use]
pub fn highlight_tags_at(
    buffer: &sourceview5::Buffer,
    offset: i32,
    except: Option<&gtk4::TextTag>,
) -> Vec<String> {
    buffer
        .iter_at_offset(offset)
        .tags()
        .into_iter()
        .filter(|tag| except != Some(tag))
        .map(|tag| {
            tag.name()
                .map_or_else(|| "(anonymous)".to_owned(), |name| name.to_string())
        })
        .collect()
}

/// One highlighting attempt on a fresh buffer holding one minified-markup
/// line of `chars` characters.
#[derive(Clone, Debug)]
pub struct CutOffAttempt {
    /// The line length.
    pub chars: usize,
    /// How long the single `ensure_highlight` call took.
    pub micros: u128,
    /// Whether the call logged the cut-off critical.
    pub cut_off: bool,
    /// Highlight tags on a short line added after the call.
    pub later_line_tags: Vec<String>,
    /// Every message the call logged.
    pub messages: Vec<String>,
}

/// Highlight one `chars`-character minified-markup line in one call, then a
/// short line added after it.
#[must_use]
pub fn highlight_attempt(chars: usize) -> Option<CutOffAttempt> {
    let fixture = SourceViewFixture::html(gtk4::WrapMode::None)?;
    let buffer = fixture.buffer;
    buffer.set_text(&paragraph(ParagraphKind::MinifiedMarkup, chars, 29));
    let capture = LogCapture::start();
    let ((), micros) = time_us(|| {
        let (start, end) = buffer.bounds();
        buffer.ensure_highlight(&start, &end);
    });
    let messages = capture.messages();
    let cut_off = messages
        .iter()
        .any(|message| message.level == "critical" && message.message.contains(CUT_OFF_MESSAGE));
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, "\n<b class=\"x\">later</b>");
    let line_start = buffer.char_count() - 20;
    let (start, end) = buffer.bounds();
    buffer.ensure_highlight(&start, &end);
    let later_line_tags = highlight_tags_at(&buffer, line_start + 2, None);
    drop(capture);
    Some(CutOffAttempt {
        chars,
        micros,
        cut_off,
        later_line_tags,
        messages: messages.iter().map(ToString::to_string).collect(),
    })
}

/// Probe A29.
#[must_use]
pub fn probe_a29() -> Observation {
    Recorder::run(AxiomId::new(29), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let fixture = recorder.require(
            SourceViewFixture::html(gtk4::WrapMode::None),
            "control: GtkSourceView has an html language",
        )?;
        let buffer = &fixture.buffer;
        let tag = invisible_tag(buffer.upcast_ref());
        let (visible, hidden) = HIDDEN_LINE;
        buffer.set_text(&format!("{visible}{hidden}"));
        let hidden_start = offset(visible.chars().count());
        let start = buffer.iter_at_offset(hidden_start);
        buffer.apply_tag(&tag, &start, &buffer.end_iter());
        let (start, end) = buffer.bounds();
        buffer.ensure_highlight(&start, &end);
        let visible_tags = highlight_tags_at(buffer, 1, Some(&tag));
        let hidden_tags = highlight_tags_at(buffer, hidden_start + 1, Some(&tag));
        recorder.measure("visible_element_tags", super::join_entries(&visible_tags));
        recorder.measure("hidden_element_tags", super::join_entries(&hidden_tags));
        recorder.control(
            !visible_tags.is_empty(),
            "control: the engine highlights a visible element",
        )?;
        recorder.axiom(
            !hidden_tags.is_empty(),
            "the engine highlights text inside an invisible range",
        )?;

        let mut chars = FIRST_LENGTH;
        let mut attempts = Vec::new();
        let mut durations = Vec::new();
        let reached = loop {
            let attempt = recorder.require(
                highlight_attempt(chars),
                "control: GtkSourceView has an html language",
            )?;
            attempts.push(format!(
                "{}: {} us{}",
                attempt.chars,
                attempt.micros,
                if attempt.cut_off { " cut-off" } else { "" }
            ));
            durations.push(attempt.micros);
            if attempt.cut_off || chars >= LENGTH_CAP || !next_attempt_fits(&durations) {
                break attempt;
            }
            chars = chars.saturating_mul(2);
        };
        recorder.measure("attempts", super::join_entries(&attempts));
        recorder.measure("attempts_wall_us", durations.iter().sum::<u128>());
        recorder.control(
            reached.cut_off || next_attempt_fits(&durations),
            "control: the 60 s wall budget did not stop the doubling before the cut-off",
        )?;
        recorder.measure("cut_off_line_chars", reached.chars);
        recorder.measure("cut_off_call_us", reached.micros);
        recorder.measure("captured_messages", super::join_entries(&reached.messages));
        recorder.measure(
            "later_line_tags",
            super::join_entries(&reached.later_line_tags),
        );
        recorder.control(
            reached.cut_off,
            "control: one analysis call reached the 2000 ms cut-off below the length cap",
        )?;
        recorder.axiom(
            reached.later_line_tags.is_empty(),
            "after the cut-off, a short line added later gets no highlighting",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{GROWTH_MARGIN, WALL_BUDGET_US, next_attempt_fits};

    /// The GNOME 51 SDK's recorded attempts (`baseline-runtimes.out`): the
    /// 32,768-character call ran 6.57 s without the cut-off, and the
    /// 65,536-character call then cut off at 2.0005 s.
    const GNOME_51: [u128; 4] = [112_065, 417_957, 1_661_444, 6_574_169];
    /// The host's ledger sequence: the 32,768-character call cut off.
    const HOST: [u128; 3] = [125_000, 464_000, 1_840_000];

    /// The nightly SDK (`469c3f9d`, GtkSourceView 5.23.0) never logged the
    /// cut-off, and its last growth (4.95x, 10.45 s to 51.7 s) was steeper
    /// than the 4x the attempts before it showed: the 131,072-character
    /// attempt ran the doubling to 65.7 s, past the budget.
    const NIGHTLY: [u128; 5] = [50_395, 175_882, 675_084, 2_639_686, 10_454_336];

    #[test]
    fn the_nightly_stops_before_an_attempt_that_would_pass_the_budget() {
        for end in 1..NIGHTLY.len() {
            assert!(next_attempt_fits(&NIGHTLY[..end]), "after {end} attempts");
        }
        assert!(!next_attempt_fits(&NIGHTLY));
    }

    #[test]
    fn the_first_attempts_always_start() {
        assert!(next_attempt_fits(&[]));
        assert!(next_attempt_fits(&[1_000_000]));
    }

    #[test]
    fn gnome_51_continues_to_its_cut_off() {
        for end in 1..=GNOME_51.len() {
            assert!(next_attempt_fits(&GNOME_51[..end]), "after {end} attempts");
        }
    }

    #[test]
    fn the_host_continues_to_its_cut_off() {
        for end in 1..=HOST.len() {
            assert!(next_attempt_fits(&HOST[..end]), "after {end} attempts");
        }
    }

    #[test]
    fn a_prediction_past_the_budget_stops() {
        // 8.5 s so far, and 5 x 6.4 s = 32 s predicted: 40.5 s fits; one
        // more doubling at the same growth predicts 34.1 + 128 s.
        assert!(next_attempt_fits(&[100_000, 400_000, 1_600_000, 6_400_000]));
        assert!(!next_attempt_fits(&[
            100_000, 400_000, 1_600_000, 6_400_000, 25_600_000
        ]));
    }

    #[test]
    fn a_steeper_growth_than_four_predicts_with_the_ratio() {
        // 10.6 s so far: 4x growth with the margin (5 x 8 s = 40 s) would
        // fit, but the last growth was 5x, which predicts 6.25 x 8 s = 50 s
        // and 60.6 s in all.
        assert!(!next_attempt_fits(&[1_000_000, 1_600_000, 8_000_000]));
        // The same last attempt after a 4x growth predicts 51 s in all.
        assert!(next_attempt_fits(&[1_000_000, 2_000_000, 8_000_000]));
    }

    #[test]
    fn the_budget_is_sixty_seconds_with_a_quarter_margin() {
        assert_eq!(WALL_BUDGET_US, 60_000_000);
        assert_eq!(GROWTH_MARGIN, (5, 4));
    }
}
