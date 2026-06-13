// SPDX-License-Identifier: GPL-3.0-or-later

//! Regression guard for the workspace folder-set rename.
//!
//! Compatibility bridges still need to mention old sidecar and payload names,
//! but ordinary code, resources, fixtures, and docs should speak in folder terms.

use lushtext_core::services::filesystem::{
    DirectoryScanPolicy, PathStatus, metadata as fs_metadata, read as fs_read, tree as fs_tree,
};
use std::path::{Path, PathBuf};

struct ForbiddenTerm {
    label: &'static str,
    needle: &'static str,
}

const FORBIDDEN_TERMS: &[ForbiddenTerm] = &[
    ForbiddenTerm {
        label: "WorkspaceRoot",
        needle: "workspaceroot",
    },
    ForbiddenTerm {
        label: "workspace_root",
        needle: "workspace_root",
    },
    ForbiddenTerm {
        label: "workspace-root",
        needle: "workspace-root",
    },
    ForbiddenTerm {
        label: "workspace root",
        needle: "workspace root",
    },
    ForbiddenTerm {
        label: "root_paths",
        needle: "root_paths",
    },
    ForbiddenTerm {
        label: "v1_root_payload",
        needle: "v1_root_payload",
    },
    ForbiddenTerm {
        label: "search(query, roots, ...)",
        needle: "search(query, roots",
    },
    ForbiddenTerm {
        label: "Open Workspace Note",
        needle: "open workspace note",
    },
    ForbiddenTerm {
        label: "WorkspaceNote",
        needle: "workspacenote",
    },
    ForbiddenTerm {
        label: "workspace_note",
        needle: "workspace_note",
    },
    ForbiddenTerm {
        label: "workspace-note",
        needle: "workspace-note",
    },
    ForbiddenTerm {
        label: "workspace note",
        needle: "workspace note",
    },
    ForbiddenTerm {
        label: "display_root",
        needle: "display_root",
    },
    ForbiddenTerm {
        label: "canonical_root",
        needle: "canonical_root",
    },
];

const SCAN_TARGETS: &[&str] = &[
    "AGENTS.md",
    "README.md",
    "data",
    "docs",
    "openspec/specs",
    "resources/ui",
    "crates/lushtext-core/benches",
    "crates/lushtext-core/src",
    "crates/lushtext-core/tests",
    "crates/lushtext/src",
    "crates/lushtext/tests",
];

const PATH_SCAN_TARGETS: &[&str] = &[
    "data",
    "resources/ui",
    "crates/lushtext-core/benches",
    "crates/lushtext-core/src",
    "crates/lushtext-core/tests",
    "crates/lushtext/src",
    "crates/lushtext/tests",
];

const SELF: &str = "crates/lushtext-core/tests/workspace_terminology.rs";

#[test]
fn workspace_folder_terminology_does_not_regress() {
    let repo_root = repo_root();
    let mut files = Vec::new();
    for target in SCAN_TARGETS {
        collect_scannable_files(&repo_root.join(target), &mut files);
    }

    let mut findings = Vec::new();
    for file in files {
        let relative = relative_path(&repo_root, &file);
        if relative == SELF {
            continue;
        }

        let content = fs_read::text(&file)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", file.display()));
        for (line_index, line) in content.lines().enumerate() {
            let lowercase_line = line.to_ascii_lowercase();
            for term in FORBIDDEN_TERMS {
                if lowercase_line.contains(term.needle)
                    && !is_allowed_compatibility_reference(&relative, term.needle, line)
                {
                    findings.push(format!(
                        "{}:{} contains {}: {}",
                        relative,
                        line_index + 1,
                        term.label,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        findings.is_empty(),
        "old workspace-root/workspace-note terminology escaped the compatibility boundary:\n{}",
        findings.join("\n")
    );
}

#[test]
fn workspace_folder_paths_do_not_regress_to_old_concepts() {
    let repo_root = repo_root();
    let mut files = Vec::new();
    for target in PATH_SCAN_TARGETS {
        collect_all_paths(&repo_root.join(target), &mut files);
    }

    let mut findings = Vec::new();
    for file in files {
        let relative = relative_path(&repo_root, &file);
        if relative == SELF {
            continue;
        }

        let lowercase_path = relative.to_ascii_lowercase();
        for term in FORBIDDEN_TERMS {
            if lowercase_path.contains(term.needle)
                && !is_allowed_compatibility_path(&relative, term.needle)
            {
                findings.push(format!("{} path contains {}", relative, term.label));
            }
        }
    }

    assert!(
        findings.is_empty(),
        "old workspace-root/workspace-note terminology escaped live path names:\n{}",
        findings.join("\n")
    );
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("lushtext repo root")
        .to_path_buf()
}

fn collect_scannable_files(path: &Path, files: &mut Vec<PathBuf>) {
    match path_status(path) {
        PathStatus::Missing | PathStatus::Other => {}
        PathStatus::File => {
            if is_scannable_file(path) {
                files.push(path.to_path_buf());
            }
        }
        PathStatus::Directory => {
            let entries = fs_tree::scan_directory(path, terminology_scan_policy())
                .unwrap_or_else(|err| panic!("failed to scan {}: {err}", path.display()));
            for entry in entries {
                collect_scannable_files(&entry.path, files);
            }
        }
    }
}

fn collect_all_paths(path: &Path, files: &mut Vec<PathBuf>) {
    match path_status(path) {
        PathStatus::Missing => {}
        PathStatus::File | PathStatus::Other => {
            files.push(path.to_path_buf());
        }
        PathStatus::Directory => {
            files.push(path.to_path_buf());
            let entries = fs_tree::scan_directory(path, terminology_scan_policy())
                .unwrap_or_else(|err| panic!("failed to scan {}: {err}", path.display()));
            for entry in entries {
                collect_all_paths(&entry.path, files);
            }
        }
    }
}

#[must_use]
fn path_status(path: &Path) -> PathStatus {
    fs_metadata::path_status(path)
        .unwrap_or_else(|err| panic!("failed to inspect {}: {err}", path.display()))
}

#[must_use]
const fn terminology_scan_policy() -> DirectoryScanPolicy {
    DirectoryScanPolicy {
        max_entries: usize::MAX,
        include_hidden: true,
    }
}

fn is_scannable_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("json" | "md" | "rs" | "ui" | "xml" | "in")
    )
}

fn relative_path(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .expect("path should be inside repo")
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_allowed_compatibility_reference(relative: &str, needle: &str, line: &str) -> bool {
    match needle {
        "workspace-note" | "workspace_note" | "workspace note" | "workspacenote" => {
            is_legacy_folder_note_compatibility(relative, line)
        }
        "display_root" | "canonical_root" => is_legacy_folder_note_identity_field(relative),
        _ => false,
    }
}

fn is_legacy_folder_note_compatibility(relative: &str, line: &str) -> bool {
    let line = line.to_ascii_lowercase();
    matches!(
        relative,
        "README.md"
            | "docs/recovery-reliability.md"
            | "openspec/specs/workspace-notes/spec.md"
            | "crates/lushtext-core/src/model/migration_ledger.rs"
            | "crates/lushtext-core/src/services/folder_note_service.rs"
            | "crates/lushtext-core/src/services/format_upgrade/diagnostics.rs"
            | "crates/lushtext-core/src/services/format_upgrade/inventory.rs"
            | "crates/lushtext-core/src/services/json_format.rs"
            | "crates/lushtext-core/src/services/note_storage.rs"
            | "crates/lushtext-core/tests/persistent_json_format.rs"
            | "crates/lushtext-core/tests/fixtures/persistent_json/legacy-folder-note-sidecar-v1.json"
    ) && (line.contains("legacy")
        || line.contains("older")
        || line.contains("workspace-note-sidecar")
        || line.contains("workspace-notes")
        || line.contains("workspacenotes")
        || line.contains("compatibility"))
}

fn is_legacy_folder_note_identity_field(relative: &str) -> bool {
    matches!(
        relative,
        "crates/lushtext-core/src/model/folder_note.rs"
            | "crates/lushtext-core/src/services/folder_note_service.rs"
            | "crates/lushtext-core/tests/fixtures/persistent_json/legacy-folder-note-sidecar-v1.json"
    )
}

fn is_allowed_compatibility_path(relative: &str, needle: &str) -> bool {
    matches!(
        (relative, needle),
        (
            "crates/lushtext-core/tests/fixtures/persistent_json/legacy-folder-note-sidecar-v1.json",
            "workspace-note" | "workspace_note" | "workspace note" | "workspacenote"
        )
    )
}
