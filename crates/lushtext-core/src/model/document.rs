// SPDX-License-Identifier: GPL-3.0-or-later

//! Document model — runtime state for an open file in a tab.

use std::path::PathBuf;

/// Runtime identity for an open document (derived from canonical path).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentId(pub PathBuf);
