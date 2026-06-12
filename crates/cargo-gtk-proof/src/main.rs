// SPDX-License-Identifier: GPL-3.0-or-later

//! Binary entry point for the `cargo gtk-proof` workspace tool.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    match cargo_gtk_proof::run_cli(std::env::args_os().skip(1), &mut stdout, &mut stderr) {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => {
            eprintln!("cargo-gtk-proof failed to write output: {error}");
            ExitCode::from(1)
        }
    }
}
