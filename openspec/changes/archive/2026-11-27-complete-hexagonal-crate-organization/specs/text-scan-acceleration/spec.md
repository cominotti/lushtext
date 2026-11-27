## MODIFIED Requirements

### Requirement: Hand-written SIMD kernels are admitted only by measured need
A hand-written SIMD kernel SHALL be added only when a loop remains >=10% of its workflow's measured cost after library rewrite, using runtime dispatch (`is_x86_feature_detected!`) with library fallback, interleaved benchmarks, explicit NEON status, and no build-time CPU requirements. `make check-policy` SHALL enforce the kernel/crate allowlist, build flags, and `memchr`'s `std` feature.

#### Scenario: Baseline x86-64 still runs the shipped binary
- **WHEN** the release, Meson, Flatpak, or Snap build is inspected
- **THEN** it sets no `target-cpu` or `target-feature` flag, and a CPU without AVX2 runs the library-based path

#### Scenario: An unlisted kernel fails the policy check
- **WHEN** a change adds `#[target_feature(enable = "avx2")]` or `is_x86_feature_detected!` in a file the allowlist does not name
- **THEN** `make check-policy` fails and names the file

#### Scenario: memchr loses its runtime dispatch
- **WHEN** the dependency graph resolves `memchr` for `lushtext-ui`, `lushtext-services`, or `lushtext-model` without its `std` feature
- **THEN** `make check-policy` fails, because the accelerated scans would silently fall back to SSE2

#### Scenario: A kernel without a measured need is rejected
- **WHEN** a loop is under 10 % of its workflow's cost after the library-based rewrite
- **THEN** no hand-written kernel is added for it and the measurement is recorded
