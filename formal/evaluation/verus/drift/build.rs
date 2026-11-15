// SPDX-License-Identifier: GPL-3.0-or-later

//! Bring in the integer functions `scripts/formal-evaluation.sh verus-drift`
//! extracted from the pinned commit with `git show` (they are `pub(crate)` in
//! production, so this package cannot call them, and the evaluation makes no
//! production item more visible).

use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=VERUS_DRIFT_EXTRACTED");
    let source = env::var("VERUS_DRIFT_EXTRACTED")
        .expect("VERUS_DRIFT_EXTRACTED is unset: run scripts/formal-evaluation.sh verus-drift");
    println!("cargo:rerun-if-changed={source}");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("extracted.rs");
    fs::copy(&source, out).expect("copy the extracted functions");
}
