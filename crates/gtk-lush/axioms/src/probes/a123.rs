// SPDX-License-Identifier: MIT OR Apache-2.0

//! A123: on a presented `GtkListView` over a `GtkTreeListModel`, hiding the
//! **row widgets** (the list's children, each the parent of a list item's
//! child) of one subtree's realized rows by opacity 0, `can-target` false and
//! the accessible `hidden` state is **paint-only**: it runs no row measure,
//! no factory `setup` or `teardown`, leaves the band's first position and the
//! list's allocation where they are, and draws no pixel of a hidden row, also
//! when it is selected, focused or under the pointer. `gtk_widget_pick` over
//! a hidden row returns no widget inside it, and a real press, double press
//! or hover over one selects, activates and prelights nothing (real input
//! only outside an SDK sandbox, which has no RemoteDesktop).
//!
//! GTK writes the `hidden` state itself: `gtk_widget_map` writes it false
//! after the `map` emission and `gtk_widget_unmap` writes it true
//! (`gtkwidget.c:3006`, `:3039`), and the list item manager binds a created
//! row widget before inserting (so mapping) it. The probe cannot read the
//! state's value (the crate forbids `unsafe`, and
//! `gtk_test_accessible_check_state` is variadic), so it uses a reset as a
//! marker: a state the probe reset is absent until something writes it. A
//! row widget the list creates is not in the list at bind (its child has no
//! root) and is still floating, so the bind cannot touch it from Rust: a
//! reference taken there sinks the floating one, and dropping it finalizes the
//! widget before GTK inserts it. A marker reset at bind on its child is
//! present again once the row is inserted (GTK's map wrote it after the
//! bind), so the hiding is applied after the insertion; a marker reset after
//! insertion stays absent
//! across frames (nothing writes it until the next unmap), and markers reset
//! on the band's rows are present again after the list is unmapped and mapped
//! on the rows the map maps: those the viewport shows (the band's rows
//! outside it stay unmapped, A10).
//! Opacity and `can-target`, which GTK does not write, survive the cycle.
//!
//! Whether the presented accessible tree keeps a hidden row is source-read:
//! `gtk_accessible_should_present` is false for a widget whose `hidden` state
//! is true (`gtkaccessible.c:1413-1444`), and the AT-SPI context's
//! `GetChildren` and `GetChildAtIndex` skip such a child, so its descendants,
//! reached only through it, are not presented either
//! (`gtkatspicontext.c:670-745`). `GTK_A11Y=test` presents no tree to read.
//!
//! LushText's sliced sidebar collapse (`slice-large-directory-collapse` D22)
//! hides a collapsing subtree's rows this way at intent, so its first frame
//! shows the collapse without removing a bound row in it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk4::glib;
use gtk4::prelude::*;

use super::LAYOUT_SETTLE;
use super::row_lists::RowCounters;
use crate::observation::Recorder;
use crate::probes::remote_input::{
    BUTTON_PRIMARY, RemoteInput, fullscreen_window, point_on_screen,
};
use crate::session::{Presented, flush_events, settle, wait_until};
use crate::{AxiomId, Observation};

/// The expanded directory's children.
pub const SUBTREE_ROWS: u32 = 256;
/// Sibling rows after the directory.
pub const AFTER_ROWS: u32 = 400;
/// The subtree row the probe selects (flattened position).
pub const SELECTED_ROW: u32 = 3;
/// The subtree row the probe focuses.
pub const FOCUSED_ROW: u32 = 5;
/// The subtree row the pointer rests on while the rows are hidden.
pub const HOVERED_ROW: u32 = 8;
/// The subtree row the pick and the real presses aim at.
pub const PRESSED_ROW: u32 = 12;
/// Children the removal of the created-row case takes from the head.
pub const REMOVED_ROWS: u32 = 32;

mod counting {
    use super::Cell;
    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    thread_local! {
        /// Measure calls of every counting bin on this thread.
        pub(super) static MEASURES: Cell<u32> = const { Cell::new(0) };
    }

    #[derive(Default)]
    pub struct MeasureCountingBin;

    #[glib::object_subclass]
    impl ObjectSubclass for MeasureCountingBin {
        const NAME: &str = "GtkLushAxiomsMeasureCountingBin";
        type Type = super::MeasureCountingBin;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk4::BinLayout>();
        }
    }

    impl ObjectImpl for MeasureCountingBin {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for MeasureCountingBin {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            MEASURES.with(|count| count.set(count.get() + 1));
            self.parent_measure(orientation, for_size)
        }
    }
}

glib::wrapper! {
    /// A one-child bin that counts its measure calls: the list item child of
    /// the A123 fixture, so a re-measure of any row shows.
    pub struct MeasureCountingBin(ObjectSubclass<counting::MeasureCountingBin>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl MeasureCountingBin {
    /// A bin holding `child`.
    #[must_use]
    pub fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let bin: Self = glib::Object::new();
        child.set_parent(&bin);
        bin
    }
}

/// Row measures counted since the last [`reset_measures`].
#[must_use]
pub fn measures() -> u32 {
    counting::MEASURES.with(Cell::get)
}

/// Reset the row measure count.
pub fn reset_measures() {
    counting::MEASURES.with(|count| count.set(0));
}

/// What the bind hook does to a row bound into the subtree while the hiding
/// is applied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BindHiding {
    /// Nothing: the rows are not hidden.
    #[default]
    Off,
    /// Opacity 0 and `can-target` false on the row widget, and its `hidden`
    /// state reset (the marker).
    HideAndMark,
}

/// The A123 tree: a directory expanded over [`SUBTREE_ROWS`] children, then
/// [`AFTER_ROWS`] siblings, in a list whose rows are an item child (a
/// [`MeasureCountingBin`]) holding a tree expander with an icon and a label.
#[derive(Clone)]
pub struct HideFixture {
    /// The directory's child store.
    pub children: gtk4::gio::ListStore,
    /// The flattened model.
    pub tree: gtk4::TreeListModel,
    /// The selection.
    pub selection: gtk4::SingleSelection,
    /// The list.
    pub list: gtk4::ListView,
    /// Its scroller.
    pub scroller: gtk4::ScrolledWindow,
    /// The window content.
    pub column: gtk4::Box,
    /// Row widget counters (setup, teardown, bind, the bound items).
    pub counters: Rc<RowCounters>,
    /// `activate` emissions of the list.
    pub activations: Rc<Cell<u32>>,
    /// What the bind hook does to subtree rows.
    pub bind_hiding: Rc<Cell<BindHiding>>,
    /// Children of the subtree rows the list created (not yet inserted at
    /// bind) that the bind hook marked since the last reset.
    pub marked_at_bind: Rc<RefCell<Vec<glib::WeakRef<gtk4::Widget>>>>,
}

fn numbered(prefix: &str, rows: u32) -> gtk4::gio::ListStore {
    let store = gtk4::gio::ListStore::new::<gtk4::StringObject>();
    let items: Vec<gtk4::StringObject> = (0..rows)
        .map(|index| gtk4::StringObject::new(&format!("{prefix}{index:05}")))
        .collect();
    store.splice(0, 0, &items);
    store
}

/// Whether `item` is bound to a row inside the expanded directory.
fn in_subtree(item: &gtk4::ListItem) -> bool {
    item.item()
        .and_downcast::<gtk4::TreeListRow>()
        .is_some_and(|row| row.parent().is_some())
}

/// The row widget of a list item: its child's parent.
fn row_widget_of(item: &gtk4::ListItem) -> Option<gtk4::Widget> {
    item.child().and_then(|child| child.parent())
}

/// Hide `row` (opacity 0, `can-target` false, accessible `hidden`).
pub fn hide_row(row: &gtk4::Widget) {
    row.set_opacity(0.0);
    row.set_can_target(false);
    row.update_state(&[gtk4::accessible::State::Hidden(true)]);
}

/// Clear what [`hide_row`] set; the `hidden` state is reset, not written.
pub fn clear_row(row: &gtk4::Widget) {
    row.set_opacity(1.0);
    row.set_can_target(true);
    row.reset_state(gtk4::AccessibleState::Hidden);
}

/// Build the fixture (not yet shown) and expand its directory row.
#[must_use]
pub fn hide_fixture() -> HideFixture {
    let root = numbered("after-", AFTER_ROWS);
    root.insert(0, &gtk4::StringObject::new("directory"));
    let child_store = numbered("child-", SUBTREE_ROWS);
    let tree = {
        let child_store = child_store.clone();
        gtk4::TreeListModel::new(root, false, false, move |item| {
            let name = item.downcast_ref::<gtk4::StringObject>()?.string();
            (name == "directory").then(|| child_store.clone().upcast())
        })
    };
    if let Some(row) = tree.item(0).and_downcast::<gtk4::TreeListRow>() {
        row.set_expanded(true);
    }
    let counters = Rc::new(RowCounters::default());
    let bind_hiding = Rc::new(Cell::new(BindHiding::Off));
    let marked_at_bind: Rc<RefCell<Vec<glib::WeakRef<gtk4::Widget>>>> = Rc::default();
    let factory = gtk4::SignalListItemFactory::new();
    {
        let counters = Rc::clone(&counters);
        factory.connect_setup(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            counters.setups.set(counters.setups.get() + 1);
            let expander = gtk4::TreeExpander::new();
            let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            content.append(&gtk4::Image::from_icon_name("text-x-generic-symbolic"));
            content.append(&gtk4::Label::new(None));
            expander.set_child(Some(&content));
            item.set_child(Some(&MeasureCountingBin::new(&expander)));
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
        let bind_hiding = Rc::clone(&bind_hiding);
        let marked_at_bind = Rc::clone(&marked_at_bind);
        factory.connect_bind(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let Some(row) = item.item().and_downcast::<gtk4::TreeListRow>() else {
                return;
            };
            if let Some(expander) = item
                .child()
                .and_then(|bin| bin.first_child())
                .and_downcast::<gtk4::TreeExpander>()
            {
                expander.set_list_row(Some(&row));
                if let Some(label) = expander.child().and_then(|content| content.last_child())
                    && let Some(label) = label.downcast_ref::<gtk4::Label>()
                    && let Some(text) = row.item().and_downcast::<gtk4::StringObject>()
                {
                    label.set_text(&text.string());
                }
            }
            // A row widget the list is creating is not in the list yet at
            // bind (its child has no root) and is still floating: taking a
            // reference to it from Rust sinks that reference, and dropping it
            // finalizes the widget before GTK inserts it. Such a row is only
            // marked on its child here, and hidden after the insertion.
            if let Some(child) = item.child() {
                let inserted = child.root().is_some();
                if bind_hiding.get() == BindHiding::HideAndMark && in_subtree(item) {
                    if inserted {
                        if let Some(row_widget) = child.parent() {
                            hide_row(&row_widget);
                        }
                    } else {
                        child.reset_state(gtk4::AccessibleState::Hidden);
                        marked_at_bind.borrow_mut().push(child.downgrade());
                    }
                } else if inserted
                    && let Some(row_widget) = child.parent()
                    && row_widget.opacity() < 1.0
                {
                    // GTK recycles row widgets without resetting them.
                    clear_row(&row_widget);
                }
            }
            counters.binds.set(counters.binds.get() + 1);
            counters.bound.borrow_mut().push(item.downgrade());
        });
    }
    {
        let counters = Rc::clone(&counters);
        factory.connect_unbind(move |_, object| {
            if let Some(expander) = object
                .downcast_ref::<gtk4::ListItem>()
                .and_then(gtk4::ListItem::child)
                .and_then(|bin| bin.first_child())
                .and_downcast::<gtk4::TreeExpander>()
            {
                expander.set_list_row(None::<&gtk4::TreeListRow>);
            }
            counters
                .bound
                .borrow_mut()
                .retain(|bound| bound.upgrade().is_some_and(|bound| &bound != object));
        });
    }
    let selection = gtk4::SingleSelection::new(Some(tree.clone()));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);
    selection.set_selected(gtk4::INVALID_LIST_POSITION);
    let list = gtk4::ListView::new(Some(selection.clone()), Some(factory));
    list.add_css_class("navigation-sidebar");
    let activations = Rc::new(Cell::new(0));
    {
        let activations = Rc::clone(&activations);
        list.connect_activate(move |_, _| activations.set(activations.get() + 1));
    }
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.append(&gtk4::Label::new(Some("header")));
    column.append(&scroller);
    HideFixture {
        children: child_store,
        tree,
        selection,
        list,
        scroller,
        column,
        counters,
        activations,
        bind_hiding,
        marked_at_bind,
    }
}

impl HideFixture {
    /// The bound list items now, with their flattened positions, ascending.
    #[must_use]
    pub fn bound(&self) -> Vec<(u32, gtk4::ListItem)> {
        let mut bound: Vec<(u32, gtk4::ListItem)> = self
            .counters
            .bound
            .borrow()
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .map(|item| (item.position(), item))
            .filter(|(position, _)| *position != gtk4::INVALID_LIST_POSITION)
            .collect();
        bound.sort_by_key(|(position, _)| *position);
        bound
    }

    /// The row widgets of the realized subtree rows, with their positions.
    #[must_use]
    pub fn subtree_rows(&self) -> Vec<(u32, gtk4::Widget)> {
        self.bound()
            .into_iter()
            .filter(|(_, item)| in_subtree(item))
            .filter_map(|(position, item)| row_widget_of(&item).map(|row| (position, row)))
            .collect()
    }

    /// The row widget bound to flattened `position`, if realized.
    #[must_use]
    pub fn row_at(&self, position: u32) -> Option<gtk4::Widget> {
        self.bound()
            .into_iter()
            .find(|(bound, _)| *bound == position)
            .and_then(|(_, item)| row_widget_of(&item))
    }

    /// Hide every realized subtree row, returning how many and the time.
    #[must_use = "the count and time are the measurement"]
    pub fn hide_subtree(&self) -> (u32, Duration) {
        let started = Instant::now();
        let rows = self.subtree_rows();
        for (_, row) in &rows {
            hide_row(row);
        }
        (
            u32::try_from(rows.len()).unwrap_or(u32::MAX),
            started.elapsed(),
        )
    }

    /// Clear every realized row the hiding touched, returning how many and
    /// the time.
    #[must_use = "the count and time are the measurement"]
    pub fn clear_subtree(&self) -> (u32, Duration) {
        let started = Instant::now();
        let mut cleared = 0;
        for (_, item) in self.bound() {
            if let Some(row) = row_widget_of(&item)
                && (row.opacity() < 1.0 || !row.can_target())
            {
                clear_row(&row);
                cleared += 1;
            }
        }
        (cleared, started.elapsed())
    }
}

/// The band and the list's layout, and the counts since the last reset.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BandReading {
    /// The first bound flattened position.
    pub first: u32,
    /// Bound list items.
    pub bound: u32,
    /// The list's width and height.
    pub allocation: (i32, i32),
    /// Row measure calls.
    pub measures: u32,
    /// Factory setups.
    pub setups: u32,
    /// Factory teardowns.
    pub teardowns: u32,
}

/// Read the band, then reset the counts.
#[must_use]
pub fn read_band(fixture: &HideFixture) -> BandReading {
    let bound = fixture.bound();
    let reading = BandReading {
        first: bound.first().map_or(u32::MAX, |(position, _)| *position),
        bound: u32::try_from(bound.len()).unwrap_or(u32::MAX),
        allocation: (fixture.list.width(), fixture.list.height()),
        measures: measures(),
        setups: fixture.counters.setups.get(),
        teardowns: fixture.counters.teardowns.get(),
    };
    reset_counts(fixture);
    reading
}

/// Reset the measure, setup and teardown counts.
pub fn reset_counts(fixture: &HideFixture) {
    reset_measures();
    fixture.counters.reset();
}

/// Wait for two painted frames of the list, then dispatch what is ready.
fn two_frames(list: &gtk4::ListView) {
    let Some(clock) = list.frame_clock() else {
        settle(LAYOUT_SETTLE);
        return;
    };
    let painted = Rc::new(Cell::new(0));
    let handler = clock.connect_after_paint({
        let painted = Rc::clone(&painted);
        move |_| painted.set(painted.get() + 1)
    });
    list.queue_draw();
    let _ = wait_until(Duration::from_secs(2), || {
        if painted.get() == 1 {
            list.queue_draw();
        }
        painted.get() >= 2
    });
    clock.disconnect(handler);
    flush_events();
}

/// The rendered pixels of the list over its whole allocation (an explicit
/// viewport: a node's own bounds shrink when rows draw nothing), and their
/// scale to list coordinates.
#[expect(
    clippy::cast_precision_loss,
    reason = "the list's size is a few thousand pixels, exact in f32"
)]
fn list_pixels(list: &gtk4::ListView) -> Option<(Vec<u8>, i32, i32, f64)> {
    let renderer = list.native()?.renderer()?;
    let (width, height) = (list.width(), list.height());
    if width <= 0 || height <= 0 {
        return None;
    }
    let paintable = gtk4::WidgetPaintable::new(Some(list));
    let snapshot = gtk4::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
    let node = snapshot.to_node()?;
    let viewport = gtk4::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
    let texture = renderer.render_texture(&node, Some(&viewport));
    let stride = usize::try_from(texture.width()).ok()? * 4;
    let mut bytes = vec![0; stride * usize::try_from(texture.height()).ok()?];
    texture.download(&mut bytes, stride);
    let scale = f64::from(texture.width()) / f64::from(width);
    Some((bytes, texture.width(), texture.height(), scale))
}

/// Whether every pixel of `row`'s rectangle in `pixels` is the same colour;
/// `None` for a row not wholly inside the list.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "row bounds are small non-negative pixel coordinates inside the texture"
)]
fn row_is_uniform(
    pixels: &(Vec<u8>, i32, i32, f64),
    list: &gtk4::ListView,
    row: &gtk4::Widget,
) -> Option<bool> {
    let (bytes, width, height, scale) = pixels;
    let bounds = row.compute_bounds(list)?;
    // Only rows wholly inside the list: an edge row may show only padding.
    if bounds.y() < 0.0 || f64::from(bounds.y() + bounds.height()) > f64::from(list.height()) {
        return None;
    }
    let x0 = (f64::from(bounds.x()) * scale).max(0.0) as usize;
    let y0 = (f64::from(bounds.y()) * scale).max(0.0) as usize;
    let x1 = ((f64::from(bounds.x() + bounds.width()) * scale) as usize).min(*width as usize);
    let y1 = ((f64::from(bounds.y() + bounds.height()) * scale) as usize).min(*height as usize);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let stride = *width as usize * 4;
    let reference = &bytes[y0 * stride + x0 * 4..y0 * stride + x0 * 4 + 4];
    Some(
        (y0..y1).all(|y| {
            (x0..x1).all(|x| &bytes[y * stride + x * 4..y * stride + x * 4 + 4] == reference)
        }),
    )
}

/// Whether the hidden `hidden`-state markers of `rows` are present.
fn markers_present(rows: &[gtk4::Widget]) -> Vec<bool> {
    rows.iter()
        .map(|row| gtk4::test_accessible_has_state(row, gtk4::AccessibleState::Hidden))
        .collect()
}

/// Move the pointer to the centre of `widget`.
fn hover(input: &RemoteInput, widget: &gtk4::Widget) -> Result<(), String> {
    let (x, y) = point_on_screen(
        widget,
        f64::from(widget.width()) / 2.0,
        f64::from(widget.height()) / 2.0,
    )
    .ok_or_else(|| "the widget is not on screen".to_owned())?;
    input.move_to(x, y)
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

/// Probe A123. See the module documentation.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one probe walks control, hide, pointer, created rows, an unmap-map cycle and the \
              clear in order on one presented fixture; splitting it would pass the fixture, the \
              input session and the recorder through helpers that read worse than the sequence"
)]
pub fn probe_a123() -> Observation {
    Recorder::run(AxiomId::new(123), |recorder| {
        let fixture = hide_fixture();
        let shown = Presented::checked_window(
            recorder,
            fullscreen_window(&fixture.column),
            "control: the list was presented",
        )?;
        let window = shown.window().clone();
        recorder.control(
            wait_until(Duration::from_secs(5), || window.is_fullscreen()),
            "control: the window is fullscreen",
        )?;
        settle(LAYOUT_SETTLE);
        let sandbox = super::remote_input::in_sdk_sandbox();
        let input = if sandbox {
            recorder.measure(
                "input_half_skipped",
                "the SDK sandbox has no RemoteDesktop (GTK_LUSH_AXIOMS_SANDBOX=1)",
            );
            None
        } else {
            match RemoteInput::start() {
                Ok(input) => Some(input),
                Err(error) => {
                    recorder.measure("remote_desktop_error", error);
                    return recorder
                        .control(false, "control: the compositor accepts RemoteDesktop input");
                }
            }
        };

        // Control: one subtree row selected, one focused, the pointer on a
        // third, nothing hidden.
        fixture.selection.set_selected(SELECTED_ROW);
        fixture
            .list
            .scroll_to(FOCUSED_ROW, gtk4::ListScrollFlags::FOCUS, None);
        settle(LAYOUT_SETTLE);
        let hovered = recorder.require(fixture.row_at(HOVERED_ROW), "control: hovered row")?;
        let mut control_prelight = None;
        if let Some(input) = &input {
            if let Err(error) = hover(input, &hovered) {
                recorder.measure("hover_error", error);
            }
            settle(LAYOUT_SETTLE);
            control_prelight = Some(hovered.state_flags().contains(gtk4::StateFlags::PRELIGHT));
        }
        recorder.measure("control_prelight", format!("{control_prelight:?}"));
        if let Some(prelight) = control_prelight {
            recorder.control(prelight, "control: the pointer prelights a shown row")?;
        }
        reset_counts(&fixture);
        two_frames(&fixture.list);
        let control = read_band(&fixture);
        let control_pixels =
            recorder.require(list_pixels(&fixture.list), "control: the list renders")?;
        recorder.measure("control_band", format!("{control:?}"));

        // Hide the realized subtree rows.
        let (hidden_rows, hide_time) = fixture.hide_subtree();
        two_frames(&fixture.list);
        let hidden = read_band(&fixture);
        let hidden_pixels =
            recorder.require(list_pixels(&fixture.list), "control: the list renders")?;
        recorder.measure("hidden_rows", hidden_rows);
        recorder.measure("hide_us", micros(hide_time));
        recorder.measure("hidden_band", format!("{hidden:?}"));
        recorder.control(
            hidden_rows > 100,
            "control: the band holds most of a viewport of subtree rows",
        )?;
        let rows: Vec<(u32, gtk4::Widget)> = fixture.subtree_rows();
        let shown_uniform: Vec<Option<bool>> = rows
            .iter()
            .map(|(_, row)| row_is_uniform(&control_pixels, &fixture.list, row))
            .collect();
        let hidden_uniform: Vec<Option<bool>> = rows
            .iter()
            .map(|(_, row)| row_is_uniform(&hidden_pixels, &fixture.list, row))
            .collect();
        let on_screen = hidden_uniform
            .iter()
            .filter(|value| value.is_some())
            .count();
        let drew_when_shown = shown_uniform
            .iter()
            .filter(|value| **value == Some(false))
            .count();
        let blank_when_hidden = hidden_uniform
            .iter()
            .filter(|value| **value == Some(true))
            .count();
        let special_blank: Vec<Option<bool>> = [SELECTED_ROW, FOCUSED_ROW, HOVERED_ROW]
            .iter()
            .map(|position| {
                rows.iter()
                    .find(|(bound, _)| bound == position)
                    .and_then(|(_, row)| row_is_uniform(&hidden_pixels, &fixture.list, row))
            })
            .collect();
        recorder.measure(
            "rows_on_screen(drew_when_shown,blank_when_hidden,on_screen)",
            format!("{drew_when_shown},{blank_when_hidden},{on_screen}"),
        );
        recorder.measure(
            "blank_when_hidden(selected,focused,hovered)",
            format!("{special_blank:?}"),
        );
        recorder.control(
            on_screen > 0 && drew_when_shown == on_screen,
            "control: every on-screen subtree row drew pixels while shown",
        )?;
        recorder.axiom(
            hidden.measures <= control.measures
                && hidden.setups == 0
                && hidden.teardowns == 0
                && hidden.first == control.first
                && hidden.bound == control.bound
                && hidden.allocation == control.allocation,
            "A123: hiding the rows runs no row measure, setup or teardown and leaves the band \
             and the list's allocation unchanged",
        )?;
        recorder.axiom(
            blank_when_hidden == on_screen
                && special_blank.iter().all(|blank| *blank == Some(true)),
            "A123: a hidden row draws no pixel, selected, focused or under the pointer",
        )?;

        // The pointer.
        let pressed = recorder.require(fixture.row_at(PRESSED_ROW), "control: pressed row")?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "graphene points are f32; row coordinates fit it exactly"
        )]
        let centre = gtk4::graphene::Point::new(
            (f64::from(pressed.width()) / 2.0) as f32,
            (f64::from(pressed.height()) / 2.0) as f32,
        );
        let in_list = pressed.compute_point(&fixture.list, &centre);
        let picked = in_list.and_then(|point| {
            fixture.list.pick(
                f64::from(point.x()),
                f64::from(point.y()),
                gtk4::PickFlags::DEFAULT,
            )
        });
        let picked_inside = picked
            .as_ref()
            .is_some_and(|widget| widget == &pressed || widget.is_ancestor(&pressed));
        recorder.measure(
            "picked_over_hidden_row",
            format!("{:?}", picked.as_ref().map(|widget| widget.type_().name())),
        );
        recorder.axiom(
            picked.is_some() && !picked_inside,
            "A123: gtk_widget_pick over a hidden row returns no widget inside it",
        )?;
        if let Some(input) = &input {
            let selected_before = fixture.selection.selected();
            let activations_before = fixture.activations.get();
            let hover_result = hover(input, &pressed);
            settle(LAYOUT_SETTLE);
            let prelight = pressed.state_flags().contains(gtk4::StateFlags::PRELIGHT);
            let press = hover_result.and_then(|()| input.click(BUTTON_PRIMARY));
            settle(LAYOUT_SETTLE);
            let selected_after_press = fixture.selection.selected();
            let double = press.clone().and_then(|()| {
                input.click(BUTTON_PRIMARY)?;
                input.click(BUTTON_PRIMARY)
            });
            settle(LAYOUT_SETTLE);
            let selected_after_double = fixture.selection.selected();
            let activations = fixture.activations.get() - activations_before;
            recorder.measure("hidden_row_prelight", prelight);
            recorder.measure("press", format!("{press:?}"));
            recorder.measure("double_press", format!("{double:?}"));
            recorder.measure(
                "selected(before,after_press,after_double)",
                format!("{selected_before},{selected_after_press},{selected_after_double}"),
            );
            recorder.measure("activations_over_hidden_row", activations);
            recorder.control(
                press.is_ok() && double.is_ok(),
                "control: the compositor delivered the presses",
            )?;
            recorder.axiom(
                !prelight
                    && selected_after_press == selected_before
                    && selected_after_double == selected_before
                    && activations == 0,
                "A123: a real hover, press and double press over a hidden row prelight, select \
                 and activate nothing",
            )?;
        }

        // A row the list creates while the hiding is applied: the bind hook
        // hides it and resets its marker; GTK maps it after the bind.
        fixture.bind_hiding.set(BindHiding::HideAndMark);
        fixture.marked_at_bind.borrow_mut().clear();
        reset_counts(&fixture);
        fixture
            .children
            .splice(0, REMOVED_ROWS, &[] as &[glib::Object]);
        let created_setups = fixture.counters.setups.get();
        let created_children: Vec<gtk4::Widget> = fixture
            .marked_at_bind
            .borrow()
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .collect();
        let created_markers = markers_present(&created_children);
        let created: Vec<gtk4::Widget> = created_children
            .iter()
            .filter_map(WidgetExt::parent)
            .collect();
        let created_in_list = created
            .iter()
            .filter(|row| row.parent().as_ref() == Some(fixture.list.upcast_ref()))
            .count();
        for row in &created {
            hide_row(row);
        }
        recorder.measure(
            "created_rows(setups,marked_at_bind,in_list_after,marker_present_after_insert)",
            format!(
                "{created_setups},{},{created_in_list},{}",
                created_children.len(),
                created_markers.iter().filter(|present| **present).count()
            ),
        );
        recorder.control(
            !created_children.is_empty(),
            "control: the removal created subtree rows while the hiding was applied",
        )?;
        recorder.axiom(
            created_in_list == created_children.len()
                && created_markers.iter().all(|present| *present),
            "A123: a row the list creates is inserted after its bind, and GTK writes its \
             hidden state then (gtk_widget_map), so it is hidden after the insertion",
        )?;
        // A write after insertion holds until the next unmap.
        two_frames(&fixture.list);
        let rows: Vec<gtk4::Widget> = fixture
            .subtree_rows()
            .into_iter()
            .map(|(_, row)| row)
            .collect();
        for row in &rows {
            row.reset_state(gtk4::AccessibleState::Hidden);
        }
        two_frames(&fixture.list);
        settle(LAYOUT_SETTLE);
        let held_markers = markers_present(&rows);
        recorder.measure(
            "markers_rewritten_while_mapped",
            held_markers.iter().filter(|present| **present).count(),
        );
        recorder.axiom(
            !rows.is_empty() && held_markers.iter().all(|present| !*present),
            "A123: GTK writes no hidden state on a mapped row after its insertion",
        )?;

        // An unmap and map of the list mid-hiding.
        reset_counts(&fixture);
        fixture.scroller.set_visible(false);
        two_frames(&fixture.list);
        fixture.scroller.set_visible(true);
        two_frames(&fixture.list);
        settle(LAYOUT_SETTLE);
        let cycle = read_band(&fixture);
        let still: Vec<gtk4::Widget> = rows
            .iter()
            .filter(|row| row.parent().as_ref() == Some(fixture.list.upcast_ref()))
            .cloned()
            .collect();
        let cycle_markers = markers_present(&still);
        let mapped_rewritten = still
            .iter()
            .zip(&cycle_markers)
            .filter(|(row, present)| row.is_mapped() && **present)
            .count();
        let mapped = still.iter().filter(|row| row.is_mapped()).count();
        let unmapped_rewritten = still
            .iter()
            .zip(&cycle_markers)
            .filter(|(row, present)| !row.is_mapped() && **present)
            .count();
        let cycle_paint = still
            .iter()
            .all(|row| row.opacity() == 0.0 && !row.can_target());
        recorder.measure("unmap_map_band", format!("{cycle:?}"));
        recorder.measure(
            "unmap_map(rows_kept,mapped,mapped_rewritten,unmapped_rewritten)",
            format!(
                "{},{mapped},{mapped_rewritten},{unmapped_rewritten}",
                still.len()
            ),
        );
        recorder.control(
            mapped > 0,
            "control: rows were mapped again after the cycle",
        )?;
        recorder.axiom(
            mapped_rewritten == mapped && cycle_paint,
            "A123: an unmap and map writes the hidden state of every row it maps again (the \
             rows the viewport shows; band rows outside it stay unmapped), and keeps every \
             row's opacity and can-target",
        )?;

        // Clear.
        fixture.bind_hiding.set(BindHiding::Off);
        two_frames(&fixture.list);
        let before_clear = read_band(&fixture);
        let (cleared, clear_time) = fixture.clear_subtree();
        two_frames(&fixture.list);
        let after_clear = read_band(&fixture);
        recorder.measure("cleared_rows", cleared);
        recorder.measure("clear_us", micros(clear_time));
        recorder.measure("cleared_band", format!("{after_clear:?}"));
        recorder.axiom(
            after_clear.setups == 0
                && after_clear.teardowns == 0
                && after_clear.first == before_clear.first
                && after_clear.bound == before_clear.bound
                && after_clear.allocation == before_clear.allocation
                && after_clear.measures <= control.measures,
            "A123: clearing the rows runs no row measure, setup or teardown and leaves the band \
             and the list's allocation unchanged",
        )?;
        recorder.measure(
            "presented_tree",
            "source-read: should_present is false for hidden true \
             (gtkaccessible.c:1413-1444), and AT-SPI GetChildren/GetChildAtIndex skip such a \
             child, so its descendants are not presented (gtkatspicontext.c:670-745)",
        );
        drop(shown);
        drop(fixture);
        flush_events();
        Ok(())
    })
}
