// SPDX-License-Identifier: GPL-3.0-or-later

//! Test-process isolation from the developer's real application data.
//!
//! Compiled only under `cfg(test)` or the `test-utils` feature, which no
//! shipping build enables. It holds both halves of the contract:
//!
//! - [`isolated_environment`] names the private XDG base directories and
//!   `LUSHTEXT_DATA_DIR` a test process sets before any test code runs. The
//!   widget-test binary applies it as its first instruction, in the harness
//!   parent and in every child, rooted in the per-attempt temporary directory.
//! - [`assert_app_data_dir_is_isolated`] is the defence in depth:
//!   `json_store::data_dir()` calls it in test builds, so a test that reaches
//!   app data without isolation panics before it can write instead of
//!   overwriting the developer's `session.json` or drafts. It is what holds
//!   the libtest binaries (unit and integration tests), which have no entry
//!   point that runs before their tests and pass explicit temporary
//!   directories instead.
//!
//! On 2026-10-20 a widget test saved a session before GTK was initialized,
//! when only `ensure_gtk_init()` set `LUSHTEXT_DATA_DIR`, and overwrote the
//! developer's real session file.

use std::path::{Path, PathBuf};

use crate::services::filesystem::metadata;

/// Environment variable that overrides the application data directory.
pub const DATA_DIR_ENV: &str = "LUSHTEXT_DATA_DIR";

/// Home-relative trees a test build must never resolve application data into:
/// the XDG base directories at their defaults, and the installed Flatpak's
/// per-application home.
pub const REAL_HOME_APP_DATA_TREES: [&str; 5] = [
    ".local/share",
    ".local/state",
    ".config",
    ".cache",
    ".var/app/dev.cominotti.lushtext",
];

/// The private environment a test process sets under `root` before any test
/// code runs: the four XDG base directories and the application data override,
/// which sits where the application would put it under the private data home.
///
/// The directories are not created; whatever writes into them creates them,
/// inside `root`, which the caller owns and removes.
#[must_use]
pub fn isolated_environment(root: &Path) -> [(&'static str, PathBuf); 5] {
    let data_home = root.join("data");
    [
        (DATA_DIR_ENV, data_home.join("lushtext")),
        ("XDG_DATA_HOME", data_home),
        ("XDG_CONFIG_HOME", root.join("config")),
        ("XDG_CACHE_HOME", root.join("cache")),
        ("XDG_STATE_HOME", root.join("state")),
    ]
}

/// Return whether every variable [`isolated_environment`] sets is present in
/// the current process and lies inside `temp_root`.
#[must_use]
pub fn environment_is_isolated_within(temp_root: &Path) -> bool {
    let temp_root = resolve_existing_ancestor(temp_root);
    isolated_environment(Path::new("")).iter().all(|(key, _)| {
        std::env::var_os(key).is_some_and(|value| {
            let value = Path::new(&value);
            value.is_absolute() && resolve_existing_ancestor(value).starts_with(&temp_root)
        })
    })
}

/// Return the real-home tree `dir` lies in, if any.
///
/// Both sides are compared after resolving their longest existing ancestor, so
/// a symlinked spelling (`/home` versus `/var/home` on Fedora Atomic) cannot
/// slip past a lexical comparison.
#[must_use]
pub fn real_home_app_data_tree(dir: &Path, home: &Path) -> Option<PathBuf> {
    let dir = resolve_existing_ancestor(dir);
    REAL_HOME_APP_DATA_TREES
        .iter()
        .map(|tree| resolve_existing_ancestor(&home.join(tree)))
        .find(|tree| dir.starts_with(tree))
}

/// Refuse application data inside the real home.
///
/// `home` is `$HOME` at the call; a test that fakes `HOME` is therefore held
/// to its fake home, which is how the guard is proved without touching the
/// developer's.
///
/// # Panics
///
/// When `dir` lies under one of [`REAL_HOME_APP_DATA_TREES`] of `home`.
pub fn assert_app_data_dir_is_isolated(dir: &Path, home: Option<&Path>) {
    if let Some(home) = home
        && let Some(tree) = real_home_app_data_tree(dir, home)
    {
        panic!(
            "test isolation violated: LushText app data resolved to {} inside the real home \
             tree {}; a test must run with {DATA_DIR_ENV} and the XDG base directories pointing \
             at a private temporary directory before it touches app data",
            dir.display(),
            tree.display()
        );
    }
}

/// Canonicalize the longest existing ancestor of `path` and re-append the rest.
fn resolve_existing_ancestor(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
    };
    for ancestor in absolute.ancestors() {
        if let Ok(resolved) = metadata::canonical_path(ancestor) {
            let rest = absolute
                .strip_prefix(ancestor)
                .unwrap_or_else(|_| Path::new(""));
            return if rest.as_os_str().is_empty() {
                resolved
            } else {
                resolved.join(rest)
            };
        }
    }
    absolute
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    #[test]
    fn every_real_home_tree_is_refused() {
        let home = TempDir::new().expect("fake home");
        for tree in REAL_HOME_APP_DATA_TREES {
            let dir = home.path().join(tree).join("lushtext");
            assert!(
                real_home_app_data_tree(&dir, home.path()).is_some(),
                "{} must count as real home app data",
                dir.display()
            );
        }
    }

    #[test]
    fn private_directories_are_accepted() {
        let home = TempDir::new().expect("fake home");
        let private = TempDir::new().expect("private data dir");
        assert_eq!(real_home_app_data_tree(private.path(), home.path()), None);
        // A checkout under the home is not application data.
        let checkout = home.path().join("Workspace/lushtext/build/data");
        assert_eq!(real_home_app_data_tree(&checkout, home.path()), None);
        assert_app_data_dir_is_isolated(private.path(), Some(home.path()));
        assert_app_data_dir_is_isolated(private.path(), None);
    }

    #[test]
    fn symlinked_home_spellings_are_refused() {
        let real = TempDir::new().expect("real home");
        let links = TempDir::new().expect("link dir");
        let alias = links.path().join("home-alias");
        fixture::symlink(real.path(), &alias);
        assert!(
            real_home_app_data_tree(&alias.join(".local/share/lushtext"), real.path()).is_some()
        );
        assert!(
            real_home_app_data_tree(&real.path().join(".local/share/lushtext"), &alias).is_some()
        );
    }

    #[test]
    #[should_panic(expected = "test isolation violated")]
    fn the_guard_fires_on_a_real_home_path() {
        // A fake HOME stands in for the developer's: nothing is written, and
        // the guard refuses before any caller could write.
        let home = TempDir::new().expect("fake home");
        assert_app_data_dir_is_isolated(
            &home.path().join(".local/share/lushtext"),
            Some(home.path()),
        );
    }

    #[test]
    #[should_panic(expected = "test isolation violated")]
    fn the_guard_fires_on_the_flatpak_home() {
        let home = TempDir::new().expect("fake home");
        assert_app_data_dir_is_isolated(
            &home
                .path()
                .join(".var/app/dev.cominotti.lushtext/data/lushtext"),
            Some(home.path()),
        );
    }

    #[test]
    fn the_isolated_environment_stays_inside_its_root() {
        let root = Path::new("/tmp/scope/lushtext-test-1");
        let env = isolated_environment(root);
        assert!(env.iter().all(|(_, path)| path.starts_with(root)));
        let keys: Vec<_> = env.iter().map(|(key, _)| *key).collect();
        assert_eq!(
            keys,
            [
                DATA_DIR_ENV,
                "XDG_DATA_HOME",
                "XDG_CONFIG_HOME",
                "XDG_CACHE_HOME",
                "XDG_STATE_HOME"
            ]
        );
        assert_eq!(env[0].1, env[1].1.join("lushtext"));
    }
}
