// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fixture shared by A108–A111: a `navigation-sidebar` `GtkListView` over a
//! `GtkStringList`, in a scroller inside a 1000×760 window, whose rows are
//! the shape of a notes-browser row (an icon and two lines of text) built
//! from one of three text widgets, with counters for the factory's row
//! widgets and a frame-phase recorder.
//!
//! With 16,384 rows GTK realizes about 205 of them (A1); with 16 rows it
//! realizes all 16. The difference between the two is the cost the realized
//! rows add, which A108 attributes to text measurement and A110 to teardown.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{glib, pango};

/// A model large enough that GTK realizes its whole band (A1).
pub const BAND_MODEL_ROWS: u32 = 16 * 1024;
/// A model GTK realizes in full.
pub const FEW_ROWS: u32 = 16;
/// The fixture window's size, as the stage-1 measurement's.
pub const WINDOW_SIZE: (i32, i32) = (1000, 760);

/// Which widget a row's two lines of text are.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowText {
    /// `GtkLabel`, end-ellipsized (shapes its text to measure).
    Label,
    /// A text widget whose measure ignores its text ([`MeasureFreeText`]).
    MeasureFree,
    /// `GtkInscription`.
    Inscription,
}

mod measure_free {
    use super::{Cell, RefCell, glib, pango};
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct MeasureFreeText {
        pub(super) text: RefCell<String>,
        pub(super) layout: RefCell<Option<pango::Layout>>,
        pub(super) line_height: Cell<Option<i32>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MeasureFreeText {
        const NAME: &'static str = "GtkLushAxiomsMeasureFreeText";
        type Type = super::MeasureFreeText;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("label");
            klass.set_accessible_role(gtk4::AccessibleRole::Label);
        }
    }

    impl ObjectImpl for MeasureFreeText {}

    impl WidgetImpl for MeasureFreeText {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk4::Orientation::Horizontal {
                return (0, 0, -1, -1);
            }
            let height = self.line_height.get().unwrap_or_else(|| {
                let context = self.obj().pango_context();
                let metrics = context.metrics(context.font_description().as_ref(), None);
                let height = (metrics.height() + pango::SCALE - 1) / pango::SCALE;
                self.line_height.set(Some(height));
                height
            });
            (height, height, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let mut slot = self.layout.borrow_mut();
            let layout = slot.get_or_insert_with(|| {
                let layout = widget.create_pango_layout(Some(&self.text.borrow()));
                layout.set_ellipsize(pango::EllipsizeMode::End);
                layout
            });
            layout.set_width(widget.width() * pango::SCALE);
            snapshot.append_layout(layout, &widget.color());
        }
    }
}

glib::wrapper! {
    /// One line of text measured from its font alone: the text never decides
    /// its size, so changing it costs a redraw, not a relayout.
    pub struct MeasureFreeText(ObjectSubclass<measure_free::MeasureFreeText>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl MeasureFreeText {
    /// An empty text.
    #[must_use]
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Show `text`: a redraw only.
    pub fn set_text(&self, text: &str) {
        let imp = self.imp();
        text.clone_into(&mut imp.text.borrow_mut());
        if let Some(layout) = imp.layout.borrow().as_ref() {
            layout.set_text(text);
        }
        self.queue_draw();
    }
}

impl Default for MeasureFreeText {
    fn default() -> Self {
        Self::new()
    }
}

/// Factory row widgets created and destroyed, and the bound list items.
#[derive(Default)]
pub struct RowCounters {
    /// `setup` emissions: row widgets created.
    pub setups: Cell<u32>,
    /// `teardown` emissions: row widgets destroyed.
    pub teardowns: Cell<u32>,
    /// The list items bound now.
    pub bound: RefCell<Vec<glib::WeakRef<gtk4::ListItem>>>,
}

impl RowCounters {
    /// Reset the setup and teardown counts.
    pub fn reset(&self) {
        self.setups.set(0);
        self.teardowns.set(0);
    }
}

/// The list, its model and scroller, and its counters.
#[derive(Clone)]
pub struct RowList {
    /// The string model the list shows.
    pub model: gtk4::StringList,
    /// The list.
    pub list: gtk4::ListView,
    /// The list's scroller.
    pub scroller: gtk4::ScrolledWindow,
    /// The window column holding the scroller.
    pub column: gtk4::Box,
    /// Row widget counters.
    pub counters: Rc<RowCounters>,
    /// The text widgets of every row created, by row widget.
    pub texts: Rc<RefCell<Vec<glib::WeakRef<gtk4::Widget>>>>,
}

fn text_widget(kind: RowText) -> gtk4::Widget {
    match kind {
        RowText::Label => {
            let label = gtk4::Label::new(None);
            label.set_ellipsize(pango::EllipsizeMode::End);
            label.set_xalign(0.0);
            label.upcast()
        }
        RowText::MeasureFree => MeasureFreeText::new().upcast(),
        RowText::Inscription => gtk4::Inscription::new(None).upcast(),
    }
}

/// Set `text` on a text widget [`text_widget`] built.
pub fn set_text(widget: &gtk4::Widget, text: &str) {
    if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
        label.set_text(text);
    } else if let Some(free) = widget.downcast_ref::<MeasureFreeText>() {
        free.set_text(text);
    } else if let Some(inscription) = widget.downcast_ref::<gtk4::Inscription>() {
        inscription.set_text(Some(text));
    }
}

/// The two lines of row `index` in text generation `generation`.
#[must_use]
pub fn row_lines(index: u32, generation: u32) -> (String, String) {
    (
        format!("c{:03}.rs:{}", index % 128, index + generation * 7),
        format!(
            "Bookmark · /tmp/workspace/left/c{:03}.rs · line {index} · generation {generation}",
            index % 128
        ),
    )
}

/// A list of `rows` rows whose text is `kind`, not yet shown.
#[must_use]
pub fn row_list(rows: u32, kind: RowText) -> RowList {
    let names: Vec<String> = (0..rows).map(|index| index.to_string()).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let model = gtk4::StringList::new(&refs);
    let selection = gtk4::NoSelection::new(Some(model.clone()));
    let counters = Rc::new(RowCounters::default());
    let texts: Rc<RefCell<Vec<glib::WeakRef<gtk4::Widget>>>> = Rc::default();
    let factory = gtk4::SignalListItemFactory::new();
    {
        let (counters, texts) = (Rc::clone(&counters), Rc::clone(&texts));
        factory.connect_setup(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            counters.setups.set(counters.setups.get() + 1);
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            row.set_margin_top(6);
            row.set_margin_bottom(6);
            let icon = gtk4::Image::from_icon_name("user-bookmarks-symbolic");
            row.append(&icon);
            let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            lines.set_hexpand(true);
            let title = text_widget(kind);
            let subtitle = text_widget(kind);
            subtitle.add_css_class("caption");
            subtitle.add_css_class("dim-label");
            lines.append(&title);
            lines.append(&subtitle);
            row.append(&lines);
            texts.borrow_mut().push(title.downgrade());
            texts.borrow_mut().push(subtitle.downgrade());
            item.set_child(Some(&row));
        });
    }
    {
        let counters = Rc::clone(&counters);
        factory.connect_teardown(move |_, _| {
            counters.teardowns.set(counters.teardowns.get() + 1);
        });
    }
    {
        let counters = Rc::clone(&counters);
        factory.connect_bind(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let (title, subtitle) = row_lines(item.position(), 0);
            if let Some(lines) = item.child().and_then(|row| row.last_child()) {
                if let Some(first) = lines.first_child() {
                    set_text(&first, &title);
                }
                if let Some(second) = lines.last_child() {
                    set_text(&second, &subtitle);
                }
            }
            counters.bound.borrow_mut().push(item.downgrade());
        });
    }
    {
        let counters = Rc::clone(&counters);
        factory.connect_unbind(move |_, object| {
            counters
                .bound
                .borrow_mut()
                .retain(|bound| bound.upgrade().is_some_and(|bound| &bound != object));
        });
    }
    let list = gtk4::ListView::new(Some(selection), Some(factory));
    list.add_css_class("navigation-sidebar");
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.append(&gtk4::Label::new(Some("header")));
    column.append(&scroller);
    RowList {
        model,
        list,
        scroller,
        column,
        counters,
        texts,
    }
}

impl RowList {
    /// Row widgets the list holds now (its `row` children).
    #[must_use]
    pub fn realized_rows(&self) -> u32 {
        let mut count = 0;
        let mut child = self.list.first_child();
        while let Some(widget) = child {
            if widget.css_name() == "row" {
                count += 1;
            }
            child = widget.next_sibling();
        }
        count
    }

    /// The lowest and highest bound positions.
    #[must_use]
    pub fn bound_span(&self) -> Option<(u32, u32)> {
        self.counters
            .bound
            .borrow()
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .map(|item| item.position())
            .filter(|position| *position != gtk4::INVALID_LIST_POSITION)
            .fold(None, |span, position| match span {
                None => Some((position, position)),
                Some((low, high)) => Some((low.min(position), high.max(position))),
            })
    }

    /// Change the text of every row widget's two lines (what a republish
    /// that retargets the realized rows does).
    pub fn retext(&self, generation: u32) {
        for (index, text) in self.texts.borrow().iter().enumerate() {
            if let Some(text) = text.upgrade() {
                let row = u32::try_from(index / 2).unwrap_or(0);
                let (title, subtitle) = row_lines(row, generation);
                set_text(&text, if index % 2 == 0 { &title } else { &subtitle });
            }
        }
    }

    /// Every bound position (the realized band, the focus row), sorted.
    #[must_use]
    pub fn bound_positions(&self) -> Vec<u32> {
        let mut positions: Vec<u32> = self
            .counters
            .bound
            .borrow()
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .map(|item| item.position())
            .filter(|position| *position != gtk4::INVALID_LIST_POSITION)
            .collect();
        positions.sort_unstable();
        positions.dedup();
        positions
    }

    /// Remove every row no list item is bound to, after, between and before
    /// the bound rows, from the last range to the first (one `items-changed`
    /// each). Returns the ranges removed.
    #[must_use = "the ranges removed are the probe's record"]
    pub fn remove_unbound(&self) -> Vec<(u32, u32)> {
        let bound = self.bound_positions();
        let mut ranges = Vec::new();
        let mut end = self.model.n_items();
        for &position in bound.iter().rev() {
            if position + 1 < end {
                ranges.push((position + 1, end - position - 1));
            }
            end = position;
        }
        if end > 0 {
            ranges.push((0, end));
        }
        for &(start, count) in &ranges {
            self.model.splice(start, count, &[]);
        }
        ranges
    }

    /// Remove the rows from `start` on.
    pub fn remove_tail(&self, start: u32) {
        let len = self.model.n_items();
        if start < len {
            self.model.splice(start, len - start, &[]);
        }
    }

    /// Remove the first `count` rows.
    pub fn remove_head(&self, count: u32) {
        self.model.splice(0, count.min(self.model.n_items()), &[]);
    }
}

/// The phases of one frame: its dispatch, layout (dispatch start to the end
/// of the frame clock's `layout` phase) and paint (`layout` end to `paint`
/// end), microseconds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FramePhases {
    /// The whole dispatch.
    pub total_us: u64,
    /// The layout phase.
    pub layout_us: u64,
    /// The paint phase.
    pub paint_us: u64,
}

/// Records the frame phases of `widget`'s frame clock.
pub struct PhaseRecorder {
    clock: gtk4::gdk::FrameClock,
    handlers: Vec<glib::SignalHandlerId>,
    layout_end: Rc<Cell<Option<Instant>>>,
    paint_end: Rc<Cell<Option<Instant>>>,
}

impl PhaseRecorder {
    /// Record `widget`'s frame clock; `None` before it is realized.
    #[must_use]
    pub fn new(widget: &impl IsA<gtk4::Widget>) -> Option<Self> {
        let clock = widget.frame_clock()?;
        let layout_end: Rc<Cell<Option<Instant>>> = Rc::default();
        let paint_end: Rc<Cell<Option<Instant>>> = Rc::default();
        let handlers = vec![
            clock.connect_layout({
                let layout_end = Rc::clone(&layout_end);
                move |_| layout_end.set(Some(Instant::now()))
            }),
            clock.connect_paint({
                let paint_end = Rc::clone(&paint_end);
                move |_| paint_end.set(Some(Instant::now()))
            }),
        ];
        Some(Self {
            clock,
            handlers,
            layout_end,
            paint_end,
        })
    }

    /// Dispatch until the next frame has painted (at most `budget`) and
    /// return its phases; `None` when no frame painted.
    #[must_use]
    pub fn next_frame(&self, budget: Duration) -> Option<FramePhases> {
        let context = glib::MainContext::default();
        let deadline = Instant::now() + budget;
        while Instant::now() < deadline {
            self.layout_end.set(None);
            self.paint_end.set(None);
            let start = Instant::now();
            if context.iteration(false) {
                // A frame that only redraws runs no layout phase: its layout
                // costs nothing and its paint starts with the dispatch.
                if let Some(paint) = self.paint_end.get() {
                    let layout = self.layout_end.get().unwrap_or(start);
                    let micros = |duration: Duration| {
                        u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
                    };
                    return Some(FramePhases {
                        total_us: micros(start.elapsed()),
                        layout_us: micros(layout.saturating_duration_since(start)),
                        paint_us: micros(paint.saturating_duration_since(layout)),
                    });
                }
            } else {
                std::thread::sleep(Duration::from_micros(300));
            }
        }
        None
    }
}

impl Drop for PhaseRecorder {
    fn drop(&mut self) {
        for handler in self.handlers.drain(..) {
            self.clock.disconnect(handler);
        }
    }
}

/// The median of `values` (the upper one of an even count); 0 when empty.
#[must_use]
pub fn median(mut values: Vec<u64>) -> u64 {
    values.sort_unstable();
    values.get(values.len() / 2).copied().unwrap_or(0)
}
