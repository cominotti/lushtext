// SPDX-License-Identifier: MIT OR Apache-2.0

//! A44: clearing a `GtkSourceBuffer`'s language (`set_language(None)` and
//! `set_highlight_syntax(false)`) inside an `insert-text` **after**-handler
//! means the inserted paragraph never receives highlighting tags: after the
//! main loop drains, no tag covers the inserted text. The buffer's class
//! handler only invalidates the region and schedules the context engine's
//! first-update idle, and the engine is gone before that idle could run.
//!
//! The control is the same insertion with the handler leaving the language in
//! place: the drained main loop then does tag the inserted text, so the probe
//! is not vacuous.
//!
//! The document scale policy (C1) downgrades syntax in the same emission as
//! an edit that makes a paragraph too long for the engine (ledger A29), and
//! relies on this ordering.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{ParagraphKind, SourceViewFixture, offset, paragraph};
use crate::session::{Presented, settle, wait_until};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Characters in the inserted paragraph.
pub const INSERTED_CHARS: usize = 1_000;
/// How long the control waits for the engine to tag the inserted text.
pub const HIGHLIGHT_BUDGET: Duration = Duration::from_secs(3);

/// A highlighted source view whose `insert-text` after-handler optionally
/// clears the language.
#[derive(Clone, Debug)]
pub struct DowngradingView {
    /// The view, its buffer and scroller.
    pub source: SourceViewFixture,
    /// Whether the after-handler clears the language.
    pub clear_on_insert: Rc<Cell<bool>>,
}

impl DowngradingView {
    /// The fixture holding a short highlighted line, or `None` without an
    /// html language.
    #[must_use]
    pub fn new(clear_on_insert: bool) -> Option<Self> {
        let source = SourceViewFixture::html(gtk4::WrapMode::None)?;
        source.buffer.set_text("<b class=\"a\">start</b>\n");
        let clear = Rc::new(Cell::new(clear_on_insert));
        let flag = clear.clone();
        source
            .buffer
            .connect_local("insert-text", true, move |values| {
                if flag.get()
                    && let Ok(buffer) = values[0].get::<sourceview5::Buffer>()
                {
                    buffer.set_language(None::<&sourceview5::Language>);
                    buffer.set_highlight_syntax(false);
                }
                None
            });
        Some(Self {
            source,
            clear_on_insert: clear,
        })
    }

    /// Insert the probe paragraph at the end; returns its start offset.
    #[must_use = "the start offset is where the inserted paragraph's tags are read"]
    pub fn insert_paragraph(&self) -> i32 {
        let buffer = &self.source.buffer;
        let start = buffer.char_count();
        let mut end = buffer.end_iter();
        buffer.insert(
            &mut end,
            &paragraph(ParagraphKind::MinifiedMarkup, INSERTED_CHARS, 40),
        );
        start
    }

    /// Tag names on the inserted text from `start` to the buffer end.
    #[must_use]
    pub fn tags_from(&self, start: i32) -> Vec<String> {
        tags_in_range(&self.source.buffer, start)
    }
}

/// Every distinct tag name present anywhere from `start` to the buffer end.
#[must_use]
pub fn tags_in_range(buffer: &sourceview5::Buffer, start: i32) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut iter = buffer.iter_at_offset(start);
    loop {
        for tag in iter.tags() {
            let name = tag
                .name()
                .map_or_else(|| "(anonymous)".to_owned(), |name| name.to_string());
            if !names.contains(&name) {
                names.push(name);
            }
        }
        if !iter.forward_to_tag_toggle(None::<&gtk4::TextTag>) || iter.is_end() {
            break;
        }
    }
    names
}

/// Probe A44.
#[must_use]
pub fn probe_a44() -> Observation {
    Recorder::run(AxiomId::new(44), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let kept = recorder.require(
            DowngradingView::new(false),
            "control: GtkSourceView has an html language",
        )?;
        let _kept_shown = Presented::checked(
            recorder,
            &kept.source.scrolled,
            "control: the language-keeping view was presented",
        )?;
        let kept_start = kept.insert_paragraph();
        let tagged = wait_until(HIGHLIGHT_BUDGET, || !kept.tags_from(kept_start).is_empty());
        let kept_tags = kept.tags_from(kept_start);
        recorder.measure("kept_language_tags", super::join_entries(&kept_tags));
        recorder.control(
            tagged,
            "control: with the language kept, the drained main loop tags the inserted text",
        )?;
        drop(_kept_shown);

        let cleared = recorder.require(
            DowngradingView::new(true),
            "control: GtkSourceView has an html language",
        )?;
        let _shown = Presented::checked(
            recorder,
            &cleared.source.scrolled,
            "control: the downgrading view was presented",
        )?;
        let start = cleared.insert_paragraph();
        let language_after = cleared.source.buffer.language().is_some();
        recorder.measure("language_after_insert", language_after);
        recorder.control(
            !language_after && cleared.source.buffer.char_count() > start + offset(10),
            "control: the after-handler cleared the language and the text was inserted",
        )?;
        let mut later = Vec::new();
        for _ in 0..10 {
            settle(LAYOUT_SETTLE);
            later.extend(cleared.tags_from(start));
        }
        recorder.measure("cleared_language_tags", super::join_entries(&later));
        recorder.axiom(
            later.is_empty(),
            "a language cleared in the insert-text after-handler leaves the inserted text untagged",
        )
    })
}
