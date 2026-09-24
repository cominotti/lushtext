// SPDX-License-Identifier: GPL-3.0-or-later

//! The Quint `journal.qnt` shapes as serde types, shared by the E1 Quint
//! Connect drivers. Quint sum types are `{ tag, value }` records.

#![allow(dead_code, reason = "each test binary uses a different subset")]

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

// --- The Quint shapes ----------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "tag", content = "value")]
pub enum QDeletionStep {
    Preserve,
    DeleteBody,
    RemoveEntry,
    DeletionDone,
    Stopped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "tag", content = "value")]
pub enum QDeletion {
    NoDeletion,
    Deleting(QDeletionStep),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "tag", content = "value")]
pub enum QEnding {
    Applied,
    Stale,
    Oversized,
    ReadFailed,
    EditedOver,
    InstallCancelled,
    Unavailable,
    MissingBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "tag", content = "value")]
pub enum QAction {
    Edit,
    Register,
    WriteBody,
    Commit,
    Save,
    Discard,
    DeletionStepAction,
    Inspect,
    ExecCleanup,
    RestoreApply(QEnding),
    ExternalMtime,
    Crash,
    Startup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QEditor {
    pub open: bool,
    pub content: i64,
    pub dirty: bool,
    pub token: bool,
    pub written: i64,
    pub restore_pending: bool,
    pub deletion: QDeletion,
    pub preserve_queued: bool,
    pub known_entry: bool,
    pub cleanup_candidate: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QWindow {
    pub editors: BTreeMap<i64, QEditor>,
    pub running: bool,
    pub trusted: bool,
    pub last_reconciliation_complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct QDiskEntry {
    pub present: bool,
    pub backing: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QJournal {
    pub has_other: bool,
    pub other: QWindow,
    pub entry: BTreeMap<i64, QDiskEntry>,
    pub body: BTreeMap<i64, i64>,
    pub backing: BTreeMap<i64, i64>,
    pub preserved: BTreeSet<i64>,
    pub editors: BTreeMap<i64, QEditor>,
    pub running: bool,
    pub trusted: bool,
    pub last_reconciliation_complete: bool,
    pub accepted: BTreeSet<i64>,
    pub resolved: BTreeSet<i64>,
    pub ancestors: BTreeMap<i64, BTreeSet<i64>>,
    pub next_content: i64,
    pub edits: i64,
    pub body_without_entry: bool,
    pub cleanup_unsafe: bool,
}

