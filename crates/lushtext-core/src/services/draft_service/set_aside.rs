// SPDX-License-Identifier: GPL-3.0-or-later

//! The drafts set-aside area: the **one owner** of every draft body that left
//! the journal without being applied.
//!
//! Bodies arrive here from three routes, all through this module: an
//! unattributable crash leftover moved out by reconciliation ([`move_in`]), a
//! stale or unseen body copied before it is retired or overwritten
//! ([`keep_copy`]), and the earlier body of an id whose registration would
//! overwrite it (also [`keep_copy`]). The set-aside copy owns the bytes: it is
//! never pruned, and nothing but the user's own confirmed decision on
//! `Preferences > Data` removes one: a per-row Delete or "Delete All Preserved
//! Drafts…", both through [`delete_confirmed`], which removes only the bodies
//! that confirmation listed, unchanged. Growth past the soft retention bound
//! (`set_aside_retention`) only asks the user to review; it deletes nothing. A local-history snapshot a
//! stale body may also receive is a browsable **view** of the same edits for
//! the file it belongs to; local history may prune it without any data loss,
//! because this copy is what counts as preserved.
//!
//! The area sits outside the manifest and outside the non-recursive `*.draft`
//! inventory, so neither reconciliation nor orphan cleanup (in this build or an
//! older one) ever classifies, restores, or deletes what it holds. Names are
//! `{draft_id}.{stamp_secs}.draft`, with a `-{n}` suffix on the stamp when that
//! name was already taken, so a new body never replaces an earlier one.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};

use crate::services::filesystem::{
    DirectoryScanPolicy, FileKind, WriteLabel, metadata as fs_metadata, mutate as fs_mutate,
    read as fs_read, tree as fs_tree, write as fs_write,
};

use super::journal_core::{SetAsideNameStep, SetAsideSlot, set_aside_name_step};
use super::set_aside_retention::{SetAsideFingerprint, SetAsideTotals, UserDecision, may_delete};
use super::{MAX_AUTOMATIC_DRAFT_BYTES, draft_body_path, drafts_dir, read_draft_path_bounded};

/// Subdirectory of `drafts/` holding preserved bodies that left the journal.
const SET_ASIDE_DIR: &str = "set-aside";
/// Largest number of set-aside names probed for one body before giving up.
const MAX_SET_ASIDE_NAME_ATTEMPTS: u32 = 64;
/// Most set-aside bodies one listing reports; the rest stay on disk untouched.
pub const MAX_LISTED_SET_ASIDE_BODIES: usize = 256;
/// Most set-aside entries one scan visits; past it the totals are a lower
/// bound (and far past the soft retention bound).
pub const MAX_SET_ASIDE_SCAN_ENTRIES: usize = 10_000;

/// Bodies newly placed in any set-aside area by this process. The retention
/// notice re-evaluates the soft bound when it moves; nothing else reads it.
static PLACEMENTS: AtomicU64 = AtomicU64::new(0);

/// How many bodies this process has newly placed in a set-aside area (a copy
/// found already kept does not count).
#[must_use]
pub fn placements() -> u64 {
    PLACEMENTS.load(Ordering::Relaxed)
}

fn record_placement() {
    PLACEMENTS.fetch_add(1, Ordering::Relaxed);
}

/// Returns the set-aside area: `{data_dir}/drafts/set-aside/`.
#[must_use]
pub fn dir(data_dir: &Path) -> PathBuf {
    drafts_dir(data_dir).join(SET_ASIDE_DIR)
}

/// How a draft body enters the set-aside area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Transfer {
    /// Rename out of `drafts/`, so the journal no longer sees the body.
    Move,
    /// Byte-identical durable copy that leaves the original in place; the
    /// journal retires the original through its own ordering.
    Copy,
}

/// The set-aside name tried on one placement attempt: `{id}.{stamp}.draft`
/// first, then `{id}.{stamp}-{attempt}.draft`.
///
/// A name existing does **not** mean this body is kept: a crash between a
/// body write and its manifest commit leaves a newer body under an unchanged
/// entry stamp, so only a byte-identical file counts as already kept (see
/// [`keep_copy`]).
fn candidate_path(area: &Path, draft_id: &str, stamp_secs: u64, attempt: u32) -> PathBuf {
    if attempt == 0 {
        area.join(format!("{draft_id}.{stamp_secs}.draft"))
    } else {
        area.join(format!("{draft_id}.{stamp_secs}-{attempt}.draft"))
    }
}

/// Copy a body into the set-aside area unless a byte-identical copy for this
/// id and stamp was already kept.
///
/// Every existing `{id}.{stamp}[-n].draft` is compared with the body: an
/// identical one is returned (so retries stay idempotent), and a different one
/// is left untouched while the body takes the next free name. The stamp alone
/// never identifies the content (the E1 finding of the formal-methods
/// evaluation): a body rewritten after the entry that names it keeps the same
/// stamp.
///
/// # Errors
///
/// Returns an error when the body cannot be read or no durable copy could be
/// placed; the caller must then keep the journal body.
pub fn keep_copy(data_dir: &Path, draft_id: &str, stamp_secs: u64) -> Result<PathBuf> {
    place(data_dir, draft_id, stamp_secs, Transfer::Copy)
}

/// Move a body out of the journal into the set-aside area.
///
/// # Errors
///
/// Returns an error when no set-aside name could take the body; the body then
/// stays where it was.
pub fn move_in(data_dir: &Path, draft_id: &str, stamp_secs: u64) -> Result<PathBuf> {
    place(data_dir, draft_id, stamp_secs, Transfer::Move)
}

/// Keep one draft body in the set-aside area under a name that never replaces
/// an earlier set-aside body.
///
/// A `Copy` of a body too large to read under the automatic-draft bound moves
/// it instead: the body still leaves nothing behind unpreserved, and the
/// journal's later deletion of the original becomes a no-op. When the
/// transfer's own durability step fails after the body is already in place,
/// the body counts as placed (with a warning) rather than as lost.
fn place(data_dir: &Path, draft_id: &str, stamp_secs: u64, transfer: Transfer) -> Result<PathBuf> {
    let source = draft_body_path(data_dir, draft_id);
    let dir = dir(data_dir);
    fs_write::create_dir_all_durable(&dir)
        .with_context(|| format!("failed to create {}", dir.display()))?;
    let copy_bytes = match transfer {
        Transfer::Move => None,
        Transfer::Copy => {
            let limit = usize::try_from(MAX_AUTOMATIC_DRAFT_BYTES)
                .unwrap_or(usize::MAX)
                .saturating_add(1);
            let bytes = fs_read::prefix_bytes(&source, limit)
                .with_context(|| format!("failed to read {}", source.display()))?;
            (u64::try_from(bytes.len()).unwrap_or(u64::MAX) <= MAX_AUTOMATIC_DRAFT_BYTES)
                .then_some(bytes)
        }
    };
    for attempt in 0..MAX_SET_ASIDE_NAME_ATTEMPTS {
        let target = candidate_path(&dir, draft_id, stamp_secs, attempt);
        let slot = if !fs_metadata::exists(&target) {
            SetAsideSlot::Free
        } else if copy_bytes
            .as_deref()
            .is_some_and(|bytes| holds_exactly(&target, bytes))
        {
            SetAsideSlot::SameBody
        } else {
            // A move never dedupes: its source leaves the journal either way.
            SetAsideSlot::OtherBody
        };
        match set_aside_name_step(slot) {
            SetAsideNameStep::Place => {}
            SetAsideNameStep::AlreadyKept => return Ok(target),
            SetAsideNameStep::NextName => continue,
        }
        let placed = match copy_bytes.as_deref() {
            Some(bytes) => fs_write::atomic_replace(&target, WriteLabel::DRAFT, bytes)
                .map_err(std::io::Error::other),
            None => fs_write::rename_durable_no_replace_or_checked(&source, &target),
        };
        match placed {
            Ok(()) => {
                record_placement();
                return Ok(target);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) if took_effect(&source, &target, copy_bytes.is_some()) => {
                tracing::warn!(
                    "Set aside draft {} as {}, but its durability is unconfirmed: {error}",
                    source.display(),
                    target.display(),
                );
                record_placement();
                return Ok(target);
            }
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "failed to set aside draft {} as {}: {error}",
                    source.display(),
                    target.display(),
                ));
            }
        }
    }
    anyhow::bail!(
        "no free set-aside name for draft {draft_id} after {MAX_SET_ASIDE_NAME_ATTEMPTS} attempts"
    )
}

/// Whether an existing set-aside file holds exactly `bytes`.
///
/// The size is checked first so a differing body is usually rejected without
/// reading it; any read failure counts as "not identical", which only ever
/// costs one more copy.
fn holds_exactly(target: &Path, bytes: &[u8]) -> bool {
    let expected = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if fs_metadata::file_stat(target).map_or(true, |facts| facts.byte_size != expected) {
        return false;
    }
    fs_read::prefix_bytes(target, bytes.len().saturating_add(1))
        .is_ok_and(|existing| existing == bytes)
}

/// Whether a failed set-aside transfer nevertheless placed the body.
///
/// A durable rename or replace can fail in its final directory sync after the
/// new entry exists; the bytes are then in the set-aside area.
fn took_effect(source: &Path, target: &Path, copied: bool) -> bool {
    fs_metadata::exists(target) && (copied || !fs_metadata::exists(source))
}

/// One preserved body in the set-aside area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetAsideBody {
    /// The set-aside file itself.
    pub path: PathBuf,
    /// The draft id the body was preserved from.
    pub draft_id: String,
    /// When the preserved edits were last saved (seconds since the epoch), as
    /// recorded in the name.
    pub stamp_secs: u64,
    /// What identifies this body as listed, including its size: a deletion
    /// confirmed over it removes it only while its file still matches.
    pub fingerprint: SetAsideFingerprint,
}

impl SetAsideBody {
    /// The body's size in bytes.
    #[must_use]
    pub const fn byte_size(&self) -> u64 {
        self.fingerprint.byte_size
    }
}

/// Parse a set-aside file name, `{id}.{stamp}.draft` or `{id}.{stamp}-{n}.draft`.
#[must_use]
pub fn parse_name(file_name: &str) -> Option<(String, u64)> {
    let stem = file_name.strip_suffix(".draft")?;
    let (draft_id, stamp) = stem.rsplit_once('.')?;
    let stamp = stamp.split_once('-').map_or(stamp, |(stamp, _)| stamp);
    if draft_id.is_empty() || stamp.is_empty() || !stamp.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((draft_id.to_string(), stamp.parse().ok()?))
}

/// One scan of the set-aside area: the newest bodies, and what the whole area
/// holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetAsideListing {
    /// At most [`MAX_LISTED_SET_ASIDE_BODIES`] bodies, newest first, chosen
    /// from every body the scan visited.
    pub rows: Vec<SetAsideBody>,
    /// What the whole area holds; `complete` means the scan reached the end
    /// of the area within [`MAX_SET_ASIDE_SCAN_ENTRIES`].
    pub totals: SetAsideTotals,
}

/// A body's place in the newest-first order: a later stamp, then the smaller
/// path, ranks higher.
fn recency(stamp_secs: u64, path: &Path) -> (u64, Reverse<&Path>) {
    (stamp_secs, Reverse(path))
}

/// Orders listed bodies by [`recency`].
#[derive(PartialEq, Eq)]
struct Newest(SetAsideBody);

impl Ord for Newest {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        recency(self.0.stamp_secs, &self.0.path).cmp(&recency(other.0.stamp_secs, &other.0.path))
    }
}

impl PartialOrd for Newest {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// The fingerprint of one set-aside file as it is now; `None` when it is gone.
fn fingerprint_of(path: &Path, file_name: &str) -> std::io::Result<Option<SetAsideFingerprint>> {
    match fs_metadata::file_stat(path) {
        Ok(facts) if facts.kind == FileKind::File => Ok(Some(SetAsideFingerprint {
            file_name: file_name.to_string(),
            byte_size: facts.byte_size,
            identity: facts.identity,
            modified_at_nanos: facts.modified_at_nanos,
        })),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// List the preserved bodies: the newest [`MAX_LISTED_SET_ASIDE_BODIES`],
/// newest first, chosen from the **whole** area, plus the area's total count
/// and size. One traversal, at most [`MAX_SET_ASIDE_SCAN_ENTRIES`] entries,
/// keeping at most the listed rows in memory; past that budget the totals are
/// a lower bound and `complete` is false. A missing area is an empty,
/// complete listing.
///
/// **Threading:** blocking directory and metadata I/O; call off GTK.
///
/// # Errors
///
/// Returns an error when the area exists but cannot be read.
pub fn list(data_dir: &Path) -> Result<SetAsideListing> {
    scan(data_dir, MAX_LISTED_SET_ASIDE_BODIES)
}

/// The area's totals alone, from the same bounded scan as [`list`] without
/// keeping any row.
///
/// **Threading:** blocking directory and metadata I/O; call off GTK.
///
/// # Errors
///
/// Returns an error when the area exists but cannot be read.
pub fn totals(data_dir: &Path) -> Result<SetAsideTotals> {
    scan(data_dir, 0).map(|listing| listing.totals)
}

/// One bounded traversal of the area keeping the newest `max_rows` bodies.
fn scan(data_dir: &Path, max_rows: usize) -> Result<SetAsideListing> {
    let dir = dir(data_dir);
    if !fs_metadata::exists(&dir) {
        return Ok(SetAsideListing {
            rows: Vec::new(),
            totals: SetAsideTotals {
                complete: true,
                ..SetAsideTotals::default()
            },
        });
    }
    let mut newest: BinaryHeap<Reverse<Newest>> = BinaryHeap::with_capacity(max_rows + 1);
    let mut totals = SetAsideTotals::default();
    let metrics = fs_tree::visit_directory_pages_with_cancel(
        &dir,
        DirectoryScanPolicy {
            max_entries: MAX_SET_ASIDE_SCAN_ENTRIES,
            include_hidden: false,
        },
        MAX_LISTED_SET_ASIDE_BODIES,
        || false,
        |page| {
            for entry in page {
                if entry.kind != FileKind::File {
                    continue;
                }
                let Some((draft_id, stamp_secs)) = parse_name(&entry.file_name) else {
                    continue;
                };
                // A body that vanished between the directory read and its
                // metadata (a concurrent Delete) is simply not counted.
                let Ok(Some(fingerprint)) = fingerprint_of(&entry.path, &entry.file_name) else {
                    continue;
                };
                totals.count = totals.count.saturating_add(1);
                totals.bytes = totals.bytes.saturating_add(fingerprint.byte_size);
                // Once the heap is full, a body no newer than its oldest row
                // would be popped again at once; skip building it.
                let displaces = newest.len() < max_rows
                    || newest.peek().is_some_and(|Reverse(Newest(oldest))| {
                        recency(stamp_secs, &entry.path) > recency(oldest.stamp_secs, &oldest.path)
                    });
                if !displaces {
                    continue;
                }
                newest.push(Reverse(Newest(SetAsideBody {
                    path: entry.path.clone(),
                    draft_id,
                    stamp_secs,
                    fingerprint,
                })));
                if newest.len() > max_rows {
                    newest.pop();
                }
            }
            true
        },
    )
    .with_context(|| format!("failed to list {}", dir.display()))?;
    // Ascending `Reverse<Newest>` is newest first.
    let rows = newest
        .into_sorted_vec()
        .into_iter()
        .map(|Reverse(Newest(body))| body)
        .collect();
    totals.complete = !metrics.stopped_by_limit;
    Ok(SetAsideListing { rows, totals })
}

/// Read one preserved body as UTF-8 text, bounded by the automatic-draft
/// limit, so it can be opened as a new untitled tab.
///
/// # Errors
///
/// Returns an error when the path is not inside the set-aside area, the body is
/// larger than the automatic-draft limit, or it is not UTF-8.
pub fn read(data_dir: &Path, path: &Path) -> Result<String> {
    ensure_inside(data_dir, path)?;
    read_draft_path_bounded(path, MAX_AUTOMATIC_DRAFT_BYTES)?
        .ok_or_else(|| anyhow::anyhow!("{} no longer exists", path.display()))
}

/// What one confirmed deletion did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetAsideBulkDeletion {
    /// Bodies removed.
    pub deleted: u64,
    /// Confirmed bodies kept because they changed after the confirmation, or
    /// were already gone.
    pub kept: u64,
    /// Confirmed bodies that could not be checked or removed; they stay.
    pub failed: u64,
    /// The removals happened but the directory sync that makes them durable
    /// failed.
    pub durability_unconfirmed: bool,
}

/// Delete exactly the bodies the user confirmed, as the confirmation listed
/// them: the one row of a per-row Delete, or every row "Delete All Preserved
/// Drafts…" showed. This is the area's only deletion.
///
/// Each body's fingerprint is re-read immediately before its removal, and the
/// pure [`may_delete`] decides whether it may go: a body that changed since
/// the confirmation, or a name now holding a different file, is kept. A body
/// set aside after the confirmation is not in `confirmed`, so it is never
/// touched. One failure keeps that body and the loop goes on; the directory is
/// synced once for the whole batch.
///
/// **Threading:** blocking I/O; call off GTK.
#[must_use]
pub fn delete_confirmed(
    data_dir: &Path,
    confirmed: &[SetAsideFingerprint],
) -> SetAsideBulkDeletion {
    let area = dir(data_dir);
    let decision = UserDecision::DeleteAll { confirmed };
    let mut outcome = SetAsideBulkDeletion::default();
    let mut last_deleted = None;
    for listed in confirmed {
        let path = area.join(&listed.file_name);
        if ensure_inside(data_dir, &path).is_err() {
            outcome.failed = outcome.failed.saturating_add(1);
            continue;
        }
        let now = match fingerprint_of(&path, &listed.file_name) {
            Ok(Some(now)) => now,
            Ok(None) => {
                outcome.kept = outcome.kept.saturating_add(1);
                continue;
            }
            Err(error) => {
                tracing::warn!(
                    "Could not check preserved draft {}: {error}",
                    path.display()
                );
                outcome.failed = outcome.failed.saturating_add(1);
                continue;
            }
        };
        if !may_delete(&now, decision) {
            outcome.kept = outcome.kept.saturating_add(1);
            continue;
        }
        match fs_mutate::remove_file_if_exists(&path) {
            Ok(_) => {
                outcome.deleted = outcome.deleted.saturating_add(1);
                last_deleted = Some(path);
            }
            Err(error) => {
                tracing::warn!(
                    "Could not delete preserved draft {}: {error}",
                    path.display()
                );
                outcome.failed = outcome.failed.saturating_add(1);
            }
        }
    }
    if let Some(path) = last_deleted
        && let Err(error) = fs_write::sync_parent_dir(&path)
    {
        tracing::warn!(
            "Could not sync {} after deleting preserved drafts: {error}",
            area.display()
        );
        outcome.durability_unconfirmed = true;
    }
    outcome
}

/// Refuse any path that is not a direct child of the set-aside area.
fn ensure_inside(data_dir: &Path, path: &Path) -> Result<()> {
    let area = dir(data_dir);
    let is_child = path.parent() == Some(area.as_path())
        && path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .and_then(parse_name)
            .is_some();
    if !is_child {
        anyhow::bail!("{} is not a set-aside draft body", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;

    #[test]
    fn set_aside_names_parse_with_and_without_a_collision_suffix() {
        assert_eq!(
            parse_name("abcdef0123456789.1700000000.draft"),
            Some(("abcdef0123456789".to_string(), 1_700_000_000))
        );
        assert_eq!(
            parse_name("untitled-0000000000000001.7-3.draft"),
            Some(("untitled-0000000000000001".to_string(), 7))
        );
        assert_eq!(parse_name("abc.draft"), None);
        assert_eq!(parse_name("abc.12x.draft"), None);
        assert_eq!(parse_name(".12.draft"), None);
        assert_eq!(parse_name("abc.12.txt"), None);
    }

    /// Seed `count` bodies with stamps `1..=count`, created in both orders
    /// (odd stamps newest-first, then even stamps oldest-first), so whichever
    /// order the filesystem returns entries in, some of the newest stamps
    /// come after the first 256 directory entries.
    fn seed_bodies(data_dir: &Path, count: u64) {
        let area = dir(data_dir);
        fixture::create_dir_all(&area);
        let odd = (1..=count).rev().filter(|stamp| stamp % 2 == 1);
        let even = (1..=count).filter(|stamp| stamp % 2 == 0);
        for stamp in odd.chain(even) {
            fixture::write_text(&area.join(format!("abcdef0123456789.{stamp}.draft")), "x");
        }
    }

    #[test]
    fn listing_past_the_row_limit_shows_the_newest_bodies() {
        let data = tempfile::tempdir().expect("tempdir");
        seed_bodies(data.path(), 300);

        let listing = list(data.path()).expect("list");

        let stamps: Vec<u64> = listing.rows.iter().map(|body| body.stamp_secs).collect();
        let newest: Vec<u64> = (45..=300).rev().collect();
        assert_eq!(stamps, newest, "the newest 256 bodies, newest first");
        assert_eq!((listing.totals.count, listing.totals.bytes), (300, 300));
        assert!(listing.totals.complete, "300 entries fit the scan budget");
        assert!(
            listing.totals.is_truncated(listing.rows.len()),
            "not every body is listed"
        );
        let totals_only = totals(data.path()).expect("totals");
        assert_eq!(totals_only, listing.totals, "the totals-only scan agrees");
    }

    #[test]
    fn a_confirmed_bulk_delete_removes_only_the_unchanged_listed_bodies() {
        let data = tempfile::tempdir().expect("tempdir");
        let area = dir(data.path());
        fixture::create_dir_all(&area);
        fixture::write_text(&area.join("aaaa.1.draft"), "one");
        fixture::write_text(&area.join("bbbb.2.draft"), "two");
        fixture::write_text(&area.join("cccc.3.draft"), "three");
        let listing = list(data.path()).expect("list");
        let confirmed: Vec<SetAsideFingerprint> = listing
            .rows
            .iter()
            .map(|body| body.fingerprint.clone())
            .collect();
        // After the confirmation: one listed body is replaced by a different
        // file under its name, and a new body is set aside.
        fixture::remove_file(&area.join("bbbb.2.draft"));
        fixture::write_text(&area.join("bbbb.2.draft"), "two, rewritten");
        fixture::write_text(&area.join("dddd.4.draft"), "four");

        let outcome = delete_confirmed(data.path(), &confirmed);

        assert_eq!(
            outcome,
            SetAsideBulkDeletion {
                deleted: 2,
                kept: 1,
                failed: 0,
                durability_unconfirmed: false,
            }
        );
        let mut left = fixture::entry_names(&area);
        left.sort();
        assert_eq!(left, vec!["bbbb.2.draft", "dddd.4.draft"]);
    }

    #[test]
    fn a_bulk_delete_with_no_matching_fingerprint_deletes_nothing() {
        let data = tempfile::tempdir().expect("tempdir");
        let area = dir(data.path());
        fixture::create_dir_all(&area);
        fixture::write_text(&area.join("aaaa.1.draft"), "one");
        let mut stale = list(data.path()).expect("list").rows[0].fingerprint.clone();
        stale.byte_size += 1;
        let outside = SetAsideFingerprint {
            file_name: "../manifest.json".to_string(),
            byte_size: 0,
            identity: None,
            modified_at_nanos: None,
        };

        let outcome = delete_confirmed(data.path(), &[stale, outside]);

        assert_eq!((outcome.deleted, outcome.kept, outcome.failed), (0, 1, 1));
        assert!(fixture::exists(&area.join("aaaa.1.draft")));
        assert!(delete_confirmed(data.path(), &[]) == SetAsideBulkDeletion::default());
    }

    #[test]
    fn listing_is_newest_first_and_reading_and_deleting_stay_inside_the_area() {
        let data = tempfile::tempdir().expect("tempdir");
        let empty = list(data.path()).expect("empty list");
        assert!(empty.rows.is_empty() && empty.totals.complete && !empty.totals.is_truncated(0));
        let area = dir(data.path());
        fixture::create_dir_all(&area);
        fixture::write_text(&area.join("aaaa.10.draft"), "older");
        fixture::write_text(&area.join("bbbb.20-1.draft"), "newer");
        fixture::write_text(&area.join("notes.txt"), "ignored");
        let listing = list(data.path()).expect("list");
        assert_eq!((listing.totals.count, listing.totals.bytes), (2, 10));
        assert!(listing.totals.complete && !listing.totals.is_truncated(listing.rows.len()));
        let listed = listing.rows;
        assert_eq!(
            listed
                .iter()
                .map(|body| (body.draft_id.as_str(), body.stamp_secs, body.byte_size()))
                .collect::<Vec<_>>(),
            vec![("bbbb", 20, 5), ("aaaa", 10, 5)]
        );
        assert_eq!(read(data.path(), &listed[0].path).expect("read"), "newer");
        assert!(read(data.path(), &area.join("notes.txt")).is_err());
        assert!(read(data.path(), &data.path().join("aaaa.10.draft")).is_err());
        let outcome = delete_confirmed(data.path(), &[listed[1].fingerprint.clone()]);
        assert_eq!(outcome.deleted, 1, "a per-row Delete of the listed body");
        assert!(!fixture::exists(&listed[1].path));
        assert_eq!(list(data.path()).expect("list").rows.len(), 1);
    }
}
