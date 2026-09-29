// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-run temporary-file ownership for the supervising harness parent.
//!
//! Every selected test runs in its own child process, and a child is free to
//! leave files behind: it may panic, abort, crash inside the toolkit, or simply
//! create a fixed-name directory and never remove it. With one child per test,
//! a single leaked directory per child becomes one directory per test per run,
//! which is how a suite of about 1,300 tests once filled a tmpfs's inode table.
//!
//! The parent therefore owns the temporary directory of every child it spawns:
//!
//! - one **run root** `<temp>/<prefix><pid>-<random>` per supervising parent;
//! - one **attempt directory** `<root>/<n>` per child attempt, handed to the
//!   child as `TMPDIR`, so `std::env::temp_dir()`, `tempfile`, and GLib's
//!   `g_get_tmp_dir()` all resolve inside it without the tests cooperating;
//! - the attempt directory is removed as soon as its child exits, and the run
//!   root when the parent returns, on failing runs as well as passing ones.
//!
//! The attempt directory is removed by the parent after the child has exited,
//! not by the child itself: a test may leave worker threads that are still
//! writing into it when the test function returns, and removing a directory
//! out from under them would turn clean teardown into spurious I/O errors.
//!
//! A parent killed outright cannot clean up. The next run's startup sweep
//! reclaims such roots: an entry is removed only when it carries one of the
//! harness's exact prefixes, is a real directory (never a symlink) owned by the
//! current user, names a process that no longer exists, and has not been
//! modified for [`STALE_SCRATCH_MIN_AGE`]. A live run creates and removes an
//! attempt directory per test, which refreshes its root's modification time,
//! so the age floor also protects a live run in another PID namespace whose
//! PID merely looks dead from here.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Minimum idle age before a dead run's scratch root may be swept.
pub(crate) const STALE_SCRATCH_MIN_AGE: Duration = Duration::from_secs(60 * 60);

/// Minimum idle age for legacy entries whose name carries no PID.
///
/// Older harness builds named the compositor runtime directory with a random
/// suffix only, so liveness cannot be proven from the name; a day is far
/// longer than any widget run.
pub(crate) const LEGACY_PIDLESS_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Prefix of the private compositor runtime directory of a headless relaunch.
pub(crate) const RUNTIME_DIR_PREFIX: &str = "gtk-lush-proof-runtime-";

/// How a sweep treats an entry whose name has no PID after the prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PidlessEntries {
    /// Never touch it: the prefix alone does not prove the entry is ours.
    Keep,
    /// Reclaim it once idle for [`LEGACY_PIDLESS_MIN_AGE`].
    ReclaimWhenIdle,
}

/// The one run root a supervising parent owns, removed on drop.
#[derive(Debug)]
pub(crate) struct RunScratch {
    root: tempfile::TempDir,
    next_attempt: usize,
}

impl RunScratch {
    /// Sweep stale roots for `prefix`, then create this run's root.
    pub(crate) fn create(prefix: &str) -> io::Result<Self> {
        let temp_dir = std::env::temp_dir();
        let root = tempfile::Builder::new()
            .prefix(&format!("{prefix}{}-", std::process::id()))
            .tempdir_in(&temp_dir)?;
        // The new root is ours by construction, so its owner is the uid the
        // sweep may act for; no other identity lookup is needed.
        if let Some(owner) = owner_uid(root.path()) {
            sweep_stale_scratch(&temp_dir, prefix, PidlessEntries::Keep, owner);
        }
        Ok(Self {
            root,
            next_attempt: 0,
        })
    }

    /// Create a fresh, empty directory for the next child attempt.
    pub(crate) fn attempt_dir(&mut self) -> io::Result<PathBuf> {
        self.next_attempt += 1;
        let dir = self.root.path().join(self.next_attempt.to_string());
        fs::create_dir(&dir)?;
        Ok(dir)
    }

    /// Remove one attempt directory after its child has exited.
    pub(crate) fn release_attempt(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }
}

/// How long to wait for a finished session's processes to let go of its
/// runtime directory before removing it anyway.
const SESSION_RELEASE_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_RELEASE_POLL: Duration = Duration::from_millis(50);

/// Remove a private session's `XDG_RUNTIME_DIR` once nothing uses it.
///
/// `dbus-run-session` returns as soon as its direct child exits, but services
/// the private bus activated — `xdg-document-portal` above all, which FUSE
/// mounts `doc/` inside the runtime directory — are still shutting down. An
/// immediate `remove_dir_all` then fails on the busy mount and leaves the whole
/// directory behind, which is how a single interrupted-free widget run used to
/// leak one `gtk-lush-proof-runtime-*` directory. Waiting until no process
/// carries this directory as its `XDG_RUNTIME_DIR` makes the removal
/// deterministic; the timeout keeps a wedged service from hanging the run.
pub(crate) fn release_session_runtime_dir(runtime_dir: tempfile::TempDir) {
    let path = runtime_dir.keep();
    let marker = session_marker(&path);
    let deadline = std::time::Instant::now() + SESSION_RELEASE_TIMEOUT;
    while std::time::Instant::now() < deadline && any_process_uses(&marker) {
        std::thread::sleep(SESSION_RELEASE_POLL);
    }
    for _ in 0..5 {
        if fs::remove_dir_all(&path).is_ok() || !path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    eprintln!(
        "gtk-lush-proof-harness: could not remove session runtime directory {}",
        path.display()
    );
}

fn session_marker(path: &Path) -> Vec<u8> {
    let mut marker = b"XDG_RUNTIME_DIR=".to_vec();
    marker.extend_from_slice(path.as_os_str().as_encoded_bytes());
    marker
}

/// Whether any readable process environment has exactly `marker` as an entry.
fn any_process_uses(marker: &[u8]) -> bool {
    let Ok(entries) = fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|entry| {
        let is_pid = entry
            .file_name()
            .to_str()
            .is_some_and(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()));
        is_pid
            && fs::read(entry.path().join("environ"))
                .is_ok_and(|environ| environ.split(|byte| *byte == 0).any(|var| var == marker))
    })
}

/// Reclaim stale entries named `<prefix><pid>` or `<prefix><pid>-<suffix>`.
///
/// Returns the number of entries removed. Unreadable directories, foreign
/// owners, symlinks, live PIDs, and recently touched entries are all left
/// alone; the sweep never fails the run.
pub(crate) fn sweep_stale_scratch(
    temp_dir: &Path,
    prefix: &str,
    pidless: PidlessEntries,
    owner: u32,
) -> usize {
    if !Path::new("/proc/self").exists() {
        // Without procfs a dead PID cannot be told from a live one.
        return 0;
    }
    let Ok(entries) = fs::read_dir(temp_dir) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(rest) = name.to_str().and_then(|name| name.strip_prefix(prefix)) else {
            continue;
        };
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !metadata.is_dir() || owner_of(&metadata) != Some(owner) {
            continue;
        }
        let idle = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .unwrap_or(Duration::ZERO);
        if is_stale(rest, pidless, idle, pid_is_live) && fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Decide whether an entry's name remainder and idle age make it stale.
pub(crate) fn is_stale(
    rest: &str,
    pidless: PidlessEntries,
    idle: Duration,
    pid_is_live: impl Fn(u32) -> bool,
) -> bool {
    match owning_pid(rest) {
        Some(pid) => {
            pid != std::process::id() && !pid_is_live(pid) && idle >= STALE_SCRATCH_MIN_AGE
        }
        None => pidless == PidlessEntries::ReclaimWhenIdle && idle >= LEGACY_PIDLESS_MIN_AGE,
    }
}

/// Parse the PID that a harness-created name carries after its prefix.
fn owning_pid(rest: &str) -> Option<u32> {
    let digits = rest.split_once('-').map_or(rest, |(pid, _)| pid);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn pid_is_live(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).exists()
}

#[cfg(unix)]
fn owner_of(metadata: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;

    Some(metadata.uid())
}

#[cfg(not(unix))]
fn owner_of(_metadata: &fs::Metadata) -> Option<u32> {
    None
}

pub(crate) fn owner_uid(path: &Path) -> Option<u32> {
    fs::symlink_metadata(path)
        .ok()
        .and_then(|metadata| owner_of(&metadata))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: Duration = Duration::from_secs(2 * 60 * 60);
    const ANCIENT: Duration = Duration::from_secs(48 * 60 * 60);

    #[test]
    fn owning_pid_accepts_only_the_harness_name_shapes() {
        assert_eq!(owning_pid("4242"), Some(4242));
        assert_eq!(owning_pid("4242-Ab3xYz"), Some(4242));
        assert_eq!(owning_pid("run-4242"), None);
        assert_eq!(owning_pid("Ab3xYz"), None);
        assert_eq!(owning_pid(""), None);
        assert_eq!(owning_pid("-4242"), None);
        assert_eq!(owning_pid("99999999999"), None);
    }

    #[test]
    fn stale_requires_a_dead_pid_and_an_idle_root() {
        let dead = |_| false;
        let live = |_| true;
        assert!(is_stale("4242-x", PidlessEntries::Keep, OLD, dead));
        assert!(is_stale("4242", PidlessEntries::Keep, OLD, dead));
        assert!(!is_stale("4242-x", PidlessEntries::Keep, OLD, live));
        assert!(!is_stale(
            "4242-x",
            PidlessEntries::Keep,
            Duration::from_secs(59 * 60),
            dead
        ));
        let own = format!("{}-x", std::process::id());
        assert!(!is_stale(&own, PidlessEntries::Keep, ANCIENT, dead));
    }

    #[test]
    fn pidless_entries_are_reclaimed_only_when_the_prefix_allows_it() {
        let dead = |_| false;
        assert!(!is_stale("Ab3xYz", PidlessEntries::Keep, ANCIENT, dead));
        assert!(!is_stale(
            "Ab3xYz",
            PidlessEntries::ReclaimWhenIdle,
            OLD,
            dead
        ));
        assert!(is_stale(
            "Ab3xYz",
            PidlessEntries::ReclaimWhenIdle,
            ANCIENT,
            dead
        ));
    }

    #[test]
    fn sweep_leaves_fresh_foreign_and_non_directory_entries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let owner = owner_uid(temp.path()).expect("owner");
        // A dead PID that is very unlikely to exist, but freshly created: the
        // age floor must protect it.
        let fresh = temp.path().join("gtk-lush-sweep-test-4000000000-fresh");
        fs::create_dir(&fresh).expect("fresh dir");
        let file = temp.path().join("gtk-lush-sweep-test-4000000001");
        fs::write(&file, b"").expect("file");
        let unrelated = temp.path().join("unrelated-4000000002");
        fs::create_dir(&unrelated).expect("unrelated dir");

        let removed = sweep_stale_scratch(
            temp.path(),
            "gtk-lush-sweep-test-",
            PidlessEntries::ReclaimWhenIdle,
            owner,
        );

        assert_eq!(removed, 0);
        assert!(fresh.is_dir() && file.is_file() && unrelated.is_dir());
        let foreign = sweep_stale_scratch(
            temp.path(),
            "gtk-lush-sweep-test-",
            PidlessEntries::ReclaimWhenIdle,
            owner.wrapping_add(1),
        );
        assert_eq!(foreign, 0);
    }

    #[test]
    fn sweep_reclaims_idle_dead_roots_and_legacy_names() {
        let temp = tempfile::tempdir().expect("tempdir");
        let owner = owner_uid(temp.path()).expect("owner");
        let backdate = |path: &Path, idle: Duration| {
            fs::File::open(path)
                .expect("open dir")
                .set_modified(SystemTime::now().checked_sub(idle).expect("past time"))
                .expect("backdate");
        };
        let dead_root = temp.path().join("gtk-lush-sweep-test-4000000000-Ab3xYz");
        fs::create_dir_all(dead_root.join("7/nested")).expect("dead root");
        backdate(&dead_root, OLD);
        let legacy = temp.path().join("gtk-lush-sweep-test-4000000001");
        fs::create_dir(&legacy).expect("legacy dir");
        backdate(&legacy, OLD);
        let live = temp
            .path()
            .join(format!("gtk-lush-sweep-test-{}-live", std::process::id()));
        fs::create_dir(&live).expect("live dir");
        backdate(&live, ANCIENT);
        let pidless = temp.path().join("gtk-lush-sweep-test-Ab3xYz");
        fs::create_dir(&pidless).expect("pidless dir");
        backdate(&pidless, ANCIENT);

        let kept_pidless = sweep_stale_scratch(
            temp.path(),
            "gtk-lush-sweep-test-",
            PidlessEntries::Keep,
            owner,
        );

        assert_eq!(kept_pidless, 2);
        assert!(!dead_root.exists() && !legacy.exists());
        assert!(
            live.is_dir(),
            "the running process's own root is never swept"
        );
        assert!(pidless.is_dir(), "a PID-less name is kept unless allowed");
        let reclaimed = sweep_stale_scratch(
            temp.path(),
            "gtk-lush-sweep-test-",
            PidlessEntries::ReclaimWhenIdle,
            owner,
        );
        assert_eq!(reclaimed, 1);
        assert!(!pidless.exists());
    }

    #[test]
    fn session_release_sees_its_own_process_environment() {
        let own = std::env::var_os("XDG_RUNTIME_DIR");
        if let Some(own) = own {
            assert!(any_process_uses(&session_marker(Path::new(&own))));
        }
        assert!(!any_process_uses(&session_marker(Path::new(
            "/nonexistent/gtk-lush-session-release-test"
        ))));

        let runtime = tempfile::tempdir().expect("runtime dir");
        let path = runtime.path().to_path_buf();
        fs::create_dir(path.join("doc")).expect("doc dir");
        release_session_runtime_dir(runtime);
        assert!(!path.exists());
    }

    #[test]
    fn run_scratch_removes_its_root_and_attempts() {
        let prefix = "gtk-lush-scratch-selftest-";
        let mut scratch = RunScratch::create(prefix).expect("run scratch");
        let root = scratch.root.path().to_path_buf();
        let first = scratch.attempt_dir().expect("first attempt");
        let second = scratch.attempt_dir().expect("second attempt");
        assert_ne!(first, second);
        fs::write(first.join("leak"), b"x").expect("leaked file");
        RunScratch::release_attempt(&first);
        assert!(!first.exists());
        fs::create_dir(second.join("fixed-name-leak")).expect("leaked dir");

        drop(scratch);

        assert!(!root.exists(), "run root must be removed on drop");
    }
}
