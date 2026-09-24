// SPDX-License-Identifier: GPL-3.0-or-later

//! The drafts set-aside area: the **one owner** of every draft body that left
//! the journal without being applied.
//!
//! Bodies arrive here from three routes, all through this module: an
//! unattributable crash leftover moved out by reconciliation ([`move_in`]), a
//! stale or unseen body copied before it is retired or overwritten
//! ([`keep_copy`]), and the earlier body of an id whose registration would
//! overwrite it (also [`keep_copy`]). The set-aside copy owns the bytes: it is
//! never pruned, and nothing but the user's own confirmed Delete on
//! `Preferences > Data` removes one ([`delete`]). A local-history snapshot a
//! stale body may also receive is a browsable **view** of the same edits for
//! the file it belongs to; local history may prune it without any data loss,
//! because this copy is what counts as preserved.
//!
//! The area sits outside the manifest and outside the non-recursive `*.draft`
//! inventory, so neither reconciliation nor orphan cleanup (in this build or an
//! older one) ever classifies, restores, or deletes what it holds. Names are
//! `{draft_id}.{stamp_secs}.draft`, with a `-{n}` suffix on the stamp when that
//! name was already taken, so a new body never replaces an earlier one.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::services::filesystem::{
    DirectoryScanPolicy, FileKind, WriteLabel, metadata as fs_metadata, mutate as fs_mutate,
    read as fs_read, tree as fs_tree, write as fs_write,
};

use super::journal_core::{SetAsideNameStep, SetAsideSlot, set_aside_name_step};
use super::{MAX_AUTOMATIC_DRAFT_BYTES, draft_body_path, drafts_dir, read_draft_path_bounded};

/// Subdirectory of `drafts/` holding preserved bodies that left the journal.
const SET_ASIDE_DIR: &str = "set-aside";
/// Largest number of set-aside names probed for one body before giving up.
const MAX_SET_ASIDE_NAME_ATTEMPTS: u32 = 64;
/// Most set-aside bodies one listing reports; the rest stay on disk untouched.
pub const MAX_LISTED_SET_ASIDE_BODIES: usize = 256;

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

/// The first set-aside name for one body, `{id}.{stamp}.draft`.
///
/// Later collisions take a `-{n}` suffix. A name existing does **not** mean
/// this body is kept: a crash between a body write and its manifest commit
/// leaves a newer body under an unchanged entry stamp, so only a
/// byte-identical file counts as already kept (see [`keep_copy`]).
fn primary_path(data_dir: &Path, draft_id: &str, stamp_secs: u64) -> PathBuf {
    dir(data_dir).join(format!("{draft_id}.{stamp_secs}.draft"))
}

/// The set-aside name tried on one placement attempt.
fn candidate_path(data_dir: &Path, draft_id: &str, stamp_secs: u64, attempt: u32) -> PathBuf {
    if attempt == 0 {
        primary_path(data_dir, draft_id, stamp_secs)
    } else {
        dir(data_dir).join(format!("{draft_id}.{stamp_secs}-{attempt}.draft"))
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
        let target = candidate_path(data_dir, draft_id, stamp_secs, attempt);
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
            Ok(()) => return Ok(target),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) if took_effect(&source, &target, copy_bytes.is_some()) => {
                tracing::warn!(
                    "Set aside draft {} as {}, but its durability is unconfirmed: {error}",
                    source.display(),
                    target.display(),
                );
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
    if fs_metadata::file_facts(target).map_or(true, |facts| facts.byte_size != expected) {
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
    /// The body's size in bytes.
    pub byte_size: u64,
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

/// List the preserved bodies, newest first, at most
/// [`MAX_LISTED_SET_ASIDE_BODIES`]. A missing area is an empty list.
///
/// **Threading:** blocking directory and metadata I/O; call off GTK.
///
/// # Errors
///
/// Returns an error when the area exists but cannot be read.
pub fn list(data_dir: &Path) -> Result<Vec<SetAsideBody>> {
    let dir = dir(data_dir);
    if !fs_metadata::exists(&dir) {
        return Ok(Vec::new());
    }
    let mut bodies = Vec::new();
    fs_tree::visit_directory(
        &dir,
        DirectoryScanPolicy {
            max_entries: MAX_LISTED_SET_ASIDE_BODIES,
            include_hidden: false,
        },
        |entry| {
            if entry.kind != FileKind::File {
                return true;
            }
            let Some((draft_id, stamp_secs)) = parse_name(&entry.file_name) else {
                return true;
            };
            let byte_size = fs_metadata::file_facts(&entry.path).map_or(0, |facts| facts.byte_size);
            bodies.push(SetAsideBody {
                path: entry.path,
                draft_id,
                stamp_secs,
                byte_size,
            });
            true
        },
    )
    .with_context(|| format!("failed to list {}", dir.display()))?;
    bodies.sort_by(|left, right| {
        right
            .stamp_secs
            .cmp(&left.stamp_secs)
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok(bodies)
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

/// Remove one preserved body durably: the file, then its directory entry is
/// synced. Only the user's confirmed Delete calls this.
///
/// # Errors
///
/// Returns an error when the path is not inside the set-aside area or the
/// removal cannot be made durable.
pub fn delete(data_dir: &Path, path: &Path) -> Result<()> {
    ensure_inside(data_dir, path)?;
    fs_mutate::remove_file_if_exists(path)
        .with_context(|| format!("failed to delete {}", path.display()))?;
    fs_write::sync_parent_dir(path)
        .with_context(|| format!("failed to sync the directory of {}", path.display()))
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

    #[test]
    fn listing_is_newest_first_and_reading_and_deleting_stay_inside_the_area() {
        let data = tempfile::tempdir().expect("tempdir");
        assert!(list(data.path()).expect("empty list").is_empty());
        let area = dir(data.path());
        fixture::create_dir_all(&area);
        fixture::write_text(&area.join("aaaa.10.draft"), "older");
        fixture::write_text(&area.join("bbbb.20-1.draft"), "newer");
        fixture::write_text(&area.join("notes.txt"), "ignored");
        let listed = list(data.path()).expect("list");
        assert_eq!(
            listed
                .iter()
                .map(|body| (body.draft_id.as_str(), body.stamp_secs, body.byte_size))
                .collect::<Vec<_>>(),
            vec![("bbbb", 20, 5), ("aaaa", 10, 5)]
        );
        assert_eq!(read(data.path(), &listed[0].path).expect("read"), "newer");
        assert!(read(data.path(), &area.join("notes.txt")).is_err());
        assert!(read(data.path(), &data.path().join("aaaa.10.draft")).is_err());
        delete(data.path(), &listed[1].path).expect("delete");
        assert!(!fixture::exists(&listed[1].path));
        assert_eq!(list(data.path()).expect("list").len(), 1);
    }
}
