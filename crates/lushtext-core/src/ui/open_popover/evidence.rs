// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — the recent-documents workflow's single observable
//! state, at the workflow's canonical role home.
//!
//! # Two accessors, because the workflow spans two GObjects
//!
//! This workflow has a **nested** role home: the canonical home is
//! `ui/open_popover/`, and `ui/window/` holds its journal and its window-side
//! presentation surface. Its state therefore lives on two objects that are
//! separately observable — a bare `LushtextOpenPopover` exists in widget tests
//! with no window at all — so the surface exposes one accessor per owning
//! object rather than one accessor that could only be called from half of them:
//!
//! * [`open_popover_evidence`] — what the popover currently projects;
//! * [`recent_documents_journal_evidence`] — the durable history the window
//!   holds in memory, and the state of its persistence.
//!
//! This is the shape `WFR-WORKSPACE-TREE` established for a nested role home
//! (`workspace_tree_evidence` / `workspace_section_evidence`), and it is named
//! here so the plural is read as the precedent it follows rather than as a
//! relapse into scattered getters. Each accessor still reads its object's
//! **whole** surface; neither is a per-field getter.
//!
//! One accessor replaces **fourteen** separate `*_for_test` inspection
//! functions, which was the largest such population of any shell surface. Most
//! of them read one scroller property each — the geometry contract that keeps a
//! dense recent list scrolling instead of growing the popover — and reading them
//! one at a time is how a test comes to assert five of the seven properties that
//! matter.
//!
//! * **Reading must not mutate.** Every field is a widget property read, a store
//!   count, or a pure derivation. The two `set_*_for_test` seams that *do*
//!   mutate — the search text and the scroll position — are **actuation** and
//!   stay outside this surface; folding a setter in would make reading change
//!   what it reports.
//! * **No field may be read from inside a mutable borrow.** This surface takes
//!   no `RefCell` borrow of workflow state at all.
//! * **A disposed widget is a stage.** Every template child is reached through
//!   `try_get()`, and a cleared child yields the type's neutral answer rather
//!   than panicking.
//!
//! **Materialization: this surface constructs nothing.** `rows_store` is a
//! `gio::ListStore` holding already-built `OpenPopoverItem`s; reading its count
//! and its items creates nothing, and every other field is a widget-property
//! read or a scalar.
//!
//! An earlier draft carried a `row_layout` field built by
//! `build_recent_row_widgets()` — **eight GTK widgets constructed on every
//! read** of a surface whose whole contract is that reading is free. It was
//! defensible under the letter of the rule (it registered nothing and was
//! deterministic) and wrong under its intent, and it earned nothing: the row
//! template is a **class constant**, identical for every popover and every read.
//! It is now reached directly through
//! `imp::LushtextOpenPopover::row_layout_snapshot()` by the one test that
//! asserts GNOME source parity, so no per-read cost is paid by the surface's
//! other eleven readers.

use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;

use crate::model::recent_document::RecentDocumentEntry;
use crate::ui::window::LushtextWindow;

use super::item::OpenPopoverItem;
use super::{LushtextOpenPopover, imp};

/// Grid placement of one child inside the GNOME-shaped recent row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenPopoverRowChildLayoutSnapshot {
    /// GtkGrid column occupied by the child.
    pub column: i32,
    /// GtkGrid row occupied by the child.
    pub row: i32,
    /// Number of GtkGrid columns spanned by the child.
    pub column_span: i32,
    /// Number of GtkGrid rows spanned by the child.
    pub row_span: i32,
}

/// Source-contract snapshot for the GNOME-shaped recent row.
///
/// This is not a public styling API. It keeps LushText's row skeleton aligned
/// with the GNOME Text Editor source constants without poking through GTK's
/// recycled list rows.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenPopoverRowLayoutSnapshot {
    /// Top margin on the row grid.
    pub grid_margin_top: i32,
    /// Bottom margin on the row grid.
    pub grid_margin_bottom: i32,
    /// Start margin on the row grid.
    pub grid_margin_start: i32,
    /// End margin on the row grid.
    pub grid_margin_end: i32,
    /// Vertical spacing between title and subtitle rows.
    pub grid_row_spacing: u32,
    /// Horizontal spacing between marker, text, age, and remove columns.
    pub grid_column_spacing: u32,
    /// Height request; GNOME's row does not force a fixed row height.
    pub grid_height_request: i32,
    /// Whether the leading marker/spacer stack is horizontally homogeneous.
    pub marker_hhomogeneous: bool,
    /// Whether the leading marker/spacer stack is vertically homogeneous.
    pub marker_vhomogeneous: bool,
    /// Grid position for the leading marker/spacer stack.
    pub marker_layout: OpenPopoverRowChildLayoutSnapshot,
    /// Title overflow mode.
    pub title_overflow: gtk4::InscriptionOverflow,
    /// Title x alignment.
    pub title_xalign: f32,
    /// Whether title can take remaining horizontal room.
    pub title_hexpand: bool,
    /// Grid position for the title.
    pub title_layout: OpenPopoverRowChildLayoutSnapshot,
    /// Subtitle overflow mode.
    pub subtitle_overflow: gtk4::InscriptionOverflow,
    /// Subtitle minimum character width.
    pub subtitle_min_chars: u32,
    /// Subtitle natural character width.
    pub subtitle_nat_chars: u32,
    /// Subtitle minimum line count.
    pub subtitle_min_lines: u32,
    /// Subtitle natural line count.
    pub subtitle_nat_lines: u32,
    /// Whether subtitle carries GNOME's caption class.
    pub subtitle_has_caption: bool,
    /// Whether subtitle carries GNOME's dim-label class.
    pub subtitle_has_dim_label: bool,
    /// Grid position for the subtitle.
    pub subtitle_layout: OpenPopoverRowChildLayoutSnapshot,
    /// Whether the optional age inscription is visible before binding row data.
    pub age_visible: bool,
    /// Age horizontal alignment.
    pub age_halign: gtk4::Align,
    /// Age vertical alignment.
    pub age_valign: gtk4::Align,
    /// Whether age carries GNOME's caption class.
    pub age_has_caption: bool,
    /// Whether age carries GNOME's dim-label class.
    pub age_has_dim_label: bool,
    /// Grid position for the age inscription.
    pub age_layout: OpenPopoverRowChildLayoutSnapshot,
    /// Icon used by the remove button.
    pub remove_icon_name: Option<String>,
    /// Tooltip used by the remove button.
    pub remove_tooltip: Option<String>,
    /// Remove button horizontal alignment.
    pub remove_halign: gtk4::Align,
    /// Remove button vertical alignment.
    pub remove_valign: gtk4::Align,
    /// Whether remove carries GNOME's flat class.
    pub remove_has_flat: bool,
    /// Whether remove carries GNOME's circular class.
    pub remove_has_circular: bool,
    /// Grid position for the remove button.
    pub remove_layout: OpenPopoverRowChildLayoutSnapshot,
}

impl From<imp::RecentRowChildLayout> for OpenPopoverRowChildLayoutSnapshot {
    fn from(value: imp::RecentRowChildLayout) -> Self {
        Self {
            column: value.column,
            row: value.row,
            column_span: value.column_span,
            row_span: value.row_span,
        }
    }
}

impl From<imp::RecentRowLayoutSnapshot> for OpenPopoverRowLayoutSnapshot {
    fn from(value: imp::RecentRowLayoutSnapshot) -> Self {
        Self {
            grid_margin_top: value.grid_margin_top,
            grid_margin_bottom: value.grid_margin_bottom,
            grid_margin_start: value.grid_margin_start,
            grid_margin_end: value.grid_margin_end,
            grid_row_spacing: value.grid_row_spacing,
            grid_column_spacing: value.grid_column_spacing,
            grid_height_request: value.grid_height_request,
            marker_hhomogeneous: value.marker_hhomogeneous,
            marker_vhomogeneous: value.marker_vhomogeneous,
            marker_layout: value.marker_layout.into(),
            title_overflow: value.title_overflow,
            title_xalign: value.title_xalign,
            title_hexpand: value.title_hexpand,
            title_layout: value.title_layout.into(),
            subtitle_overflow: value.subtitle_overflow,
            subtitle_min_chars: value.subtitle_min_chars,
            subtitle_nat_chars: value.subtitle_nat_chars,
            subtitle_min_lines: value.subtitle_min_lines,
            subtitle_nat_lines: value.subtitle_nat_lines,
            subtitle_has_caption: value.subtitle_has_caption,
            subtitle_has_dim_label: value.subtitle_has_dim_label,
            subtitle_layout: value.subtitle_layout.into(),
            age_visible: value.age_visible,
            age_halign: value.age_halign,
            age_valign: value.age_valign,
            age_has_caption: value.age_has_caption,
            age_has_dim_label: value.age_has_dim_label,
            age_layout: value.age_layout.into(),
            remove_icon_name: value.remove_icon_name,
            remove_tooltip: value.remove_tooltip,
            remove_halign: value.remove_halign,
            remove_valign: value.remove_valign,
            remove_has_flat: value.remove_has_flat,
            remove_has_circular: value.remove_has_circular,
            remove_layout: value.remove_layout.into(),
        }
    }
}

/// Everything a test or probe may observe about the recent-documents popover.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenPopoverEvidence {
    /// Rows currently in the visible model, after open-tab exclusion and search.
    pub visible_row_count: u32,
    /// Titles of those rows, in visible order.
    ///
    /// Bounded by the visible row count, which the workflow already caps.
    pub visible_titles: Vec<String>,
    /// Whether the list page is showing rather than the empty-state page.
    pub list_visible: bool,
    /// The empty-state title currently rendered.
    ///
    /// Distinguishes an empty history from an excluded filter — the two states
    /// `policy::empty_state_copy` chooses between.
    pub empty_title: String,
    /// The scroller geometry contract that keeps a dense list bounded.
    pub scroller: OpenPopoverScrollerGeometry,
    /// Whether the recent list uses a non-selecting selection model.
    pub uses_no_selection: bool,
    /// Synthetic keyboard row position, when one is active.
    pub keyboard_row_position: Option<u32>,
    /// The filter text currently in the search entry.
    ///
    /// On the surface rather than read through the entry widget, so a test
    /// asserting "the query was cleared" reads the workflow's state instead of
    /// reaching into a template child for it.
    pub query: String,
}

/// The scrolled-window contract for the recent list.
///
/// Grouped rather than flattened into the parent because these seven properties
/// are one contract — *the item region scrolls, the shell does not grow* — and a
/// test that asserts some of them has not asserted it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenPopoverScrollerGeometry {
    /// Maximum content height before the list scrolls.
    pub max_content_height: i32,
    /// Minimum content width.
    pub min_content_width: i32,
    /// Maximum content width.
    pub max_content_width: i32,
    /// Whether natural height propagates to the popover.
    pub propagates_natural_height: bool,
    /// Whether natural width propagates; must stay false so a long path cannot
    /// widen the popover.
    pub propagates_natural_width: bool,
    /// Horizontal scrollbar policy; a horizontal scrollbar here is a defect.
    pub hscrollbar_policy: gtk4::PolicyType,
    /// Current vertical scroll offset.
    pub scroll_value: f64,
}

/// Read the whole recent-documents popover surface.
#[must_use]
pub fn open_popover_evidence(popover: &LushtextOpenPopover) -> OpenPopoverEvidence {
    let imp = popover.imp();

    let visible_row_count = imp.rows_store.n_items();
    let visible_titles = (0..visible_row_count)
        .filter_map(|position| {
            imp.rows_store
                .item(position)
                .and_downcast::<OpenPopoverItem>()
                .map(|item| item.title())
        })
        .collect();

    OpenPopoverEvidence {
        visible_row_count,
        visible_titles,
        list_visible: list_page_visible(popover),
        empty_title: imp
            .empty_title
            .try_get()
            .map(|label| label.label().into())
            .unwrap_or_default(),
        scroller: scroller_geometry(popover),
        // `list_view` is a `TemplateChild`. GTK4 clears template children in
        // `dispose()`, before Rust's `Drop`, and the panicking `Deref` accessor
        // would turn a teardown observation into a crash — the exact trap this
        // row's disposal proof exists to pin.
        uses_no_selection: imp
            .list_view
            .try_get()
            .and_then(|view| view.model())
            .and_downcast::<gtk4::NoSelection>()
            .is_some(),
        keyboard_row_position: imp.keyboard_row_position.get(),
        query: imp
            .search_entry
            .try_get()
            .map(|entry| entry.text().to_string())
            .unwrap_or_default(),
    }
}

/// Whether the stack currently shows the list rather than the empty state.
fn list_page_visible(popover: &LushtextOpenPopover) -> bool {
    let imp = popover.imp();
    let (Some(stack), Some(scroller)) = (imp.stack.try_get(), imp.recent_scroller.try_get()) else {
        return false;
    };
    stack
        .visible_child()
        .as_ref()
        .is_some_and(|child| child.as_ptr() == scroller.upcast_ref::<gtk4::Widget>().as_ptr())
}

/// The scroller's bounded-geometry contract, or neutral values once disposed.
fn scroller_geometry(popover: &LushtextOpenPopover) -> OpenPopoverScrollerGeometry {
    let Some(scroller) = popover.imp().recent_scroller.try_get() else {
        return OpenPopoverScrollerGeometry {
            max_content_height: 0,
            min_content_width: 0,
            max_content_width: 0,
            propagates_natural_height: false,
            propagates_natural_width: false,
            hscrollbar_policy: gtk4::PolicyType::Never,
            scroll_value: 0.0,
        };
    };
    OpenPopoverScrollerGeometry {
        max_content_height: scroller.max_content_height(),
        min_content_width: scroller.min_content_width(),
        max_content_width: scroller.max_content_width(),
        propagates_natural_height: scroller.propagates_natural_height(),
        propagates_natural_width: scroller.propagates_natural_width(),
        hscrollbar_policy: scroller.hscrollbar_policy(),
        scroll_value: scroller.vadjustment().value(),
    }
}

/// The durable recent-document history the window holds, and its persistence.
///
/// This is the journal half of the workflow's surface. Its fields are a clone of
/// the in-memory entry list plus six scalars; reading it takes shared borrows
/// only, releases each before the struct literal is built, and advances nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentDocumentsJournalEvidence {
    /// Newest-first entries currently held in memory.
    ///
    /// Bounded by the journal's own retention rule, which the service applies
    /// before the list ever reaches the window.
    pub entries: Vec<RecentDocumentEntry>,
    /// Whether the startup load is still in flight.
    pub loading: bool,
    /// Paths removed by the user while that load was in flight, which the
    /// completion must subtract from what it read off disk.
    pub removed_while_loading: Vec<std::path::PathBuf>,
    /// Monotonic version advanced by in-memory user mutations. A load whose
    /// generation no longer matches must merge rather than replace.
    pub generation: u64,
    /// Whether the popover projection is stale and owes a rebuild before popup.
    pub rows_dirty: bool,
    /// Whether a save is running on a worker.
    pub save_inflight: bool,
    /// Whether another save is owed once the in-flight one finishes.
    pub save_pending: bool,
}

/// Read the whole recent-documents journal surface.
///
/// Every borrow taken here is released before the returned value is built, so
/// this accessor is callable from anywhere the window is reachable — including
/// from inside a callback that will later take a mutable borrow of the same
/// state.
#[must_use]
pub fn recent_documents_journal_evidence(
    window: &LushtextWindow,
) -> RecentDocumentsJournalEvidence {
    let state = &window.imp().recent_documents;
    let entries = state.entries.borrow().clone();
    let removed_while_loading = state.removed_while_loading.borrow().clone();
    RecentDocumentsJournalEvidence {
        entries,
        loading: state.loading.get(),
        removed_while_loading,
        generation: state.generation.get(),
        rows_dirty: state.rows_dirty.get(),
        save_inflight: state.save_inflight.get(),
        save_pending: state.save_pending.get(),
    }
}
