// SPDX-License-Identifier: GPL-3.0-or-later

//! Sweeping temp files left behind by an interrupted durable write.
//!
//! A durable write creates `.{file}.{tag}.{pid}.{seq}.tmp` beside its target and
//! renames it over the target. A crash between those two steps leaves the temp
//! file behind forever. This module removes such a file only when it is provably
//! LushText's and provably abandoned:
//!
//! - its name parses exactly as `temp_name` (the one owner of the format,
//!   shared with the builder) emits it, with a tag from the closed
//!   `WriteLabel` set;
//! - the embedded process id is not this process, so no write of ours can still
//!   be in flight on it;
//! - it is a regular file (never a symlink, directory, or special file);
//! - it is at least [`STALE_LEFTOVER_AGE`] old, which also keeps a concurrently
//!   running second instance's in-flight write safe.
//!
//! The decision is the pure [`is_sweepable_leftover`]; the executor
//! [`sweep_directory`] applies it to one directory at a time under an explicit
//! [`LeftoverSweepBudget`]. It never recurses: callers name every directory, and
//! in a user workspace the only directory ever named is a just-written target's
//! own, with the sweep restricted to leftovers of that exact target.

use std::ffi::OsStr;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::sys;

/// Minimum age before a matching leftover may be removed.
pub const STALE_LEFTOVER_AGE: Duration = Duration::from_hours(24);

pub use super::temp_name::{DurableTempName, parse as parse_durable_temp_name};

/// What a sweep may remove, and for whom.
#[derive(Debug, Clone, Copy)]
pub enum LeftoverScope<'a> {
    /// An app-data directory: leftovers of any target.
    AppData,
    /// A workspace directory: only leftovers of this exact target file name.
    Target(&'a OsStr),
}

impl LeftoverScope<'_> {
    /// Whether a parsed leftover belongs to this scope.
    fn admits(self, parsed: DurableTempName<'_>) -> bool {
        match self {
            Self::AppData => true,
            Self::Target(target) => target.to_str() == Some(parsed.target_file),
        }
    }
}

/// The facts the sweep decision needs, taken without following symlinks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeftoverFacts {
    /// The entry itself is a regular file.
    pub is_regular_file: bool,
    /// Its modification time, when the platform reports one.
    pub modified: Option<SystemTime>,
}

/// The process and clock a sweep decision is made against.
#[derive(Debug, Clone, Copy)]
pub struct LeftoverClock {
    /// This process's id; with `current_launch` it identifies this launch's
    /// own temp files, which are never swept.
    pub current_pid: u32,
    /// This launch's nonce, carried in the high half of the sequence field.
    /// A pid alone repeats across launches in a pid namespace.
    pub current_launch: u32,
    /// The time the sweep is deciding at.
    pub now: SystemTime,
}

impl LeftoverClock {
    /// The current process at the current time.
    #[must_use]
    pub fn current() -> Self {
        Self {
            current_pid: std::process::id(),
            current_launch: crate::services::filesystem::write::temp_name_launch_nonce(),
            now: SystemTime::now(),
        }
    }
}

/// Decide whether one directory entry is an abandoned durable-write leftover.
///
/// Pure: every fact is passed in. All conditions must hold; a missing mtime or
/// an mtime in the future is never stale.
#[must_use]
pub fn is_sweepable_leftover(
    name: &OsStr,
    facts: LeftoverFacts,
    scope: LeftoverScope<'_>,
    clock: LeftoverClock,
) -> bool {
    let Some(parsed) = name.to_str().and_then(parse_durable_temp_name) else {
        return false;
    };
    let in_scope = scope.admits(parsed);
    let stale = facts.modified.is_some_and(|modified| {
        clock
            .now
            .duration_since(modified)
            .is_ok_and(|age| age >= STALE_LEFTOVER_AGE)
    });
    let this_launch =
        parsed.pid == clock.current_pid && parsed.launch_nonce() == clock.current_launch;
    in_scope && !this_launch && facts.is_regular_file && stale
}

/// Per-pass ceilings for a sweep, shared across every directory it visits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeftoverSweepBudget {
    /// Directory entries that may still be read.
    pub remaining_entries: usize,
    /// Leftovers that may still be removed.
    pub remaining_removals: usize,
}

impl LeftoverSweepBudget {
    /// Budget for the startup sweep of all app-data directories.
    #[must_use]
    pub const fn startup() -> Self {
        Self {
            remaining_entries: 16 * 1024,
            remaining_removals: 256,
        }
    }

    /// Budget for the sweep that follows one successful workspace write.
    #[must_use]
    pub const fn single_target() -> Self {
        Self {
            remaining_entries: 4 * 1024,
            remaining_removals: 16,
        }
    }

    const fn is_exhausted(self) -> bool {
        self.remaining_entries == 0 || self.remaining_removals == 0
    }
}

/// What a sweep did, for logs and tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LeftoverSweepReport {
    /// Directory entries read.
    pub examined: usize,
    /// Leftovers removed.
    pub removed: usize,
    /// Directories or leftovers that could not be read or removed.
    pub failures: usize,
    /// The budget ran out before the directories were fully read.
    pub truncated: bool,
}

/// Sweep one directory, non-recursively, under the shared budget.
///
/// Names are filtered by shape and scope before any metadata call, so an
/// ordinary directory costs one `readdir` pass. Failures are counted, never
/// propagated: a leftover that cannot be removed today is simply left for a
/// later pass.
///
/// **Threading:** Performs blocking filesystem calls. Call from a background
/// thread.
pub fn sweep_directory(
    dir: &Path,
    scope: LeftoverScope<'_>,
    budget: &mut LeftoverSweepBudget,
    report: &mut LeftoverSweepReport,
) {
    if budget.is_exhausted() {
        report.truncated = true;
        return;
    }
    let clock = LeftoverClock::current();
    let mut candidates = Vec::new();
    let visited = sys::visit_directory_names(dir, |name| {
        if budget.remaining_entries == 0 {
            report.truncated = true;
            return false;
        }
        budget.remaining_entries -= 1;
        report.examined += 1;
        if name
            .to_str()
            .and_then(parse_durable_temp_name)
            .is_some_and(|parsed| scope.admits(parsed))
        {
            candidates.push(dir.join(name));
        }
        true
    });
    match visited {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(_) => report.failures += 1,
    }

    for candidate in candidates {
        if budget.remaining_removals == 0 {
            report.truncated = true;
            return;
        }
        let Some(name) = candidate.file_name() else {
            continue;
        };
        let Ok(metadata) = sys::symlink_metadata(&candidate) else {
            continue;
        };
        let facts = LeftoverFacts {
            is_regular_file: metadata.file_type().is_file(),
            modified: metadata.modified().ok(),
        };
        if !is_sweepable_leftover(name, facts, scope, clock) {
            continue;
        }
        match sys::remove_file(&candidate) {
            Ok(()) => {
                budget.remaining_removals -= 1;
                report.removed += 1;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => report.failures += 1,
        }
    }
}

/// List at most `max_entries` subdirectories of `root` for a later sweep,
/// charging every entry read to the shared `budget`.
///
/// Entries are classified without following symlinks, so a symlinked
/// directory is never returned and a sweep can never leave app data through
/// one. Dot-named entries are skipped.
///
/// **Threading:** Performs blocking filesystem calls. Call from a background
/// thread.
pub fn child_directories(
    root: &Path,
    max_entries: usize,
    budget: &mut LeftoverSweepBudget,
    report: &mut LeftoverSweepReport,
) -> Vec<std::path::PathBuf> {
    let mut remaining = max_entries.min(budget.remaining_entries);
    let mut children = Vec::new();
    let visited = sys::visit_directory_names(root, |name| {
        if remaining == 0 {
            return false;
        }
        remaining -= 1;
        budget.remaining_entries -= 1;
        report.examined += 1;
        if !name.as_encoded_bytes().starts_with(b".") {
            let path = root.join(name);
            if sys::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_dir()) {
                children.push(path);
            }
        }
        true
    });
    match visited {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => report.failures += 1,
    }
    if budget.remaining_entries == 0 {
        report.truncated = true;
    }
    children
}

/// Sweep stale leftovers of one just-written workspace target, in its directory only.
///
/// Call only after the durable write of `target` succeeded. `target` should be
/// the resolved path the write used, because the temp file was created beside
/// the resolved target rather than beside a symlink to it.
#[must_use]
pub fn sweep_target_leftovers(target: &Path) -> LeftoverSweepReport {
    let mut report = LeftoverSweepReport::default();
    let Some(file_name) = target.file_name() else {
        return report;
    };
    let dir = super::write::parent_or_current(target);
    let mut budget = LeftoverSweepBudget::single_target();
    sweep_directory(
        dir,
        LeftoverScope::Target(file_name),
        &mut budget,
        &mut report,
    );
    report
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use tempfile::TempDir;

    use super::*;
    use crate::services::filesystem::types::WriteLabel;
    use crate::services::filesystem::{fixture, metadata};

    const OTHER_PID: u32 = 4_000_000_001;

    fn clock() -> LeftoverClock {
        LeftoverClock {
            current_pid: 4_242,
            current_launch: 7,
            now: SystemTime::UNIX_EPOCH + Duration::from_hours(24 * 365),
        }
    }

    fn before_now(offset: Duration) -> SystemTime {
        clock()
            .now
            .checked_sub(offset)
            .expect("test clock is far past the epoch")
    }

    fn stale_file() -> LeftoverFacts {
        LeftoverFacts {
            is_regular_file: true,
            modified: Some(before_now(STALE_LEFTOVER_AGE)),
        }
    }

    fn sweepable(name: &str, facts: LeftoverFacts, scope: LeftoverScope<'_>) -> bool {
        is_sweepable_leftover(OsStr::new(name), facts, scope, clock())
    }

    #[test]
    fn parses_the_exact_builder_pattern_from_the_right() {
        let built = crate::services::filesystem::write::unique_temp_path(
            Path::new("/workspace/notes.v2.md"),
            WriteLabel::SAVE.as_str(),
        );
        let name = built.file_name().and_then(OsStr::to_str).expect("utf-8");

        let parsed = parse_durable_temp_name(name).expect("builder output parses");

        assert_eq!(parsed.target_file, "notes.v2.md");
        assert_eq!(parsed.tag, "save");
        assert_eq!(parsed.pid, std::process::id());
        assert_eq!(
            parse_durable_temp_name(".manifest.json.json.12.0.tmp"),
            Some(DurableTempName {
                target_file: "manifest.json",
                tag: "json",
                pid: 12,
                sequence: 0,
            })
        );
    }

    #[test]
    fn exact_pattern_with_a_foreign_pid_stale_regular_file_is_sweepable() {
        assert!(sweepable(
            &format!(".manifest.json.json.{OTHER_PID}.7.tmp"),
            stale_file(),
            LeftoverScope::AppData,
        ));
    }

    #[test]
    fn unknown_tag_is_never_swept() {
        assert!(!sweepable(
            &format!(".manifest.json.backup.{OTHER_PID}.7.tmp"),
            stale_file(),
            LeftoverScope::AppData,
        ));
    }

    #[test]
    fn current_pid_is_never_swept() {
        let this_launch = (7u64 << 32) | 3;
        assert!(!sweepable(
            &format!(
                ".manifest.json.json.{}.{this_launch}.tmp",
                clock().current_pid
            ),
            stale_file(),
            LeftoverScope::AppData,
        ));
    }

    #[test]
    fn same_pid_from_an_earlier_launch_is_sweepable() {
        // A pid namespace hands every launch the same pid; the launch nonce in
        // the sequence field is what tells an old leftover from a live temp.
        let earlier_launch = (6u64 << 32) | 3;
        assert!(sweepable(
            &format!(
                ".manifest.json.json.{}.{earlier_launch}.tmp",
                clock().current_pid
            ),
            stale_file(),
            LeftoverScope::AppData,
        ));
        assert!(sweepable(
            &format!(".manifest.json.json.{}.3.tmp", clock().current_pid),
            stale_file(),
            LeftoverScope::AppData,
        ));
    }

    #[test]
    fn builder_names_carry_this_launch() {
        let built = crate::services::filesystem::write::unique_temp_path(
            Path::new("/workspace/notes.md"),
            WriteLabel::SAVE.as_str(),
        );
        let name = built.file_name().and_then(OsStr::to_str).expect("utf-8");
        let parsed = parse_durable_temp_name(name).expect("builder output parses");
        let current = LeftoverClock::current();
        assert_eq!(parsed.pid, current.current_pid);
        assert_eq!(parsed.launch_nonce(), current.current_launch);
        assert!(!is_sweepable_leftover(
            OsStr::new(name),
            LeftoverFacts {
                is_regular_file: true,
                modified: current.now.checked_sub(STALE_LEFTOVER_AGE),
            },
            LeftoverScope::AppData,
            current,
        ));
    }

    #[test]
    fn fresh_file_is_never_swept() {
        let almost_stale = LeftoverFacts {
            modified: Some(before_now(
                STALE_LEFTOVER_AGE
                    .checked_sub(Duration::from_secs(1))
                    .expect("stale age exceeds one second"),
            )),
            ..stale_file()
        };
        let future = LeftoverFacts {
            modified: Some(clock().now + Duration::from_hours(1)),
            ..stale_file()
        };
        let unknown = LeftoverFacts {
            modified: None,
            ..stale_file()
        };
        let name = format!(".manifest.json.json.{OTHER_PID}.7.tmp");
        for facts in [almost_stale, future, unknown] {
            assert!(!sweepable(&name, facts, LeftoverScope::AppData));
        }
    }

    #[test]
    fn non_regular_file_is_never_swept() {
        let not_a_file = LeftoverFacts {
            is_regular_file: false,
            ..stale_file()
        };
        assert!(!sweepable(
            &format!(".manifest.json.json.{OTHER_PID}.7.tmp"),
            not_a_file,
            LeftoverScope::AppData,
        ));
    }

    #[test]
    fn look_alike_user_files_are_never_swept() {
        let target = OsStr::new("notes.md");
        for name in [
            // Wrong target in a workspace directory.
            format!(".todo.md.save.{OTHER_PID}.7.tmp"),
            // Not hidden, missing fields, extra suffix, non-canonical numbers.
            format!("notes.md.save.{OTHER_PID}.7.tmp"),
            format!(".notes.md.save.{OTHER_PID}.tmp"),
            format!(".notes.md.save.{OTHER_PID}.7.tmp.bak"),
            format!(".notes.md.save.+{OTHER_PID}.7.tmp"),
            format!(".notes.md.save.{OTHER_PID}.07.tmp"),
            ".notes.md.save.99999999999.7.tmp".to_string(),
            format!("..save.{OTHER_PID}.7.tmp"),
            ".notes.md.tmp".to_string(),
        ] {
            assert!(
                !sweepable(&name, stale_file(), LeftoverScope::Target(target)),
                "{name} must not be swept"
            );
        }
        assert!(sweepable(
            &format!(".notes.md.save.{OTHER_PID}.7.tmp"),
            stale_file(),
            LeftoverScope::Target(target),
        ));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_names_are_never_swept() {
        use std::os::unix::ffi::OsStringExt;

        let name = OsString::from_vec(b".n\xffotes.md.save.4000000001.7.tmp".to_vec());
        assert!(!is_sweepable_leftover(
            &name,
            stale_file(),
            LeftoverScope::AppData,
            clock()
        ));
    }

    fn age(path: &Path) {
        let old = SystemTime::now()
            .checked_sub(STALE_LEFTOVER_AGE + Duration::from_mins(1))
            .expect("wall clock is far past the epoch");
        fixture::set_modified(path, old);
    }

    #[test]
    fn app_data_sweep_removes_only_the_stale_foreign_leftover() {
        let dir = TempDir::new().expect("temp dir");
        let leftover = dir
            .path()
            .join(format!(".manifest.json.json.{OTHER_PID}.3.tmp"));
        let fresh = dir
            .path()
            .join(format!(".manifest.json.json.{OTHER_PID}.4.tmp"));
        // This launch's own temp, named by the real builder (pid + nonce).
        let own = crate::services::filesystem::write::unique_temp_path(
            &dir.path().join("manifest.json"),
            WriteLabel::JSON.as_str(),
        );
        let manifest = dir.path().join("manifest.json");
        for path in [&leftover, &fresh, &own, &manifest] {
            fixture::write_text(path, "{}");
        }
        for path in [&leftover, &own, &manifest] {
            age(path);
        }

        let mut budget = LeftoverSweepBudget::startup();
        let mut report = LeftoverSweepReport::default();
        sweep_directory(dir.path(), LeftoverScope::AppData, &mut budget, &mut report);

        assert_eq!(report.removed, 1);
        assert!(!metadata::exists(&leftover));
        for path in [&fresh, &own, &manifest] {
            assert!(metadata::exists(path), "{} must stay", path.display());
        }
    }

    #[cfg(unix)]
    #[test]
    fn sweep_never_follows_or_removes_a_symlinked_or_directory_leftover() {
        let dir = TempDir::new().expect("temp dir");
        let precious = dir.path().join("precious.txt");
        fixture::write_text(&precious, "keep");
        age(&precious);
        let link = dir
            .path()
            .join(format!(".precious.txt.save.{OTHER_PID}.1.tmp"));
        fixture::symlink(&precious, &link);
        let directory = dir
            .path()
            .join(format!(".other.txt.save.{OTHER_PID}.2.tmp"));
        fixture::create_dir_all(&directory);
        age(&directory);

        let mut budget = LeftoverSweepBudget::startup();
        let mut report = LeftoverSweepReport::default();
        sweep_directory(dir.path(), LeftoverScope::AppData, &mut budget, &mut report);

        assert_eq!(report.removed, 0);
        assert!(metadata::is_symlink(&link).expect("link stays"));
        assert!(metadata::exists(&directory));
        fixture::assert_text(&precious, "keep");
    }

    #[test]
    fn sweep_stops_at_the_removal_budget() {
        let dir = TempDir::new().expect("temp dir");
        for sequence in 0..3 {
            let path = dir
                .path()
                .join(format!(".a.json.json.{OTHER_PID}.{sequence}.tmp"));
            fixture::write_text(&path, "");
            age(&path);
        }

        let mut budget = LeftoverSweepBudget {
            remaining_entries: 100,
            remaining_removals: 2,
        };
        let mut report = LeftoverSweepReport::default();
        sweep_directory(dir.path(), LeftoverScope::AppData, &mut budget, &mut report);

        assert_eq!(report.removed, 2);
        assert!(report.truncated);
        assert_eq!(fixture::entry_names(dir.path()).len(), 1);
    }

    #[test]
    fn sweep_stops_reading_at_the_entry_budget() {
        let dir = TempDir::new().expect("temp dir");
        for index in 0..5 {
            fixture::write_text(&dir.path().join(format!("file-{index}")), "");
        }

        let mut budget = LeftoverSweepBudget {
            remaining_entries: 2,
            remaining_removals: 10,
        };
        let mut report = LeftoverSweepReport::default();
        sweep_directory(dir.path(), LeftoverScope::AppData, &mut budget, &mut report);

        assert_eq!(report.examined, 2);
        assert!(report.truncated);
    }

    #[test]
    fn missing_directory_is_not_a_failure() {
        let dir = TempDir::new().expect("temp dir");
        let mut budget = LeftoverSweepBudget::startup();
        let mut report = LeftoverSweepReport::default();
        sweep_directory(
            &dir.path().join("absent"),
            LeftoverScope::AppData,
            &mut budget,
            &mut report,
        );
        assert_eq!(report, LeftoverSweepReport::default());
    }

    #[test]
    fn target_sweep_removes_only_that_targets_stale_leftovers() {
        let dir = TempDir::new().expect("temp dir");
        let target = dir.path().join("notes.md");
        fixture::write_text(&target, "saved");
        let own_leftover = dir.path().join(format!(".notes.md.save.{OTHER_PID}.9.tmp"));
        let other_target = dir.path().join(format!(".todo.md.save.{OTHER_PID}.9.tmp"));
        for path in [&own_leftover, &other_target] {
            fixture::write_text(path, "partial");
            age(path);
        }

        let report = sweep_target_leftovers(&target);

        assert_eq!(report.removed, 1);
        assert!(!metadata::exists(&own_leftover));
        assert!(metadata::exists(&other_target));
        fixture::assert_text(&target, "saved");
    }
}
