// SPDX-License-Identifier: GPL-3.0-or-later

//! Durable retry state for post-rename sidecar and local-history migrations.
//!
//! File and directory renames are user-visible filesystem operations. Related
//! bookmark, note, and local-history sidecars may be migrated afterward, so this
//! model records what still needs retry if that background work is interrupted.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// App-owned data categories that can be migrated after an in-app rename.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum MigrationKind {
    /// Per-file line bookmark sidecars.
    Bookmarks,
    /// Per-file rich document-note sidecars.
    DocumentNotes,
    /// Workspace-folder rich note sidecars.
    // Preserve retry ledgers written before the note concept was renamed to folder notes.
    #[serde(alias = "WorkspaceNotes")]
    FolderNotes,
    /// Per-file local-history lineage directories.
    LocalHistory,
}

impl MigrationKind {
    /// Stable diagnostic label used in logs and tests.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bookmarks => "bookmarks",
            Self::DocumentNotes => "document-notes",
            Self::FolderNotes => "folder-notes",
            Self::LocalHistory => "local-history",
        }
    }
}

/// Retry state for one migration kind inside a broader rename entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationKindState {
    /// Data category tracked by this state record.
    pub kind: MigrationKind,
    /// Whether this kind has completed and no longer needs retry.
    pub completed: bool,
    /// Number of failed attempts observed for this kind.
    pub attempts: u32,
    /// Last retry timestamp in epoch seconds, if any attempt has run.
    pub last_attempt_secs: Option<u64>,
    /// Last failure detail preserved for diagnostics.
    pub last_error: Option<String>,
}

impl MigrationKindState {
    /// Create a pending kind state.
    #[must_use]
    pub const fn pending(kind: MigrationKind) -> Self {
        Self {
            kind,
            completed: false,
            attempts: 0,
            last_attempt_secs: None,
            last_error: None,
        }
    }
}

/// One source-to-target path migration created after an in-app rename.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationEntry {
    /// Monotonic identity for stale completion protection.
    pub generation: u64,
    /// Path before the user-visible rename.
    pub old_path: PathBuf,
    /// Path after the user-visible rename.
    pub new_path: PathBuf,
    /// Creation timestamp in epoch seconds.
    pub created_at_secs: u64,
    /// Last mutation timestamp in epoch seconds.
    pub updated_at_secs: u64,
    /// Per-kind retry and completion state.
    pub kinds: Vec<MigrationKindState>,
}

impl MigrationEntry {
    /// Build a new pending migration entry.
    #[must_use]
    pub fn new(
        generation: u64,
        old_path: PathBuf,
        new_path: PathBuf,
        kinds: &[MigrationKind],
        now_secs: u64,
    ) -> Self {
        let mut entry = Self {
            generation,
            old_path,
            new_path,
            created_at_secs: now_secs,
            updated_at_secs: now_secs,
            kinds: Vec::new(),
        };
        entry.ensure_kinds(kinds, now_secs);
        entry
    }

    /// Return whether this entry tracks the same source and target path.
    #[must_use]
    pub fn matches_paths(&self, old_path: &Path, new_path: &Path) -> bool {
        self.old_path == old_path && self.new_path == new_path
    }

    /// Ensure all requested kinds have a state record on this entry.
    pub fn ensure_kinds(&mut self, kinds: &[MigrationKind], now_secs: u64) {
        for kind in kinds {
            if self.kinds.iter().any(|state| state.kind == *kind) {
                continue;
            }
            self.kinds.push(MigrationKindState::pending(*kind));
        }
        self.kinds.sort_by_key(|state| state.kind);
        self.updated_at_secs = now_secs;
    }

    /// Find a mutable kind state.
    pub fn kind_state_mut(&mut self, kind: MigrationKind) -> Option<&mut MigrationKindState> {
        self.kinds.iter_mut().find(|state| state.kind == kind)
    }

    /// Return incomplete kinds still eligible for retry.
    #[must_use]
    pub fn incomplete_kinds(&self) -> Vec<MigrationKind> {
        self.kinds
            .iter()
            .filter(|state| !state.completed)
            .map(|state| state.kind)
            .collect()
    }

    /// Return whether every tracked kind has completed.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.kinds.iter().all(|state| state.completed)
    }
}

/// Persisted migration ledger document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationLedgerDocument {
    /// Next generation to allocate for newly created entries.
    pub next_generation: u64,
    /// Pending or recently updated migration entries.
    pub entries: Vec<MigrationEntry>,
}

impl Default for MigrationLedgerDocument {
    fn default() -> Self {
        Self {
            next_generation: 1,
            entries: Vec::new(),
        }
    }
}

impl MigrationLedgerDocument {
    /// Allocate a monotonic generation.
    pub fn allocate_generation(&mut self) -> u64 {
        let generation = self.next_generation.max(1);
        self.next_generation = generation.saturating_add(1);
        generation
    }

    /// Find an entry by generation.
    pub fn entry_mut(&mut self, generation: u64) -> Option<&mut MigrationEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.generation == generation)
    }

    /// Remove entries whose tracked kinds all completed.
    pub fn remove_completed(&mut self) {
        self.entries.retain(|entry| !entry.is_complete());
    }
}
