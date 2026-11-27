// SPDX-License-Identifier: GPL-3.0-or-later

//! Criterion benchmarks for pure domain-model policies and scanners.

use std::cell::Cell;
use std::collections::VecDeque;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use lushtext_model::document::document_scale::DocumentScale;
use lushtext_model::document::editor_memory::{
    EDITOR_MEMORY_UPPER_BUDGET_BYTES, EditorResidency, EditorResidencyLedger,
    estimate_live_editor_bytes, evaluate_editor_memory_budget,
};
use lushtext_model::document::file_load::{
    FileLoadAdmissionPolicy, FileLoadAdmissionRequest, FileLoadPriority,
    TRANSIENT_LOAD_SHARED_BUDGET_BYTES, next_install_boundary, transient_load_weight,
};
use lushtext_model::document::paragraph_boundary::{DocumentProfile, ProfileSummary};
use lushtext_model::values::{AccessGeneration, Bytes, CharCount, EditorId, PolicyGeneration};

fn bench_document_scale(c: &mut Criterion) {
    let mut group = c.benchmark_group("document_scale");

    // The policy itself: a handful of comparisons per profile.
    let profiles: &[(&str, ProfileSummary)] = &[
        (
            "short_lines",
            ProfileSummary {
                total_bytes: 500_000,
                longest_paragraph_chars: 120,
                worst_batch_chars: 20_000,
            },
        ),
        (
            "giant_paragraph",
            ProfileSummary {
                total_bytes: 5_000_000,
                longest_paragraph_chars: 1_900_000,
                worst_batch_chars: 1_900_000,
            },
        ),
        (
            "too_large",
            ProfileSummary {
                total_bytes: 1_000_000_000,
                longest_paragraph_chars: 80,
                worst_batch_chars: 18_000,
            },
        ),
    ];
    for &(label, profile) in profiles {
        group.bench_with_input(BenchmarkId::new("for_profile", label), &profile, |b, &p| {
            b.iter(|| DocumentScale::for_profile(black_box(p)));
        });
    }

    // The worker-side paragraph profile over 10 MB: many short lines, one
    // giant paragraph, and mixed GTK delimiters.
    let short_lines = format!("{}\n", "a".repeat(79)).repeat(125_000);
    let giant = "g".repeat(10_000_000);
    let mixed = "abc\r\ndef\rghi\u{2029}jkl\n".repeat(400_000);
    for (label, text) in [
        ("short_lines_10mb", &short_lines),
        ("giant_10mb", &giant),
        ("mixed_10mb", &mixed),
    ] {
        group.bench_with_input(BenchmarkId::new("profile", label), text, |b, text| {
            b.iter(|| DocumentProfile::of_text(black_box(text)));
        });
    }
    group.finish();
}

/// Benchmark the scalar-only editor memory policy at deliberately large tab counts.
fn bench_editor_memory_policy(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor_memory_policy");
    // One percent makes the 10k-tab clean case traverse many candidates rather
    // than ending after only a handful of evictions.
    let per_page = Bytes::from_bytes(EDITOR_MEMORY_UPPER_BUDGET_BYTES.as_bytes() / 100);

    for tab_count in [1_000usize, 10_000, 100_000] {
        for &(label, eligible) in &[("clean", true), ("protected", false)] {
            let pages = (0..tab_count)
                .map(|editor_id| EditorResidency {
                    editor_id: EditorId::new(editor_id),
                    estimated_bytes: per_page,
                    access_generation: AccessGeneration::new(
                        u64::try_from(editor_id).expect("benchmark generation"),
                    ),
                    policy_generation: PolicyGeneration::new(1),
                    eligible_for_eviction: eligible,
                })
                .collect::<Vec<_>>();
            group.bench_function(BenchmarkId::new(format!("{tab_count}_tabs"), label), |b| {
                b.iter(|| evaluate_editor_memory_budget(black_box(&pages)));
            });
        }
    }

    let bookkeeping_heavy = (0..10_000usize)
        .map(|editor_id| EditorResidency {
            editor_id: EditorId::new(editor_id),
            estimated_bytes:
                lushtext_model::document::editor_memory::EVICTED_EDITOR_BOOKKEEPING_BYTES
                    .saturating_add(Bytes::from_bytes(1)),
            access_generation: AccessGeneration::new(
                u64::try_from(editor_id).expect("benchmark generation"),
            ),
            policy_generation: PolicyGeneration::new(1),
            eligible_for_eviction: true,
        })
        .chain(std::iter::once(EditorResidency {
            editor_id: EditorId::new(10_000),
            estimated_bytes: EDITOR_MEMORY_UPPER_BUDGET_BYTES,
            access_generation: AccessGeneration::new(10_000),
            policy_generation: PolicyGeneration::new(1),
            eligible_for_eviction: false,
        }))
        .collect::<Vec<_>>();
    group.bench_function("10k_tabs/bookkeeping_heavy", |b| {
        b.iter(|| evaluate_editor_memory_budget(black_box(&bookkeeping_heavy)));
    });

    let incremental_pages = (0..100_000usize)
        .map(|editor_id| EditorResidency {
            editor_id: EditorId::new(editor_id),
            estimated_bytes: Bytes::from_bytes(1024),
            access_generation: AccessGeneration::new(
                u64::try_from(editor_id).expect("benchmark generation"),
            ),
            policy_generation: PolicyGeneration::new(1),
            eligible_for_eviction: false,
        })
        .collect::<Vec<_>>();
    let mut incremental = EditorResidencyLedger::default();
    incremental.reconcile(incremental_pages.iter().copied());
    let edited_generation = Cell::new(1u64);
    group.bench_function("100k_tabs/incremental_edit", |b| {
        b.iter(|| {
            let generation = edited_generation.get().wrapping_add(1);
            edited_generation.set(generation);
            black_box(incremental.upsert(EditorResidency {
                editor_id: EditorId::new(50_000),
                estimated_bytes: Bytes::from_bytes(1025),
                access_generation: AccessGeneration::new(50_000),
                policy_generation: PolicyGeneration::new(generation),
                eligible_for_eviction: false,
            }))
        });
    });
    eprintln!(
        "editor-memory-incremental-evidence records={} records_touched_per_edit=1 full_scans_below_threshold=0 aggregate_bytes={}",
        incremental.len(),
        incremental.total_bytes(),
    );

    for &(label, characters, file_bytes) in &[
        (
            "ascii_growth",
            CharCount::new(10_000_000),
            Some(Bytes::from_bytes(10_000_000)),
        ),
        (
            "unicode_growth",
            CharCount::new(10_000_000),
            Some(Bytes::from_bytes(30_000_000)),
        ),
    ] {
        group.bench_function(BenchmarkId::new("live_estimate", label), |b| {
            b.iter(|| {
                estimate_live_editor_bytes(
                    black_box(characters),
                    black_box(file_bytes),
                    black_box(false),
                )
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Transient file-load admission and bounded installation planning
// ---------------------------------------------------------------------------

fn run_transient_load_policy(weights: &[u64], cancel_even: bool) -> (u64, usize) {
    let mut policy = FileLoadAdmissionPolicy::default();
    for (index, weight) in weights.iter().copied().enumerate() {
        let request_id = u64::try_from(index + 1).expect("benchmark request id");
        policy.queue(FileLoadAdmissionRequest {
            request_id,
            owner_id: request_id,
            sequence: request_id,
            weight,
            priority: if index % 7 == 0 {
                FileLoadPriority::Active
            } else {
                FileLoadPriority::Normal
            },
        });
        if cancel_even && index % 2 == 0 {
            assert!(policy.cancel_queued(request_id));
        }
    }

    let mut active = VecDeque::new();
    loop {
        if let Some(grant) = policy.admit_next(false) {
            active.push_back(grant.request_id);
            continue;
        }
        if let Some(request_id) = active.pop_front() {
            assert!(policy.release(request_id));
            continue;
        }
        break;
    }
    let snapshot = policy.snapshot();
    (snapshot.high_water_weight, snapshot.queued_count)
}

fn bench_transient_file_load(c: &mut Criterion) {
    let evidence_weights = [transient_load_weight(8 * 1024 * 1024); 8];
    let mut evidence_policy = FileLoadAdmissionPolicy::default();
    for (index, weight) in evidence_weights.iter().copied().enumerate() {
        let request_id = u64::try_from(index + 1).expect("evidence request id");
        evidence_policy.queue(FileLoadAdmissionRequest {
            request_id,
            owner_id: request_id,
            sequence: request_id,
            weight,
            priority: FileLoadPriority::Normal,
        });
    }
    while evidence_policy.admit_next(false).is_some() {}
    let evidence = evidence_policy.snapshot();
    eprintln!(
        "transient-load-policy-evidence active_payload_weight={} queued_scalar_count={} high_water_weight={} shared_budget={}",
        evidence.active_weight,
        evidence.queued_count,
        evidence.high_water_weight,
        TRANSIENT_LOAD_SHARED_BUDGET_BYTES
    );

    let mut group = c.benchmark_group("transient_file_load");
    let many_small = vec![transient_load_weight(64 * 1024); 512];
    group.bench_function("admission/many_small_512", |b| {
        b.iter(|| black_box(run_transient_load_policy(&many_small, false)));
    });

    let concurrent_large = vec![transient_load_weight(8 * 1024 * 1024); 8];
    group.bench_function("admission/concurrent_large_8", |b| {
        b.iter(|| black_box(run_transient_load_policy(&concurrent_large, false)));
    });

    let exclusive_near_limit = [transient_load_weight(
        lushtext_model::document::document_scale::REFUSE_TO_OPEN - 1,
    )];
    group.bench_function("admission/exclusive_near_supported_limit", |b| {
        b.iter(|| black_box(run_transient_load_policy(&exclusive_near_limit, false)));
    });

    let stale_queue = vec![transient_load_weight(256 * 1024); 1024];
    group.bench_function("admission/stale_queued_1024", |b| {
        b.iter(|| black_box(run_transient_load_policy(&stale_queue, true)));
    });

    let pattern = "🙂é\n";
    let unicode_text = pattern.repeat((50 * 1024 * 1024) / pattern.len());
    group.bench_function("install_boundaries/unicode_50_mib", |b| {
        b.iter(|| {
            let mut start = 0;
            let mut slices = 0u64;
            while start < unicode_text.len() {
                start = next_install_boundary(black_box(&unicode_text), start);
                slices = slices.saturating_add(1);
            }
            black_box(slices)
        });
    });
    // One 50 MiB paragraph, the main-thread worst case of the slicer: ASCII,
    // and typographic text full of `0xE2`-led scalars that are not U+2029.
    let single_line_ascii = "lorem ipsum dolor sit amet ".repeat((50 * 1024 * 1024) / 27);
    let typographic = "it\u{2019}s \u{2014} \u{201C}quoted\u{201D}\u{2026} ";
    let single_line_typographic = typographic.repeat((50 * 1024 * 1024) / typographic.len());
    for (name, text) in [
        (
            "install_boundaries/single_line_ascii_50_mib",
            &single_line_ascii,
        ),
        (
            "install_boundaries/single_line_typographic_50_mib",
            &single_line_typographic,
        ),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| {
                let mut start = 0;
                let mut slices = 0u64;
                while start < text.len() {
                    start = next_install_boundary(black_box(text), start);
                    slices = slices.saturating_add(1);
                }
                black_box(slices)
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_document_scale,
    bench_editor_memory_policy,
    bench_transient_file_load,
);
criterion_main!(benches);
