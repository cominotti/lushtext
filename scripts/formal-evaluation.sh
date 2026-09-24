#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Reproduce the Quint vs TLA+ evaluation (docs/next/formal-verification-quint-vs-tlaplus.md).
#
# LOCAL ONLY. This script is not a prerequisite of check, check-policy, test,
# kani, or end-user-smoke, and no CI workflow calls it. The models it runs
# (formal/evaluation/) are disposable evaluation models, not maintained
# properties; Kani stays the single maintained formal tool.
#
# Usage: scripts/formal-evaluation.sh {install|t1|t2|t3|all|report-data|versions}
#        scripts/formal-evaluation.sh measure NAME pass|violation -- CMD...
#
# Environment:
#   FORMAL_EVAL_TIMEOUT   seconds per check (default 1800, the CI job cap)
#   FORMAL_EVAL_MEM_MB    process-tree RSS ceiling in MiB (default 24576)
#   FORMAL_EVAL_T2_DEPTHS space-separated K8 depths for t2 (default "6 8")
#   FORMAL_EVAL_DIR       tools and runs root (default build/formal-evaluation)
#   FORMAL_EVAL_WORKERS   TLC workers (default 1, comparable with Kani's
#                         single-threaded solver; "auto" for every core)
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
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

QUINT="$TOOLS/quint/node_modules/.bin/quint"
TLA_JAR="$TOOLS/tlc/tla2tools.jar"
APALACHE="$TOOLS/apalache/apalache-$APALACHE_VERSION/bin/apalache-mc"
export QUINT_HOME="$TOOLS/quint-home"

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
  if ((${#missing[@]})); then
    echo "formal-evaluation: missing tools: ${missing[*]}" >&2
    echo "Run: scripts/formal-evaluation.sh install   (or: make formal-evaluation FORMAL_EVAL_TARGET=install)" >&2
    exit 2
  fi
}

fetch() { # url dest
  curl -fsSL --retry 3 -o "$2" "$1"
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
  mkdir -p "$TOOLS/tlc" "$TOOLS/apalache" "$QUINT_HOME" "$RUNS"
  local start=$SECONDS
  npm install --silent --no-fund --no-audit --prefix "$TOOLS/quint" "@informalsystems/quint@$QUINT_VERSION"
  if [[ ! -f $TLA_JAR ]]; then
    fetch "https://github.com/tlaplus/tlaplus/releases/download/v$TLA_TOOLS_VERSION/tla2tools.jar" "$TLA_JAR"
  fi
  verify_sha "$TLA_JAR" tla2tools.jar
  install_apalache "$APALACHE_VERSION"
  install_apalache "$APALACHE_FOR_QUINT_VERSION"
  # Quint looks for $QUINT_HOME/apalache-dist-<version>/apalache; point it at
  # the verified archive instead of letting it download its default (0.56.1).
  mkdir -p "$QUINT_HOME/apalache-dist-$APALACHE_FOR_QUINT_VERSION"
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
  echo "quint rust evaluator $(ls "$QUINT_HOME" | sed -n 's/^rust-evaluator-//p')"
  echo "apalache $("$APALACHE" version 2>/dev/null | tail -n 1)"
  echo "apalache for quint $APALACHE_FOR_QUINT_VERSION"
  echo "tlc $(java -cp "$TLA_JAR" tlc2.TLC 2>&1 | sed -n 's/^TLC2 Version \(.*\)/\1/p' | head -n 1)"
  echo "java $(java -version 2>&1 | head -n 1)"
  echo "node $(node --version)"
}

# measure NAME EXPECT -- CMD...
# Run CMD in its own process group, sample the whole tree's RSS every second,
# kill it at TIMEOUT or MEM_MB, and record wall time, peak tree RSS, the exit
# status, and whether the outcome matched EXPECT (pass | violation).
measure() {
  local name=$1 expect=$2
  shift 3
  local dir="$RUNS/$name"
  mkdir -p "$dir"
  printf '%q ' "$@" >"$dir/command.txt"
  local start end peak=0 rss status=0 reason=finished pgid
  start=$(date +%s.%N)
  (cd "$dir" && exec setsid "$@" >"$dir/output.txt" 2>&1) &
  local pid=$!
  sleep 0.2
  pgid=$(ps -o pgid= -p "$pid" 2>/dev/null | tr -d ' ' || true)
  while kill -0 "$pid" 2>/dev/null; do
    if [[ -n $pgid ]]; then
      rss=$(ps -o rss= -g "$pgid" 2>/dev/null | awk '{s+=$1} END {print s+0}')
      ((rss > peak)) && peak=$rss
    fi
    end=$(date +%s.%N)
    if (($(printf '%.0f' "$(echo "$end - $start" | bc)") > TIMEOUT)); then
      reason=timeout
      kill -TERM -- "-$pgid" 2>/dev/null || true
      sleep 2
      kill -KILL -- "-$pgid" 2>/dev/null || true
      break
    fi
    if ((peak / 1024 > MEM_MB)); then
      reason=memory-ceiling
      kill -KILL -- "-$pgid" 2>/dev/null || true
      break
    fi
    sleep 1
  done
  wait "$pid" || status=$?
  # A server the tool spawned may outlive it; the group is ours to stop.
  [[ -n $pgid ]] && kill -TERM -- "-$pgid" 2>/dev/null || true
  end=$(date +%s.%N)
  local wall outcome
  wall=$(printf '%.1f' "$(echo "$end - $start" | bc)")
  outcome=$(classify "$dir/output.txt" "$status" "$reason")
  local verdict=unexpected
  [[ $outcome == "$expect" ]] && verdict=expected
  local states
  states=$(sed -n -e 's/.* \([0-9,]*\) distinct states found.*/\1/p' -e 's/^unique_states=\([0-9]*\)$/\1/p' "$dir/output.txt" | tail -n 1 | tr -d ,)
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$expect" "$outcome" "$verdict" \
    "$wall" "$((peak / 1024))" "${states:--}" "$reason" | tee -a "$RUNS/results.tsv"
  [[ $verdict == expected ]]
}

classify() { # output status reason
  local out=$1 status=$2 reason=$3
  if [[ $reason != finished ]]; then
    echo "$reason"
  elif grep -qE 'is violated|was violated|were violated|\[violation\]|Found [0-9]+ error|found a counterexample|Invariant violated|failed [0-9]+ test|[0-9]+ failing|: VIOLATED' "$out"; then
    echo violation
  elif grep -qE 'No error has been found|\[ok\] No violation found|NoError|[0-9]+ passing|^wall_seconds=' "$out" && ((status == 0)); then
    echo pass
  else
    echo "error($status)"
  fi
}


tlc_config() { # dir -> path of a TLC runtime config for quint --backend tlc
  local cfg="$1/tlc-config.json"
  printf '{"workers": "%s", "maxHeap": "-Xmx%dm"}\n' "$WORKERS" $((MEM_MB * 3 / 4)) >"$cfg"
  echo "$cfg"
}

cmd_t1() {
  require_tools
  local q="$MODELS/quint/write_protocol.qnt" mr="$MODELS/quint/move_rename.qnt"
  local t="$MODELS/tlaplus" ok=0
  local tlccfg
  mkdir -p "$RUNS"
  tlccfg=$(tlc_config "$RUNS")
  # Quint: simulator, Apalache (through Quint), TLC (through Quint).
  measure t1-quint-sim-production pass -- "$QUINT" run "$q" --invariant safety --max-steps 20 --max-samples 200000 || ok=1
  measure t1-quint-sim-mutant violation -- "$QUINT" run "$q" --init initSkipsTempSync --invariant runWriteAssertions --max-steps 20 --max-samples 200000 || ok=1
  measure t1-quint-apalache-production pass -- "$QUINT" verify "$q" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --max-steps 17 --invariant safety || ok=1
  measure t1-quint-apalache-mutant violation -- "$QUINT" verify "$q" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --init initSkipsTempSync --max-steps 17 --invariant runWriteAssertions || ok=1
  measure t1-quint-tlc-production pass -- "$QUINT" verify "$q" --backend tlc --tlc-config "$tlccfg" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --invariant safety --verbosity 3 || ok=1
  measure t1-quint-tlc-mutant violation -- "$QUINT" verify "$q" --backend tlc --tlc-config "$tlccfg" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --init initSkipsTempSync --invariant runWriteAssertions --verbosity 3 || ok=1
  measure t1-quint-tlc-liveness-fair pass -- "$QUINT" verify "$q" --backend tlc --tlc-config "$tlccfg" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --temporal eventuallyTerminalFair --verbosity 3 || ok=1
  measure t1-quint-tlc-liveness-unfair violation -- "$QUINT" verify "$q" --backend tlc --tlc-config "$tlccfg" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --temporal eventuallyTerminal --verbosity 3 || ok=1
  measure t1-quint-tlc-move-rename pass -- "$QUINT" verify "$mr" --backend tlc --tlc-config "$tlccfg" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --invariant safety --verbosity 3 || ok=1
  # TLA+ (PlusCal): TLC, and Apalache on the annotated wrapper.
  local tlc=(java -XX:+UseParallelGC -Xmx$((MEM_MB * 3 / 4))m -cp "$TLA_JAR" tlc2.TLC -workers "$WORKERS")
  measure t1-tlc-production pass -- "${tlc[@]}" -config "$t/WriteProtocol.cfg" "$t/WriteProtocol.tla" || ok=1
  measure t1-tlc-mutant violation -- "${tlc[@]}" -config "$t/WriteProtocolSkipsTempSync.cfg" "$t/WriteProtocol.tla" || ok=1
  measure t1-tlc-liveness-unfair violation -- "${tlc[@]}" -config "$t/WriteProtocolNoFairness.cfg" "$t/WriteProtocol.tla" || ok=1
  measure t1-tlc-move-rename pass -- "${tlc[@]}" -config "$t/MoveRename.cfg" "$t/MoveRename.tla" || ok=1
  measure t1-apalache-tla-production pass -- "$APALACHE" check --cinit=CInitProduction --init=Init --next=Next --inv=Safety --length=17 "$t/MC_WriteProtocolApalache.tla" || ok=1
  measure t1-apalache-tla-mutant violation -- "$APALACHE" check --cinit=CInitMutant --init=Init --next=Next --inv=RunWriteAssertions --length=17 "$t/MC_WriteProtocolApalache.tla" || ok=1
  return $ok
}

cmd_t2() {
  require_tools
  local q="$MODELS/quint/journal.qnt" t="$MODELS/tlaplus" ok=0 depth tlccfg
  mkdir -p "$RUNS"
  tlccfg=$(tlc_config "$RUNS")
  local tlc=(java -XX:+UseParallelGC -Xmx$((MEM_MB * 3 / 4))m -cp "$TLA_JAR" tlc2.TLC -workers "$WORKERS")
  measure t2-quint-test-k8-traces pass -- "$QUINT" test "$q" --main k8_traces || ok=1
  measure t2-quint-tlc-k3-8 pass -- "$QUINT" verify "$q" --main k3_8 --backend tlc --tlc-config "$tlccfg" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --invariant invariants --verbosity 3 || ok=1
  measure t2-tlc-k3-8 pass -- "${tlc[@]}" -config "$t/Journal_k3_8.cfg" "$t/Journal.tla" || ok=1
  for depth in ${FORMAL_EVAL_T2_DEPTHS:-6 8}; do
    measure "t2-quint-tlc-k8-$depth" violation -- "$QUINT" verify "$q" --main "k8_$depth" --backend tlc --tlc-config "$tlccfg" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --invariant invariants --verbosity 3 || ok=1
    measure "t2-tlc-k8-$depth" violation -- "${tlc[@]}" -config "$t/Journal_k8_$depth.cfg" "$t/Journal.tla" || ok=1
  done
  return $ok
}

cmd_t3() {
  require_tools
  local q="$MODELS/quint/data_dir_lock.qnt" t="$MODELS/tlaplus" ok=0 tlccfg
  mkdir -p "$RUNS"
  tlccfg=$(tlc_config "$RUNS")
  local tlc=(java -XX:+UseParallelGC -Xmx$((MEM_MB * 3 / 4))m -cp "$TLA_JAR" tlc2.TLC -workers "$WORKERS")
  local cfg
  for cfg in "$t"/DataDirLock_*.cfg; do
    local base expect
    base=$(basename "$cfg" .cfg)
    expect=$(sed -n 's/^\\\* expect: \(pass\|violation\)$/\1/p' "$cfg")
    measure "t3-tlc-${base#DataDirLock_}" "$expect" -- "${tlc[@]}" -config "$cfg" "$t/DataDirLock.tla" || ok=1
  done
  local line name args expect
  while IFS='|' read -r name expect args; do
    [[ -z $name || $name == \#* ]] && continue
    # shellcheck disable=SC2086 # the argument list is word-split on purpose
    measure "t3-quint-$name" "$expect" -- "$QUINT" verify "$q" --backend tlc --tlc-config "$tlccfg" --apalache-version "$APALACHE_FOR_QUINT_VERSION" --verbosity 3 $args || ok=1
  done <"$MODELS/quint/data_dir_lock.checks"
  return $ok
}

cmd_report_data() {
  [[ -f $RUNS/results.tsv ]] || die "no results yet under $RUNS"
  echo "| run | expected | outcome | wall (s) | peak tree RSS (MiB) | distinct states | end |"
  echo "|---|---|---|---|---|---|---|"
  awk -F'\t' '{printf "| %s | %s | %s | %s | %s | %s | %s |\n", $1, $2, $3, $5, $6, $7, $8}' "$RUNS/results.tsv"
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
  # Ad-hoc measured run with the same ceilings: measure NAME EXPECT -- CMD...
  measure)
    shift
    require_tools
    mkdir -p "$RUNS"
    measure "$@"
    ;;
  *)
    echo "usage: $0 {install|versions|t1|t2|t3|all|report-data}" >&2
    exit 64
    ;;
esac
