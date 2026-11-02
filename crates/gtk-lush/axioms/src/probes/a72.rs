// SPDX-License-Identifier: MIT OR Apache-2.0

//! A72: the two ways of restyling a dense document without making
//! `GtkTextView` revalidate every line in document-sized batches
//! (`bound-post-open-settle`, D9).
//!
//! **A, a tag-carried document style.** Two views of the same dense text
//! (wrap on and wrap off) at the reference size: the *property* view takes
//! its family, zoomed size and line height from view CSS (as LushText's
//! `.monospace` zoom does) and its tab stops from GtkSourceView's
//! `tab-width`; the *tag* view keeps a fixed base font in its own CSS, and
//! every character carries one lowest-priority tag with the same `family`, a
//! relative `scale` equal to the zoom, and the property view's `tabs`. The
//! probe compares the line geometry of both (line heights, x positions on a
//! dense line and after tabs, the content extent), re-tags a range of
//! paragraphs to a new scale and counts the validation dispatches that follow
//! against a view-wide CSS change, and records what the consumers that read
//! the view's own font see: the line-number gutter (a `gutterrenderer` takes
//! its font from CSS, so CSS can zoom it without the text, at the cost of a
//! gutter width change), a `GtkSourceMap` on the tagged buffer (a relative
//! `scale` scales the map's own font), the print compositor (which honours
//! only colours, style, weight, underline and strikethrough of a tag,
//! `gtksourceprintcompositor.c` `get_iter_attrs` l. 2177–2290, so the tag
//! prints nothing differently), and the character width a right margin or a
//! readable-column margin must use (the tag font's, not the view font's).
//!
//! **B, hide and reveal.** On the property view, wrap on, scrolled into the
//! middle with the cursor below the top: the paragraphs outside the visible
//! window are tagged `invisible`, the wrap mode is changed, and the hidden
//! paragraphs are revealed outward in ranges. The probe records the top line
//! and the insert mark through every step, what a default (`visible-only`)
//! search counts meanwhile (A33), the map's extent meanwhile, and each step's
//! dispatches. The AT-SPI text of hidden ranges is A35's, not observable
//! with the headless lane's accessibility bridge off.
//!
//! What the steps check (GTK 4.22.5, GtkSourceView 5.20.0): with the tag
//! carrying the property view's tab array, every non-empty line has the same
//! height, the same x positions and the same width in both wrap modes; only
//! the empty last line, which has no character to tag, keeps the view font's
//! height. GtkSourceView's own `tab-width` stops are computed from the view's
//! font (`calculate_real_tab_width`, `gtksourceview.c` l. 3005–3031), so the tag
//! must carry tabs measured in the tag font. Re-tagging ten paragraphs changes
//! only their heights, in one or two validation dispatches, where the same
//! zoom as view CSS revalidates the document. The line-number renderer takes
//! its font from CSS (`gtksourcegutterrendererlines.c` `update_cached_items`),
//! so outside the zoom CSS it stays at the base size, and the map's lines grow
//! with the relative scale. Hide-and-reveal keeps the top line, its offset and
//! the insert mark through every step, while a default search and the map
//! see only the revealed paragraphs.
//!
//! Recorded, not checked: the gutter keeps its width when CSS zooms its
//! renderer font (39 px against the property view's 55 px), so zooming the
//! gutter needs more than CSS; and the dispatch costs of every step.
//!
//! Every duration is a recorded magnitude, never a verdict input.

use std::time::Duration;

use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::{Recorder, Stop};
use crate::probes::post_open::{
    DENSE_LINE_CHARS, DenseView, FrameCounter, REFERENCE_WINDOW, WatchedDispatch,
    changing_dispatches, longest_frameless, printed_pages, watch_until_quiet,
};
use crate::probes::text::{ParagraphKind, invisible_tag, paragraph};
use crate::session::{Presented, settle};
use crate::{AxiomId, Observation};

use super::LAYOUT_SETTLE;

/// Lines in the probe document.
pub const LINES: usize = 160;
/// [`LINES`] as a buffer line number.
pub const LINE_COUNT: i32 = 160;
/// Every `TAB_LINE_PERIOD`th line (from line 3) starts with tabs.
pub const TAB_LINE_PERIOD: usize = 8;
/// The base font size both views' CSS starts from, pt.
pub const BASE_PT: f64 = 11.0;
/// The zoom the property view's CSS and the tag's `scale` carry.
pub const ZOOM: f64 = 1.5;
/// The zoom the restyle comparisons change to.
pub const NEW_ZOOM: f64 = 1.6;
/// The line height the editor's CSS sets (LushText's
/// `EDITOR_GLYPH_LINE_HEIGHT`).
pub const LINE_HEIGHT: f64 = 1.18;
/// The paragraphs the re-tag comparison moves to the new scale.
pub const RETAGGED: (i32, i32) = (100, 110);
/// The line the hide-and-reveal run scrolls to the top.
pub const SCROLLED_TOP: i32 = 80;
/// The line the hide-and-reveal run puts the cursor on.
pub const CURSOR_LINE: i32 = 85;
/// Paragraphs revealed per step.
pub const REVEAL_STEP: i32 = 20;
/// The word the hide-and-reveal search looks for, on lines 5, 85 and 150.
pub const NEEDLE: &str = "zqneedlezq";
/// Lines holding [`NEEDLE`].
pub const NEEDLE_LINES: [usize; 3] = [5, 85, 150];
/// The column whose x the margin check reads.
pub const MARGIN_COLUMN: i32 = 80;
/// Seed of the probe document.
pub const SEED: u64 = 57;

const QUIET: Duration = Duration::from_millis(250);
const BUDGET: Duration = Duration::from_secs(20);
const CSS_PROPERTY_CLASS: &str = "gtk-lush-axioms-a72-property";
const CSS_FIXED_CLASS: &str = "gtk-lush-axioms-a72-fixed";

/// The probe document: `LINES` dense lines, tab lines every
/// `TAB_LINE_PERIOD`, `NEEDLE` on `NEEDLE_LINES`, ending in `\n`.
#[must_use]
pub fn document() -> String {
    let mut text = String::new();
    for line in 0..LINES {
        let seed = SEED + line as u64;
        let mut body = if line % TAB_LINE_PERIOD == 3 {
            format!(
                "\tx\ty\t{}",
                paragraph(ParagraphKind::Ascii, DENSE_LINE_CHARS - 6, seed)
            )
        } else {
            paragraph(ParagraphKind::Ascii, DENSE_LINE_CHARS, seed)
        };
        if NEEDLE_LINES.contains(&line) {
            body.replace_range(..=NEEDLE.len(), &format!("{NEEDLE} "));
        }
        text.push_str(&body);
        text.push('\n');
    }
    text
}

/// The CSS of both views: the property view at `zoom`, the tag view at the
/// base size, and optionally the tag view's gutter at `gutter_zoom`.
#[must_use]
pub fn style_css(zoom: f64, gutter_zoom: Option<f64>) -> String {
    let zoomed = BASE_PT * zoom;
    let mut css = format!(
        "textview.{CSS_PROPERTY_CLASS}, textview.{CSS_PROPERTY_CLASS} text {{ font-family: \
         \"Monospace\"; font-size: {zoomed:.2}pt; line-height: {LINE_HEIGHT}; }}\n\
         textview.{CSS_FIXED_CLASS}, textview.{CSS_FIXED_CLASS} text {{ font-family: \
         \"Monospace\"; font-size: {BASE_PT:.2}pt; line-height: {LINE_HEIGHT}; }}\n"
    );
    if let Some(gutter) = gutter_zoom {
        css.push_str(&format!(
            "textview.{CSS_FIXED_CLASS} gutter, textview.{CSS_FIXED_CLASS} gutterrenderer \
             {{ font-size: {:.2}pt; }}\n",
            BASE_PT * gutter
        ));
    }
    css
}

/// A CSS provider on the default display, removed when dropped.
pub struct DisplayCss {
    provider: gtk4::CssProvider,
}

impl DisplayCss {
    /// Install `css`. `None` without a default display.
    #[must_use]
    pub fn install(css: &str) -> Option<Self> {
        let display = gtk4::gdk::Display::default()?;
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(css);
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        Some(Self { provider })
    }

    /// Replace the CSS.
    pub fn load(&self, css: &str) {
        self.provider.load_from_string(css);
    }
}

impl Drop for DisplayCss {
    fn drop(&mut self) {
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_remove_provider_for_display(&display, &self.provider);
        }
    }
}

/// One presented view with a minimap beside it.
pub struct MappedDense {
    /// The view.
    pub dense: DenseView,
    /// The map of the view.
    pub map: sourceview5::Map,
    /// Its frames.
    pub frames: FrameCounter,
    _shown: Presented,
}

impl MappedDense {
    fn new(
        recorder: &mut Recorder,
        wrap: gtk4::WrapMode,
        css_class: &str,
        text: &str,
    ) -> Result<Self, Stop> {
        let dense = DenseView::new(wrap);
        dense.view.set_monospace(false);
        dense.view.add_css_class(css_class);
        dense.view.set_tab_width(4);
        dense.buffer.set_text(text);
        dense.buffer.place_cursor(&dense.buffer.start_iter());
        let map = sourceview5::Map::new();
        map.set_view(&dense.view);
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        container.append(&dense.scrolled);
        container.append(&map);
        let shown = Presented::checked_sized(
            recorder,
            &container,
            REFERENCE_WINDOW,
            "control: a dense view with a map was presented",
        )?;
        let frames = recorder.require(
            FrameCounter::watch(&dense.view),
            "control: the view has a frame clock",
        )?;
        let fixture = Self {
            dense,
            map,
            frames,
            _shown: shown,
        };
        fixture.quiet(|| 0.0);
        Ok(fixture)
    }

    /// Drive the main loop until quiet, watching `read`.
    fn quiet(&self, read: impl FnMut() -> f64) -> Vec<WatchedDispatch> {
        let watched = watch_until_quiet(Some(&self.frames), QUIET, BUDGET, read);
        settle(Duration::from_millis(20));
        watched
    }

    fn vupper(&self) -> f64 {
        self.dense.uppers().1
    }

    fn line_yrange(&self, line: i32) -> (i32, i32) {
        self.dense.view.line_yrange(&self.line_iter(line))
    }

    fn line_iter(&self, line: i32) -> gtk4::TextIter {
        self.dense
            .buffer
            .iter_at_line(line)
            .unwrap_or_else(|| self.dense.buffer.end_iter())
    }

    fn x_at(&self, line: i32, column: i32) -> i32 {
        let mut iter = self.line_iter(line);
        iter.set_line_offset(column);
        self.dense.view.iter_location(&iter).x()
    }

    fn gutter_width(&self) -> i32 {
        sourceview5::prelude::ViewExt::gutter(&self.dense.view, gtk4::TextWindowType::Left).width()
    }

    /// The map's content height: the bottom of its last line.
    fn map_upper(&self) -> f64 {
        let (y, height) = self.map.line_yrange(&self.dense.buffer.end_iter());
        f64::from(y + height)
    }

    /// The point size of the gutter's first renderer's font, or 0.
    fn gutter_font_pt(&self) -> f64 {
        sourceview5::prelude::ViewExt::gutter(&self.dense.view, gtk4::TextWindowType::Left)
            .first_child()
            .and_then(|renderer| renderer.pango_context().font_description())
            .map_or(0.0, |font| f64::from(font.size()) / f64::from(pango::SCALE))
    }

    fn top_line(&self) -> (i32, i32) {
        let visible = self.dense.view.visible_rect();
        let (iter, top) = self.dense.view.line_at_y(visible.y());
        (iter.line(), visible.y() - top)
    }
}

/// The geometry the identity check compares.
fn geometry(fixture: &MappedDense) -> Vec<f64> {
    let mut values = Vec::new();
    for line in [0, 3, 11, 50, 159] {
        let (y, height) = fixture.line_yrange(line);
        values.extend([f64::from(y), f64::from(height)]);
    }
    for column in [1, 50, 200, 700] {
        values.push(f64::from(fixture.x_at(0, column)));
    }
    for column in [1, 2, 3, 4, 10] {
        values.push(f64::from(fixture.x_at(3, column)));
    }
    let (h, v) = fixture.dense.uppers();
    values.extend([h, v]);
    values
}

/// The document tag: `family`, relative `scale` and the given `tabs`, added
/// first so it has the lowest priority.
#[must_use]
pub fn document_tag(
    buffer: &sourceview5::Buffer,
    scale: f64,
    tabs: Option<&pango::TabArray>,
) -> gtk4::TextTag {
    let tag = gtk4::TextTag::builder()
        .family("Monospace")
        .scale(scale)
        .build();
    if let Some(tabs) = tabs {
        tag.set_tabs(Some(tabs));
    }
    buffer.tag_table().add(&tag);
    tag.set_priority(0);
    let (start, end) = buffer.bounds();
    buffer.apply_tag(&tag, &start, &end);
    tag
}

/// What one wrap mode's mechanism-A comparison observed.
struct TagRun {
    property_geometry: Vec<f64>,
    tag_geometry: Vec<f64>,
    empty_last_heights: (i32, i32),
    tabs_property: String,
    tabs_tag_view: String,
    tab_stops: (i32, i32),
    gutter: (i32, i32, i32),
    gutter_fonts: ((f64, f64), (f64, f64), u128),
    gutter_restyle: (usize, u128, u128),
    retag: (usize, u128, u128),
    retag_outside_unchanged: bool,
    retag_inside_changed: bool,
    css_restyle: (usize, u128, u128),
    map_upper: (f64, f64),
    margin: (i32, f64, f64),
}

/// The first stop of `tabs`, or 0 without tabs.
fn first_stop(tabs: Option<&pango::TabArray>) -> i32 {
    tabs.map_or(0, |tabs| tabs.tab(0).1)
}

fn describe_tabs(tabs: Option<pango::TabArray>) -> String {
    tabs.map_or_else(
        || "none".to_owned(),
        |tabs| {
            let (_, position) = tabs.tab(0);
            format!("{} stops, first at {position}", tabs.size())
        },
    )
}

fn tag_run(recorder: &mut Recorder, wrap: gtk4::WrapMode) -> Result<TagRun, Stop> {
    let css = recorder.require(
        DisplayCss::install(&style_css(ZOOM, None)),
        "control: a default display exists",
    )?;
    let text = document();
    let property = MappedDense::new(recorder, wrap, CSS_PROPERTY_CLASS, &text)?;
    let tagged = MappedDense::new(recorder, wrap, CSS_FIXED_CLASS, &text)?;
    let tabs = property.dense.view.tabs();
    let tag_view_tabs = tagged.dense.view.tabs();
    let tab_stops = (
        first_stop(tabs.as_ref()),
        first_stop(tag_view_tabs.as_ref()),
    );
    let tabs_tag_view = describe_tabs(tag_view_tabs);
    let tag = document_tag(&tagged.dense.buffer, ZOOM, tabs.as_ref());
    tagged.quiet(|| tagged.vupper());
    let property_geometry = geometry(&property);
    let tag_geometry = geometry(&tagged);
    let empty_last_heights = (property.line_yrange(160).1, tagged.line_yrange(160).1);

    // Character widths: the tag's metrics, not the view's.
    let context = tagged.dense.view.pango_context();
    let view_char =
        f64::from(context.metrics(None, None).approximate_char_width()) / f64::from(pango::SCALE);
    let zoomed = pango::FontDescription::from_string(&format!("Monospace {}", BASE_PT * ZOOM));
    let tag_char = f64::from(
        context
            .metrics(Some(&zoomed), None)
            .approximate_char_width(),
    ) / f64::from(pango::SCALE);
    let margin_x = tagged.x_at(0, MARGIN_COLUMN) - tagged.x_at(0, 0);

    // Re-tag a range to the new zoom; then the same zoom as a view-wide CSS change.
    let before: Vec<(i32, i32)> = (0..LINE_COUNT).map(|l| tagged.line_yrange(l)).collect();
    let new_tag = gtk4::TextTag::builder()
        .family("Monospace")
        .scale(NEW_ZOOM)
        .build();
    if let Some(tabs) = tabs.as_ref() {
        new_tag.set_tabs(Some(tabs));
    }
    tagged.dense.buffer.tag_table().add(&new_tag);
    new_tag.set_priority(1);
    let (start, end) = (tagged.line_iter(RETAGGED.0), tagged.line_iter(RETAGGED.1));
    tagged.dense.buffer.remove_tag(&tag, &start, &end);
    tagged.dense.buffer.apply_tag(&new_tag, &start, &end);
    let retag = changing_dispatches(&tagged.quiet(|| tagged.vupper()));
    let mut retagged =
        usize::try_from(RETAGGED.0).unwrap_or(0)..usize::try_from(RETAGGED.1).unwrap_or(0);
    let after: Vec<(i32, i32)> = (0..LINE_COUNT).map(|l| tagged.line_yrange(l)).collect();
    let retag_outside_unchanged = (0..LINES)
        .filter(|l| !retagged.contains(l))
        .all(|l| after[l].1 == before[l].1);
    let retag_inside_changed =
        retagged.all(|l| after[l].1 > before[l].1 || wrap == gtk4::WrapMode::None);
    css.load(&style_css(NEW_ZOOM, None));
    let css_restyle = changing_dispatches(&property.quiet(|| property.vupper()));

    // Map: the relative scale reaches the map's own font.
    let map_zoomed = tagged.map_upper();
    tag.set_scale(1.0);
    new_tag.set_scale(1.0);
    tagged.quiet(|| tagged.map_upper());
    let map_plain = tagged.map_upper();

    // Gutter: zoomed through CSS on the tag view's renderers.
    for fixture in [&property, &tagged] {
        fixture.dense.view.set_show_line_numbers(true);
        fixture.quiet(|| fixture.vupper());
    }
    let gutter_property = property.gutter_width();
    let gutter_tag = tagged.gutter_width();
    let gutter_fonts_before = (property.gutter_font_pt(), tagged.gutter_font_pt());
    css.load(&style_css(NEW_ZOOM, Some(NEW_ZOOM)));
    let watched = tagged.quiet(|| f64::from(tagged.gutter_width()));
    let gutter_restyle = changing_dispatches(&watched);
    let gutter_longest = longest_frameless(&watched);
    let gutter_tag_zoomed = tagged.gutter_width();
    let gutter_fonts_after = (property.gutter_font_pt(), tagged.gutter_font_pt());
    drop((property, tagged));
    drop(css);
    Ok(TagRun {
        property_geometry,
        tag_geometry,
        empty_last_heights,
        tabs_property: describe_tabs(tabs),
        tab_stops,
        tabs_tag_view,
        gutter: (gutter_property, gutter_tag, gutter_tag_zoomed),
        gutter_fonts: (gutter_fonts_before, gutter_fonts_after, gutter_longest),
        gutter_restyle,
        retag,
        retag_outside_unchanged,
        retag_inside_changed,
        css_restyle,
        map_upper: (map_zoomed, map_plain),
        margin: (
            margin_x,
            view_char * f64::from(MARGIN_COLUMN),
            tag_char * f64::from(MARGIN_COLUMN),
        ),
    })
}

fn describe_dispatches((count, longest, total): (usize, u128, u128)) -> String {
    format!("{count} changing dispatches, longest {longest} us, total {total} us")
}

fn record_tag_run(recorder: &mut Recorder, prefix: &'static [&'static str; 10], run: &TagRun) {
    recorder.measure(prefix[0], super::join_entries(&run.property_geometry));
    recorder.measure(prefix[1], super::join_entries(&run.tag_geometry));
    recorder.measure(
        prefix[9],
        format!(
            "property {} px, tag view {} px",
            run.empty_last_heights.0, run.empty_last_heights.1
        ),
    );
    recorder.measure(
        prefix[2],
        format!(
            "property view {}; tag view's own {}",
            run.tabs_property, run.tabs_tag_view
        ),
    );
    recorder.measure(
        prefix[3],
        format!(
            "property {} px, tag view {} px, tag view with gutter CSS {} px; renderer fonts \
             {:?} pt, after the gutter CSS {:?} pt; longest dispatch after it {} us",
            run.gutter.0,
            run.gutter.1,
            run.gutter.2,
            run.gutter_fonts.0,
            run.gutter_fonts.1,
            run.gutter_fonts.2
        ),
    );
    recorder.measure(prefix[4], describe_dispatches(run.gutter_restyle));
    recorder.measure(
        prefix[5],
        format!(
            "{}; outside heights unchanged {}; inside grew {}",
            describe_dispatches(run.retag),
            run.retag_outside_unchanged,
            run.retag_inside_changed
        ),
    );
    recorder.measure(prefix[6], describe_dispatches(run.css_restyle));
    recorder.measure(
        prefix[7],
        format!(
            "scale {ZOOM}: {}, scale 1: {}",
            run.map_upper.0, run.map_upper.1
        ),
    );
    recorder.measure(
        prefix[8],
        format!(
            "column {MARGIN_COLUMN} at {} px; view font predicts {:.1}, tag font {:.1}",
            run.margin.0, run.margin.1, run.margin.2
        ),
    );
}

/// What the hide-and-reveal run observed.
struct RevealRun {
    steps: Vec<String>,
    top_lines: Vec<(i32, i32)>,
    cursors: Vec<i32>,
    searches: Vec<usize>,
    map_uppers: Vec<f64>,
    longest: Vec<u128>,
}

/// The matches a forward search walk finds (navigation honours
/// `visible-only`, A33).
fn search_count(context: &sourceview5::SearchContext) -> usize {
    crate::probes::a33::forward_matches(context)
}

fn reveal_run(recorder: &mut Recorder) -> Result<RevealRun, Stop> {
    let css = recorder.require(
        DisplayCss::install(&style_css(ZOOM, None)),
        "control: a default display exists",
    )?;
    let fixture = MappedDense::new(
        recorder,
        gtk4::WrapMode::Word,
        CSS_PROPERTY_CLASS,
        &document(),
    )?;
    let buffer = fixture.dense.buffer.clone();
    let view = fixture.dense.view.clone();
    let mut top = fixture.line_iter(SCROLLED_TOP);
    view.scroll_to_iter(&mut top, 0.0, true, 0.0, 0.0);
    buffer.place_cursor(&fixture.line_iter(CURSOR_LINE));
    fixture.quiet(|| fixture.dense.scrolled.vadjustment().value());
    let settings = sourceview5::SearchSettings::new();
    settings.set_search_text(Some(NEEDLE));
    let search = sourceview5::SearchContext::new(&buffer, Some(&settings));
    let mut run = RevealRun {
        steps: Vec::new(),
        top_lines: Vec::new(),
        cursors: Vec::new(),
        searches: Vec::new(),
        map_uppers: Vec::new(),
        longest: Vec::new(),
    };
    let observe = |run: &mut RevealRun, step: String, watched: &[WatchedDispatch]| {
        run.steps.push(step);
        run.top_lines.push(fixture.top_line());
        run.cursors.push(buffer.cursor_position());
        run.searches.push(search_count(&search));
        run.map_uppers.push(fixture.map_upper());
        run.longest.push(longest_frameless(watched));
    };
    observe(&mut run, "scrolled".to_owned(), &[]);
    let adjustment = fixture.dense.scrolled.vadjustment();
    let visible = view.visible_rect();
    let (first, _) = view.line_at_y(visible.y());
    let (last, _) = view.line_at_y(visible.y() + visible.height());
    let (first, last) = (first.line(), last.line() + 1);
    let hidden = invisible_tag(buffer.upcast_ref());
    buffer.apply_tag(&hidden, &buffer.start_iter(), &fixture.line_iter(first));
    buffer.apply_tag(&hidden, &fixture.line_iter(last), &buffer.end_iter());
    let watched = fixture.quiet(|| adjustment.upper());
    observe(
        &mut run,
        format!("hid all but lines {first}..{last}"),
        &watched,
    );
    view.set_wrap_mode(gtk4::WrapMode::Char);
    view.queue_allocate();
    let watched = fixture.quiet(|| adjustment.upper());
    observe(&mut run, "changed wrap mode".to_owned(), &watched);
    let (mut above, mut below) = (first, last);
    while above > 0 || below < LINE_COUNT + 1 {
        if below < LINE_COUNT + 1 {
            let next = (below + REVEAL_STEP).min(LINE_COUNT + 1);
            buffer.remove_tag(&hidden, &fixture.line_iter(below), &fixture.line_iter(next));
            if next > LINE_COUNT {
                buffer.remove_tag(&hidden, &fixture.line_iter(below), &buffer.end_iter());
            }
            below = next;
            let watched = fixture.quiet(|| adjustment.upper());
            observe(&mut run, format!("revealed below to {below}"), &watched);
        }
        if above > 0 {
            let next = (above - REVEAL_STEP).max(0);
            buffer.remove_tag(&hidden, &fixture.line_iter(next), &fixture.line_iter(above));
            above = next;
            let watched = fixture.quiet(|| adjustment.upper());
            observe(&mut run, format!("revealed above to {above}"), &watched);
        }
    }
    drop(search);
    drop(fixture);
    drop(css);
    Ok(run)
}

/// Pages the print compositor prints for the first lines of the document,
/// without and with a document tag of scale 3.
fn printed(recorder: &mut Recorder) -> Result<(i32, i32), Stop> {
    let text: String = document()
        .lines()
        .take(50)
        .map(|l| format!("{l}\n"))
        .collect();
    let plain = sourceview5::Buffer::new(None);
    plain.set_text(&text);
    let tagged = sourceview5::Buffer::new(None);
    tagged.set_text(&text);
    let _tag = document_tag(&tagged, 3.0, None);
    let plain_pages = recorder.require(printed_pages(&plain), "control: the PDF export ran")?;
    let tagged_pages = recorder.require(printed_pages(&tagged), "control: the PDF export ran")?;
    Ok((plain_pages, tagged_pages))
}

/// Probe A72.
#[must_use]
pub fn probe_a72() -> Observation {
    Recorder::run(AxiomId::new(72), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);
        settle(LAYOUT_SETTLE);
        let wrapped = tag_run(recorder, gtk4::WrapMode::Word)?;
        record_tag_run(
            recorder,
            &[
                "wrap_on_property_geometry",
                "wrap_on_tag_geometry",
                "wrap_on_tabs",
                "wrap_on_gutter_widths",
                "wrap_on_gutter_restyle",
                "wrap_on_retag",
                "wrap_on_view_css_restyle",
                "wrap_on_map_upper",
                "wrap_on_margin_column",
                "wrap_on_empty_last_line_height",
            ],
            &wrapped,
        );
        let unwrapped = tag_run(recorder, gtk4::WrapMode::None)?;
        record_tag_run(
            recorder,
            &[
                "wrap_off_property_geometry",
                "wrap_off_tag_geometry",
                "wrap_off_tabs",
                "wrap_off_gutter_widths",
                "wrap_off_gutter_restyle",
                "wrap_off_retag",
                "wrap_off_view_css_restyle",
                "wrap_off_map_upper",
                "wrap_off_margin_column",
                "wrap_off_empty_last_line_height",
            ],
            &unwrapped,
        );
        let (plain_pages, tagged_pages) = printed(recorder)?;
        recorder.measure(
            "printed_pages",
            format!("plain {plain_pages}, tag scale 3: {tagged_pages}"),
        );

        let reveal = reveal_run(recorder)?;
        recorder.measure("reveal_steps", super::join_entries(&reveal.steps));
        recorder.measure(
            "reveal_top_lines",
            super::join_entries(reveal.top_lines.iter().map(|(l, o)| format!("{l}+{o}"))),
        );
        recorder.measure("reveal_cursors", super::join_entries(&reveal.cursors));
        recorder.measure(
            "reveal_search_counts",
            super::join_entries(&reveal.searches),
        );
        recorder.measure("reveal_map_uppers", super::join_entries(&reveal.map_uppers));
        recorder.measure("reveal_longest_us", super::join_entries(&reveal.longest));

        recorder.control(
            plain_pages > 0 && wrapped.retag.0 > 0 && wrapped.css_restyle.0 > 0,
            "control: the PDF export paginated and both restyles were laid out",
        )?;
        for run in [&wrapped, &unwrapped] {
            let (property, tag) = (&run.property_geometry, &run.tag_geometry);
            let lines_and_columns = property.len() - 1;
            recorder.axiom(
                property[..lines_and_columns] == tag[..lines_and_columns],
                "a lowest-priority tag carrying family, relative scale and tabs lays out every \
                 non-empty line exactly as the view-wide font and tabs",
            )?;
            recorder.axiom(
                run.empty_last_heights.0 > run.empty_last_heights.1,
                "the empty last line, which has no character to tag, keeps the view's own font",
            )?;
            recorder.axiom(
                run.tab_stops.0 > run.tab_stops.1 && run.tab_stops.1 > 0,
                "GtkSourceView's tab-width stops follow the view's own font, so the tag must \
                 carry its own tabs",
            )?;
            recorder.axiom(
                run.retag_outside_unchanged
                    && run.retag_inside_changed
                    && run.retag.0 <= 2
                    && run.css_restyle.0 >= run.retag.0,
                "re-tagging a range changes only that range's lines, in at most two validation \
                 dispatches",
            )?;
            recorder.axiom(
                (run.gutter_fonts.0.1 - run.gutter_fonts.0.0).abs() > 1.0
                    && (run.gutter_fonts.1.1 - run.gutter_fonts.1.0).abs() < 0.01,
                "the line-number renderer takes its font from CSS, not from the tag",
            )?;
            recorder.axiom(
                run.map_upper.0 > run.map_upper.1,
                "a relative scale tag scales a GtkSourceMap's own font",
            )?;
            let (measured, view_font, tag_font) = run.margin;
            recorder.axiom(
                (f64::from(measured) - tag_font).abs() <= 1.0
                    && (f64::from(measured) - view_font).abs() > 1.0,
                "a column's x follows the tag font's character width, not the view font's",
            )?;
        }
        recorder.axiom(
            plain_pages == tagged_pages,
            "the print compositor prints a scale tag unchanged",
        )?;
        let first = reveal.top_lines.first().copied();
        recorder.axiom(
            reveal.top_lines.iter().all(|top| Some(*top) == first)
                && reveal.cursors.array_windows::<2>().all(|[a, b]| a == b),
            "hiding, changing the wrap mode and revealing keep the top line, its offset and the \
             insert mark",
        )?;
        let (initial_matches, final_matches) = (
            reveal.searches.first().copied().unwrap_or_default(),
            reveal.searches.last().copied().unwrap_or_default(),
        );
        recorder.axiom(
            reveal
                .searches
                .get(1)
                .is_some_and(|hidden| *hidden < initial_matches)
                && final_matches == initial_matches,
            "a default search skips the hidden paragraphs until they are revealed",
        )?;
        let initial_map = reveal.map_uppers.first().copied().unwrap_or_default();
        recorder.axiom(
            reveal
                .map_uppers
                .get(1)
                .is_some_and(|hidden| *hidden < initial_map)
                && reveal
                    .map_uppers
                    .last()
                    .is_some_and(|last| (last - initial_map).abs() < 1.0),
            "a GtkSourceMap of the buffer shows only the revealed paragraphs meanwhile",
        )?;
        Ok(())
    })
}
