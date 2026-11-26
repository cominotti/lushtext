// SPDX-License-Identifier: GPL-3.0-or-later

//! The conformance tap: the test-only fault seam of the durable-write shells
//! and of every proven-core shell that runs through `protocol_drive::drive`.
//!
//! It has two sites (`verify-shell-conformance-against-proven-cores` D3):
//!
//! - **the action site**, in `drive`: every action of every shell and the
//!   outcome fed back for it pass through [`before`] and [`after`], tagged
//!   with the protocol kind and the run they belong to (a move's copy is a
//!   nested run). Here the tap records the trace, runs before-action hooks,
//!   and simulates a crash before or after an action;
//! - **the call site**, in `sys.rs`: every fallible backend primitive runs
//!   through [`call`], which records the call's result and, when an
//!   injection is armed for it, returns that error instead of making the
//!   call. The shell then builds its outcome and its recorded error through
//!   its ordinary error path, so an injected fault is handled exactly as a
//!   real one.
//!
//! The tap is a per-invocation, thread-local seam with no process-global
//! slot: [`install`] arms a [`TapPlan`] on the calling thread and returns a
//! [`TapGuard`] that uninstalls it on drop. The durable-write test hooks
//! (`filesystem::write::fail_next_parent_sync_for_test` and its siblings) arm
//! guardless one-entry injections that stay armed on this thread until they
//! fire, as the thread-locals they replaced did.
//!
//! A crash unwinds with a private [`TapCrash`] payload through
//! `std::panic::resume_unwind`, which skips the panic hook. It is faithful
//! only where no `Drop` on the unwind path does I/O and no lock it poisons is
//! read afterwards; the drivers crash only where that holds.
//!
//! Outside `cfg(test)` and the `test-utils` feature every entry point is an
//! `#[inline(always)]` no-op and the module holds no state.

use std::fmt::Debug;
#[cfg(not(any(test, feature = "test-utils")))]
use std::io;
#[cfg(not(any(test, feature = "test-utils")))]
use std::path::Path;

/// Which proven core a `drive` run steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ProtocolKind {
    /// `WriteProtocol`, the atomic write.
    Write,
    /// `MoveProtocol`, the durable move by copy.
    Move,
    /// `RenameProtocol`, the durable rename.
    Rename,
    /// The Replace All per-file protocol.
    ReplaceFile,
    /// The Replace All undo protocol.
    ReplaceUndo,
    /// The Replace All journal shrink.
    Shrink,
}

/// A fallible backend primitive of `sys.rs`, or the post-content point of a
/// write, as the tap names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Call {
    /// `sys::read` and `sys::read_to_string`.
    Read,
    /// The open of a bounded or prefix reader.
    OpenForRead,
    /// `sys::metadata`.
    Metadata,
    /// `sys::symlink_metadata`.
    SymlinkMetadata,
    /// `sys::canonicalize`.
    Canonicalize,
    /// `sys::create_dir`.
    CreateDir,
    /// `sys::create_dir_all`.
    CreateDirAll,
    /// `sys::remove_file`.
    RemoveFile,
    /// `sys::remove_dir`.
    RemoveDir,
    /// `sys::remove_dir_all`.
    RemoveDirAll,
    /// `sys::rename`.
    Rename,
    /// `sys::rename_no_replace`.
    RenameNoReplace,
    /// `sys::free_space`.
    FreeSpace,
    /// The open of a directory traversal.
    OpenDirectory,
    /// `sys::mode`, `sys::inode`, `sys::link_inode`, `sys::descriptor_file_len`.
    Stat,
    /// `sys::sync_dir_descriptor`.
    SyncDir,
    /// `sys::create_temp_file`.
    CreateTemp,
    /// `sys::sync_file`.
    SyncFile,
    /// `sys::required_metadata`.
    RequiredMetadata,
    /// `sys::apply_mode`.
    ApplyMode,
    /// The point after a write's content was written and flushed. It is not a
    /// backend call: a write's `WriteContent` makes none. Only the active tap
    /// records it.
    #[cfg(any(test, feature = "test-utils"))]
    ContentWritten,
}

#[cfg(any(test, feature = "test-utils"))]
mod active;
#[cfg(any(test, feature = "test-utils"))]
pub(crate) use active::*;

/// Shipping builds: the tap compiles to nothing.
#[cfg(not(any(test, feature = "test-utils")))]
mod inert {
    use super::{Call, Debug, Path, ProtocolKind, io};

    /// A `drive` run, as the tap would track it; nothing in shipping builds.
    pub(crate) struct RunGuard;

    impl RunGuard {
        #[inline(always)]
        pub(crate) fn start(_kind: ProtocolKind) -> Self {
            Self
        }
    }

    #[inline(always)]
    pub(crate) fn before<A: Copy + Debug + 'static>(_kind: ProtocolKind, _action: &A) {}

    #[inline(always)]
    pub(crate) fn after<A: Copy + Debug + 'static, O: Copy + Debug + 'static>(
        _kind: ProtocolKind,
        _action: &A,
        _outcome: &O,
    ) {
    }

    #[inline(always)]
    pub(crate) fn call<T>(
        _call: Call,
        _path: &Path,
        primitive: impl FnOnce() -> io::Result<T>,
    ) -> io::Result<T> {
        primitive()
    }

    #[inline(always)]
    pub(crate) fn content_written(_path: &Path) -> io::Result<()> {
        Ok(())
    }

    #[inline(always)]
    pub(crate) fn parent_sync() -> io::Result<()> {
        Ok(())
    }
}
#[cfg(not(any(test, feature = "test-utils")))]
pub(crate) use inert::*;

#[cfg(test)]
mod tests;
