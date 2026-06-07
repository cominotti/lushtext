# Fuzzing

LushText uses `cargo-fuzz` for hostile byte-ingestion checks that example,
property, mutation, and widget tests do not cover. The fuzz lane feeds arbitrary
bytes into deterministic parser and decoder surfaces without starting GTK.
Committed corpus seeds can also be replayed on stable Rust without running
`cargo-fuzz`.

## Scope

Current fuzz targets:

- `editor_bytes`: raw bytes through editor decoding, encoding-state detection,
  line-ending detection, and file-health classification.
- `markdown_preprocess`: lossy UTF-8 text through Markdown inline-footnote
  preprocessing and `pulldown-cmark` parser setup.
- `operation_script`: raw bytes decoded into a bounded script of deterministic
  editor/service operations, including save-formatting, decode/redecode,
  Markdown preprocessing, replacement previews, session/draft JSON round-trips,
  and raw corrupt session/draft JSON decode attempts.

`markdown_preprocess` is deliberately a text-level target: it converts arbitrary
bytes with lossy UTF-8 before exercising Markdown preprocessing and parser
setup. Raw invalid-UTF-8 behavior belongs to `editor_bytes`, which exercises the
editor byte-decoding, encoding-state, line-ending, and file-health boundary.

Fuzz targets must not start GTK, create widgets, use GSettings, open file
choosers, watch the filesystem, use portals, or require a compositor. Keep live
UI behavior in widget tests and broad deterministic invariants in property
tests.

LibAFL is intentionally not used. The current needs are covered by standard
`cargo-fuzz` discovery, stable corpus replay, and property tests; adding custom
executors, schedulers, feedback, distributed launchers, or fuzzer state
persistence would add framework surface without a matching product need.

## Commands

Install the tool when needed:

```sh
# Fedora/toolbox
sudo dnf install -y gcc-c++

# Rust tooling
rustup toolchain install nightly
cargo install --locked cargo-fuzz --version 0.13.1
```

List configured targets:

```sh
make fuzz-list
cargo +nightly fuzz list
```

Replay committed corpus seeds on stable Rust:

```sh
make fuzz-corpus-replay
```

`make fuzz-corpus-replay` runs ordinary stable Rust tests against the committed
`fuzz/corpus/**` seed files. It does not invoke `cargo-fuzz`, compile
`libfuzzer-sys`, use sanitizer flags, require nightly Rust, or need a C/C++
compiler. Replay is read-only: it does not mutate corpus files and does not
write crash artifacts, coverage output, or generated corpus growth.

Run bounded local smoke:

```sh
make fuzz-smoke
make fuzz-operation-smoke
make fuzz-smoke FUZZ_SMOKE_RUNS=256 FUZZ_SMOKE_SECONDS=15 FUZZ_SMOKE_MAX_LEN=8192
```

`make fuzz-smoke` copies committed seed corpora to a temporary directory before
running each target. That lets libFuzzer discover and save new inputs during the
run without dirtying the checkout.

`make fuzz-operation-smoke` runs only the structured operation target. The
operation script decoder caps raw input length at 4096 bytes, operation count at
32 operations, generated text at 256 bytes, path suffixes at 64 bytes, and
synthetic file/model counts at 3 entries so one fuzz case stays bounded even
during longer runs.

Run a longer manual target:

```sh
cargo +nightly fuzz run editor_bytes fuzz/corpus/editor_bytes -- -max_len=4096 -max_total_time=3600
cargo +nightly fuzz run markdown_preprocess fuzz/corpus/markdown_preprocess -- -max_len=4096 -max_total_time=3600
cargo +nightly fuzz run operation_script fuzz/corpus/operation_script -- -max_len=4096 -max_total_time=3600
```

## Crash Handling

When `cargo-fuzz` reports a crash, it prints a path under
`fuzz/artifacts/<target>/` plus reproduction and minimization commands.

Reproduce:

```sh
cargo +nightly fuzz run editor_bytes fuzz/artifacts/editor_bytes/crash-...
```

Minimize:

```sh
cargo +nightly fuzz tmin editor_bytes fuzz/artifacts/editor_bytes/crash-...
```

Real fixes should include one durable follow-up:

- a minimized corpus seed under `fuzz/corpus/<target>/`,
- a deterministic unit, service, property, or widget regression test, or
- a short rationale in the review explaining why the crash input should not be
  kept as a seed.

Crash artifacts, coverage, and fuzz build output stay ignored. Intentional seed
corpus files are reviewable source files.

## CI Policy

Fuzzing and stable corpus replay are not part of default `make test`, default
property tests, widget tests, benchmark compile checks, or mutation testing.
Stable corpus replay does run in the ordinary GitHub Actions CI workflow through
`make fuzz-corpus-replay`, because it uses stable Rust and replays only
committed seeds through ordinary test tooling.

Coverage-guided `cargo-fuzz` smoke stays out of pull-request CI because it needs
a nightly sanitizer build plus a separate tool install, which would make
pull-request latency less predictable. It runs from the scheduled/manual Fuzz
Smoke workflow with explicit target, run, time, and input-length budgets. Use
`make fuzz-smoke` or `make fuzz-operation-smoke` locally when that latency
budget is acceptable.
