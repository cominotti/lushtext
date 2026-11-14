// SPDX-License-Identifier: GPL-3.0-or-later

//! Called presentation surface: the command palette's category view model.
//!
//! # Role
//!
//! This module carries **no role**. It projects one category ranking of
//! `WFR-COMMAND-PALETTE` (`drill-down-palette-categories` D2) onto a lazy
//! `GListModel` that the category view's `GtkListView` reads. Named in that
//! workflow's matrix row.
//!
//! # Why it is lazy
//!
//! A category can list every one of the index's 131,072 files. The model holds
//! the ranking as source ordinals (`Arc<[u32]>`, four bytes a row) and builds
//! one [`PaletteItem`] only when GTK asks for a position, which it does for the
//! realized range (about 205 rows, ledger A1), the selected item and an
//! activated one. Nothing here walks the rows: the list's length is the
//! ordinal slice's.
//!
//! # Publishing
//!
//! The pattern of the notes browser's `ui/window/notes/row_model.rs`
//! (`bound-notes-browser-by-budget`): a weak map of handed-out row objects by
//! position lets a publish **retarget** the live rows below the shorter length
//! in place (their `content-changed` signal makes the item factory rebind), so
//! a query neither tears down nor rebuilds the realized rows; only the tail is
//! removed or added, and every `items-changed` matches the length the model
//! reports while GTK handles it. Never one replacing `items-changed`, which
//! makes GTK look up every added row (A93).
//!
//! # Ownership
//!
//! A projection holds `Arc` clones of the request's source snapshots, so an
//! ordinal can never outlive the source it indexes. The palette already owns
//! the same `Arc`s; their final drop is `DisposalOwned`'s nonblocking handoff
//! to a disposal worker. Leaving the category view and closing the palette
//! [`PaletteCategoryModel::clear`] the projection, so a hidden model never keeps
//! a superseded index or note source alive.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use crate::model::palette::{PaletteCategory, PaletteFileEntry, PaletteNoteEntry};
use crate::services::palette::{FileIndex, all_commands};
use crate::ui::plain_disposal::DisposalOwned;

use super::item::PaletteItem;

/// Entries in the map of handed-out row objects past which dead ones are
/// pruned; GTK holds about 205 (A1) plus the selection, so the live set stays
/// far below it.
///
/// Kind: cap; a power of two, rounded down.
const CATEGORY_LIVE_ROW_PRUNE_AT: usize = 1_024;

/// The source snapshots a category projection's ordinals index.
#[derive(Clone)]
pub(super) struct CategorySources {
    pub index: Arc<DisposalOwned<FileIndex>>,
    pub open_tabs: Arc<[PaletteFileEntry]>,
    pub note_entries: Arc<DisposalOwned<Box<[PaletteNoteEntry]>>>,
}

/// One published ranking.
#[derive(Clone, Default)]
struct CategoryProjection {
    ordinals: Arc<[u32]>,
    category: Option<PaletteCategory>,
    sources: Option<CategorySources>,
}

impl CategoryProjection {
    fn len(&self) -> u32 {
        u32::try_from(self.ordinals.len()).unwrap_or(u32::MAX)
    }

    /// The row at `position`, built from its source entry with the grouped
    /// rows' own constructors.
    fn row_at(&self, position: u32) -> Option<PaletteItem> {
        let category = self.category?;
        let sources = self.sources.as_ref()?;
        let ordinal = usize::try_from(*self.ordinals.get(usize::try_from(position).ok()?)?).ok()?;
        let item = match category {
            PaletteCategory::OpenTabs => {
                let entry = sources.open_tabs.get(ordinal)?;
                PaletteItem::new_file_raw(
                    entry.display_name.clone(),
                    entry.subtitle.clone(),
                    entry.path.clone(),
                )
            }
            PaletteCategory::WorkspaceFiles => {
                PaletteItem::from_indexed_file(sources.index.files().get(ordinal)?)
            }
            PaletteCategory::Note(_) => {
                let entry = sources.note_entries.get(ordinal)?;
                PaletteItem::new_note_raw(
                    entry.title.clone(),
                    entry.display_subtitle(),
                    entry.target.clone(),
                )
            }
            PaletteCategory::Commands => {
                PaletteItem::from_command_def(all_commands().get(ordinal)?)
            }
        };
        item.set_category(Some(category));
        Some(item)
    }
}

mod model_imp {
    use super::{CategoryProjection, Cell, HashMap, PaletteItem, RefCell, gio, glib};
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct PaletteCategoryModel {
        pub(super) projection: RefCell<CategoryProjection>,
        /// While a first publish grows (`lighten-notes-browser-rows` D5), the
        /// rows listed so far; `None` once every ranked row is listed.
        pub(super) listed: Cell<Option<u32>>,
        pub(super) live: RefCell<HashMap<u32, glib::WeakRef<PaletteItem>>>,
        /// Row objects created on demand (evidence: GTK asks only around the
        /// realized rows).
        pub(super) item_requests: Cell<u64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PaletteCategoryModel {
        const NAME: &'static str = "LushtextPaletteCategoryModel";
        type Type = super::PaletteCategoryModel;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for PaletteCategoryModel {}

    impl PaletteCategoryModel {
        /// The rows GTK is told about.
        pub(super) fn listed_len(&self) -> u32 {
            let ranked = self.projection.borrow().len();
            self.listed
                .get()
                .map_or(ranked, |listed| listed.min(ranked))
        }
    }

    impl ListModelImpl for PaletteCategoryModel {
        fn item_type(&self) -> glib::Type {
            PaletteItem::static_type()
        }

        fn n_items(&self) -> u32 {
            self.listed_len()
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            let listed = self.listed_len();
            let projection = self.projection.borrow();
            // While a shrinking publish announces its removal the ordinals
            // still hold the old rows, and a growing first publish ranks rows
            // it does not list yet: answer by the reported length.
            if position >= listed {
                return None;
            }
            let mut live = self.live.borrow_mut();
            if let Some(row) = live.get(&position).and_then(glib::WeakRef::upgrade) {
                return Some(row.upcast());
            }
            if live.len() >= super::CATEGORY_LIVE_ROW_PRUNE_AT {
                live.retain(|_, row| row.upgrade().is_some());
            }
            let row = projection.row_at(position)?;
            self.item_requests
                .set(self.item_requests.get().saturating_add(1));
            live.insert(position, row.downgrade());
            Some(row.upcast())
        }
    }
}

glib::wrapper! {
    /// The category view's model: a lazy `GListModel` of one ranked category.
    pub struct PaletteCategoryModel(ObjectSubclass<model_imp::PaletteCategoryModel>)
        @implements gio::ListModel;
}

impl Default for PaletteCategoryModel {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PaletteCategoryModel {
    /// List `ordinals` of `category` over `sources`: the tail is removed or
    /// added, the kept rows GTK holds are retargeted in place.
    pub(super) fn publish(
        &self,
        ordinals: Arc<[u32]>,
        category: PaletteCategory,
        sources: CategorySources,
    ) {
        self.replace(CategoryProjection {
            ordinals,
            category: Some(category),
            sources: Some(sources),
        });
    }

    /// List nothing and release the source snapshots.
    pub(super) fn clear(&self) {
        self.replace(CategoryProjection::default());
        self.imp().live.borrow_mut().clear();
    }

    fn replace(&self, new: CategoryProjection) {
        let new_len = new.len();
        let old_len = self.len();
        // A publish into an empty view is staged (`lighten-notes-browser-rows`
        // D5): the frame after it (X1) realizes the first rows only, and the
        // view's row growth lists the rest over the next frames.
        if old_len == 0 && new_len > 0 {
            let first = crate::model::list_budget::staged_first_rows(new_len);
            *self.imp().projection.borrow_mut() = new;
            self.imp().listed.set((first < new_len).then_some(first));
            self.imp().live.borrow_mut().clear();
            self.items_changed(0, 0, first);
            return;
        }
        self.freeze_growth();
        let kept = old_len.min(new_len);
        // Every emission matches what the model reports while GTK handles it
        // (see the notes browser's row model): the tail alone changes length.
        if old_len > new_len {
            {
                let mut projection = self.imp().projection.borrow_mut();
                let cut: Arc<[u32]> = Arc::from(
                    &projection.ordinals[..usize::try_from(new_len).unwrap_or(usize::MAX)],
                );
                projection.ordinals = cut;
            }
            self.items_changed(new_len, old_len - new_len, 0);
            *self.imp().projection.borrow_mut() = new;
        } else {
            *self.imp().projection.borrow_mut() = new;
            if new_len > old_len {
                self.items_changed(old_len, 0, new_len - old_len);
            }
        }
        self.retarget_live_rows(kept, new_len);
    }

    /// Retarget every live row object below `kept` to the new projection, and
    /// forget the ones at or past `len`.
    fn retarget_live_rows(&self, kept: u32, len: u32) {
        // Collect first: retargeting emits `content-changed`, whose handlers
        // read the model.
        let rows: Vec<(u32, PaletteItem)> = {
            let mut live = self.imp().live.borrow_mut();
            live.retain(|position, row| *position < len && row.upgrade().is_some());
            live.iter()
                .filter(|(position, _)| **position < kept)
                .filter_map(|(position, row)| row.upgrade().map(|row| (*position, row)))
                .collect()
        };
        for (position, row) in rows {
            let fresh = self.imp().projection.borrow().row_at(position);
            if let Some(fresh) = fresh {
                row.retarget_from(&fresh);
            }
        }
    }

    /// Stop listing `count` rows from `start`, as one removal (a retirement,
    /// `lighten-notes-browser-rows` D4); held row objects past the range keep
    /// their entries and move down with their positions.
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
            let rest: Arc<[u32]> = projection
                .ordinals
                .get(..from)
                .unwrap_or_default()
                .iter()
                .chain(projection.ordinals.get(to..).unwrap_or_default())
                .copied()
                .collect();
            projection.ordinals = rest;
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

    /// End a growth where it stands: the ranking keeps only the listed rows
    /// (nothing GTK knows changes, so nothing is emitted).
    fn freeze_growth(&self) {
        if let Some(listed) = self.imp().listed.take() {
            let mut projection = self.imp().projection.borrow_mut();
            let cut: Arc<[u32]> = Arc::from(
                projection
                    .ordinals
                    .get(..usize::try_from(listed).unwrap_or(usize::MAX))
                    .unwrap_or_default(),
            );
            projection.ordinals = cut;
        }
    }

    /// List the next growth step of a staged first publish; `false` once
    /// every ranked row is listed.
    pub(super) fn grow(&self) -> bool {
        let Some(listed) = self.imp().listed.get() else {
            return false;
        };
        let ranked = self.full_len();
        let Some(next) = crate::model::list_budget::next_growth(listed, ranked) else {
            self.imp().listed.set(None);
            return false;
        };
        self.imp().listed.set((next < ranked).then_some(next));
        self.items_changed(listed, 0, next - listed);
        next < ranked
    }

    /// List every ranked row a growing first publish has not listed yet.
    pub(super) fn finish_growth(&self) {
        let Some(listed) = self.imp().listed.take() else {
            return;
        };
        let ranked = self.full_len();
        if ranked > listed {
            self.items_changed(listed, 0, ranked - listed);
        }
    }

    /// Ranked rows still to be listed by a growing first publish.
    #[must_use]
    pub fn growing_rows(&self) -> u32 {
        self.full_len() - self.len()
    }

    /// Rows the published ranking holds, listed or still growing.
    #[must_use]
    pub fn full_len(&self) -> u32 {
        self.imp().projection.borrow().len()
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

    /// Whether a projection holds source snapshots.
    #[must_use]
    pub(super) fn holds_sources(&self) -> bool {
        self.imp().projection.borrow().sources.is_some()
    }

    /// Row objects the model created on demand since it was built.
    #[must_use]
    pub fn item_requests(&self) -> u64 {
        self.imp().item_requests.get()
    }
}

impl crate::ui::list_retirement::RetiringRows for PaletteCategoryModel {
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
        self.clear();
    }
}

impl crate::ui::list_growth::GrowingRows for PaletteCategoryModel {
    fn grow(&self) -> bool {
        Self::grow(self)
    }
}
