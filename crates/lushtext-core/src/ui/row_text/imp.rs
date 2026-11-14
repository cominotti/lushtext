// SPDX-License-Identifier: GPL-3.0-or-later

//! `LushtextRowText` implementation: the measure, the draw and the
//! accessible label (`lighten-notes-browser-rows` D2).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{glib, graphene, pango};
use pango::prelude::FontExt;

/// Distinct primary fonts whose line metrics one thread keeps. A process
/// shows a handful (the row title, caption and heading fonts at one or two
/// scales); past this many the cache starts again.
///
/// Kind: cap; a power of two, rounded down.
const FONT_METRICS_CACHE_ENTRIES: usize = 64;

/// The probe text of a font's line metrics: any character the primary font
/// covers gives the same single-font line, so one is shaped once per font.
const LINE_PROBE: &str = "x";

/// What one primary font gives every line it covers alone.
pub(super) struct FontLineMetrics {
    /// Keeps the font alive, so the cache key (its address) is never reused
    /// by another font while the entry exists.
    font: pango::Font,
    /// Logical height of one single-font line, pixels.
    height: i32,
    /// Baseline of that line from its top, pixels.
    baseline: i32,
    /// Width of the ellipsis a non-empty text shrinks to, pixels.
    ellipsis_width: i32,
}

/// The text attributes GTK's CSS gives a label with one set of classes
/// (`line-height`, `letter-spacing`, font features), by the sorted classes.
type CssAttributes = Option<pango::AttrList>;

thread_local! {
    static FONT_METRICS: RefCell<HashMap<(usize, String), Rc<FontLineMetrics>>> =
        RefCell::new(HashMap::new());
    static CSS_ATTRIBUTES: RefCell<HashMap<String, CssAttributes>> =
        RefCell::new(HashMap::new());
    static MEASURING_LAYOUTS: RefCell<HashMap<(usize, String), (pango::Layout, pango::Font)>> =
        RefCell::new(HashMap::new());
}

/// The text's own measure, exact as a `GtkLabel` takes it, cached until the
/// text or the font changes.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ShapedText {
    /// Natural single-line width, pixels.
    pub(super) width: i32,
    /// Logical height, pixels.
    pub(super) height: i32,
    /// Baseline from the top, pixels.
    pub(super) baseline: i32,
    /// Width the text ellipsizes to at width 0, pixels (`GtkLabel`'s minimum).
    pub(super) min_width: i32,
}

#[derive(Default)]
pub struct LushtextRowText {
    pub(super) text: RefCell<String>,
    pub(super) ellipsize: Cell<Option<pango::EllipsizeMode>>,
    pub(super) xalign: Cell<f32>,
    pub(super) natural_width_from_text: Cell<bool>,
    /// The primary font's line metrics, resolved on the first measure and
    /// again whenever the widget's font changes.
    pub(super) metrics: RefCell<Option<Rc<FontLineMetrics>>>,
    /// Whether the primary font covers every character of the text: `None`
    /// until known for the current text and font.
    pub(super) covered: Cell<Option<bool>>,
    /// The exact measure of the current text, when one was needed.
    pub(super) shaped: Cell<Option<ShapedText>>,
    /// The drawing layout, created on the first snapshot (or the first exact
    /// measure) and kept; its text follows the widget's.
    pub(super) layout: RefCell<Option<pango::Layout>>,
    /// Whether `layout` holds an older text.
    pub(super) layout_stale: Cell<bool>,
    /// Serial of the widget's Pango context when the font state was last
    /// read: GTK updates the context (font, size, resolution) on a style
    /// change and queues a resize, so a new serial at measure or draw time
    /// invalidates everything font-dependent. A state change (hover,
    /// selection) keeps the serial and costs a redraw only.
    pub(super) context_serial: Cell<Option<u32>>,
    /// The CSS classes the font state was read under (a class can change the
    /// line height without changing the font).
    pub(super) classes: RefCell<Vec<glib::GString>>,
    /// The CSS text attributes for those classes, once read.
    pub(super) css_attributes: RefCell<Option<CssAttributes>>,
    /// Evidence for tests: exact measures taken (shapings of the row text).
    pub(super) exact_measures: Cell<u64>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextRowText {
    const NAME: &'static str = "LushtextRowText";
    type Type = super::LushtextRowText;
    type ParentType = gtk4::Widget;

    fn class_init(klass: &mut Self::Class) {
        // Adwaita's label rules (`.title`, `.subtitle`, `.caption`,
        // `.dim-label`, `.heading`, the `.navigation-sidebar` row colours and
        // high contrast) select on the `label` node name.
        klass.set_css_name("label");
        crate::ui::accessibility::set_class_role(klass, gtk4::AccessibleRole::Label);
    }
}

impl ObjectImpl for LushtextRowText {
    fn constructed(&self) {
        self.parent_constructed();
        self.ellipsize.set(Some(pango::EllipsizeMode::End));
    }
}

impl WidgetImpl for LushtextRowText {
    fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
        self.revalidate_font();
        let ellipsize = self.ellipsize_mode();
        if orientation == gtk4::Orientation::Horizontal {
            if self.text.borrow().is_empty() {
                return (0, 0, -1, -1);
            }
            let minimum = if ellipsize == pango::EllipsizeMode::None {
                self.shaped_text().width
            } else if self.is_covered()
                && let Some(metrics) = self.line_metrics()
            {
                metrics.ellipsis_width
            } else {
                self.shaped_text().min_width
            };
            let natural =
                if ellipsize != pango::EllipsizeMode::None && self.natural_width_from_text.get() {
                    self.shaped_text().width.max(minimum)
                } else {
                    minimum
                };
            return (minimum, natural, -1, -1);
        }
        let (height, baseline) = if self.is_covered()
            && let Some(metrics) = self.line_metrics()
        {
            (metrics.height, metrics.baseline)
        } else {
            let shaped = self.shaped_text();
            (shaped.height, shaped.baseline)
        };
        (height, height, baseline, baseline)
    }

    fn snapshot(&self, snapshot: &gtk4::Snapshot) {
        let widget = self.obj();
        let width = widget.width();
        let height = widget.height();
        if width <= 0 || height <= 0 || self.text.borrow().is_empty() {
            return;
        }
        self.revalidate_font();
        let layout = self.drawing_layout();
        let ellipsize = self.ellipsize_mode();
        layout.set_ellipsize(ellipsize);
        layout.set_width(if ellipsize == pango::EllipsizeMode::None {
            -1
        } else {
            width.saturating_mul(pango::SCALE)
        });
        let (_, logical) = layout.pixel_extents();
        let mut xalign = self.xalign.get();
        if widget.direction() == gtk4::TextDirection::Rtl {
            xalign = 1.0 - xalign;
        }
        // GtkLabel's placement: the line's logical box aligned by `xalign`
        // inside the allocation, centred vertically.
        let free_x = (width - logical.width()).max(0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            reason = "xalign is in [0, 1] and the free width is a pixel count"
        )]
        let x = (xalign * free_x as f32).floor() as i32 - logical.x();
        let y = (height - logical.height()) / 2 - logical.y();
        snapshot.save();
        #[expect(
            clippy::cast_precision_loss,
            reason = "pixel offsets of one widget fit an f32 exactly"
        )]
        snapshot.translate(&graphene::Point::new(x as f32, y as f32));
        snapshot.append_layout(&layout, &widget.color());
        snapshot.restore();
    }

    fn direction_changed(&self, previous: gtk4::TextDirection) {
        self.parent_direction_changed(previous);
        self.layout.replace(None);
        self.obj().queue_draw();
    }
}

impl LushtextRowText {
    pub(super) fn ellipsize_mode(&self) -> pango::EllipsizeMode {
        self.ellipsize.get().unwrap_or(pango::EllipsizeMode::End)
    }

    /// Forget the font state when the widget's Pango context changed.
    fn revalidate_font(&self) {
        let serial = self.obj().pango_context().serial();
        let classes = self.obj().css_classes();
        let classes_changed = *self.classes.borrow() != classes;
        if self.context_serial.replace(Some(serial)) != Some(serial) || classes_changed {
            self.classes.replace(classes);
            self.invalidate_font();
        }
    }

    /// Forget everything that depends on the font.
    pub(super) fn invalidate_font(&self) {
        self.metrics.replace(None);
        self.css_attributes.replace(None);
        self.covered.set(None);
        self.shaped.set(None);
        self.layout.replace(None);
    }

    fn load_primary_font(&self) -> Option<pango::Font> {
        let context = self.obj().pango_context();
        let description = context.font_description()?;
        context.load_font(&description)
    }

    /// The primary font's line metrics, from the per-thread cache.
    pub(super) fn line_metrics(&self) -> Option<Rc<FontLineMetrics>> {
        if let Some(metrics) = self.metrics.borrow().as_ref() {
            return Some(Rc::clone(metrics));
        }
        let font = self.load_primary_font()?;
        if !font.has_char('x') || !font.has_char('…') {
            return None;
        }
        let key = (font.as_ptr() as usize, self.classes_key());
        let cached = FONT_METRICS.with(|cache| cache.borrow().get(&key).cloned());
        let metrics = if let Some(metrics) = cached {
            metrics
        } else {
            let probe = self.new_layout(LINE_PROBE);
            let (_, height) = logical_size(&probe);
            let ellipsis = self.new_layout("…");
            let (ellipsis_width, _) = logical_size(&ellipsis);
            let metrics = Rc::new(FontLineMetrics {
                font,
                height,
                baseline: pixels_ceil(probe.baseline()),
                ellipsis_width,
            });
            FONT_METRICS.with(|cache| {
                let mut cache = cache.borrow_mut();
                if cache.len() >= FONT_METRICS_CACHE_ENTRIES {
                    cache.clear();
                }
                cache.insert(key, Rc::clone(&metrics));
            });
            metrics
        };
        self.metrics.replace(Some(Rc::clone(&metrics)));
        Some(metrics)
    }

    /// Whether the primary font draws the whole text on one line by itself,
    /// so its cached line metrics are the text's: a per-character coverage
    /// lookup, no shaping. A line separator or a character the font lacks
    /// (a fallback font changes the line's height) needs the exact measure.
    pub(super) fn is_covered(&self) -> bool {
        if let Some(covered) = self.covered.get() {
            return covered;
        }
        let covered = self
            .line_metrics()
            .is_some_and(|metrics| covers(&metrics.font, &self.text.borrow()));
        self.covered.set(Some(covered));
        covered
    }

    /// The text measured exactly, as `GtkLabel` measures it.
    pub(super) fn shaped_text(&self) -> ShapedText {
        if let Some(shaped) = self.shaped.get() {
            return shaped;
        }
        self.exact_measures
            .set(self.exact_measures.get().saturating_add(1));
        let layout = self.measuring_layout();
        layout.set_text(&self.text.borrow());
        layout.set_width(-1);
        layout.set_ellipsize(pango::EllipsizeMode::None);
        let (width, height) = logical_size(&layout);
        let baseline = pixels_ceil(layout.baseline());
        let min_width = if self.ellipsize_mode() == pango::EllipsizeMode::None {
            width
        } else {
            layout.set_width(0);
            layout.set_ellipsize(self.ellipsize_mode());
            logical_size(&layout).0
        };
        let shaped = ShapedText {
            width,
            height,
            baseline,
            min_width,
        };
        self.shaped.set(Some(shaped));
        shaped
    }

    /// A layout of `text` in the widget's font with the text attributes CSS
    /// gives a `GtkLabel` of the same classes: `gtk_widget_create_pango_layout`
    /// applies none of them, and Adwaita's `.caption` sets a `line-height`
    /// that makes a caption line 16 px where the font alone gives 15.
    fn new_layout(&self, text: &str) -> pango::Layout {
        let layout = self.obj().create_pango_layout(Some(text));
        if let Some(attributes) = self.css_text_attributes() {
            layout.set_attributes(Some(&attributes));
        }
        layout
    }

    /// A layout to measure texts with, shared by every row text of the same
    /// primary font and classes on this thread: a measure needs no layout of
    /// its own, and creating one per realized row (a Pango context each)
    /// costs the dispatch that creates the rows. The drawing layout is still
    /// per widget, created on the first snapshot.
    fn measuring_layout(&self) -> pango::Layout {
        let Some(font) = self.load_primary_font() else {
            return self.new_layout("");
        };
        let key = (font.as_ptr() as usize, self.classes_key());
        if let Some((layout, _)) = MEASURING_LAYOUTS.with(|cache| cache.borrow().get(&key).cloned())
        {
            return layout;
        }
        let layout = self.new_layout("");
        MEASURING_LAYOUTS.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= FONT_METRICS_CACHE_ENTRIES {
                cache.clear();
            }
            // The entry keeps the font alive, so the key's address is not
            // reused while it lives.
            cache.insert(key, (layout.clone(), font));
        });
        layout
    }

    /// The sorted CSS classes, as a cache key.
    fn classes_key(&self) -> String {
        let mut classes: Vec<String> = self
            .classes
            .borrow()
            .iter()
            .map(ToString::to_string)
            .collect();
        classes.sort_unstable();
        classes.join(" ")
    }

    /// The text attributes CSS gives a label with this widget's classes,
    /// read once per class set from an unparented `GtkLabel` probe (the rules
    /// that set them, such as `.caption`, select on the label alone).
    fn css_text_attributes(&self) -> Option<pango::AttrList> {
        if let Some(attributes) = self.css_attributes.borrow().as_ref() {
            return attributes.clone();
        }
        // The probe's attributes are absolute (a caption's line height is
        // in pixels of its font size), so the font is part of the key.
        let font = self
            .load_primary_font()
            .map_or(0, |font| font.as_ptr() as usize);
        let key = format!("{font:x}|{}", self.classes_key());
        let cached = CSS_ATTRIBUTES.with(|cache| cache.borrow().get(&key).cloned());
        let attributes = cached.unwrap_or_else(|| {
            let probe = gtk4::Label::new(Some(LINE_PROBE));
            for class in self.classes.borrow().iter() {
                probe.add_css_class(class);
            }
            let attributes = probe.layout().attributes();
            CSS_ATTRIBUTES.with(|cache| {
                let mut cache = cache.borrow_mut();
                if cache.len() >= FONT_METRICS_CACHE_ENTRIES {
                    cache.clear();
                }
                cache.insert(key, attributes.clone());
            });
            attributes
        });
        self.css_attributes.replace(Some(attributes.clone()));
        attributes
    }

    /// The kept layout, with the current text.
    fn drawing_layout(&self) -> pango::Layout {
        let mut slot = self.layout.borrow_mut();
        if let Some(layout) = slot.as_ref() {
            if self.layout_stale.replace(false) {
                layout.set_text(&self.text.borrow());
            }
            return layout.clone();
        }
        let layout = self.new_layout(&self.text.borrow());
        self.layout_stale.set(false);
        *slot = Some(layout.clone());
        layout
    }
}

/// A layout's logical width and height in whole pixels, rounded up, as
/// `GtkLabel` measures them.
fn logical_size(layout: &pango::Layout) -> (i32, i32) {
    let (_, logical) = layout.extents();
    (pixels_ceil(logical.width()), pixels_ceil(logical.height()))
}

/// Pango units to whole pixels, rounded up, as `GtkLabel` reports a size or
/// a baseline.
const fn pixels_ceil(units: i32) -> i32 {
    (units + pango::SCALE - 1).div_euclid(pango::SCALE)
}

/// Whether `font` covers every character of `text` on a single line.
pub(super) fn covers(font: &pango::Font, text: &str) -> bool {
    text.chars().all(|character| {
        !matches!(character, '\n' | '\r' | '\u{2028}' | '\u{2029}') && font.has_char(character)
    })
}
