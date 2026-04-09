// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for LushText GTK4 UI components.
//!
//! These tests require a display server. For headless environments, use
//! `mutter --headless` — see `.github/workflows/ci.yml` for the full invocation.
//! They verify widget construction, property behavior, and signal wiring.
//!
//! GTK widgets must remain on one stable thread for the lifetime of the test
//! process, so this test target uses a custom single-threaded harness instead
//! of Rust's default libtest runner.

use std::io::{self, Write};
use std::panic::{self, AssertUnwindSafe};
use std::process::Command;
use std::process::ExitCode;

include!(concat!(env!("OUT_DIR"), "/widget_test_registry.rs"));

fn main() -> ExitCode {
    if let Ok(test_name) = std::env::var("LUSHTEXT_WIDGET_CHILD") {
        return run_single_test(&test_name);
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    let exact = args.iter().any(|arg| arg == "--exact");
    let skip_filters: Vec<String> = args
        .windows(2)
        .filter_map(|window| {
            if window[0] == "--skip" {
                Some(window[1].clone())
            } else {
                None
            }
        })
        .collect();
    let name_filters: Vec<String> = args
        .iter()
        .filter(|arg| !arg.starts_with('-') && !skip_filters.contains(arg))
        .cloned()
        .collect();

    let selected: Vec<_> = all_widget_tests()
        .into_iter()
        .filter(|(name, _)| matches_filters(name, &name_filters, &skip_filters, exact))
        .collect();

    println!("running {} tests", selected.len());

    let mut failed = Vec::new();
    let current_exe = std::env::current_exe().expect("current_exe");
    for (name, _test_fn) in selected {
        print!("test {name} ... ");
        let _ = io::stdout().flush();

        let status = Command::new(&current_exe)
            .env("LUSHTEXT_WIDGET_CHILD", name)
            .status()
            .expect("spawn widget child");
        if status.success() {
            println!("ok");
        } else {
            println!("FAILED");
            failed.push(name.to_string());
        }
    }

    if failed.is_empty() {
        println!("\ntest result: ok. all tests passed");
        ExitCode::SUCCESS
    } else {
        println!("\nfailures:");
        for name in &failed {
            println!("    {name}");
        }
        println!("\ntest result: FAILED. {} failed", failed.len());
        ExitCode::from(101)
    }
}

fn matches_filters(name: &str, filters: &[String], skips: &[String], exact: bool) -> bool {
    let included = if filters.is_empty() {
        true
    } else if exact {
        filters.iter().any(|filter| name == filter)
    } else {
        filters.iter().any(|filter| name.contains(filter))
    };

    included && !skips.iter().any(|skip| name.contains(skip))
}

fn run_single_test(name: &str) -> ExitCode {
    let Some((_, test_fn)) = all_widget_tests()
        .into_iter()
        .find(|(test_name, _)| *test_name == name)
    else {
        eprintln!("unknown widget test: {name}");
        return ExitCode::from(101);
    };

    let result = panic::catch_unwind(AssertUnwindSafe(test_fn));
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(101)
    }
}
