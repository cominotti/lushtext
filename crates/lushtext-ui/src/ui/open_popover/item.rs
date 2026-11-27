// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** of `WFR-RECENT-DOCUMENTS` — GObject row item,
//! recycled row widget tree, and list factory for the recent-document list.
//!
//! The GTK list model needs `glib::Object` values, while the service layer
//! returns plain Rust rows. This adapter stores display fields and the path to
//! activate, builds the GNOME-shaped row subtree, and wires row focus, keynav,
//! removal, tooltips, and accessibility metadata on bind/unbind.

use glib::subclass::prelude::*;
use gtk4::prelude::*;
use gtk4::{self, glib};
use lushtext_model::persistence::recent_document::RecentDocumentRow;
use std::cell::RefCell;
use std::path::PathBuf;

use crate::ui::accessibility::{self, RowAccessibility};

// The private GObject implementation holds row data; the public wrapper below
// is what `gio::ListStore` stores.
mod imp {
    use super::{ObjectImpl, ObjectSubclass, PathBuf, RefCell, glib};

    /// Private storage for one recent-document row.
    #[derive(Default)]
    pub struct OpenPopoverItem {
        /// Primary row title.
        pub title: RefCell<String>,
        /// Secondary path/location text.
        pub subtitle: RefCell<String>,
        /// Short age/context label.
        pub age_label: RefCell<Option<String>>,
        /// File path opened when the row activates.
        pub path: RefCell<PathBuf>,
    }

    // ObjectSubclass registers this data object with GLib so ListStore can hold it.
    #[glib::object_subclass]
    impl ObjectSubclass for OpenPopoverItem {
        const NAME: &str = "LushtextOpenPopoverItem";
        type Type = super::OpenPopoverItem;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for OpenPopoverItem {}
}

// glib::wrapper! generates the public GObject wrapper for ListStore rows.
glib::wrapper! {
    /// Public GObject row model used by the Open popover ListView.
    pub struct OpenPopoverItem(ObjectSubclass<imp::OpenPopoverItem>);
}

impl OpenPopoverItem {
    /// Convert a service row into a GTK row item.
    #[must_use]
    pub fn from_row(row: RecentDocumentRow) -> Self {
        let obj: Self = glib::Object::builder().build();
        let imp = obj.imp();
        imp.title.replace(row.title);
        imp.subtitle.replace(row.subtitle);
        imp.age_label.replace(row.age_label);
        imp.path.replace(row.path);
        obj
    }

    /// Primary row title.
    #[must_use]
    pub fn title(&self) -> String {
        self.imp().title.borrow().clone()
    }

    /// Secondary row text.
    #[must_use]
    pub fn subtitle(&self) -> String {
        self.imp().subtitle.borrow().clone()
    }

    /// Optional age/context label.
    #[must_use]
    pub fn age_label(&self) -> Option<String> {
        self.imp().age_label.borrow().clone()
    }

    /// File path opened when activated.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.imp().path.borrow().clone()
    }
}

/// Row subtree shared by production factory setup and test snapshots.
struct RecentRowWidgets {
    /// Stable grid parent returned to the list-factory row.
    grid: gtk4::Grid,
    /// Stack showing the optional open-document marker in the leading gutter.
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(
            dead_code,
            reason = "production uses the widget through the grid; test snapshots inspect it directly"
        )
    )]
    marker_stack: gtk4::Stack,
    /// Primary filename text reused by recycled rows as their item changes.
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(
            dead_code,
            reason = "production uses the widget through the grid; test snapshots inspect it directly"
        )
    )]
    title: gtk4::Inscription,
    /// Secondary path text reused by recycled rows as their item changes.
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(
            dead_code,
            reason = "production uses the widget through the grid; test snapshots inspect it directly"
        )
    )]
    subtitle: gtk4::Inscription,
    /// Relative-age text kept separate so layout snapshots can verify GNOME parity.
    #[cfg_attr(
        not(feature = "test-utils"),
        expect(
            dead_code,
            reason = "production uses the widget through the grid; test snapshots inspect it directly"
        )
    )]
    age: gtk4::Inscription,
    /// Remove button wired by the factory after the row receives its item.
    remove: gtk4::Button,
}

/// Internal grid attachment tuple captured before conversion to the public row snapshot.
#[cfg(feature = "test-utils")]
pub(super) struct RecentRowChildLayout {
    /// GtkGrid column occupied by the child.
    pub column: i32,
    /// GtkGrid row occupied by the child.
    pub row: i32,
    /// Number of GtkGrid columns spanned by the child.
    pub column_span: i32,
    /// Number of GtkGrid rows spanned by the child.
    pub row_span: i32,
}

/// Internal test-only snapshot of the GNOME-shaped recent-row skeleton.
#[cfg(feature = "test-utils")]
pub(super) struct RecentRowLayoutSnapshot {
    pub grid_margin_top: i32,
    pub grid_margin_bottom: i32,
    pub grid_margin_start: i32,
    pub grid_margin_end: i32,
    pub grid_row_spacing: u32,
    pub grid_column_spacing: u32,
    pub grid_height_request: i32,
    pub marker_hhomogeneous: bool,
    pub marker_vhomogeneous: bool,
    pub marker_layout: RecentRowChildLayout,
    pub title_overflow: gtk4::InscriptionOverflow,
    pub title_xalign: f32,
    pub title_hexpand: bool,
    pub title_layout: RecentRowChildLayout,
    pub subtitle_overflow: gtk4::InscriptionOverflow,
    pub subtitle_min_chars: u32,
    pub subtitle_nat_chars: u32,
    pub subtitle_min_lines: u32,
    pub subtitle_nat_lines: u32,
    pub subtitle_has_caption: bool,
    pub subtitle_has_dim_label: bool,
    pub subtitle_layout: RecentRowChildLayout,
    pub age_visible: bool,
    pub age_halign: gtk4::Align,
    pub age_valign: gtk4::Align,
    pub age_has_caption: bool,
    pub age_has_dim_label: bool,
    pub age_layout: RecentRowChildLayout,
    pub remove_icon_name: Option<String>,
    pub remove_tooltip: Option<String>,
    pub remove_halign: gtk4::Align,
    pub remove_valign: gtk4::Align,
    pub remove_has_flat: bool,
    pub remove_has_circular: bool,
    pub remove_layout: RecentRowChildLayout,
}

/// Build a snapshot of the row skeleton before recycled rows bind data.
#[cfg(feature = "test-utils")]
pub(super) fn row_layout_snapshot() -> RecentRowLayoutSnapshot {
    let row = build_recent_row_widgets();
    RecentRowLayoutSnapshot {
        grid_margin_top: row.grid.margin_top(),
        grid_margin_bottom: row.grid.margin_bottom(),
        grid_margin_start: row.grid.margin_start(),
        grid_margin_end: row.grid.margin_end(),
        grid_row_spacing: row.grid.row_spacing(),
        grid_column_spacing: row.grid.column_spacing(),
        grid_height_request: row.grid.height_request(),
        marker_hhomogeneous: row.marker_stack.is_hhomogeneous(),
        marker_vhomogeneous: row.marker_stack.is_vhomogeneous(),
        marker_layout: child_layout(&row.grid, &row.marker_stack),
        title_overflow: row.title.text_overflow(),
        title_xalign: row.title.xalign(),
        title_hexpand: row.title.hexpands(),
        title_layout: child_layout(&row.grid, &row.title),
        subtitle_overflow: row.subtitle.text_overflow(),
        subtitle_min_chars: row.subtitle.min_chars(),
        subtitle_nat_chars: row.subtitle.nat_chars(),
        subtitle_min_lines: row.subtitle.min_lines(),
        subtitle_nat_lines: row.subtitle.nat_lines(),
        subtitle_has_caption: row.subtitle.has_css_class("caption"),
        subtitle_has_dim_label: row.subtitle.has_css_class("dim-label"),
        subtitle_layout: child_layout(&row.grid, &row.subtitle),
        age_visible: row.age.property::<bool>("visible"),
        age_halign: row.age.halign(),
        age_valign: row.age.valign(),
        age_has_caption: row.age.has_css_class("caption"),
        age_has_dim_label: row.age.has_css_class("dim-label"),
        age_layout: child_layout(&row.grid, &row.age),
        remove_icon_name: row.remove.icon_name().map(Into::into),
        remove_tooltip: row.remove.tooltip_text().map(Into::into),
        remove_halign: row.remove.halign(),
        remove_valign: row.remove.valign(),
        remove_has_flat: row.remove.has_css_class("flat"),
        remove_has_circular: row.remove.has_css_class("circular"),
        remove_layout: child_layout(&row.grid, &row.remove),
    }
}

/// Build the recycled `ListView` row factory and wire row focus, keys, removal, and path tooltips.
///
/// `unbind` clears row-level tooltips so GTK row recycling cannot leak a
/// previous document path into later filtered results.
pub(super) fn build_recent_row_factory(
    owner: &super::LushtextOpenPopover,
) -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    let obj_weak = owner.downgrade();

    factory.connect_setup(move |_, list_item| {
        let list_item = list_item
            .downcast_ref::<gtk4::ListItem>()
            .expect("ListItem");
        list_item.set_focusable(false);
        let row = build_recent_row_widgets();
        let focus = gtk4::EventControllerFocus::new();
        let list_item_weak = list_item.downgrade();
        let obj_weak_for_focus = obj_weak.clone();
        focus.connect_enter(move |_| {
            let Some(obj) = obj_weak_for_focus.upgrade() else {
                return;
            };
            let Some(list_item) = list_item_weak.upgrade() else {
                return;
            };
            obj.imp().sync_keyboard_position_from_focus(&list_item);
        });
        row.grid.add_controller(focus);

        let row_keys = gtk4::EventControllerKey::new();
        row_keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let list_item_weak = list_item.downgrade();
        let obj_weak_for_keys = obj_weak.clone();
        row_keys.connect_key_pressed(move |_, keyval, _, _| {
            let Some(obj) = obj_weak_for_keys.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let Some(list_item) = list_item_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            obj.imp().handle_focused_row_key(&list_item, keyval)
        });
        row.grid.add_controller(row_keys);

        accessibility::set_label(&row.remove, "Remove recent document");
        let list_item_weak = list_item.downgrade();
        let obj_weak = obj_weak.clone();
        row.remove.connect_clicked(move |_| {
            let Some(obj) = obj_weak.upgrade() else {
                return;
            };
            let Some(list_item) = list_item_weak.upgrade() else {
                return;
            };
            let Some(item) = list_item.item().and_downcast::<OpenPopoverItem>() else {
                return;
            };
            obj.imp().remove_recent(item.path());
        });

        list_item.set_child(Some(&row.grid));
    });

    let bind_obj_weak = owner.downgrade();
    factory.connect_bind(move |_, list_item| {
        let list_item = list_item
            .downcast_ref::<gtk4::ListItem>()
            .expect("ListItem");
        let item = list_item
            .item()
            .and_downcast::<OpenPopoverItem>()
            .expect("OpenPopoverItem");
        let grid = list_item
            .child()
            .and_downcast::<gtk4::Grid>()
            .expect("recent row grid");
        let title = grid
            .child_at(1, 0)
            .and_downcast::<gtk4::Inscription>()
            .expect("recent row title");
        let subtitle = grid
            .child_at(1, 1)
            .and_downcast::<gtk4::Inscription>()
            .expect("recent row subtitle");
        let age = grid
            .child_at(2, 1)
            .and_downcast::<gtk4::Inscription>()
            .expect("recent row age");
        let remove = grid
            .child_at(3, 0)
            .and_downcast::<gtk4::Button>()
            .expect("recent row remove button");

        let path_tooltip = item.path().display().to_string();
        set_recent_row_non_action_tooltip(&grid, Some(&path_tooltip));
        title.set_text(Some(&item.title()));
        subtitle.set_text(Some(&item.subtitle()));
        match item.age_label() {
            Some(label) if !label.is_empty() => {
                age.set_text(Some(&label));
                age.set_visible(true);
            }
            _ => age.set_visible(false),
        }
        let row_label = format!("Open recent document {}", item.title());
        let row_description = match item.age_label() {
            Some(age) if !age.is_empty() => format!("{}. {}", item.subtitle(), age),
            _ => item.subtitle(),
        };
        let position = i32::try_from(list_item.position()).unwrap_or(i32::MAX - 1) + 1;
        let set_size = bind_obj_weak.upgrade().map_or(position, |obj| {
            i32::try_from(obj.imp().rows_store.n_items()).unwrap_or(i32::MAX)
        });
        accessibility::apply_row_accessibility(
            &grid,
            RowAccessibility::new(&row_label)
                .description(&row_description)
                .position(position, set_size),
        );

        let bounded_path = accessibility::bounded_announcement_text(&path_tooltip, 160);
        accessibility::set_labelled_description(
            &remove,
            &format!("Remove {} from recent documents", item.title()),
            &format!("Keeps the file on disk at {bounded_path}"),
        );
    });

    factory.connect_unbind(|_, list_item| {
        let list_item = list_item
            .downcast_ref::<gtk4::ListItem>()
            .expect("ListItem");
        if let Some(grid) = list_item.child().and_downcast::<gtk4::Grid>() {
            set_recent_row_non_action_tooltip(&grid, None);
            accessibility::clear_row_accessibility(&grid);
            if let Some(remove) = grid.child_at(3, 0).and_downcast::<gtk4::Button>() {
                accessibility::set_label(&remove, "Remove recent document");
                accessibility::reset_property(&remove, gtk4::AccessibleProperty::Description);
            }
        }
    });

    factory
}

/// Build the GNOME Text Editor recent-row skeleton.
///
/// The marker stack and hidden age cell are present even though LushText does
/// not currently show modified markers or ages for every row; source parity
/// depends on preserving those columns so text and close buttons align.
fn build_recent_row_widgets() -> RecentRowWidgets {
    let grid = gtk4::Grid::builder()
        .row_spacing(3)
        .column_spacing(6)
        .focusable(true)
        .margin_top(3)
        .margin_bottom(3)
        .margin_start(0)
        .margin_end(6)
        .build();

    let marker_stack = gtk4::Stack::builder()
        .hhomogeneous(true)
        .vhomogeneous(true)
        .build();
    let empty = gtk4::Label::new(None);
    let is_modified = gtk4::Label::builder()
        .label("•")
        .halign(gtk4::Align::Center)
        .valign(gtk4::Align::Baseline)
        .build();
    marker_stack.add_child(&empty);
    marker_stack.add_child(&is_modified);
    grid.attach(&marker_stack, 0, 0, 1, 1);

    let title = gtk4::Inscription::builder()
        .xalign(0.0)
        .text_overflow(gtk4::InscriptionOverflow::EllipsizeMiddle)
        .hexpand(true)
        .build();
    grid.attach(&title, 1, 0, 2, 1);

    let subtitle = gtk4::Inscription::builder()
        .text_overflow(gtk4::InscriptionOverflow::EllipsizeEnd)
        .min_chars(25)
        .nat_chars(25)
        .hexpand(true)
        .min_lines(1)
        .nat_lines(1)
        .xalign(0.0)
        .build();
    subtitle.add_css_class("caption");
    subtitle.add_css_class("dim-label");
    grid.attach(&subtitle, 1, 1, 1, 1);

    let age = gtk4::Inscription::builder()
        .halign(gtk4::Align::End)
        .valign(gtk4::Align::Center)
        .visible(false)
        .build();
    age.add_css_class("caption");
    age.add_css_class("dim-label");
    grid.attach(&age, 2, 1, 1, 1);

    let remove = gtk4::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Remove")
        .halign(gtk4::Align::End)
        .valign(gtk4::Align::Center)
        .build();
    remove.add_css_class("flat");
    remove.add_css_class("circular");
    grid.attach(&remove, 3, 0, 1, 2);

    RecentRowWidgets {
        grid,
        marker_stack,
        title,
        subtitle,
        age,
        remove,
    }
}

/// Apply the document-path tooltip only to row surfaces that open the document.
///
/// The remove button keeps its own action tooltip, while GTK's recycled list
/// rows get refreshed or cleared every time their bound item changes.
fn set_recent_row_non_action_tooltip(grid: &gtk4::Grid, tooltip: Option<&str>) {
    grid.set_tooltip_text(tooltip);
    // These coordinates mirror build_recent_row_widgets and deliberately skip
    // the remove button at column 3 so it keeps the action tooltip.
    for (column, row) in [(0, 0), (1, 0), (1, 1), (2, 1)] {
        if let Some(child) = grid.child_at(column, row) {
            child.set_tooltip_text(tooltip);
        }
    }
}

#[cfg(feature = "test-utils")]
fn child_layout(grid: &gtk4::Grid, child: &impl IsA<gtk4::Widget>) -> RecentRowChildLayout {
    let (column, row, column_span, row_span) = grid.query_child(child);
    RecentRowChildLayout {
        column,
        row,
        column_span,
        row_span,
    }
}
