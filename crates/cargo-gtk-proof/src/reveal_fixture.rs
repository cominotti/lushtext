// SPDX-License-Identifier: GPL-3.0-or-later

//! Synthetic workspace trees for the `workspace-tree-reveal` scenario type.
//!
//! Each fixture kind is a deterministic folder tree inside the case's own
//! fixture directory plus a versioned `workspaces.json` naming its workspace
//! folders. The plan is recomputed from the case directory whenever the live
//! runner needs a target path, so the synthetic file names never have to be
//! recorded in `case.json` or any other artifact outside the fixture
//! directory: the only place a target name leaves the fixture tree is the
//! D-Bus action parameter itself, which no artifact records.

use std::fs;
use std::path::{Path, PathBuf};

/// Subdirectory of the case fixture directory that holds the workspace tree.
const TREE_DIRECTORY: &str = "workspace-tree";
/// Entries in a revealed directory: enough to cross GTK's ~200-row realized cap.
const LARGE_DIRECTORY_ENTRIES: usize = 400;
/// Entries in the nested and two-workspace directories.
const NESTED_DIRECTORY_ENTRIES: usize = 300;

/// One workspace folder the fixture registers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RevealWorkspace {
    /// Stable workspace id written to `workspaces.json`.
    pub(crate) id: &'static str,
    /// User-visible workspace name; synthetic, carries no file name.
    pub(crate) name: &'static str,
    /// The workspace's single folder.
    pub(crate) folder: PathBuf,
}

/// Everything one fixture kind needs: the tree, the workspaces, and targets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RevealFixturePlan {
    /// Workspaces in sidebar order.
    pub(crate) workspaces: Vec<RevealWorkspace>,
    /// Directories to fill, each with `entry_name(0..count)` files.
    pub(crate) filled_directories: Vec<(PathBuf, usize)>,
    /// A reveal issued before the before-capture, so the target's section is
    /// expanded and on screen at rest (`already-visible` only).
    pub(crate) setup_target: Option<PathBuf>,
    /// The path the primary action reveals.
    pub(crate) target: PathBuf,
}

/// Synthetic entry name for index `index`; sorts in index order.
pub(crate) fn entry_name(index: usize) -> String {
    format!("entry-{index:05}.txt")
}

/// The workspace tree root for one case directory.
///
/// Always absolute: the artifact root is usually given relative to the
/// working directory, while the app only reveals absolute paths and stores
/// workspace folders as given, so a relative root would be refused as
/// `not-in-workspace`.
pub(crate) fn tree_root(case_dir: &Path) -> PathBuf {
    let root = case_dir.join("fixtures").join(TREE_DIRECTORY);
    std::path::absolute(&root).unwrap_or(root)
}

/// Plan the fixture for `fixture_kind` under `root`.
///
/// # Errors
///
/// Returns an error naming the kind when it is not a supported fixture kind.
pub(crate) fn plan(fixture_kind: &str, root: &Path) -> Result<RevealFixturePlan, String> {
    let alpha = root.join("alpha");
    let alpha_workspace = RevealWorkspace {
        id: "reveal-alpha",
        name: "Reveal Alpha",
        folder: alpha.clone(),
    };
    match fixture_kind {
        "beyond-cap" => Ok(RevealFixturePlan {
            workspaces: vec![alpha_workspace],
            filled_directories: vec![(alpha.clone(), LARGE_DIRECTORY_ENTRIES)],
            setup_target: None,
            target: alpha.join(entry_name(LARGE_DIRECTORY_ENTRIES - 1)),
        }),
        "nested-beyond-cap" => {
            let leaf = alpha.join("branch").join("leaf");
            Ok(RevealFixturePlan {
                workspaces: vec![alpha_workspace],
                filled_directories: vec![(alpha, 3), (leaf.clone(), NESTED_DIRECTORY_ENTRIES)],
                setup_target: None,
                // Entry 250 counted from one.
                target: leaf.join(entry_name(249)),
            })
        }
        "two-workspaces" => {
            let beta = root.join("beta");
            Ok(RevealFixturePlan {
                workspaces: vec![
                    alpha_workspace,
                    RevealWorkspace {
                        id: "reveal-beta",
                        name: "Reveal Beta",
                        folder: beta.clone(),
                    },
                ],
                filled_directories: vec![
                    (alpha, NESTED_DIRECTORY_ENTRIES),
                    (beta.clone(), NESTED_DIRECTORY_ENTRIES),
                ],
                setup_target: None,
                target: beta.join(entry_name(NESTED_DIRECTORY_ENTRIES - 1)),
            })
        }
        "already-visible" => Ok(RevealFixturePlan {
            workspaces: vec![alpha_workspace],
            filled_directories: vec![(alpha.clone(), LARGE_DIRECTORY_ENTRIES)],
            // The setup reveal expands the folder and selects its first entry
            // without scrolling. The target is then the folder row itself,
            // which sits wholly inside the bin's band, margins included, at
            // every matrix size. A row at the band's bottom edge is not a valid
            // target here: in the short-wide size GTK counts its 2 px margin as
            // clipped, asks for 1 px, and the slice bin honours that by
            // scrolling the chrome above it away (the documented price of
            // `child_value - viewport_top` in `scroll_request.rs`).
            setup_target: Some(alpha.join(entry_name(0))),
            target: alpha.clone(),
        }),
        other => Err(format!(
            "unsupported workspace-tree-reveal fixture kind {other}"
        )),
    }
}

/// Create the planned tree and write `workspaces.json` into `data_dir`.
///
/// # Errors
///
/// Returns an error when a directory, file, or the workspace state cannot be
/// written.
pub(crate) fn materialize(plan: &RevealFixturePlan, data_dir: &Path) -> Result<(), String> {
    for workspace in &plan.workspaces {
        fs::create_dir_all(&workspace.folder)
            .map_err(|error| format!("cannot create reveal workspace folder: {error}"))?;
    }
    for (directory, count) in &plan.filled_directories {
        fs::create_dir_all(directory)
            .map_err(|error| format!("cannot create reveal fixture directory: {error}"))?;
        for index in 0..*count {
            fs::write(directory.join(entry_name(index)), b"x\n")
                .map_err(|error| format!("cannot write reveal fixture entry {index}: {error}"))?;
        }
    }
    // Nested ancestors exist through `create_dir_all` above; make sure every
    // target's parent does too, whatever the plan's fill order.
    if let Some(parent) = plan.target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create reveal target parent: {error}"))?;
    }
    let workspaces = plan
        .workspaces
        .iter()
        .map(|workspace| {
            serde_json::json!({
                "id": workspace.id,
                "name": workspace.name,
                "folders": [{
                    "id": format!("{}-folder", workspace.id),
                    "path": workspace.folder,
                }],
            })
        })
        .collect::<Vec<_>>();
    let document = serde_json::json!({
        "kind": "dev.cominotti.lushtext.workspace-state",
        "version": 1,
        "data": {
            "current_scope": { "kind": "all" },
            "workspaces": workspaces,
        }
    });
    let bytes = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("cannot serialize reveal workspaces: {error}"))?;
    fs::create_dir_all(data_dir)
        .map_err(|error| format!("cannot create {}: {error}", data_dir.display()))?;
    fs::write(data_dir.join("workspaces.json"), bytes)
        .map_err(|error| format!("cannot write reveal workspaces: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scrolling_kind_puts_the_target_past_the_realized_cap() {
        let root = Path::new("/fixture");
        for kind in ["beyond-cap", "nested-beyond-cap", "two-workspaces"] {
            let plan = plan(kind, root).expect("plan");
            let parent = plan.target.parent().expect("target parent");
            let entries = plan
                .filled_directories
                .iter()
                .find(|(directory, _)| directory == parent)
                .map(|(_, count)| *count)
                .expect("the revealed directory is filled");
            assert!(entries >= 300, "{kind}: {entries} entries");
            assert!(plan.setup_target.is_none(), "{kind}");
            assert!(plan.target.starts_with(root));
        }
    }

    #[test]
    fn two_workspaces_reveals_in_the_second_section_and_nested_two_levels_down() {
        let root = Path::new("/fixture");
        let two = plan("two-workspaces", root).expect("plan");
        assert_eq!(two.workspaces.len(), 2);
        assert!(two.target.starts_with(&two.workspaces[1].folder));

        let nested = plan("nested-beyond-cap", root).expect("plan");
        let relative = nested
            .target
            .strip_prefix(&nested.workspaces[0].folder)
            .expect("target inside the folder");
        assert_eq!(relative.components().count(), 3);
        assert!(nested.target.ends_with(entry_name(249)));
    }

    #[test]
    fn already_visible_issues_a_setup_reveal_before_the_target() {
        let plan = plan("already-visible", Path::new("/fixture")).expect("plan");
        assert_eq!(
            plan.setup_target,
            Some(Path::new("/fixture/alpha").join(entry_name(0)))
        );
        assert_eq!(plan.target, Path::new("/fixture/alpha"));
    }

    #[test]
    fn an_unknown_kind_is_rejected_by_name() {
        let error = plan("huge", Path::new("/fixture")).expect_err("unknown kind");
        assert!(error.contains("huge"));
    }

    #[test]
    fn materialized_fixture_names_stay_inside_the_fixture_directory() {
        let case = tempfile::tempdir().expect("tempdir");
        let case_dir = case.path();
        let data_dir = case_dir.join("app-data");
        let plan = plan("nested-beyond-cap", &tree_root(case_dir)).expect("plan");
        materialize(&plan, &data_dir).expect("materialize");
        assert!(plan.target.is_file());
        let target_name = plan
            .target
            .file_name()
            .and_then(|name| name.to_str())
            .expect("target name")
            .to_string();
        let fixture_dir = case_dir.join("fixtures");
        let mut stack = vec![case_dir.to_path_buf()];
        let mut checked = 0;
        while let Some(directory) = stack.pop() {
            for entry in fs::read_dir(&directory).expect("read dir") {
                let path = entry.expect("entry").path();
                if path.starts_with(&fixture_dir) {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let text = fs::read_to_string(&path).unwrap_or_default();
                checked += 1;
                assert!(
                    !text.contains(&target_name) && !text.contains("entry-0"),
                    "{} exposes a fixture file name",
                    path.display()
                );
            }
        }
        assert!(checked >= 1, "workspaces.json was checked");
    }

    #[test]
    fn the_tree_root_is_absolute_for_a_relative_case_directory() {
        let root = tree_root(Path::new("build/smoke/visual-geometry/case"));
        assert!(root.is_absolute(), "{}", root.display());
        assert!(root.ends_with("build/smoke/visual-geometry/case/fixtures/workspace-tree"));
    }
}
