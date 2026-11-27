// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed errors for draft persistence, set-aside preservation, and body I/O.

use std::io;
use std::path::PathBuf;

use crate::filesystem::write::DurableWriteError;

use super::DraftReadError;

/// Typed failures from durably writing a draft body file.
#[derive(Debug, thiserror::Error)]
pub enum DraftBodyWriteError {
    /// Creating the drafts directory failed before writing the body.
    #[error("failed to create drafts dir: {}: {source}", path.display())]
    CreateDir {
        /// Drafts directory path that could not be created.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Resolving the target write identity for the draft body failed.
    #[error("failed to resolve draft target: {}: {source}", path.display())]
    ResolveTarget {
        /// Draft body path whose target identity could not be resolved.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Writing or syncing the draft body failed.
    #[error("failed to write draft: {}: {source}", path.display())]
    Write {
        /// Draft body path that could not be written.
        path: PathBuf,
        /// Underlying phase-classified durable write error.
        #[source]
        source: DurableWriteError,
    },
}

/// Typed failures from deleting one draft body file.
#[derive(Debug, thiserror::Error)]
pub enum DraftDeleteError {
    /// Removing an existing draft body file failed.
    #[error("failed to delete draft {}: {source}", path.display())]
    Delete {
        /// Draft body path that could not be deleted.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
}

/// Typed failures from placing, listing, or reading preserved set-aside bodies.
#[derive(Debug, thiserror::Error)]
pub enum DraftSetAsideError {
    /// Creating the set-aside directory failed.
    #[error("failed to create {}: {source}", path.display())]
    CreateDir {
        /// Set-aside directory path that could not be created.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Reading the source journal body before copying it aside failed.
    #[error("failed to read {}: {source}", path.display())]
    ReadSource {
        /// Journal draft body path that could not be read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Inspecting a candidate set-aside path failed.
    #[error("failed to inspect {}: {source}", path.display())]
    InspectTarget {
        /// Candidate set-aside path whose status could not be read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Syncing the set-aside directory to confirm an existing identical copy failed.
    #[error("failed to confirm the set-aside copy {}: {source}", path.display())]
    ConfirmCopy {
        /// Existing set-aside copy path whose directory entry could not be synced.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// A set-aside copy landed on disk, but its directory sync did not complete.
    #[error(
        "the set-aside copy {} of draft {} is not yet durable: {source}",
        target.display(),
        source_path.display()
    )]
    UnconfirmedCopy {
        /// Set-aside copy path awaiting durable directory sync.
        target: PathBuf,
        /// Original journal draft body path.
        source_path: PathBuf,
        /// Underlying I/O error from the directory sync.
        #[source]
        source: io::Error,
    },
    /// Copying or moving the draft body into the set-aside area failed before taking effect.
    #[error(
        "failed to set aside draft {} as {}: {source}",
        source_path.display(),
        target.display()
    )]
    Place {
        /// Original journal draft body path.
        source_path: PathBuf,
        /// Target set-aside path.
        target: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Every candidate set-aside name in the retry budget was already occupied.
    #[error("no free set-aside name for draft {draft_id} after {attempts} attempts")]
    NameAttemptsExhausted {
        /// Draft ID that could not be assigned a unique set-aside filename.
        draft_id: String,
        /// Retry budget attempted.
        attempts: u32,
    },
    /// Traversing the set-aside directory failed.
    #[error("failed to list {}: {source}", path.display())]
    ListDir {
        /// Set-aside directory path that could not be scanned.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Requested path was not a direct child set-aside body inside the area.
    #[error("{} is not a set-aside draft body", path.display())]
    OutsideSetAsideArea {
        /// Rejected path outside the set-aside area or not matching set-aside naming.
        path: PathBuf,
    },
    /// Reading or UTF-8 decoding the set-aside body failed.
    #[error(transparent)]
    ReadBody(#[from] DraftReadError),
    /// The set-aside body no longer existed when read.
    #[error("{} no longer exists", path.display())]
    Missing {
        /// Missing set-aside body path.
        path: PathBuf,
    },
}

/// Typed failures from preserving a stale draft body before retiring its journal record.
#[derive(Debug, thiserror::Error)]
pub enum PreserveStaleDraftError {
    /// Inspecting the journal draft body path failed.
    #[error("failed to inspect {}: {source}", path.display())]
    InspectBody {
        /// Journal draft body path that could not be inspected.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: io::Error,
    },
    /// Placing the durable set-aside copy failed.
    #[error(transparent)]
    SetAside(#[from] DraftSetAsideError),
}
