#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Reproduce the Quint vs TLA+ evaluation (docs/next/formal-verification-quint-vs-tlaplus.md)
# and the Verus evaluation (docs/next/formal-verification-verus.md).
#
# LOCAL ONLY. This script is not a prerequisite of check, check-policy, test,
# kani, or end-user-smoke, and no CI workflow calls it. The models it runs
# (formal/evaluation/) are disposable evaluation models, not maintained
# properties; Kani stays the single maintained formal tool.
#
# Usage: scripts/formal-evaluation.sh {install|t1|t2|t3|all|report-data|versions}
#        scripts/formal-evaluation.sh {verus-install|verus-versions|verus-v1|verus-v2|verus-v3|verus-drift|verus-classify|verus-all}
#        scripts/formal-evaluation.sh measure NAME pass|violation -- CMD...
#        scripts/formal-evaluation.sh verus-measure NAME pass|violation -- CMD...
#
# The verus-* targets run several checks each. Run them directly, in the
# inherited cpuset (no taskset pin), and never inside one scripts/heavy.sh
# call: the script wraps every check,
# and every cargo build a target needs, in its own `scripts/heavy.sh --mem 16G`
# call (.agents/rules/verification-runs.md, "Shared-machine agent work").
#
# Environment:
#   FORMAL_EVAL_TIMEOUT   seconds per check (default 1800, the 30-minute cap
#                         the report uses)
#   FORMAL_EVAL_MEM_MB    process-tree RSS ceiling in MiB (default 24576; 14336,
#                         2 GiB under heavy.sh's 16G scope, for verus-*)
#   FORMAL_EVAL_T2_DEPTHS space-separated K8 depths for t2 (default "6 8")
#   FORMAL_EVAL_DIR       tools and runs root (default build/formal-evaluation)
#   FORMAL_EVAL_WORKERS   TLC workers (default 1, comparable with Kani's
#                         single-threaded solver; "auto" for every core).
#                         A value other than 1 suffixes run names with -wN.
#   VERUS_RELEASE         Verus release for verus-* (default the one the report
#                         records; the newest rolling one for re-verification)
#   VERUS_DRIFT_REV       commit verus-drift compares the copies against
#                         (default the commit they were copied from)
#   FORMAL_EVAL_HEAVY_MEM heavy.sh --mem cap for each verus-* check and build
#                         (default 16G; with a smaller cap, set FORMAL_EVAL_MEM_MB
#                         2 GiB under it)
#   FORMAL_EVAL_OBLIGATION  for a verus-measure run expecting `violation`: a
#                         regular expression the failing obligation must match,
#                         so a different failure is not counted as the named one
#
# Requires bash 5 (EPOCHREALTIME), curl, sha256sum, setsid, ps, npm/node, java;
# for verus-*: unzip, rustup, git, patch, rustfmt.
set -euo pipefail

REPO_ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
EVAL_DIR="${FORMAL_EVAL_DIR:-$REPO_ROOT/build/formal-evaluation}"
TOOLS="$EVAL_DIR/tools"
RUNS="$EVAL_DIR/runs"
MODELS="$REPO_ROOT/formal/evaluation"

# Versions resolved on 2026-09-24 (see the report's "Tool versions" section).
QUINT_VERSION=0.32.0
APALACHE_VERSION=0.62.2
# Quint 0.32.0 cannot drive Apalache >= 0.59 (its gRPC config shape changed);
# this is the newest release Quint still drives.
APALACHE_FOR_QUINT_VERSION=0.58.3
TLA_TOOLS_VERSION=1.7.4
declare -A SHA256=(
  [tla2tools.jar]=936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88
  [apalache-0.62.2.tgz]=765f610537281a0f25b8c30f2554f19523e2859c824e80e62276653ee23c10e2
  [apalache-0.58.3.tgz]=ba622db9538aebf942cc7a7815f942a6b2b419012707e16dfdc25a73ff95d0a5
)

TIMEOUT=${FORMAL_EVAL_TIMEOUT:-1800}
MEM_MB=${FORMAL_EVAL_MEM_MB:-24576}
WORKERS=${FORMAL_EVAL_WORKERS:-1}
# TLC's heap, direct or through Quint: three quarters of the tree ceiling.
TLC_HEAP_MB=$((MEM_MB * 3 / 4))

QUINT="$TOOLS/quint/node_modules/.bin/quint"
TLA_JAR="$TOOLS/tlc/tla2tools.jar"
APALACHE="$TOOLS/apalache/apalache-$APALACHE_VERSION/bin/apalache-mc"
export QUINT_HOME="$TOOLS/quint-home"
TLC_CONFIG="$RUNS/tlc-config.json"

# The two checker command prefixes every tier shares, so their flags cannot
# drift between tiers.
TLC=(java -XX:+UseParallelGC "-Xmx${TLC_HEAP_MB}m" -cp "$TLA_JAR" tlc2.TLC -workers "$WORKERS")
QUINT_TLC=("$QUINT" verify --backend tlc --tlc-config "$TLC_CONFIG"
  --apalache-version "$APALACHE_FOR_QUINT_VERSION" --verbosity 3)

die() {
  echo "formal-evaluation: $*" >&2
  exit 1
}

require_tools() {
  local missing=()
  [[ -x $QUINT ]] || missing+=("quint $QUINT_VERSION")
  [[ -f $TLA_JAR ]] || missing+=("tla2tools.jar $TLA_TOOLS_VERSION")
  [[ -x $APALACHE ]] || missing+=("apalache $APALACHE_VERSION")
  [[ -d $QUINT_HOME/apalache-dist-$APALACHE_FOR_QUINT_VERSION ]] || missing+=("apalache $APALACHE_FOR_QUINT_VERSION (for quint)")
  command -v java >/dev/null || missing+=("java (17 or newer)")
  command -v node >/dev/null || missing+=("node (20 or newer)")
  if ((${#missing[@]})); then
    echo "formal-evaluation: missing tools: ${missing[*]}" >&2
    echo "Run: scripts/formal-evaluation.sh install   (or: make formal-evaluation FORMAL_EVAL_TARGET=install)" >&2
    exit 2
  fi
}

fetch() { # url dest -- downloads beside dest, so an interrupted fetch never
  # leaves a partial file under the name the skip-if-present checks test.
  curl -fsSL --retry 3 -o "$2.part" "$1"
  mv -f "$2.part" "$2"
}

verify_sha() { # file name
  local want=${SHA256[$2]} got
  got=$(sha256sum "$1" | cut -d' ' -f1)
  [[ $got == "$want" ]] || die "checksum mismatch for $2: $got (expected $want)"
}

install_apalache() { # version
  local version=$1 archive
  [[ -x $TOOLS/apalache/apalache-$version/bin/apalache-mc ]] && return 0
  archive="$TOOLS/apalache/apalache-$version.tgz"
  fetch "https://github.com/apalache-mc/apalache/releases/download/v$version/apalache-$version.tgz" "$archive"
  verify_sha "$archive" "apalache-$version.tgz"
  tar -xzf "$archive" -C "$TOOLS/apalache"
  rm -f "$archive"
}

cmd_install() {
  command -v npm >/dev/null || die "npm (Node 20 or newer) is required"
  command -v java >/dev/null || die "java (17 or newer) is required"
  mkdir -p "$TOOLS/tlc" "$TOOLS/apalache" "$QUINT_HOME/apalache-dist-$APALACHE_FOR_QUINT_VERSION"
  local start=$SECONDS
  if [[ ! -x $QUINT || $("$QUINT" --version) != "$QUINT_VERSION" ]]; then
    npm install --silent --no-fund --no-audit --prefix "$TOOLS/quint" "@informalsystems/quint@$QUINT_VERSION"
  fi
  if [[ ! -f $TLA_JAR ]]; then
    fetch "https://github.com/tlaplus/tlaplus/releases/download/v$TLA_TOOLS_VERSION/tla2tools.jar" "$TLA_JAR"
  fi
  verify_sha "$TLA_JAR" tla2tools.jar
  install_apalache "$APALACHE_VERSION"
  install_apalache "$APALACHE_FOR_QUINT_VERSION"
  # Quint looks for $QUINT_HOME/apalache-dist-<version>/apalache; point it at
  # the verified archive instead of letting it download its default (0.56.1).
  ln -sfn "../../apalache/apalache-$APALACHE_FOR_QUINT_VERSION" \
    "$QUINT_HOME/apalache-dist-$APALACHE_FOR_QUINT_VERSION/apalache"
  # The first simulator run downloads Quint's Rust evaluator into QUINT_HOME.
  "$QUINT" run --max-samples 1 --max-steps 1 "$MODELS/quint/move_rename.qnt" >/dev/null
  echo "installed in $((SECONDS - start)) s; $(du -sh "$TOOLS" | cut -f1) under $TOOLS"
  cmd_versions
}

cmd_versions() {
  require_tools
  echo "quint $("$QUINT" --version)"
  local evaluator
  for evaluator in "$QUINT_HOME"/rust-evaluator-*; do
    echo "quint rust evaluator ${evaluator##*/rust-evaluator-}"
  done
  echo "apalache $("$APALACHE" version 2>/dev/null | tail -n 1)"
  echo "apalache for quint $APALACHE_FOR_QUINT_VERSION"
  echo "tlc $(java -cp "$TLA_JAR" tlc2.TLC 2>&1 | sed -n 's/^TLC2 Version \(.*\)/\1/p' | head -n 1)"
  echo "java $(java -version 2>&1 | head -n 1)"
  echo "node $(node --version)"
}

# measure NAME EXPECT -- CMD...
# Run CMD in its own session, sample the whole tree's RSS once a second, kill
# it at TIMEOUT or MEM_MB, and record wall time, peak tree RSS, the exit
# status, and whether the outcome matched EXPECT (pass | violation).
measure() {
  (($# >= 4)) && [[ $3 == -- && $2 =~ ^(pass|violation|rlimit|build-error\([0-9]+\))$ ]] ||
    die "usage: measure NAME pass|violation -- CMD..."
  local name=$1 expect=$2
  shift 3
  [[ $WORKERS == 1 ]] || name+="-w$WORKERS"
  local dir="$RUNS/$name"
  mkdir -p "$dir"
  printf '%q ' "$@" >"$dir/command.txt"
  local start now peak=0 rss status=0 reason=finished tick=0 load_start load_end
  load_start=$(cut -d' ' -f1-3 /proc/loadavg | tr ' ' /)
  start=${EPOCHREALTIME/[.,]/}
  (cd "$dir" && exec setsid "$@" >"$dir/output.txt" 2>&1) &
  # The subshell is not a process-group leader (no job control here), so
  # setsid(1) calls setsid() in place: the checker's session and process group
  # ids are both its pid.
  local pid=$!
  trap 'kill -KILL -- "-$pid" 2>/dev/null; exit 130' INT TERM
  # Liveness is polled every 0.1 s so the recorded wall time is not rounded up
  # to the sampling second; RSS is sampled on every tenth tick.
  while kill -0 "$pid" 2>/dev/null; do
    if ((tick++ % 10 == 0)); then
      # ps fails once the session is gone; with pipefail that used to end the
      # whole script under `set -e` for a command that exits before a sample.
      rss=$(ps -o rss= -s "$pid" 2>/dev/null | awk '{s+=$1} END {print int(s / 1024)}') || rss=0
      ((rss > peak)) && peak=$rss
      if ((peak > MEM_MB)); then
        reason=memory-ceiling
        kill -KILL -- "-$pid" 2>/dev/null || true
        break
      fi
    fi
    now=${EPOCHREALTIME/[.,]/}
    if (((now - start) / 1000000 > TIMEOUT)); then
      reason=timeout
      kill -TERM -- "-$pid" 2>/dev/null || true
      sleep 2
      kill -KILL -- "-$pid" 2>/dev/null || true
      break
    fi
    sleep 0.1
  done
  wait "$pid" || status=$?
  now=${EPOCHREALTIME/[.,]/}
  trap - INT TERM
  # A server the tool spawned may outlive it; the group is ours to stop.
  kill -TERM -- "-$pid" 2>/dev/null || true
  load_end=$(cut -d' ' -f1-3 /proc/loadavg | tr ' ' /)
  local tenths=$(((now - start + 50000) / 100000)) outcome verdict=unexpected states
  outcome=$(classify "$dir/output.txt" "$status" "$reason")
  [[ $outcome == "$expect" ]] && verdict=expected
  states=$(sed -n -e 's/.* \([0-9,]*\) distinct states found.*/\1/p' -e 's/^unique_states=\([0-9]*\)$/\1/p' "$dir/output.txt" | tail -n 1 | tr -d ,)
  # The 1/5/15-minute load at start and end: a time taken at a 1-minute load
  # of 4 or more is a cost figure, never a verdict.
  printf '%s\t%s\t%s\t%s\t%d.%d\t%s\t%s\t%s\t%s\t%s\n' "$name" "$expect" "$outcome" "$verdict" \
    $((tenths / 10)) $((tenths % 10)) "$peak" "${states:--}" "$reason" "$load_start" "$load_end" |
    tee -a "$RUNS/results.tsv"
  [[ $verdict == expected ]]
}

classify() { # output status reason
  local out=$1 status=$2 reason=$3 verus
  # cargo verus prints one results line per verified crate (vstd included on a
  # cold build): sum them.
  verus=$(sed -n 's/^verification results:: \([0-9]*\) verified, \([0-9]*\) errors.*/\1 \2/p' "$out" |
    awk '{v += $1; e += $2} END {if (NR) print v, e}')
  if [[ $reason != finished ]]; then
    echo "$reason"
  elif grep -qiE '^error: .*resource limit \(rlimit\) exceeded' "$out"; then
    # A resource stop is never a refutation, whatever else failed.
    echo rlimit
  elif [[ -n $verus ]]; then
    classify_verus "$out" "$status" "$verus"
  elif grep -qE '^error(\[E[0-9]+\])?: ' "$out"; then
    # A Verus build error (no verification results line) is never a refutation.
    printf '%s\n' "build-error($status)"
  elif grep -qE '^VERIFICATION:- (FAILED|SUCCESSFUL)' "$out"; then
    # A scratch Kani run (the evaluation's Kani-first attempts).
    if grep -q '^VERIFICATION:- FAILED' "$out"; then echo violation; elif ((status == 0)); then echo pass; else printf '%s\n' "error($status)"; fi
  elif grep -qE 'is violated|was violated|were violated|\[violation\]|Found [0-9]+ error|found a counterexample|Invariant violated|failed [0-9]+ test|[0-9]+ failing|: VIOLATED' "$out"; then
    echo violation
  elif grep -qE 'No error has been found|\[ok\] No violation found|NoError|[0-9]+ passing|^wall_seconds=' "$out" && ((status == 0)); then
    echo pass
  else
    # A classification value on stdout for the caller, not a diagnostic.
    printf '%s\n' "error($status)"
  fi
}

classify_verus() { # output status "verified errors"
  local out=$1 status=$2 errors=${3#* }
  if ((errors == 0)); then
    ((status == 0)) && echo pass || printf '%s\n' "error($status)"
  elif grep -qE '^error: (postcondition not satisfied|assertion failed|invariant not satisfied|precondition not satisfied|invariant not satisfied (before|at end of) loop)' "$out"; then
    # A named obligation failed. With FORMAL_EVAL_OBLIGATION set, it must be
    # the obligation the run expects, read from the failure's context lines.
    if [[ -n ${FORMAL_EVAL_OBLIGATION:-} ]] && ! grep -qE "$FORMAL_EVAL_OBLIGATION" "$out"; then
      echo violation-other
    else
      echo violation
    fi
  else
    printf '%s\n' "verus-error($errors)"
  fi
}

prepare() { # tools present, runs directory, and Quint's TLC runtime config
  require_tools
  mkdir -p "$RUNS"
  printf '{"workers": "%s", "maxHeap": "-Xmx%dm"}\n' "$WORKERS" "$TLC_HEAP_MB" >"$TLC_CONFIG"
}

cmd_t1() {
  prepare
  local q="$MODELS/quint/write_protocol.qnt" mr="$MODELS/quint/move_rename.qnt"
  local t="$MODELS/tlaplus" ok=0
  # Quint: simulator, Apalache (through Quint), TLC (through Quint).
  measure t1-quint-sim-production pass -- "$QUINT" run "$q" --invariant safety --max-steps 20 --max-samples 200000 || ok=1
  measure t1-quint-sim-mutant violation -- "$QUINT" run "$q" --init initSkipsTempSync --invariant runWriteAssertions --max-steps 20 --max-samples 200000 || ok=1
  measure t1-quint-apalache-production pass -- "$QUINT" verify "$q" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --max-steps 17 --invariant safety || ok=1
  measure t1-quint-apalache-mutant violation -- "$QUINT" verify "$q" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --init initSkipsTempSync --max-steps 17 --invariant runWriteAssertions || ok=1
  measure t1-quint-tlc-production pass -- "${QUINT_TLC[@]}" "$q" --invariant safety || ok=1
  measure t1-quint-tlc-mutant violation -- "${QUINT_TLC[@]}" "$q" --init initSkipsTempSync --invariant runWriteAssertions || ok=1
  measure t1-quint-tlc-liveness-fair pass -- "${QUINT_TLC[@]}" "$q" --temporal eventuallyTerminalFair || ok=1
  measure t1-quint-tlc-liveness-unfair violation -- "${QUINT_TLC[@]}" "$q" --temporal eventuallyTerminal || ok=1
  measure t1-quint-tlc-move-rename pass -- "${QUINT_TLC[@]}" "$mr" --invariant safety || ok=1
  # TLA+ (PlusCal): TLC, and Apalache on the annotated wrapper.
  measure t1-tlc-production pass -- "${TLC[@]}" -config "$t/WriteProtocol.cfg" "$t/WriteProtocol.tla" || ok=1
  measure t1-tlc-mutant violation -- "${TLC[@]}" -config "$t/WriteProtocolSkipsTempSync.cfg" "$t/WriteProtocol.tla" || ok=1
  measure t1-tlc-liveness-unfair violation -- "${TLC[@]}" -config "$t/WriteProtocolNoFairness.cfg" "$t/WriteProtocol.tla" || ok=1
  measure t1-tlc-move-rename pass -- "${TLC[@]}" -config "$t/MoveRename.cfg" "$t/MoveRename.tla" || ok=1
  measure t1-apalache-tla-production pass -- "$APALACHE" check --cinit=CInitProduction --init=Init --next=Next --inv=Safety --length=17 "$t/MC_WriteProtocolApalache.tla" || ok=1
  measure t1-apalache-tla-mutant violation -- "$APALACHE" check --cinit=CInitMutant --init=Init --next=Next --inv=RunWriteAssertions --length=17 "$t/MC_WriteProtocolApalache.tla" || ok=1
  return $ok
}

cmd_t2() {
  prepare
  local q="$MODELS/quint/journal.qnt" t="$MODELS/tlaplus" ok=0 depth
  measure t2-quint-test-k8-traces pass -- "$QUINT" test "$q" --main k8_traces || ok=1
  measure t2-quint-tlc-k3-8 pass -- "${QUINT_TLC[@]}" "$q" --main k3_8 --invariant invariants || ok=1
  measure t2-tlc-k3-8 pass -- "${TLC[@]}" -config "$t/Journal_k3_8.cfg" "$t/Journal.tla" || ok=1
  for depth in ${FORMAL_EVAL_T2_DEPTHS:-6 8}; do
    measure "t2-quint-tlc-k8-$depth" violation -- "${QUINT_TLC[@]}" "$q" --main "k8_$depth" --invariant invariants || ok=1
    measure "t2-tlc-k8-$depth" violation -- "${TLC[@]}" -config "$t/Journal_k8_$depth.cfg" "$t/Journal.tla" || ok=1
  done
  return $ok
}

cmd_t3() {
  prepare
  local q="$MODELS/quint/data_dir_lock.qnt" t="$MODELS/tlaplus" ok=0 cfg base name expect args
  for cfg in "$t"/DataDirLock_*.cfg; do
    base=$(basename "$cfg" .cfg)
    expect=$(sed -n 's/^\\\* expect: \(pass\|violation\)$/\1/p' "$cfg")
    [[ -n $expect ]] || die "$cfg has no '\\* expect: pass|violation' header"
    measure "t3-tlc-${base#DataDirLock_}" "$expect" -- "${TLC[@]}" -config "$cfg" "$t/DataDirLock.tla" || ok=1
  done
  while IFS='|' read -r name expect args; do
    [[ -z $name || $name == \#* ]] && continue
    # shellcheck disable=SC2086 # the argument list is word-split on purpose
    measure "t3-quint-$name" "$expect" -- "${QUINT_TLC[@]}" "$q" $args || ok=1
  done <"$MODELS/quint/data_dir_lock.checks"
  return $ok
}

cmd_report_data() {
  [[ -f $RUNS/results.tsv ]] || die "no results yet under $RUNS"
  echo "| run | expected | outcome | wall (s) | peak tree RSS (MiB) | distinct states | end | load at start | load at end |"
  echo "|---|---|---|---|---|---|---|---|---|"
  awk -F'\t' '{printf "| %s | %s | %s | %s | %s | %s | %s | %s | %s |\n", $1, $2, $3, $5, $6, $7, $8, ($9 == "" ? "-" : $9), ($10 == "" ? "-" : $10)}' "$RUNS/results.tsv"
}

# --- Verus (evaluate-verus-for-unbounded-proofs) ------------------------------
#
# Versions resolved on 2026-11-15 (see docs/next/formal-verification-verus.md,
# "Versions"). The latest non-rolling release is the default; the rolling one
# is the newest release, used for the re-verification before the report.
VERUS_RELEASE=${VERUS_RELEASE:-0.2026.11.02.8bf68ee}
declare -A VERUS_SHA256=(
  [0.2026.11.02.8bf68ee]=f3a0119292df8855e4435da8130bee1873b0e6a60c306accf81a08ea687bba3c
  [0.2026.11.05.31e44e6]=4e0ab2a689645144d82e57a50190dad0bcd5524533e9c1f9f4062846deab3732
)
# The vstd each release's `cargo verus new` pins: a crates.io version for a
# non-rolling release, a git revision for a rolling one. A vstd from another
# release does not build ("unrecognized verifier attribute"), so verus_check
# rewrites the copy's dependency for the selected release.
declare -A VERUS_VSTD=(
  [0.2026.11.02.8bf68ee]='"=0.0.0-2026-11-01-0251"'
  [0.2026.11.05.31e44e6]='{ git = "https://github.com/verus-lang/verus.git", rev = "31e44e600408242839ff456e84550e9f849344e3" }'
)
declare -A VERUS_TAG=(
  [0.2026.11.02.8bf68ee]=release/0.2026.11.02.8bf68ee
  [0.2026.11.05.31e44e6]=release/rolling/0.2026.11.05.31e44e6
)
VERUS_HOME="$TOOLS/verus-$VERUS_RELEASE"
VERUS_RUSTUP_HOME="$TOOLS/verus-rustup"
VERUS_CARGO_HOME="$TOOLS/verus-cargo"
VERUS_MODELS="$MODELS/verus"
# The commit every copy under formal/evaluation/verus/proofs/ was taken from.
VERUS_COPY_REV=85758eaf8fb8190439719dd6926a5b2b3ed21fb0
VERUS_DRIFT_REV=${VERUS_DRIFT_REV:-$VERUS_COPY_REV}
VERUS_THREADS=16
# heavy.sh's memory cap per check (FORMAL_EVAL_HEAVY_MEM): 16G by default;
# FORMAL_EVAL_MEM_MB must then stay under it.
HEAVY=("$REPO_ROOT/scripts/heavy.sh" --mem "${FORMAL_EVAL_HEAVY_MEM:-16G}" --)

verus_rustc() { # the rustc the release pins, from its version.json
  sed -n 's/.*"toolchain": "\([0-9.]*\)-.*/\1/p' "$VERUS_HOME/version.json"
}

require_verus() {
  local missing=()
  [[ -x $VERUS_HOME/verus && -x $VERUS_HOME/cargo-verus ]] || missing+=("verus $VERUS_RELEASE")
  [[ -x $VERUS_HOME/z3 ]] || missing+=("z3 (bundled with verus $VERUS_RELEASE)")
  if [[ -f $VERUS_HOME/version.json ]] &&
    ! [[ -d $VERUS_RUSTUP_HOME/toolchains/$(verus_rustc)-x86_64-unknown-linux-gnu ]]; then
    missing+=("rustc $(verus_rustc) under $VERUS_RUSTUP_HOME")
  fi
  if ((${#missing[@]})); then
    echo "formal-evaluation: missing tools: ${missing[*]}" >&2
    echo "Run: scripts/formal-evaluation.sh verus-install   (or: make formal-evaluation FORMAL_EVAL_TARGET=verus-install)" >&2
    exit 2
  fi
}

# Run CMD with Verus's toolchain, selected explicitly (never by
# rust-toolchain.toml lookup), with the private rustup and cargo homes.
verus_env() {
  env RUSTUP_HOME="$VERUS_RUSTUP_HOME" CARGO_HOME="$VERUS_CARGO_HOME" \
    RUSTUP_TOOLCHAIN="$(verus_rustc)" PATH="$VERUS_HOME:$PATH" "$@"
}

# Run CMD with the repository's own pinned toolchain.
workspace_env() {
  env RUSTUP_TOOLCHAIN="$(sed -n 's/^channel = "\(.*\)"/\1/p' "$REPO_ROOT/rust-toolchain.toml")" "$@"
}

cmd_verus_install() {
  local tag=${VERUS_TAG[$VERUS_RELEASE]:-} want=${VERUS_SHA256[$VERUS_RELEASE]:-}
  [[ -n $tag && -n $want ]] || die "no recorded tag and checksum for verus $VERUS_RELEASE"
  command -v unzip >/dev/null || die "unzip is required"
  command -v rustup >/dev/null || die "rustup is required (it installs Verus's rustc into $VERUS_RUSTUP_HOME)"
  mkdir -p "$TOOLS"
  local start=$SECONDS zip="$TOOLS/verus-$VERUS_RELEASE-x86-linux.zip" unpack got
  if [[ ! -x $VERUS_HOME/verus ]]; then
    fetch "https://github.com/verus-lang/verus/releases/download/${tag//\//%2F}/verus-$VERUS_RELEASE-x86-linux.zip" "$zip"
    got=$(sha256sum "$zip" | cut -d' ' -f1)
    [[ $got == "$want" ]] || die "checksum mismatch for verus $VERUS_RELEASE: $got (expected $want)"
    echo "downloaded $(stat -c %s "$zip") bytes in $((SECONDS - start)) s"
    unpack=$(mktemp -d "$TOOLS/verus-unpack.XXXXXX")
    unzip -q "$zip" -d "$unpack"
    rm -rf "$VERUS_HOME"
    mv "$unpack/verus-x86-linux" "$VERUS_HOME"
    rm -rf "$unpack" "$zip"
  fi
  # The 0.2026.09.06 release shipped without z3 (verus-lang/verus#2914).
  [[ -x $VERUS_HOME/z3 ]] || die "verus $VERUS_RELEASE has no bundled z3 (verus-lang/verus#2914)"
  RUSTUP_HOME="$VERUS_RUSTUP_HOME" CARGO_HOME="$VERUS_CARGO_HOME" \
    rustup toolchain install "$(verus_rustc)" --profile minimal --no-self-update
  echo "installed in $((SECONDS - start)) s; $(du -sh "$VERUS_HOME" | cut -f1) under $VERUS_HOME," \
    "$(du -sh "$VERUS_RUSTUP_HOME" | cut -f1) under $VERUS_RUSTUP_HOME"
  cmd_verus_versions
}

cmd_verus_versions() {
  require_verus
  verus_env verus --version | sed -n 's/^ *Version: /verus /p'
  echo "verus commit $(sed -n 's/.*"commit": "\(.*\)".*/\1/p' "$VERUS_HOME/version.json")"
  echo "verus rustc $(verus_env rustc --version)"
  "$VERUS_HOME/z3" --version
  echo "vstd ${VERUS_VSTD[$VERUS_RELEASE]:-unrecorded}"
  echo "workspace rustc $(workspace_env rustc --version)"
}

# One Verus check, in its own heavy.sh call, measured with the evaluation's
# ceilings: verus_check NAME EXPECT DIR [VERUS-ARGS...]. DIR is a copy of
# proofs/ (a mutant's) or proofs/ itself. `cargo verus focus` is the subcommand
# partial verification (`--verify-module`) requires; it forwards the Verus
# arguments to the root crate only, so vstd is not re-verified.
verus_check() {
  local name=$1 expect=$2 dir=$3
  shift 3
  # Cargo (through cargo-verus) skips a crate whose contents it has already
  # checked, which would report no verification at all; neither touching the
  # sources nor a fresh copy at the same path changes that. So the copy's root
  # module gets a unique trailing comment, and its dependencies (vstd) stay
  # cached in the shared target directory.
  # The copy keeps proofs/ beside table/, which src/bin/table.rs includes.
  local fresh="$EVAL_DIR/verus-checks/$name"
  rm -rf "$fresh"
  mkdir -p "$fresh/proofs"
  cp -r "$dir/." "$fresh/proofs/"
  cp -r "$VERUS_MODELS/table" "$fresh/"
  printf '\n// verus check %s %s\n' "$name" "${EPOCHREALTIME/[.,]/}" >>"$fresh/proofs/src/lib.rs"
  local vstd=${VERUS_VSTD[$VERUS_RELEASE]:-}
  [[ -n $vstd ]] || die "no recorded vstd for verus $VERUS_RELEASE"
  VSTD=$vstd awk '/^vstd = / {print "vstd = " ENVIRON["VSTD"]; next} {print}' \
    "$fresh/proofs/Cargo.toml" >"$fresh/proofs/Cargo.toml.new"
  mv "$fresh/proofs/Cargo.toml.new" "$fresh/proofs/Cargo.toml"
  dir=$fresh/proofs
  "${HEAVY[@]}" "$0" verus-measure "$name" "$expect" -- \
    env RUSTUP_HOME="$VERUS_RUSTUP_HOME" CARGO_HOME="$VERUS_CARGO_HOME" \
    RUSTUP_TOOLCHAIN="$(verus_rustc)" PATH="$VERUS_HOME:$PATH" \
    CARGO_TARGET_DIR="$EVAL_DIR/target-verus" \
    cargo-verus verus focus --manifest-path "$dir/Cargo.toml" --lib -- \
    --num-threads "$VERUS_THREADS" --time "$@"
}

# A copy of proofs/ with MUTANT.patch applied, for one mutant check.
mutant_tree() { # mutant -> prints the directory
  local mutant=$1 dir="$EVAL_DIR/verus-mutants/$1"
  rm -rf "$dir"
  mkdir -p "$dir/proofs"
  cp -r "$VERUS_MODELS/proofs/." "$dir/proofs/"
  cp -r "$VERUS_MODELS/table" "$dir/"
  patch -s -p1 -d "$dir/proofs" <"$VERUS_MODELS/mutants/$mutant.patch" ||
    die "mutant $mutant no longer applies"
  printf '%s\n' "$dir/proofs"
}

# Each mutants/*.patch names, on `# module:` and `# obligation:` header lines,
# the module it verifies and the obligation that must fail.
verus_mutants() { # prefix
  local patch mutant module obligation ok=0
  for patch in "$VERUS_MODELS/mutants/$1"*.patch; do
    [[ -e $patch ]] || continue
    mutant=$(basename "$patch" .patch)
    module=$(sed -n 's/^# module: //p' "$patch")
    obligation=$(sed -n 's/^# obligation: //p' "$patch")
    [[ -n $module && -n $obligation ]] || die "$patch lacks a '# module:' or '# obligation:' header"
    FORMAL_EVAL_OBLIGATION=$obligation verus_check "$mutant" violation "$(mutant_tree "$mutant")" \
      --verify-module "$module" || ok=1
  done
  return $ok
}

cmd_verus_v1() {
  require_verus
  local ok=0
  verus_check "verus-$VERUS_RELEASE-v1" pass "$VERUS_MODELS/proofs" --verify-module preview_width || ok=1
  verus_mutants v1- || ok=1
  return $ok
}

# classify on a resource stop and on a build error, so neither is ever read
# as a proof or a refutation (task 1.4): a nonlinear goal under a tiny rlimit,
# and a copy that does not compile.
cmd_verus_classify() {
  require_verus
  local ok=0
  verus_check "verus-classify-rlimit" rlimit "$(mutant_tree classify-rlimit)" --verify-module preview_width || ok=1
  verus_check "verus-classify-build-error" "build-error(101)" "$(mutant_tree classify-build-error)" \
    --verify-module preview_width || ok=1
  return $ok
}

# V2 (the unbounded journal invariant) and V3 (L1 from every reachable state)
# were not completed: V2 stopped after scoping (report §5) and the maintainer
# closed both unattempted on 2026-11-15 (report §13). The targets stay so a
# revisit on one of the report's triggers has a home: each verifies its module
# once proofs/src/ has it, and says so plainly until then.
verus_target_module() { # target module
  local target=$1 module=$2
  require_verus
  if [[ ! -f $VERUS_MODELS/proofs/src/$module.rs ]]; then
    echo "formal-evaluation: $target: proofs/src/$module.rs does not exist; $target was closed unattempted" \
      "(maintainer decision 2026-11-15; docs/next/formal-verification-verus.md §5, §6, §13). Nothing to verify." >&2
    return 0
  fi
  local ok=0
  verus_check "verus-$VERUS_RELEASE-$target" pass "$VERUS_MODELS/proofs" --verify-module "$module" || ok=1
  verus_mutants "$target-" || ok=1
  return $ok
}

cmd_verus_v2() { verus_target_module v2 journal_model; }

cmd_verus_v3() { verus_target_module v3 journal_liveness; }

# Print FUNCTION's text from FILE at REV: the item from its signature line to
# the first line that is exactly `}`. Fails unless the signature occurs once.
extract_fn() { # rev file signature-regex
  local text count
  text=$(git -C "$REPO_ROOT" show "$1:$2") || die "cannot read $2 at $1"
  count=$(grep -cE "$3" <<<"$text" || true)
  ((count == 1)) || die "drift: '$3' occurs $count times in $2 at $1 (expected once)"
  # The pattern goes through the environment: `awk -v` would eat its escapes.
  RE=$3 awk '$0 ~ ENVIRON["RE"] {on = 1} on {print} on && /^}$/ {exit}' <<<"$text"
}

extract_const() { # rev file name
  local text count
  text=$(git -C "$REPO_ROOT" show "$1:$2") || die "cannot read $2 at $1"
  count=$(grep -cE "const $3: " <<<"$text" || true)
  ((count == 1)) || die "drift: const $3 occurs $count times in $2 at $1 (expected once)"
  grep -E "const $3: " <<<"$text"
}

# The copy's text of a function kept with the production body: the
# `#[verus_spec(...)]` attribute removed, and nothing else.
copy_fn_without_spec() { # file signature-regex
  RE=$2 awk '
    /^#\[verus_spec/ {skip = 1}
    skip && /^\)\]$/ {skip = 0; next}
    skip {next}
    $0 ~ ENVIRON["RE"] {on = 1}
    on {print}
    on && /^}$/ {exit}' "$1"
}

cmd_verus_drift() {
  require_verus
  local dir="$EVAL_DIR/verus-drift" rev policy="crates/lushtext-ui/src/ui/markdown_preview/policy.rs"
  local sig='^pub\(crate\) fn clamped_preview_width\(' tree ok=0 copy="$VERUS_MODELS/proofs"
  # VERUS_DRIFT_MUTANT=<mutants/NAME.patch> checks a deliberately changed copy
  # instead: the failing-first proof that the exchange sees a one-line change.
  [[ -z ${VERUS_DRIFT_MUTANT:-} ]] || copy=$(mutant_tree "$VERUS_DRIFT_MUTANT")
  rev=$(git -C "$REPO_ROOT" rev-parse "$VERUS_DRIFT_REV^{commit}") || die "unknown revision $VERUS_DRIFT_REV"
  mkdir -p "$dir" "$RUNS"
  # 1. The integer functions, extracted from the revision.
  {
    extract_const "$rev" "$policy" PREVIEW_MIN_WIDTH_SP
    extract_const "$rev" "$policy" PREVIEW_MAX_WIDTH_DIVISOR
    extract_fn "$rev" "$policy" "$sig"
  } >"$dir/extracted.rs"
  # 2. Text identity where the copy keeps the production body.
  extract_fn "$rev" "$policy" "$sig" | sed 's/^pub(crate) fn/pub fn/' | rustfmt --edition 2024 --emit stdout >"$dir/production-body.rs"
  copy_fn_without_spec "$copy/src/preview_width.rs" '^pub fn clamped_preview_width\(' |
    rustfmt --edition 2024 --emit stdout >"$dir/copy-body.rs"
  if diff -u "$dir/production-body.rs" "$dir/copy-body.rs"; then
    echo "drift: clamped_preview_width body identical to $rev (rustfmt-normalised)"
  else
    echo "drift: clamped_preview_width body differs from $rev" >&2
    ok=1
  fi
  # 3. The production table, in a detached scratch worktree at the revision,
  # with the workspace's own toolchain.
  tree="$dir/tree-${rev:0:12}"
  if [[ ! -d $tree ]]; then
    git -C "$REPO_ROOT" worktree add --detach "$tree" "$rev" >/dev/null
  fi
  mkdir -p "$tree/formal/evaluation/verus"
  rm -rf "$tree/formal/evaluation/verus/drift" "$tree/formal/evaluation/verus/table"
  cp -r "$VERUS_MODELS/drift" "$VERUS_MODELS/table" "$tree/formal/evaluation/verus/"
  # Resolve exactly the dependency versions production builds with at the
  # revision: a fresh resolution picks newer gtk-rs patch releases, which do
  # not build lushtext-core at older revisions.
  cp "$tree/Cargo.lock" "$tree/formal/evaluation/verus/drift/Cargo.lock"
  local built=0
  "${HEAVY[@]}" env VERUS_DRIFT_EXTRACTED="$dir/extracted.rs" CARGO_TARGET_DIR="$EVAL_DIR/target-verus-drift" \
    RUSTUP_TOOLCHAIN="$(sed -n 's/^channel = "\(.*\)"/\1/p' "$tree/rust-toolchain.toml")" \
    cargo run --quiet --release --manifest-path "$tree/formal/evaluation/verus/drift/Cargo.toml" \
    >"$dir/production.tsv" || built=1
  [[ $rev != "$VERUS_COPY_REV" ]] || cp "$tree/formal/evaluation/verus/drift/Cargo.lock" "$VERUS_MODELS/drift/"
  git -C "$REPO_ROOT" worktree remove --force "$tree"
  ((built == 0)) || die "the production table did not build or run"
  # 4. The copy's table, built by `cargo verus build` with Verus's toolchain
  # (verification itself is the verus-v1/v2/v3 targets' job).
  "${HEAVY[@]}" env RUSTUP_HOME="$VERUS_RUSTUP_HOME" CARGO_HOME="$VERUS_CARGO_HOME" \
    RUSTUP_TOOLCHAIN="$(verus_rustc)" PATH="$VERUS_HOME:$PATH" CARGO_TARGET_DIR="$EVAL_DIR/target-verus" \
    cargo-verus verus build --manifest-path "$copy/Cargo.toml" --release --bin table -- \
    --no-verify >"$dir/copy-build.log" 2>&1 ||
    { cat "$dir/copy-build.log" >&2; die "the copy's table did not build"; }
  "$EVAL_DIR/target-verus/release/table" >"$dir/copy.tsv"
  # 5. The exchange.
  if diff -u "$dir/production.tsv" "$dir/copy.tsv" >"$dir/table.diff"; then
    echo "drift: $(wc -l <"$dir/copy.tsv") table lines identical to $rev"
  else
    echo "drift: the copy's table differs from $rev ($dir/table.diff):" >&2
    head -n 40 "$dir/table.diff" >&2
    ok=1
  fi
  printf '%s\t%s\t%s\t%s\n' "verus-drift" "$rev" "${VERUS_DRIFT_MUTANT:-copy}" \
    "$( ((ok == 0)) && echo pass || echo drift)" | tee -a "$RUNS/drift.tsv"
  return $ok
}

case ${1:-} in
  install) cmd_install ;;
  versions) cmd_versions ;;
  t1) cmd_t1 ;;
  t2) cmd_t2 ;;
  t3) cmd_t3 ;;
  all)
    status=0
    cmd_t1 || status=1
    cmd_t2 || status=1
    cmd_t3 || status=1
    exit $status
    ;;
  report-data) cmd_report_data ;;
  verus-install) cmd_verus_install ;;
  verus-versions) cmd_verus_versions ;;
  verus-v1) cmd_verus_v1 ;;
  verus-v2) cmd_verus_v2 ;;
  verus-v3) cmd_verus_v3 ;;
  verus-drift) cmd_verus_drift ;;
  verus-classify) cmd_verus_classify ;;
  verus-all)
    status=0
    cmd_verus_drift || status=1
    cmd_verus_v1 || status=1
    cmd_verus_v2 || status=1
    cmd_verus_v3 || status=1
    exit $status
    ;;
  # The measured run verus_check wraps in heavy.sh: the Verus ceilings, and no
  # Quint/TLA+ tools required.
  verus-measure)
    shift
    MEM_MB=${FORMAL_EVAL_MEM_MB:-14336}
    mkdir -p "$RUNS"
    measure "$@"
    ;;
  # Ad-hoc measured run with the same ceilings: measure NAME EXPECT -- CMD...
  # prepare writes $TLC_CONFIG, so "${QUINT_TLC[@]}"-shaped commands work here.
  measure)
    shift
    prepare
    measure "$@"
    ;;
  *)
    echo "usage: $0 {install|versions|t1|t2|t3|all|report-data}" >&2
    echo "       $0 {verus-install|verus-versions|verus-v1|verus-v2|verus-v3|verus-drift|verus-classify|verus-all}" >&2
    echo "       $0 measure NAME pass|violation -- CMD..." >&2
    echo "       $0 verus-measure NAME pass|violation -- CMD..." >&2
    exit 64
    ;;
esac
