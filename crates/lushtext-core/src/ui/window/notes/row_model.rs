// SPDX-License-Identifier: GPL-3.0-or-later

//! Called presentation surface: the notes browser's lazy list and section model.
//!
//! # Role
//!
//! This module carries **no role**. It projects one query result of
//! `WFR-NOTES-BOOKMARKS` onto a `GListModel` + `GtkSectionModel` that the
//! browser's `GtkListView` reads (`bound-notes-browser-by-budget` D2). Every
//! decision it renders comes from `policy` (`group_note_rows`,
//! `NoteSectionBounds`).
//!
//! # Why it is lazy
//!
//! `item(position)` builds one small [`NotesBrowserRow`] when GTK asks for it,
//! and GTK asks only for the realized range, the selected item and an activated
//! one (ledger A93). Publishing a projection swaps one `Rc<[u32]>`, retargets
//! the row objects GTK holds below the shorter length in place (their
//! `content-changed` signal rebinds them), and emits `items-changed` for the
//! tail alone: a removal or an addition, never one replacing `items-changed`,
//! which makes GTK look up every added row (A93). Publication therefore costs
//! the same at 512 rows as at 131,072, where a `GListStore` of row objects
//! would create one GObject per row on GTK.
//!
//! # Ownership
//!
//! The model holds **no reference to the source**: the source is a
//! `DisposalOwned` payload whose final drop must run on a disposal worker, and a
//! model reference could make its last drop land on GTK. A row carries only
//! its source index and the source generation; the item factory reads the
//! entry through the browser state after checking that generation.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use crate::model::palette::PaletteNoteCategory;

use super::policy::{GroupedNoteRows, NoteSectionBounds};

mod row_imp {
    use super::{Cell, PaletteNoteCategory, glib};
    use gtk4::subclass::prelude::*;

    /// One listed row: a source index, its source generation and its category.
    #[derive(Default)]
    pub struct NotesBrowserRow {
        pub(super) source_index: Cell<u32>,
        pub(super) generation: Cell<u64>,
        pub(super) category: Cell<Option<PaletteNoteCategory>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NotesBrowserRow {
        const NAME: &'static str = "LushtextNotesBrowserRow";
        type Type = super::NotesBrowserRow;
    }

    impl ObjectImpl for NotesBrowserRow {
        fn signals() -> &'static [glib::subclass::Signal] {
            use std::sync::OnceLock;
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![glib::subclass::Signal::builder("content-changed").build()])
        }
    }
}

glib::wrapper! {
    /// One row of the notes browser list, created on demand.
    pub struct NotesBrowserRow(ObjectSubclass<row_imp::NotesBrowserRow>);
}

impl NotesBrowserRow {
    fn new(source_index: u32, generation: u64, category: Option<PaletteNoteCategory>) -> Self {
        let row: Self = glib::Object::new();
        row.imp().source_index.set(source_index);
        row.imp().generation.set(generation);
        row.imp().category.set(category);
        row
    }

    /// Index of the row's entry in the browser source.
    #[must_use]
    pub fn source_index(&self) -> u32 {
        self.imp().source_index.get()
    }

    /// Source generation the row was listed under.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.imp().generation.get()
    }

    /// Category of the row's section.
    #[must_use]
    pub fn category(&self) -> Option<PaletteNoteCategory> {
        self.imp().category.get()
    }

    /// Point this row at another entry and tell its bound widgets to project
    /// it again (`content-changed`).
    fn retarget(&self, source_index: u32, generation: u64, category: Option<PaletteNoteCategory>) {
        self.imp().source_index.set(source_index);
        self.imp().generation.set(generation);
        self.imp().category.set(category);
        self.emit_by_name::<()>("content-changed", &[]);
    }

    /// Run `f` whenever the row is retargeted; the caller disconnects it.
    pub fn connect_content_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("content-changed", false, move |values| {
            if let Some(row) = values.first().and_then(|value| value.get::<Self>().ok()) {
                f(&row);
            }
            None
        })
    }
}

/// Entries in the map of handed-out row objects past which dead ones are
/// pruned; GTK holds about 205 (A1) plus the selection, so the live set stays
/// far below it.
///
/// Kind: cap; a power of two, rounded down.
const LIVE_ROW_PRUNE_AT: usize = 1_024;

/// One published projection: grouped source indexes and their sections.
#[derive(Clone, Debug, Default)]
struct NotesBrowserProjection {
    rows: Rc<[u32]>,
    sections: NoteSectionBounds,
    generation: u64,
}

mod model_imp {
    use super::{NotesBrowserProjection, RefCell, gio, glib};
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct NotesBrowserRowModel {
        pub(super) projection: RefCell<NotesBrowserProjection>,
        /// While a first publish grows (`lighten-notes-browser-rows` D5), the
        /// rows listed so far; `None` once every projected row is listed.
        pub(super) listed: std::cell::Cell<Option<u32>>,
        pub(super) live:
            RefCell<std::collections::HashMap<u32, glib::WeakRef<super::NotesBrowserRow>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NotesBrowserRowModel {
        const NAME: &'static str = "LushtextNotesBrowserRowModel";
        type Type = super::NotesBrowserRowModel;
        type Interfaces = (gio::ListModel, gtk4::SectionModel);
    }

    impl ObjectImpl for NotesBrowserRowModel {}

    impl ListModelImpl for NotesBrowserRowModel {
        fn item_type(&self) -> glib::Type {
            super::NotesBrowserRow::static_type()
        }

        fn n_items(&self) -> u32 {
            self.listed_len()
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            let listed = self.listed_len();
            let projection = self.projection.borrow();
            // While a shrinking publish announces its removal, `rows` still
            // holds the old rows past the new length, and a growing first
            // publish projects rows it does not list yet: answer by the
            // reported length, as `GListModel` requires.
            if position >= listed {
                return None;
            }
            let source_index = *projection.rows.get(usize::try_from(position).ok()?)?;
            let mut live = self.live.borrow_mut();
            if let Some(row) = live.get(&position).and_then(glib::WeakRef::upgrade) {
                return Some(row.upcast());
            }
            if live.len() >= super::LIVE_ROW_PRUNE_AT {
                live.retain(|_, row| row.upgrade().is_some());
            }
            let row = super::NotesBrowserRow::new(
                source_index,
                projection.generation,
                projection.sections.category_at(position),
            );
            live.insert(position, row.downgrade());
            Some(row.upcast())
        }
    }

    impl NotesBrowserRowModel {
        /// The rows GTK is told about: the listed rows of a growing first
        /// publish, else every projected row.
        pub(super) fn listed_len(&self) -> u32 {
            let projected = self.projection.borrow().sections.len();
            self.listed
                .get()
                .map_or(projected, |listed| listed.min(projected))
        }
    }

    impl SectionModelImpl for NotesBrowserRowModel {
        fn section(&self, position: u32) -> (u32, u32) {
            let listed = self.listed_len();
            let projection = self.projection.borrow();
            // GtkSectionModel's contract past the end: one range from the item
            // count to G_MAXUINT; a section is cut at the listed rows.
            if position >= listed {
                return (listed, u32::MAX);
            }
            projection
                .sections
                .section_at(position)
                .map_or((listed, u32::MAX), |(start, end)| (start, end.min(listed)))
        }
    }
}

glib::wrapper! {
    /// The browser list's model: a lazy `GListModel` with `GtkSectionModel`
    /// sections, one per non-empty `PaletteNoteCategory`.
    pub struct NotesBrowserRowModel(ObjectSubclass<model_imp::NotesBrowserRowModel>)
        @implements gio::ListModel, gtk4::SectionModel;
}

impl Default for NotesBrowserRowModel {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl NotesBrowserRowModel {
    /// Replace the listed rows with one grouped projection: a removal and an
    /// addition, each answered around the realized rows only (A93).
    ///
    /// A publish into an empty list is **staged** (`lighten-notes-browser-rows`
    /// D5): it lists `list_budget::staged_first_rows` rows, and
    /// [`NotesBrowserRowModel::grow`] lists the rest over the next frames,
    /// so the frame that first shows the rows realizes the rows on screen
    /// rather than GTK's whole band (A1). Any other publish ends a growth.
    pub fn publish(&self, grouped: GroupedNoteRows, generation: u64) {
        let new_len = grouped.sections.len();
        let new = NotesBrowserProjection {
            rows: Rc::from(grouped.rows),
            sections: grouped.sections,
            generation,
        };
        let old_len = self.len();
        if old_len == 0 && new_len > 0 {
            let first = crate::model::list_budget::staged_first_rows(new_len);
            let _ = self.replace_projection(new);
            self.imp().listed.set((first < new_len).then_some(first));
            self.imp().live.borrow_mut().clear();
            self.items_changed(0, 0, first);
            return;
        }
        // The rows GTK knows are `old_len`; from here every projected row is
        // listed.
        self.freeze_growth();
        let kept = old_len.min(new_len);
        // Positions both projections list keep their row objects: the rows
        // GTK holds (its realized range, about 205, A1, and the selection)
        // are retargeted in place and rebind through `content-changed`, so a
        // republish neither tears down nor rebuilds their widgets (every
        // replaced row object costs a widget teardown and a factory setup).
        // A position GTK holds no object for gets a fresh one when it asks.
        //
        // Every emission matches what the model reports while GTK handles it
        // (its list item manager reads the model inside the handler, and a
        // length it did not expect crashed it): first the tail alone changes
        // length, removed while the model still lists the old rows cut to
        // the new length, or added once the model lists the new rows; then
        // the kept rows are retargeted and their sections announced. The tail
        // is never one replacing `items-changed`, which makes GTK look up
        // every added row (A93).
        if old_len > new_len {
            {
                let mut projection = self.imp().projection.borrow_mut();
                let sections = projection.sections.truncated(new_len);
                projection.sections = sections;
            }
            self.items_changed(new_len, old_len - new_len, 0);
            let _ = self.replace_projection(new);
        } else {
            let _ = self.replace_projection(new);
            if new_len > old_len {
                self.items_changed(old_len, 0, new_len - old_len);
            }
        }
        self.retarget_live_rows(kept, new_len);
        if kept != 0 {
            self.sections_changed(0, new_len);
        }
    }

    /// Retarget every live row object below `kept` to the new projection, and
    /// forget the ones at or past `len`. Rows in `kept..len` were handed out
    /// during this publish's addition, already pointing at the new rows.
    fn retarget_live_rows(&self, kept: u32, len: u32) {
        // Collect first: retargeting emits `content-changed`, whose handlers
        // read the model.
        let rows: Vec<(u32, NotesBrowserRow)> = {
            let mut live = self.imp().live.borrow_mut();
            live.retain(|position, row| *position < len && row.upgrade().is_some());
            live.iter()
                .filter(|(position, _)| **position < kept)
                .filter_map(|(position, row)| row.upgrade().map(|row| (*position, row)))
                .collect()
        };
        for (position, row) in rows {
            let target = {
                let projection = self.imp().projection.borrow();
                usize::try_from(position)
                    .ok()
                    .and_then(|index| projection.rows.get(index).copied())
                    .map(|source| {
                        (
                            source,
                            projection.generation,
                            projection.sections.category_at(position),
                        )
                    })
            };
            if let Some((source, generation, category)) = target {
                row.retarget(source, generation, category);
            }
        }
    }

    /// Swap the projection and return the previous row count.
    fn replace_projection(&self, projection: NotesBrowserProjection) -> u32 {
        let mut current = self.imp().projection.borrow_mut();
        let old_len = current.sections.len();
        *current = projection;
        old_len
    }

    /// List nothing.
    pub fn clear(&self) {
        self.publish(GroupedNoteRows::default(), self.generation());
    }

    /// List the next growth step of a staged first publish (one
    /// `items-changed` addition); `false` once every row is listed.
    pub fn grow(&self) -> bool {
        let Some(listed) = self.imp().listed.get() else {
            return false;
        };
        let projected = self.full_len();
        let Some(next) = crate::model::list_budget::next_growth(listed, projected) else {
            self.imp().listed.set(None);
            return false;
        };
        self.imp().listed.set((next < projected).then_some(next));
        self.items_changed(listed, 0, next - listed);
        next < projected
    }

    /// List every remaining row of a growing first publish at once (a key
    /// that moves the selection, an index past the listed rows).
    pub fn finish_growth(&self) {
        let Some(listed) = self.imp().listed.take() else {
            return;
        };
        let projected = self.full_len();
        if projected > listed {
            self.items_changed(listed, 0, projected - listed);
        }
    }

    /// Rows a growing first publish still has to list (0 once listed).
    #[must_use]
    pub fn growing_rows(&self) -> u32 {
        self.full_len() - self.len()
    }

    /// Rows the published projection holds, listed or still growing.
    #[must_use]
    pub fn full_len(&self) -> u32 {
        self.imp().projection.borrow().sections.len()
    }

    /// Rows listed.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.imp().listed_len()
    }

    /// Whether nothing is listed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Source generation of the listed rows.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.imp().projection.borrow().generation
    }

    /// End a growth where it stands: the projection keeps only the listed
    /// rows (nothing GTK knows changes, so nothing is emitted).
    fn freeze_growth(&self) {
        if let Some(listed) = self.imp().listed.take() {
            let mut projection = self.imp().projection.borrow_mut();
            let sections = projection.sections.truncated(listed);
            projection.sections = sections;
        }
    }

    /// Stop listing `count` rows from `start`, as one removal; the held row
    /// objects keep their entries and those past the range move down with
    /// their positions (a retirement, `lighten-notes-browser-rows` D4).
    fn cut_range(&self, start: u32, count: u32) {
        self.freeze_growth();
        let old_len = self.len();
        let start = start.min(old_len);
        let count = count.min(old_len - start);
        if count == 0 {
            return;
        }
        {
            let mut projection = self.imp().projection.borrow_mut();
            let (from, to) = (
                usize::try_from(start).unwrap_or(usize::MAX),
                usize::try_from(start + count).unwrap_or(usize::MAX),
            );
            let rows: Rc<[u32]> = projection
                .rows
                .get(..from)
                .unwrap_or_default()
                .iter()
                .chain(projection.rows.get(to..).unwrap_or_default())
                .copied()
                .collect();
            projection.rows = rows;
            let sections = projection.sections.without_range(start, count);
            projection.sections = sections;
        }
        {
            let mut live = self.imp().live.borrow_mut();
            let shifted = live
                .drain()
                .filter(|(position, _)| *position < start || *position >= start + count)
                .map(|(position, row)| {
                    (
                        if position >= start + count {
                            position - count
                        } else {
                            position
                        },
                        row,
                    )
                })
                .collect();
            *live = shifted;
        }
        self.items_changed(start, count, 0);
    }

    /// Source index listed at `position`, without creating a row object.
    #[must_use]
    pub fn source_index_at(&self, position: u32) -> Option<u32> {
        let projection = self.imp().projection.borrow();
        projection
            .rows
            .get(usize::try_from(position).ok()?)
            .copied()
    }

    /// Position of `source_index` in the listed rows: one linear scan over a
    /// `u32` slice (`bound-notes-browser-by-budget` D2; measured in task 1.1).
    #[must_use]
    pub fn position_of(&self, source_index: u32) -> Option<u32> {
        let projection = self.imp().projection.borrow();
        projection
            .rows
            .iter()
            .position(|index| *index == source_index)
            .and_then(|position| u32::try_from(position).ok())
    }
}

impl crate::ui::list_retirement::RetiringRows for NotesBrowserRowModel {
    fn rows_len(&self) -> u32 {
        self.len()
    }

    fn held_positions(&self) -> Vec<u32> {
        crate::ui::list_retirement::held_positions(&self.imp().live.borrow())
    }

    fn remove_range(&self, start: u32, count: u32) {
        self.cut_range(start, count);
    }

    fn release(&self) {
        self.imp().listed.set(None);
        let generation = self.generation();
        *self.imp().projection.borrow_mut() = NotesBrowserProjection {
            generation,
            ..NotesBrowserProjection::default()
        };
        self.imp().live.borrow_mut().clear();
    }
}

impl crate::ui::list_growth::GrowingRows for NotesBrowserRowModel {
    fn grow(&self) -> bool {
        Self::grow(self)
    }
}
