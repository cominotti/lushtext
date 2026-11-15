// SPDX-License-Identifier: GPL-3.0-or-later

//! Copy side of the `verus-drift` table exchange: the Verus copies over the
//! shared inputs of `../../../table/emit.rs`. Built by `cargo verus build`
//! with Verus's toolchain. Prints the sorted table to stdout.

mod jc {
    pub use lushtext_verus_proofs::journal_core::*;
}

mod pw {
    pub use lushtext_verus_proofs::preview_width::clamped_preview_width;
}

#[verifier::external]
mod table {
    use super::{jc, pw};
    include!("../../../table/emit.rs");
}

#[verifier::external]
fn main() {
    for line in table::emit() {
        println!("{line}");
    }
}
