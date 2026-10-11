// SPDX-License-Identifier: MIT OR Apache-2.0

//! A25: toggle gravity at a tagged run's boundaries. Text inserted exactly at
//! a tagged run's start or end is **untagged**; text inserted strictly inside
//! the run is **tagged**. Buffer-level, no view.
//!
//! The protected view (C2) and the sliced install (C3) insert text next to an
//! `invisible` run and rely on which side of the toggle the new text lands.

use gtk4::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::offset;
use crate::{AxiomId, Observation};

/// The untagged prefix.
pub const PREFIX: &str = "aaaa";
/// The tagged run.
pub const RUN: &str = "TAGGED";
/// The untagged suffix.
pub const SUFFIX: &str = "bbbb";
/// What each insertion inserts.
pub const INSERTED: &str = "xy";

/// Where an insertion goes, relative to the tagged run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Site {
    /// Exactly at the run's first character.
    Start,
    /// Strictly inside the run.
    Interior,
    /// Exactly after the run's last character.
    End,
}

impl Site {
    /// The buffer offset the insertion goes to.
    #[must_use]
    pub fn offset(self) -> i32 {
        let prefix = offset(PREFIX.len());
        let run = offset(RUN.len());
        match self {
            Self::Start => prefix,
            Self::Interior => prefix + run / 2,
            Self::End => prefix + run,
        }
    }
}

/// A buffer holding [`PREFIX`], [`RUN`] (tagged with the returned tag) and
/// [`SUFFIX`].
#[must_use]
pub fn tagged_buffer() -> (gtk4::TextBuffer, gtk4::TextTag) {
    let buffer = gtk4::TextBuffer::new(None);
    let tag = gtk4::TextTag::builder().name("run").weight(700).build();
    buffer.tag_table().add(&tag);
    buffer.set_text(&format!("{PREFIX}{RUN}{SUFFIX}"));
    let start = buffer.iter_at_offset(Site::Start.offset());
    let end = buffer.iter_at_offset(Site::End.offset());
    buffer.apply_tag(&tag, &start, &end);
    (buffer, tag)
}

/// Insert [`INSERTED`] at `site` in a fresh fixture and report how many of
/// the inserted characters carry the tag.
#[must_use]
pub fn tagged_inserted_chars(site: Site) -> usize {
    let (buffer, tag) = tagged_buffer();
    let mut at = buffer.iter_at_offset(site.offset());
    buffer.insert(&mut at, INSERTED);
    (0..offset(INSERTED.len()))
        .filter(|index| buffer.iter_at_offset(site.offset() + index).has_tag(&tag))
        .count()
}

/// Probe A25.
#[must_use]
pub fn probe_a25() -> Observation {
    Recorder::run(AxiomId::new(25), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        let (buffer, tag) = tagged_buffer();
        let tagged = (0..buffer.char_count())
            .filter(|offset| buffer.iter_at_offset(*offset).has_tag(&tag))
            .count();
        recorder.measure("tagged_chars_before", tagged);
        recorder.control(tagged == RUN.len(), "control: exactly the run is tagged")?;

        let start = tagged_inserted_chars(Site::Start);
        let interior = tagged_inserted_chars(Site::Interior);
        let end = tagged_inserted_chars(Site::End);
        recorder.measure("tagged_inserted_at_start", start);
        recorder.measure("tagged_inserted_inside", interior);
        recorder.measure("tagged_inserted_at_end", end);
        recorder.axiom(start == 0, "text inserted at the run's start is untagged")?;
        recorder.axiom(end == 0, "text inserted at the run's end is untagged")?;
        recorder.axiom(
            interior == INSERTED.len(),
            "text inserted strictly inside the run is tagged",
        )
    })
}
