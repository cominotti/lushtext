// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace-wide content-search services.
//!
//! This service layer stays GTK-free and splits the two main use cases into
//! separate modules: streaming search execution and on-disk replace/undo flows.

mod replace;
mod search;

pub use replace::{apply_replacements, undo_replacements};
pub use search::search;
