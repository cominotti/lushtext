// SPDX-License-Identifier: GPL-3.0-or-later

//! Stable replay tests for committed fuzz corpus seeds.
//!
//! These tests reuse the same feature-gated harnesses as `cargo-fuzz`, but they
//! run through ordinary stable Rust test tooling. They do not discover inputs,
//! minimize crashes, or write fuzz artifacts.

use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use lushtext_core::fuzzing::{
    exercise_editor_bytes_for_fuzzing, exercise_markdown_for_fuzzing,
    exercise_operation_script_for_fuzzing,
};

/// Relative path from the `lushtext-core` crate to the reviewable corpus root.
const CORPUS_ROOT: &str = "../../fuzz/corpus";

#[test]
fn replay_editor_bytes_corpus() {
    replay_corpus("editor_bytes", exercise_editor_bytes_for_fuzzing);
}

#[test]
fn replay_markdown_preprocess_corpus() {
    replay_corpus("markdown_preprocess", exercise_markdown_for_fuzzing);
}

#[test]
fn replay_operation_script_corpus() {
    replay_corpus("operation_script", exercise_operation_script_for_fuzzing);
}

/// Replay every seed for one logical fuzz target and attach the seed path on failure.
fn replay_corpus<T>(target: &str, mut replay_seed: impl FnMut(&[u8]) -> T) {
    let corpus_dir = corpus_root().join(target);
    let seeds = corpus_files(&corpus_dir)
        .unwrap_or_else(|error| panic!("failed to read `{target}` corpus: {error}"));
    assert!(
        !seeds.is_empty(),
        "expected at least one committed `{target}` corpus seed"
    );

    for seed in seeds {
        let bytes = fs::read(&seed).unwrap_or_else(|error| {
            panic!("failed to read corpus seed `{}`: {error}", seed.display())
        });
        let result = panic::catch_unwind(AssertUnwindSafe(|| replay_seed(&bytes)));
        assert!(
            result.is_ok(),
            "fuzz corpus replay target `{target}` failed for seed `{}`",
            seed.display()
        );
    }
}

/// Locate the repository-level fuzz corpus from this crate's manifest directory.
fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS_ROOT)
}

/// Collect committed corpus files recursively so future nested seeds replay automatically.
fn corpus_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_corpus_files(dir, &mut files)?;
    files.sort();
    Ok(files)
}

/// Recursively walk one corpus directory without depending on extra crates.
fn collect_corpus_files(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_corpus_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}
