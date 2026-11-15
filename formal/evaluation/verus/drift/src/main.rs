// SPDX-License-Identifier: GPL-3.0-or-later

//! Production side of the `verus-drift` table exchange: the real
//! `journal_core` decisions (through the path dependency) and the integer
//! functions extracted from the pinned commit, over the shared inputs of
//! `../table/emit.rs`. Prints the sorted table to stdout.

/// The production journal decisions, plus the one type they take from
/// `model::draft`.
mod jc {
    pub use lushtext_core::model::draft::DraftManifestCompleteness;
    pub use lushtext_core::services::draft_service::journal_core::*;
}

/// `clamped_preview_width` and the constants it reads, as extracted.
#[allow(dead_code)]
mod pw {
    include!(concat!(env!("OUT_DIR"), "/extracted.rs"));
}

mod table {
    use super::{jc, pw};
    include!("../../table/emit.rs");
}

fn main() {
    for line in table::emit() {
        println!("{line}");
    }
}
