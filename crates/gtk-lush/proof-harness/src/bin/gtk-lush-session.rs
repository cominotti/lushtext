// SPDX-License-Identifier: MIT OR Apache-2.0

//! `gtk-lush-session`: the command-line form of the session helper.
//!
//! See `gtk_lush_proof_harness::session` for the stages, profiles and the
//! runtime guard, and `gtk-lush-session --help` for the options.

#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    gtk_lush_proof_harness::session::cli_main()
}
