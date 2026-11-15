// SPDX-License-Identifier: GPL-3.0-or-later

//! Constructors from owned path triples, for tests and benches
//! (`scale-file-index` task 3.6): the per-file row the index stored before
//! the arena, kept as the reference's row type and as fixture input.

use std::path::PathBuf;
use std::sync::Arc;

use crate::model::palette::PaletteFileIdentity;

use super::{FileIndex, FileIndexMutationLedger, MAX_INDEXED_FILES};

/// One indexed file with owned fields: what the index stored per file before
/// the arena (`scale-file-index` D1), and what the reference model stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedIndexedFile {
    /// Absolute path to the file on disk.
    pub path: PathBuf,
    /// Canonical identity captured during index construction.
    pub identity: PaletteFileIdentity,
    /// File name component, lossy UTF-8.
    pub name: String,
    /// The workspace folder that contains this file.
    pub workspace_folder: Arc<PathBuf>,
}

impl OwnedIndexedFile {
    /// Create an indexed file, deriving the name from the path's last component.
    #[must_use]
    pub fn new(
        path: PathBuf,
        identity: PaletteFileIdentity,
        workspace_folder: Arc<PathBuf>,
    ) -> Self {
        let name = path
            .file_name()
            .map_or_default(|name| name.to_string_lossy().into_owned());
        Self {
            path,
            identity,
            name,
            workspace_folder,
        }
    }

    /// Path relative to the workspace folder, for display purposes.
    #[must_use]
    pub fn relative_display(&self) -> String {
        self.path
            .strip_prefix(self.workspace_folder.as_path())
            .map_or_else(
                |_| self.path.display().to_string(),
                |path| path.display().to_string(),
            )
    }
}

fn identity_result(
    identity: PaletteFileIdentity,
) -> Result<PathBuf, crate::model::palette::PaletteFileIdentityFailure> {
    match identity {
        PaletteFileIdentity::Canonical(path) => Ok(path),
        PaletteFileIdentity::Unavailable(failure) => Err(failure),
    }
}

impl FileIndex {
    /// Build an index from owned `(path, identity, workspace folder)` triples,
    /// streamed: the iterator is consumed one triple at a time, so a fixture
    /// of 2^20 files never holds them all at once. Duplicate paths and
    /// canonical identities keep their first occurrence; files beyond
    /// [`MAX_INDEXED_FILES`] or the installed budget are left out.
    #[must_use]
    pub fn from_paths<I>(files: I) -> Self
    where
        I: IntoIterator<Item = (PathBuf, PaletteFileIdentity, Arc<PathBuf>)>,
    {
        let mut index = Self::default();
        let mut ledger = index.incremental_mutation_ledger();
        let mut admitted = 0usize;
        for (path, identity, folder) in files {
            if admitted == MAX_INDEXED_FILES {
                break;
            }
            if index.add_resolved(&path, identity_result(identity), &folder, &mut ledger) {
                admitted += 1;
            }
        }
        index.finish_mutation_batch(&mut ledger);
        index.shrink_arena();
        index
    }

    /// Add a single owned file to the index, as an incremental update would.
    pub fn add_file(&mut self, file: OwnedIndexedFile) {
        let mut ledger = self.incremental_mutation_ledger();
        self.add_file_for_bounded_batch(file, &mut ledger);
        self.finish_mutation_batch(&mut ledger);
    }

    /// Add one owned file inside a mutation batch; `false` when it is a
    /// duplicate or does not fit.
    pub(crate) fn add_file_for_bounded_batch(
        &mut self,
        file: OwnedIndexedFile,
        ledger: &mut FileIndexMutationLedger,
    ) -> bool {
        self.add_resolved(
            &file.path,
            identity_result(file.identity),
            &file.workspace_folder,
            ledger,
        )
    }
}

impl From<Vec<OwnedIndexedFile>> for FileIndex {
    fn from(files: Vec<OwnedIndexedFile>) -> Self {
        Self::from_paths(
            files
                .into_iter()
                .map(|file| (file.path, file.identity, file.workspace_folder)),
        )
    }
}

impl FileIndex {
    /// Every file as an owned row, in source order: the comparison form tests
    /// and the property model use. Allocates one row per file.
    #[must_use]
    pub fn owned_files(&self) -> Vec<OwnedIndexedFile> {
        self.files().map(|file| file.to_owned_file()).collect()
    }
}
