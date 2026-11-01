// SPDX-License-Identifier: MIT OR Apache-2.0

//! A52: a `GtkTextView` that is **unparented but still alive** stays attached
//! to its buffer and **keeps validating** the paragraphs an edit invalidates,
//! from its own idle: unrealizing it (A45) stops its frame-clock work, not its
//! layout. Dropping its last reference finalizes it, which detaches it from the
//! buffer, and logs nothing.
//!
//! The control is a second view of the same buffer kept in the tree: both see
//! the edited paragraph grow.
//!
//! LushText retires its minimap (a `GtkSourceMap`, itself a text view) by
//! unparenting and dropping it; a retired map that something kept alive went
//! on laying out every edited paragraph, 7.5 ms per keystroke in a
//! 50,000-character paragraph (`explore-editing-long-paragraphs`, design D1).

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{CapturedLog, LogCapture, ParagraphKind, TextViewFixture, paragraph};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Characters in the paragraph both views show.
pub const PARAGRAPH_CHARS: usize = 3_000;
/// Characters appended to it after the second view is unparented.
pub const APPENDED_CHARS: usize = 3_000;

/// Two wrapping views of one buffer side by side; one is later unparented.
#[derive(Clone, Debug)]
pub struct SharedBufferViews {
    /// The box holding both.
    pub container: gtk4::Box,
    /// The view that stays in the tree.
    pub kept: TextViewFixture,
    /// The view that is unparented and kept alive.
    pub retired: TextViewFixture,
}

impl SharedBufferViews {
    /// Two views of one buffer holding one paragraph, then a short line.
    #[must_use]
    pub fn new() -> Self {
        let kept = TextViewFixture::new(gtk4::WrapMode::Word);
        let retired = TextViewFixture::new(gtk4::WrapMode::Word);
        retired.view.set_buffer(Some(&kept.buffer));
        let retired = TextViewFixture {
            buffer: kept.buffer.clone(),
            ..retired
        };
        kept.buffer.set_text(&format!(
            "{}\nshort line\n",
            paragraph(ParagraphKind::Ascii, PARAGRAPH_CHARS, 52)
        ));
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        container.set_homogeneous(true);
        container.append(&kept.scrolled);
        container.append(&retired.scrolled);
        Self {
            container,
            kept,
            retired,
        }
    }

    /// Unparent the retired view's scroller (the view stays alive).
    pub fn unparent_retired(&self) {
        if self.retired.scrolled.parent().is_some() {
            self.container.remove(&self.retired.scrolled);
        }
    }

    /// Append characters to the end of the first paragraph.
    pub fn grow_paragraph(&self, chars: usize) {
        let mut end = self.kept.buffer.start_iter();
        end.forward_to_line_end();
        self.kept
            .buffer
            .insert(&mut end, &paragraph(ParagraphKind::Ascii, chars, 53));
    }

    /// The height `view`'s layout records for the first paragraph (it reads
    /// the layout's line data and validates nothing).
    #[must_use]
    pub fn paragraph_height(view: &gtk4::TextView) -> i32 {
        view.line_yrange(&view.buffer().start_iter()).1
    }
}

impl Default for SharedBufferViews {
    fn default() -> Self {
        Self::new()
    }
}

fn warnings(messages: &[CapturedLog]) -> Vec<String> {
    messages.iter().map(ToString::to_string).collect()
}

/// Probe A52.
#[must_use]
pub fn probe_a52() -> Observation {
    Recorder::run(AxiomId::new(52), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let views = SharedBufferViews::new();
        let shown = Presented::checked(
            recorder,
            &views.container,
            "control: the two views of one buffer were presented",
        )?;
        settle(LAYOUT_SETTLE);
        let capture = LogCapture::start();
        let retired_before = SharedBufferViews::paragraph_height(&views.retired.view);
        views.unparent_retired();
        settle(LAYOUT_SETTLE);
        // The kept view takes the whole width once the other leaves the box,
        // so its paragraph wraps into fewer lines: measure it after that.
        let kept_before = SharedBufferViews::paragraph_height(&views.kept.view);
        recorder.control(
            kept_before > 0 && retired_before > 0,
            "control: both views laid the paragraph out",
        )?;
        recorder.control(
            !views.retired.view.is_realized(),
            "control: the unparented view is unrealized",
        )?;
        views.grow_paragraph(APPENDED_CHARS);
        settle(LAYOUT_SETTLE);
        let kept_after = SharedBufferViews::paragraph_height(&views.kept.view);
        let retired_after = SharedBufferViews::paragraph_height(&views.retired.view);
        recorder.measure("kept_height_before", kept_before);
        recorder.measure("kept_height_after", kept_after);
        recorder.measure("retired_height_before", retired_before);
        recorder.measure("retired_height_after", retired_after);
        recorder.control(
            kept_after > kept_before,
            "control: the view kept in the tree re-laid out the grown paragraph",
        )?;
        recorder.axiom(
            retired_after > retired_before,
            "an unparented text view that is still alive re-lays out an edited paragraph",
        )?;
        let weak = views.retired.view.downgrade();
        let SharedBufferViews { retired, .. } = views;
        drop(retired);
        settle(LAYOUT_SETTLE);
        let messages = warnings(&capture.messages());
        drop(capture);
        recorder.measure("retired_disposed", weak.upgrade().is_none());
        recorder.measure("dispose_messages", super::join_entries(&messages));
        drop(shown);
        recorder.axiom(
            weak.upgrade().is_none() && messages.is_empty(),
            "dropping the unparented view's last reference finalizes it and logs nothing",
        )
    })
}
