// SPDX-License-Identifier: GPL-3.0-or-later

//! The detached path cache of a sliced collapse (`slice-large-directory-collapse`
//! design D24, task 10.15), part of the scan stage's `execution` role.
//!
//! A sliced collapse removes a directory's rows in steps. Each step used to
//! take the removed rows out of the section's path caches too
//! (`item_locations`, `visible_path_counts`, `child_paths`): a hash removal
//! and a `PathBuf` comparison per row, inside the step (design R11, round
//! Q2: at least one frame of a 131,072-entry collapse). Instead, the
//! collapse's intent **detaches** each of its directories' caches whole:
//!
//! - `child_paths[dir]` leaves the map in one removal, and its paths go to
//!   the listing retirement, which removes their entries in slices of
//!   `RETIRED_LISTING_ROWS_PER_UNIT` at `G_PRIORITY_LOW`;
//! - the directory stops owning its cache, so no step reaches the cache, nor
//!   `drain_child_rows_cache`'s consistency fallback;
//! - the section's cache epoch advances, and the directory records the last
//!   epoch its locations carry: every location under it cached at or before
//!   that epoch is **stale**. No reader returns a stale location
//!   ([`location_is_live`]), and a path whose only counted occurrence is
//!   stale counts as not shown ([`drop_stale_unique`]).
//!
//! # Duplicates
//!
//! `visible_path_counts` counts more than one occurrence of a path only when
//! overlapping workspace folders list it twice, and the only path listed by
//! two parents is a workspace folder listed as a child of another folder's
//! directory (a child cache has one owning store per directory). A subtree
//! with another workspace folder strictly inside it is therefore **not
//! detached** (it keeps the per-step splice, which keeps those counts exact),
//! and every path a detached cache held has a count of one. That is what
//! makes the stale rule enough: no count above one ever refers to a detached
//! occurrence.
//!
//! # Re-expansion before the retirement ends
//!
//! A re-expansion in the middle of the collapse refills the partly drained
//! store (design D7). Its remaining rows are cached again by an **adoption**:
//! append-only, from the store's mirror, in slices of
//! `RETIRED_LISTING_ROWS_PER_UNIT` rows while `turn_continues` holds, from a
//! `G_PRIORITY_DEFAULT_IDLE` source. The refill's scan result waits for the
//! adoption to end, so its first batch finds the caches exact and splices
//! them per batch as any load does (no whole-listing recache in one
//! dispatch). A location an adoption caches carries the new epoch, so the
//! retirement keeps it. If the store's mirror changes while the adoption runs
//! (a rename, a removal, a new item), the adoption stops: the partly cached
//! directory no longer matches the mirror, and the next batch's consistency
//! check rebuilds it, as for any drifted cache.

use super::super::LushtextWorkspaceSection;
use super::super::imp::ItemLocation;
use super::{
    RETIRED_LISTING_ROWS_PER_UNIT, RetiringListing, ScannedChildrenPlan, apply_scanned_children,
    child_mirror_generation, child_store_key, queue_listing_retirement,
};
use crate::model::list_budget::turn_continues;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One directory's detached cache on its way out: its sibling paths, and the
/// last epoch their locations carry.
pub(crate) struct DetachedCache {
    pub(super) parent: PathBuf,
    pub(super) epoch: u64,
    pub(super) paths: Vec<PathBuf>,
}

/// A re-expanded directory whose remaining rows are being cached again.
pub(crate) struct ChildCacheAdoption {
    store_key: usize,
    /// The store's mirror generation the adoption started from; any other
    /// generation stops it.
    mirror_generation: u64,
    /// The next mirror row to adopt.
    next: usize,
    /// The refill's scan result, held until the adoption ends.
    deferred: Option<ScannedChildrenPlan>,
}

/// Whether `location` is current: not under a directory whose cache was
/// detached at or after the epoch it was cached in.
#[must_use]
pub(super) fn location_is_live(location: &ItemLocation, detached: &HashMap<PathBuf, u64>) -> bool {
    location.parent_dir.as_ref().is_none_or(|parent| {
        detached
            .get(parent)
            .is_none_or(|stale_through| location.epoch > *stale_through)
    })
}

/// Forget `path`'s count and location when its only counted occurrence is a
/// stale one (a detached cache's row), so the caller counts it from zero.
/// Returns whether it did.
pub(super) fn drop_stale_unique(
    visible_path_counts: &mut HashMap<PathBuf, usize>,
    item_locations: &mut HashMap<PathBuf, ItemLocation>,
    detached: &HashMap<PathBuf, u64>,
    path: &Path,
) -> bool {
    if detached.is_empty() || visible_path_counts.get(path) != Some(&1) {
        return false;
    }
    // A count of one whose location is stale, or gone (a removal swept it)
    // while its parent's detach is pending, counts the detached occurrence.
    let stale = match item_locations.get(path) {
        Some(location) => !location_is_live(location, detached),
        None => path
            .parent()
            .is_some_and(|parent| detached.contains_key(parent)),
    };
    if stale {
        visible_path_counts.remove(path);
        item_locations.remove(path);
    }
    stale
}

/// Remove up to `unit` of `listing`'s paths from the caches: a path's
/// location only while it is still the stale one of this detach (a
/// re-expansion's adoption may have cached it again since), and its count
/// only while no location is left behind a count of one. Forgets the detach
/// once its last path is gone and no later detach of the directory replaced
/// it. Returns the entries removed.
pub(super) fn retire_detached_rows(
    visible_path_counts: &mut HashMap<PathBuf, usize>,
    item_locations: &mut HashMap<PathBuf, ItemLocation>,
    detached: &mut HashMap<PathBuf, u64>,
    listing: &mut DetachedCache,
    unit: usize,
) -> usize {
    let keep = listing.paths.len().saturating_sub(unit);
    let mut removed = 0usize;
    for path in listing.paths.drain(keep..) {
        let retired_location = item_locations.get(&path).is_some_and(|location| {
            location.parent_dir.as_deref() == Some(listing.parent.as_path())
                && location.epoch <= listing.epoch
        });
        if retired_location {
            item_locations.remove(&path);
            removed += 1;
        }
        // A count of one with no location left counted the detached
        // occurrence; a location another epoch cached owns its count.
        if visible_path_counts.get(&path) == Some(&1) && !item_locations.contains_key(&path) {
            visible_path_counts.remove(&path);
        }
    }
    if listing.paths.is_empty() && detached.get(&listing.parent) == Some(&listing.epoch) {
        detached.remove(&listing.parent);
    }
    removed
}

/// Whether a directory of the subtree at `root` can be listed by a second
/// live store: another workspace folder's root lies strictly inside it, or
/// it lies inside two folders (one inside the other), so the same directory
/// shows under both occurrences. Such a subtree is not detached: the store
/// that owns a shared directory's cache may be the one that stays.
fn subtree_is_listed_twice(folder_paths: &[PathBuf], root: &Path) -> bool {
    folder_paths
        .iter()
        .any(|folder| folder.starts_with(root) && folder.as_path() != root)
        || folder_paths
            .iter()
            .filter(|folder| root.starts_with(folder))
            .count()
            > 1
}

/// Detach the path caches of `stores`, the stores of a collapsing subtree
/// rooted at `root` (design D24), at the collapse's intent: after the
/// subtree's descendants were captured and selection and focus moved, so
/// nothing this dispatch needs reads a detached location afterwards. A store
/// that does not own its directory's cache is left alone, and so is the
/// whole subtree when one of its directories can be listed twice
/// ([`subtree_is_listed_twice`]).
/// Returns the directories detached.
pub(in crate::ui::sidebar) fn detach_child_caches(
    section: &LushtextWorkspaceSection,
    root: &Path,
    stores: &[gio::ListStore],
) -> usize {
    let imp = section.imp();
    if subtree_is_listed_twice(&imp.folder_paths.borrow(), root) {
        return 0;
    }
    let mut detached_count = 0usize;
    for store in stores {
        let store_key = child_store_key(store);
        let Some(parent) = imp.child_store_paths.borrow().get(&store_key).cloned() else {
            continue;
        };
        if imp.child_cache_owners.borrow().get(&parent) != Some(&store_key) {
            continue;
        }
        cancel_child_cache_adoption(section, &parent);
        imp.child_cache_owners.borrow_mut().remove(&parent);
        let paths = imp
            .child_paths
            .borrow_mut()
            .remove(&parent)
            .unwrap_or_default();
        detached_count += 1;
        // An empty cache leaves nothing stale behind it.
        if !paths.is_empty() {
            let epoch = imp.child_cache_epoch.get();
            imp.child_cache_epoch.set(epoch.wrapping_add(1));
            imp.detached_child_caches
                .borrow_mut()
                .insert(parent.clone(), epoch);
            queue_listing_retirement(
                section,
                RetiringListing {
                    store: gio::ListStore::new::<glib::Object>(),
                    mirror: Vec::new(),
                    items: Vec::new(),
                    paths: Vec::new(),
                    detached: Some(DetachedCache {
                        parent,
                        epoch,
                        paths,
                    }),
                },
            );
        }
    }
    let refresh = &imp.refresh_runtime;
    refresh.cache_detached_parents.set(
        refresh
            .cache_detached_parents
            .get()
            .saturating_add(u64::try_from(detached_count).unwrap_or(u64::MAX)),
    );
    detached_count
}

/// Start caching `store`'s remaining rows again for the re-expanded
/// directory `dir`, whose cache a collapse detached (design D24). Nothing
/// when the cache is not detached, or another store owns it.
pub(in crate::ui::sidebar) fn adopt_child_cache(
    section: &LushtextWorkspaceSection,
    dir: &Path,
    store: &gio::ListStore,
) {
    let imp = section.imp();
    if !imp.detached_child_caches.borrow().contains_key(dir)
        || imp.child_cache_owners.borrow().contains_key(dir)
        || imp.child_paths.borrow().contains_key(dir)
    {
        return;
    }
    let store_key = child_store_key(store);
    imp.child_cache_owners
        .borrow_mut()
        .insert(dir.to_path_buf(), store_key);
    imp.child_cache_adoptions.borrow_mut().insert(
        dir.to_path_buf(),
        ChildCacheAdoption {
            store_key,
            mirror_generation: child_mirror_generation(section, store),
            next: 0,
            deferred: None,
        },
    );
    arm_adoption_source(section);
}

/// Hold `scanned` until its directory's adoption ends, if one runs for its
/// store; otherwise hand it back.
pub(super) fn defer_until_adopted(
    section: &LushtextWorkspaceSection,
    scanned: ScannedChildrenPlan,
) -> Option<ScannedChildrenPlan> {
    let store_key = child_store_key(&scanned.store);
    let mut adoptions = section.imp().child_cache_adoptions.borrow_mut();
    match adoptions.get_mut(&scanned.dir_path) {
        Some(adoption) if adoption.store_key == store_key => {
            adoption.deferred = Some(scanned);
            None
        }
        _ => Some(scanned),
    }
}

/// Stop the adoption of `dir`, if one runs, handing its held scan result
/// back to the main loop (its checks then decide whether it still applies).
pub(super) fn cancel_child_cache_adoption(section: &LushtextWorkspaceSection, dir: &Path) {
    let adoption = section.imp().child_cache_adoptions.borrow_mut().remove(dir);
    if let Some(deferred) = adoption.and_then(|adoption| adoption.deferred) {
        release_deferred_scan(section, deferred);
    }
}

/// Stop the adoption `store_key`'s store runs, if any: the store is being
/// retired.
pub(super) fn cancel_adoption_of_store(section: &LushtextWorkspaceSection, store_key: usize) {
    let dir = section
        .imp()
        .child_cache_adoptions
        .borrow()
        .iter()
        .find_map(|(dir, adoption)| (adoption.store_key == store_key).then(|| dir.clone()));
    if let Some(dir) = dir {
        cancel_child_cache_adoption(section, &dir);
    }
}

/// Stop every adoption whose directory matches `cancelled`.
pub(super) fn cancel_child_cache_adoptions(
    section: &LushtextWorkspaceSection,
    cancelled: impl Fn(&Path) -> bool,
) {
    let dirs: Vec<PathBuf> = section
        .imp()
        .child_cache_adoptions
        .borrow()
        .keys()
        .filter(|dir| cancelled(dir))
        .cloned()
        .collect();
    for dir in dirs {
        cancel_child_cache_adoption(section, &dir);
    }
}

/// Apply a held scan result from its own dispatch, never inside the caller's
/// cache or mirror mutation.
fn release_deferred_scan(section: &LushtextWorkspaceSection, deferred: ScannedChildrenPlan) {
    let section_weak = section.downgrade();
    let mut deferred = Some(deferred);
    glib::idle_add_local_once(move || {
        if let (Some(section), Some(deferred)) = (section_weak.upgrade(), deferred.take()) {
            apply_scanned_children(&section, deferred);
        }
    });
}

fn arm_adoption_source(section: &LushtextWorkspaceSection) {
    let imp = section.imp();
    if imp.child_cache_adoption_source.borrow().is_some() {
        return;
    }
    let section_weak = section.downgrade();
    // `G_PRIORITY_DEFAULT_IDLE`: below input and the frame clock, above the
    // listing retirement, so the refill waits no longer than its rows take.
    let source = glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
        let Some(section) = section_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if run_adoption_turn(&section) {
            return glib::ControlFlow::Continue;
        }
        section
            .imp()
            .child_cache_adoption_source
            .borrow_mut()
            .take();
        glib::ControlFlow::Break
    });
    imp.child_cache_adoption_source.replace(Some(source));
}

/// One adoption turn: cache units of `RETIRED_LISTING_ROWS_PER_UNIT` mirror
/// rows while `turn_continues` holds. Returns whether adoptions remain.
fn run_adoption_turn(section: &LushtextWorkspaceSection) -> bool {
    let imp = section.imp();
    let started_us = glib::monotonic_time();
    let mut units = 0usize;
    let mut turn_rows = 0usize;
    loop {
        let elapsed = u64::try_from(glib::monotonic_time().saturating_sub(started_us)).unwrap_or(0);
        if !turn_continues(elapsed, units) {
            break;
        }
        let Some(dir) = imp.child_cache_adoptions.borrow().keys().next().cloned() else {
            break;
        };
        let (finished, rows) = adopt_unit(section, &dir);
        units += 1;
        turn_rows = turn_rows.saturating_add(rows);
        if finished {
            let deferred = imp
                .child_cache_adoptions
                .borrow_mut()
                .remove(&dir)
                .and_then(|adoption| adoption.deferred);
            if let Some(deferred) = deferred {
                release_deferred_scan(section, deferred);
            }
        }
    }
    let refresh = &imp.refresh_runtime;
    refresh.cache_adopted_rows_turn_high_water.set(
        refresh
            .cache_adopted_rows_turn_high_water
            .get()
            .max(turn_rows),
    );
    !imp.child_cache_adoptions.borrow().is_empty()
}

/// Adopt one unit of `dir`'s mirror rows. Returns whether the adoption is
/// over (finished, or stopped by a change it cannot follow) and the rows it
/// cached.
fn adopt_unit(section: &LushtextWorkspaceSection, dir: &Path) -> (bool, usize) {
    let imp = section.imp();
    let (store_key, mirror_generation, next) = {
        let adoptions = imp.child_cache_adoptions.borrow();
        let Some(adoption) = adoptions.get(dir) else {
            return (true, 0);
        };
        (
            adoption.store_key,
            adoption.mirror_generation,
            adoption.next,
        )
    };
    let current_generation = imp
        .child_row_mirror_generations
        .borrow()
        .get(&store_key)
        .copied()
        .unwrap_or(0);
    if current_generation != mirror_generation
        || imp.child_cache_owners.borrow().get(dir) != Some(&store_key)
    {
        return (true, 0);
    }
    let rows: Vec<PathBuf> = imp
        .child_row_mirrors
        .borrow()
        .get(&store_key)
        .map(|mirror| {
            mirror
                .iter()
                .skip(next)
                .take(RETIRED_LISTING_ROWS_PER_UNIT)
                .map_while(|row| row.path.clone())
                .collect()
        })
        .unwrap_or_default();
    let epoch = imp.child_cache_epoch.get();
    let detached = imp.detached_child_caches.borrow();
    let mut counts = imp.visible_path_counts.borrow_mut();
    let mut locations = imp.item_locations.borrow_mut();
    let mut child_paths = imp.child_paths.borrow_mut();
    let siblings = child_paths.entry(dir.to_path_buf()).or_default();
    for (offset, path) in rows.iter().enumerate() {
        drop_stale_unique(&mut counts, &mut locations, &detached, path);
        let count = counts.entry(path.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            locations.insert(
                path.clone(),
                ItemLocation {
                    parent_dir: Some(dir.to_path_buf()),
                    index: next + offset,
                    epoch,
                },
            );
        } else {
            locations.remove(path);
        }
        siblings.push(path.clone());
    }
    drop((detached, counts, locations, child_paths));
    let adopted = rows.len();
    let refresh = &imp.refresh_runtime;
    refresh.cache_adopted_rows.set(
        refresh
            .cache_adopted_rows
            .get()
            .saturating_add(u64::try_from(adopted).unwrap_or(u64::MAX)),
    );
    if adopted < RETIRED_LISTING_ROWS_PER_UNIT {
        return (true, adopted);
    }
    if let Some(adoption) = imp.child_cache_adoptions.borrow_mut().get_mut(dir) {
        adoption.next = next + adopted;
    }
    (false, adopted)
}

#[cfg(test)]
mod tests {
    use super::super::{CacheContext, replace_child_cache};
    use super::*;
    use proptest::prelude::*;

    fn location(parent: &str, index: usize, epoch: u64) -> ItemLocation {
        ItemLocation {
            parent_dir: Some(PathBuf::from(parent)),
            index,
            epoch,
        }
    }

    fn detached_cache(parent: &str, epoch: u64, paths: &[&str]) -> DetachedCache {
        DetachedCache {
            parent: PathBuf::from(parent),
            epoch,
            paths: paths.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn a_lookup_under_a_detached_parent_misses_from_the_detach_on() {
        let mut detached = HashMap::new();
        let before = location("/w/big", 3, 4);
        let elsewhere = location("/w/other", 0, 4);
        let top_level = ItemLocation {
            parent_dir: None,
            index: 0,
            epoch: 0,
        };
        assert!(location_is_live(&before, &detached));
        // The detach at epoch 4: everything cached under it through 4 is
        // stale, a location cached afterwards (epoch 5) is live.
        detached.insert(PathBuf::from("/w/big"), 4);
        assert!(!location_is_live(&before, &detached));
        assert!(location_is_live(&location("/w/big", 3, 5), &detached));
        assert!(location_is_live(&elsewhere, &detached));
        assert!(location_is_live(&top_level, &detached));
    }

    #[test]
    fn a_stale_unique_path_counts_from_zero_and_a_live_one_does_not() {
        let mut counts = HashMap::from([
            (PathBuf::from("/w/big/a"), 1),
            (PathBuf::from("/w/big/b"), 1),
        ]);
        let mut locations = HashMap::from([
            (PathBuf::from("/w/big/a"), location("/w/big", 0, 4)),
            (PathBuf::from("/w/big/b"), location("/w/big", 1, 5)),
        ]);
        let detached = HashMap::from([(PathBuf::from("/w/big"), 4)]);
        assert!(drop_stale_unique(
            &mut counts,
            &mut locations,
            &detached,
            Path::new("/w/big/a")
        ));
        assert!(!counts.contains_key(Path::new("/w/big/a")));
        assert!(!locations.contains_key(Path::new("/w/big/a")));
        assert!(!drop_stale_unique(
            &mut counts,
            &mut locations,
            &detached,
            Path::new("/w/big/b")
        ));
        assert_eq!(counts.get(Path::new("/w/big/b")), Some(&1));
    }

    #[test]
    fn the_retirement_removes_detached_entries_in_slices_and_keeps_the_recached_ones() {
        let parent = "/w/big";
        let paths = ["/w/big/a", "/w/big/b", "/w/big/c", "/w/big/d"];
        let mut counts: HashMap<PathBuf, usize> =
            paths.iter().map(|path| (PathBuf::from(path), 1)).collect();
        let mut locations: HashMap<PathBuf, ItemLocation> = paths
            .iter()
            .enumerate()
            .map(|(index, path)| (PathBuf::from(path), location(parent, index, 4)))
            .collect();
        let mut detached = HashMap::from([(PathBuf::from(parent), 4)]);
        let mut listing = detached_cache(parent, 4, &paths);
        // A re-expansion's adoption cached `b` again, at the new epoch.
        locations.insert(PathBuf::from("/w/big/b"), location(parent, 0, 5));

        let removed =
            retire_detached_rows(&mut counts, &mut locations, &mut detached, &mut listing, 2);
        assert_eq!(removed, 2, "the first slice takes the last two paths");
        assert_eq!(listing.paths.len(), 2);
        assert!(!locations.contains_key(Path::new("/w/big/d")));
        assert!(!counts.contains_key(Path::new("/w/big/c")));
        assert!(
            detached.contains_key(Path::new(parent)),
            "the detach stays while paths remain"
        );

        let removed =
            retire_detached_rows(&mut counts, &mut locations, &mut detached, &mut listing, 2);
        assert_eq!(removed, 1, "only `a` was still the stale location");
        assert!(listing.paths.is_empty());
        assert_eq!(
            locations.get(Path::new("/w/big/b")),
            Some(&location(parent, 0, 5)),
            "the re-cached location is kept"
        );
        assert_eq!(counts.get(Path::new("/w/big/b")), Some(&1));
        assert!(!locations.contains_key(Path::new("/w/big/a")));
        assert!(!counts.contains_key(Path::new("/w/big/a")));
        assert!(
            !detached.contains_key(Path::new(parent)),
            "the detach is forgotten with its last path"
        );
    }

    #[test]
    fn a_later_detach_of_the_same_directory_outlives_an_earlier_retirement() {
        let parent = "/w/big";
        let mut counts = HashMap::from([(PathBuf::from("/w/big/a"), 1)]);
        let mut locations = HashMap::from([(PathBuf::from("/w/big/a"), location(parent, 0, 6))]);
        // Detached at 4, re-adopted at 5, detached again at 6.
        let mut detached = HashMap::from([(PathBuf::from(parent), 6)]);
        let mut first = detached_cache(parent, 4, &["/w/big/a"]);
        let removed =
            retire_detached_rows(&mut counts, &mut locations, &mut detached, &mut first, 8);
        assert_eq!(removed, 0, "epoch 6 is not the first detach's");
        assert_eq!(counts.get(Path::new("/w/big/a")), Some(&1));
        assert_eq!(detached.get(Path::new(parent)), Some(&6));
        let mut second = detached_cache(parent, 6, &["/w/big/a"]);
        let removed =
            retire_detached_rows(&mut counts, &mut locations, &mut detached, &mut second, 8);
        assert_eq!(removed, 1);
        assert!(counts.is_empty() && locations.is_empty() && detached.is_empty());
    }

    #[test]
    fn a_subtree_two_folders_can_list_is_not_detached() {
        let folders = [PathBuf::from("/w/root"), PathBuf::from("/w/root/shared")];
        // A folder root strictly inside the subtree.
        assert!(subtree_is_listed_twice(&folders, Path::new("/w/root")));
        // The inner occurrence of the second folder, and a directory under
        // it: both folders list them.
        assert!(subtree_is_listed_twice(
            &folders,
            Path::new("/w/root/shared")
        ));
        assert!(subtree_is_listed_twice(
            &folders,
            Path::new("/w/root/shared/sub")
        ));
        // A directory only the first folder lists.
        assert!(!subtree_is_listed_twice(&folders, Path::new("/w/root/big")));
        assert!(!subtree_is_listed_twice(
            &folders[..1],
            Path::new("/w/root")
        ));
        // A sibling folder named with the same prefix is not inside it.
        let siblings = [PathBuf::from("/w/root"), PathBuf::from("/w/rootless")];
        assert!(!subtree_is_listed_twice(&siblings, Path::new("/w/root")));
    }

    proptest! {
        /// Detach a directory's cache, re-cache any subset of its rows (a
        /// re-expansion), then retire the detached paths in slices of any
        /// size: the caches end exactly as if only the re-cached rows had
        /// ever been listed, the other directory's entries are untouched,
        /// and the detach is forgotten. Between the detach and the re-cache,
        /// no detached row is located.
        #[test]
        fn detach_recache_and_retire_match_the_recached_listing(
            old_ids in prop::collection::btree_set(0u16..64, 0..40),
            kept_ids in prop::collection::btree_set(0u16..64, 0..40),
            other_ids in prop::collection::btree_set(0u16..64, 0..16),
            unit in 1usize..9,
        ) {
            let parent = PathBuf::from("/w/big");
            let other = PathBuf::from("/w/other");
            let under = |dir: &Path, id: u16| dir.join(format!("e{id:02}"));
            let old: Vec<PathBuf> = old_ids.iter().map(|id| under(&parent, *id)).collect();
            let others: Vec<PathBuf> = other_ids.iter().map(|id| under(&other, *id)).collect();
            let mut counts = HashMap::new();
            let mut locations = HashMap::new();
            for (index, path) in old.iter().enumerate() {
                counts.insert(path.clone(), 1);
                locations.insert(path.clone(), location("/w/big", index, 0));
            }
            for (index, path) in others.iter().enumerate() {
                counts.insert(path.clone(), 1);
                locations.insert(path.clone(), location("/w/other", index, 0));
            }
            let mut child_paths = HashMap::from([(other, others.clone())]);
            // The detach at epoch 0: the sibling list leaves whole.
            // (`detach_child_caches` records no detach for an empty cache.)
            let mut detached = HashMap::new();
            if !old.is_empty() {
                detached.insert(parent.clone(), 0u64);
            }
            let mut listing = DetachedCache { parent: parent.clone(), epoch: 0, paths: old.clone() };
            for path in &old {
                let located = locations.get(path).is_some_and(|l| location_is_live(l, &detached));
                prop_assert!(!located, "a detached row is not located");
            }
            // The re-expansion re-caches `kept` at epoch 1.
            let kept: Vec<(usize, PathBuf)> = kept_ids
                .iter()
                .map(|id| under(&parent, *id))
                .enumerate()
                .collect();
            replace_child_cache(
                &CacheContext { folder_paths: &[], epoch: 1, detached: &detached },
                &mut child_paths,
                &mut counts,
                &mut locations,
                &parent,
                &kept,
            );
            while !listing.paths.is_empty() {
                retire_detached_rows(&mut counts, &mut locations, &mut detached, &mut listing, unit);
            }
            prop_assert!(detached.is_empty());
            let mut expected_counts = HashMap::new();
            let mut expected_locations = HashMap::new();
            for (index, path) in &kept {
                expected_counts.insert(path.clone(), 1);
                expected_locations.insert(path.clone(), location("/w/big", *index, 1));
            }
            for (index, path) in others.iter().enumerate() {
                expected_counts.insert(path.clone(), 1);
                expected_locations.insert(path.clone(), location("/w/other", index, 0));
            }
            prop_assert_eq!(counts, expected_counts);
            prop_assert_eq!(&locations, &expected_locations);
            for (path, kept_location) in &locations {
                prop_assert!(location_is_live(kept_location, &detached), "{path:?} is live");
            }
        }
    }
}
