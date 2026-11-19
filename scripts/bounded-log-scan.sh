#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# A bounded scan of a command's output, beside its live stream
# (bound-widget-log-capture). Source it from a runner:
#
#   source scripts/bounded-log-scan.sh
#   bls_start "$WARN_ERE" "$BENIGN_ERE" ["$KEEP_ERE"] [--keep-ignore-case]
#   bls_isolated "$@" 2>&1 | tee -p "/dev/fd/$BLS_IN" | <live consumer> || status=$?
#   bls_finish
#   bls_check_unexpected || ...       # count, excerpt, "scan incomplete"
#   ... "$(bls_kept_path)" ...         # the kept lines, reopened at offset 0
#   bls_release
#
# What the stage holds is bounded whatever the command prints:
#   - `grep -E "$WARN_ERE" | grep -Ev "$BENIGN_ERE"`, the runner's own two
#     invocations and regexes in the caller's locale, feed one `LC_ALL=C awk`
#     sink: an exact count of unexpected lines and at most BLS_REPORT_CAP
#     (64 KiB) of them for the report, cut at a line boundary (a single longer
#     line is cut at the cap);
#   - with a keep ERE, `grep -E "$KEEP_ERE"` on the raw stream feeds a second
#     sink: at most BLS_KEEP_CAP (1 MiB) of kept lines and their exact count,
#     with overflow recorded. An empty warn ERE is keep-only mode.
# Every sink reads to EOF, so the command never sees a broken pipe or stalls,
# and writes its summary line only at EOF; a missing summary fails closed.
#
# Nothing is named on any exit path: the outputs are created in one
# `lt-scan-<pid>-XXXXXX` directory, opened for writing and for reading, and the
# directory is removed before the stage starts (a kill -9 inside those few
# commands leaves that directory, which `with-temp-scope.sh`'s stale sweep
# reclaims). Readers reopen `/dev/fd/<n>`, which starts at offset 0; the
# sinks only write through the descriptors they inherit (reopening
# `/dev/fd/<n>` for writing would truncate the file).
#
# The stage is a process substitution of the calling shell (`$BLS_PID`), so
# bls_start and bls_finish run in the same shell, once per attempt, and one
# stage at a time. `tee -p` keeps the live stream and the command going if the
# stage dies. `bls_isolated` runs the command with every stage descriptor
# closed, so a daemon it leaves behind cannot hold the verdict.
#
#   scripts/bounded-log-scan.sh --self-test
#       The bound, the drain, the wait, a dead stage, a lingering daemon and
#       kill -9 / SIGTERM of a runner mid-flood. Linux only (/proc).

BLS_REPORT_CAP=65536
BLS_KEEP_CAP=1048576
BLS_SUMMARY_RE='^bls-summary count=([0-9]+) lines=([0-9]+) bytes=([0-9]+) overflow=([01]) done$'

BLS_IN=
BLS_PID=
BLS_R_REPORT=
BLS_R_SUMMARY=
BLS_R_KEPT=
BLS_R_KEPT_SUMMARY=
BLS_WARN=
BLS_KEEP=
BLS_COMPLETE=0
BLS_INCOMPLETE=
BLS_COUNT=0
BLS_REPORT=
BLS_REPORT_LINES=0
BLS_KEPT_COUNT=0
BLS_KEPT_BYTES=0
BLS_KEPT_OVERFLOW=0

# The sink: count every line, write lines while they fit in $1 bytes, read to
# EOF, then write the summary to stderr (the summary descriptor).
bls__sink() {
    LC_ALL=C awk -v cap="$1" '
        {
            n++
            if (!over) {
                len = length($0) + 1
                if (bytes + len <= cap) {
                    print
                    bytes += len
                    lines++
                } else {
                    if (bytes == 0) {
                        print substr($0, 1, cap - 1)
                        bytes = cap
                        lines++
                    }
                    over = 1
                }
            }
        }
        END {
            printf "bls-summary count=%d lines=%d bytes=%d overflow=%d done\n", n, lines, bytes, over > "/dev/stderr"
        }'
}

# The keep branch: the raw stream's lines matching the keep ERE.
bls__keep_branch() {
    { grep "${BLS__KEEP_FLAGS[@]}" -- "$BLS_KEEP" || true; } \
        | bls__sink "$BLS_KEEP_CAP" 1>&"$BLS__W_KEPT" 2>&"$BLS__W_KEPT_SUMMARY"
}

# The warning branch: the runner's two grep invocations, verbatim.
bls__warn_branch() {
    { grep -E -- "$BLS_WARN" || true; } \
        | if [[ -n "$BLS__BENIGN" ]]; then { grep -Ev -- "$BLS__BENIGN" || true; }; else cat; fi \
        | bls__sink "$BLS_REPORT_CAP" 1>&"$BLS__W_REPORT" 2>&"$BLS__W_SUMMARY"
}

# The stage: reads the raw stream on stdin to EOF, fans it out to the keep
# branch itself and awaits it, so it exits only after every sink summarized.
# Its exit status carries nothing (a grep that matched nothing exits 1).
bls__stage() {
    set +e
    trap - EXIT INT TERM
    [[ -z "${_BLS_TEST_STAGE_DELAY:-}" ]] || sleep "$_BLS_TEST_STAGE_DELAY"
    if [[ -z "$BLS_WARN" ]]; then
        bls__keep_branch
    elif [[ -z "$BLS_KEEP" ]]; then
        bls__warn_branch
    else
        local keep_in keep_pid
        exec {keep_in}> >(bls__keep_branch)
        keep_pid=$!
        tee -p "/dev/fd/$keep_in" | { exec {keep_in}>&-; bls__warn_branch; }
        exec {keep_in}>&-
        wait "$keep_pid"
    fi
    return 0
}

# Close a stage descriptor by number, if set.
bls__close() {
    [[ -n "$1" ]] || return 0
    eval "exec $1>&-"
}

# bls_start <warn-ERE> <benign-ERE> [<keep-ERE>] [--keep-ignore-case]
# Starts the stage and sets BLS_IN, the descriptor `tee -p` writes to.
bls_start() {
    local dir
    bls_release
    BLS_WARN="$1"
    BLS__BENIGN="$2"
    BLS_KEEP="${3:-}"
    BLS__KEEP_FLAGS=(-E)
    [[ "${4:-}" != --keep-ignore-case ]] || BLS__KEEP_FLAGS=(-Ei)
    BLS_COMPLETE=0 BLS_INCOMPLETE='' BLS_COUNT=0 BLS_REPORT='' BLS_REPORT_LINES=0
    BLS_KEPT_COUNT=0 BLS_KEPT_BYTES=0 BLS_KEPT_OVERFLOW=0
    dir="$(mktemp -d "${TMPDIR:-/tmp}/lt-scan-$BASHPID-XXXXXX")"
    # shellcheck disable=SC2094 # each output is opened once to write, once to read back
    exec {BLS__W_REPORT}>"$dir/report" {BLS_R_REPORT}<"$dir/report" \
        {BLS__W_SUMMARY}>"$dir/summary" {BLS_R_SUMMARY}<"$dir/summary" \
        {BLS__W_KEPT}>"$dir/kept" {BLS_R_KEPT}<"$dir/kept" \
        {BLS__W_KEPT_SUMMARY}>"$dir/kept-summary" {BLS_R_KEPT_SUMMARY}<"$dir/kept-summary"
    rm -rf -- "$dir"
    exec {BLS_IN}> >(bls__stage)
    BLS_PID=$!
    bls__close "$BLS__W_REPORT"
    bls__close "$BLS__W_SUMMARY"
    bls__close "$BLS__W_KEPT"
    bls__close "$BLS__W_KEPT_SUMMARY"
    BLS__W_REPORT='' BLS__W_SUMMARY='' BLS__W_KEPT='' BLS__W_KEPT_SUMMARY=''
}

# Run "$@" with every stage descriptor closed.
bls_isolated() {
    (
        bls__close "$BLS_IN"
        bls__close "$BLS_R_REPORT"
        bls__close "$BLS_R_SUMMARY"
        bls__close "$BLS_R_KEPT"
        bls__close "$BLS_R_KEPT_SUMMARY"
        "$@"
    )
}

# Read one summary descriptor into the named fields; fails when it is missing.
bls__read_summary() {
    local fd="$1" line
    line="$(head -c 4096 <"/dev/fd/$fd")"
    [[ "$line" =~ $BLS_SUMMARY_RE ]]
}

# Close the stage's input, wait for it, and read its summaries.
bls_finish() {
    bls__close "$BLS_IN"
    BLS_IN=
    [[ -z "$BLS_PID" ]] || wait "$BLS_PID" 2>/dev/null || true
    BLS_PID=
    BLS_COMPLETE=1
    if [[ -n "$BLS_WARN" ]]; then
        if bls__read_summary "$BLS_R_SUMMARY"; then
            BLS_COUNT="${BASH_REMATCH[1]}"
            BLS_REPORT_LINES="${BASH_REMATCH[2]}"
            BLS_REPORT="$(head -c "$BLS_REPORT_CAP" <"/dev/fd/$BLS_R_REPORT")"
        else
            BLS_COMPLETE=0
            BLS_INCOMPLETE="the warning scan wrote no summary"
        fi
    fi
    if [[ -n "$BLS_KEEP" ]]; then
        if bls__read_summary "$BLS_R_KEPT_SUMMARY"; then
            BLS_KEPT_COUNT="${BASH_REMATCH[1]}"
            BLS_KEPT_BYTES="${BASH_REMATCH[3]}"
            BLS_KEPT_OVERFLOW="${BASH_REMATCH[4]}"
        else
            BLS_COMPLETE=0
            BLS_INCOMPLETE="${BLS_INCOMPLETE:+$BLS_INCOMPLETE; }the kept-line scan wrote no summary"
        fi
    fi
    bls__close "$BLS_R_REPORT"
    bls__close "$BLS_R_SUMMARY"
    bls__close "$BLS_R_KEPT_SUMMARY"
    BLS_R_REPORT='' BLS_R_SUMMARY='' BLS_R_KEPT_SUMMARY=''
}

# Close the kept lines' descriptor (and a stage never finished).
bls_release() {
    if [[ -n "$BLS_IN" || -n "$BLS_PID" ]]; then
        bls_finish
    fi
    bls__close "$BLS_R_KEPT"
    BLS_R_KEPT=
}

bls_unexpected_count() { printf '%s\n' "$BLS_COUNT"; }
bls_unexpected_report() { [[ -z "$BLS_REPORT" ]] || printf '%s\n' "$BLS_REPORT"; }
# The kept lines, at most BLS_KEEP_CAP bytes; each open starts at offset 0.
bls_kept_path() { printf '/dev/fd/%s\n' "$BLS_R_KEPT"; }

# The kept lines' byte count (at most BLS_KEEP_CAP).
bls_kept_bytes() { printf '%s\n' "$BLS_KEPT_BYTES"; }

# The verdict of the warning scan: 0 when it finished and found nothing.
# Otherwise print the bounded excerpt and the exact count to stderr, and fail.
bls_check_unexpected() {
    if (( BLS_COMPLETE != 1 )); then
        echo "Error: the output scan was incomplete (${BLS_INCOMPLETE:-no summary}); failing closed." >&2
        return 1
    fi
    (( BLS_COUNT > 0 )) || return 0
    bls_unexpected_report >&2
    if (( BLS_COUNT > BLS_REPORT_LINES )); then
        echo "... and $((BLS_COUNT - BLS_REPORT_LINES)) more unexpected lines ($BLS_COUNT in total)" >&2
    fi
    echo "$BLS_COUNT unexpected line(s)." >&2
    return 1
}

# The kept lines' verdict for a runner that reads them: they must be whole.
bls_check_kept() {
    if (( BLS_COMPLETE != 1 )); then
        echo "Error: the output scan was incomplete (${BLS_INCOMPLETE:-no summary}); failing closed." >&2
        return 1
    fi
    if (( BLS_KEPT_OVERFLOW != 0 )); then
        echo "Error: the kept output exceeded $((BLS_KEEP_CAP / 1048576)) MiB ($BLS_KEEP_CAP bytes; $BLS_KEPT_COUNT kept lines); failing closed." >&2
        return 1
    fi
}

# --- self-test ---------------------------------------------------------------

bls__fail() {
    echo "bounded-log-scan self-test: $*" >&2
    BLS__FAILED=1
}

# The PIDs whose parent (field ppid) or process group (field pgrp) is $2,
# read from /proc (the CI image has no pgrep). The command name in
# /proc/<pid>/stat may hold spaces, so the fields are counted after its `)`.
# A process may exit between the glob and the read: getline reports that as
# -1, which is not fatal (a main-loop read of a vanished file aborts gawk and
# loses every later PID).
bls__procs() {
    local field="$1" value="$2"
    awk -v want="$value" -v col="$([[ "$field" == ppid ]] && echo 2 || echo 3)" '
        BEGIN {
            PROCINFO["NONFATAL"] = 1
            for (i = 1; i < ARGC; i++) {
                file = ARGV[i]
                if ((getline line < file) > 0) {
                    sub(/^.*\) /, "", line)
                    split(line, f, " ")
                    if (f[col] == want) {
                        pid = file
                        sub(/^\/proc\//, "", pid)
                        sub(/\/stat$/, "", pid)
                        print pid
                    }
                }
                close(file)
            }
        }' /proc/[0-9]*/stat 2>/dev/null || true
}

# Stop process $1, then each of its descendants, parent before child, and
# print their PIDs: a stopped parent forks no child the walk could miss.
bls__freeze_tree() {
    local child
    kill -STOP "$1" 2>/dev/null || true
    printf '%s\n' "$1"
    for child in $(bls__procs ppid "$1"); do
        bls__freeze_tree "$child"
    done
}

# Kill process $1 and all its descendants at once: frozen first, so no sink
# sees EOF from a killed upstream and writes its summary before its own kill,
# and no process forks a child after the walk.
bls__kill_tree() {
    local pids
    pids="$(bls__freeze_tree "$1")"
    # shellcheck disable=SC2086 # one PID per word
    kill -KILL $pids 2>/dev/null || true
}

# A flood of $2 copies of the line $1.
bls__flood() {
    { yes "$1" 2>/dev/null || true; } | head -n "$2"
}

BLS__CRITICAL='(lushtext:1): Gtk-CRITICAL **: 00:00:00.000: gtk_widget_get_parent: assertion failed'
BLS__WARN_ERE='(WARNING|CRITICAL)'
BLS__BENIGN_ERE='^benign: '

bls__case_classification() {
    bls_start "$BLS__WARN_ERE" "$BLS__BENIGN_ERE"
    bls_isolated printf '%s\n' 'plain line' 'benign: a WARNING we expect' 'Gtk-WARNING **: one' \
        'x CRITICAL y' 'benign: CRITICAL too' 2>&1 | tee -p "/dev/fd/$BLS_IN" >/dev/null
    bls_finish
    [[ "$BLS_COMPLETE" == 1 && "$BLS_COUNT" == 2 && "$BLS_REPORT" == $'Gtk-WARNING **: one\nx CRITICAL y' ]] \
        || bls__fail "classification: complete=$BLS_COMPLETE count=$BLS_COUNT report='$BLS_REPORT'"
    bls_check_unexpected 2>/dev/null && bls__fail "classification: two unexpected lines passed"
    bls_start "$BLS__WARN_ERE" "$BLS__BENIGN_ERE"
    bls_isolated printf '%s\n' 'ok' 'benign: WARNING' 2>&1 | tee -p "/dev/fd/$BLS_IN" >/dev/null
    bls_finish
    bls_check_unexpected || bls__fail "classification: a clean stream failed"
    return 0
}

bls__case_flood() {
    local status err
    bls_start "$BLS__WARN_ERE" "$BLS__BENIGN_ERE"
    bls_isolated bls__flood "$BLS__CRITICAL" 1048576 2>&1 | tee -p "/dev/fd/$BLS_IN" >/dev/null
    bls_finish
    (( BLS_COUNT == 1048576 && ${#BLS_REPORT} < BLS_REPORT_CAP && BLS_REPORT_LINES > 0 )) \
        || bls__fail "flood: count=$BLS_COUNT report bytes=${#BLS_REPORT} lines=$BLS_REPORT_LINES"
    status=0
    err="$(bls_check_unexpected 2>&1)" || status=$?
    (( status != 0 )) && [[ "$err" == *"more unexpected lines (1048576 in total)"* && ${#err} -le $((BLS_REPORT_CAP + 200)) ]] \
        || bls__fail "flood: verdict status $status, message of ${#err} bytes: ${err: -200}"
    bls_start "$BLS__WARN_ERE" "$BLS__BENIGN_ERE"
    bls_isolated bls__flood 'benign: Gtk-CRITICAL expected' 1048576 2>&1 | tee -p "/dev/fd/$BLS_IN" >/dev/null
    bls_finish
    if ! bls_check_unexpected || (( BLS_COUNT != 0 )); then
        bls__fail "benign flood: count=$BLS_COUNT"
    fi
    return 0
}

bls__case_drain_and_wait() {
    local status start elapsed
    # Well past both caps, then exit 3: no EPIPE, and 3 reaches the caller.
    bls_start "$BLS__WARN_ERE" "$BLS__BENIGN_ERE" 'CRITICAL'
    set +e
    bls_isolated sh -c 'yes "x CRITICAL y" 2>/dev/null | head -n 400000; exit 3' 2>&1 | tee -p "/dev/fd/$BLS_IN" >/dev/null
    status=("${PIPESTATUS[@]}")
    set -e
    bls_finish
    [[ "${status[*]}" == "3 0" ]] || bls__fail "drain: pipeline status ${status[*]}, expected 3 0"
    (( BLS_COUNT == 400000 && BLS_KEPT_OVERFLOW == 1 && BLS_KEPT_COUNT == 400000 )) \
        || bls__fail "drain: count=$BLS_COUNT kept=$BLS_KEPT_COUNT overflow=$BLS_KEPT_OVERFLOW"
    (( $(stat -L -c %s "$(bls_kept_path)") <= BLS_KEEP_CAP )) || bls__fail "drain: kept output over the cap"
    bls_check_kept 2>/dev/null && bls__fail "drain: kept overflow passed bls_check_kept"
    bls_release
    # A slow stage is awaited: its summary is read only after it finished.
    start="$(date +%s%N)"
    _BLS_TEST_STAGE_DELAY=0.4 bls_start "$BLS__WARN_ERE" "$BLS__BENIGN_ERE"
    bls_isolated printf 'CRITICAL %s\n' 1 2 3 2>&1 | tee -p "/dev/fd/$BLS_IN" >/dev/null
    bls_finish
    elapsed=$(( ($(date +%s%N) - start) / 1000000 ))
    (( BLS_COMPLETE == 1 && BLS_COUNT == 3 && elapsed >= 400 )) \
        || bls__fail "slow stage: complete=$BLS_COMPLETE count=$BLS_COUNT after ${elapsed} ms"
    return 0
}

bls__case_dead_stage() {
    local ctl="$1" status lines
    # The command kills the whole stage mid-stream, then keeps writing: it
    # runs to its end, the live stream stays complete, the verdict fails closed.
    bls_start "$BLS__WARN_ERE" "$BLS__BENIGN_ERE" 'CRITICAL'
    export BLS__STAGE_PID="$BLS_PID"
    set +e
    # shellcheck disable=SC2016 # expanded by the inner shell
    bls_isolated bash -c 'source "$1"; bls__flood "a CRITICAL" 1000; sleep 0.1; bls__kill_tree "$BLS__STAGE_PID"; sleep 0.1; bls__flood "b CRITICAL" 200000; exit 0' \
        bls-dead-stage "${BASH_SOURCE[0]}" 2>&1 | tee -p "/dev/fd/$BLS_IN" >"$ctl/live"
    status=("${PIPESTATUS[@]}")
    set -e
    bls_finish
    lines="$(wc -l <"$ctl/live")"
    [[ "${status[*]}" == "0 0" && "$lines" == 201000 ]] \
        || bls__fail "dead stage: pipeline status ${status[*]}, live lines $lines (expected 0 0 and 201000)"
    (( BLS_COMPLETE == 0 )) || bls__fail "dead stage: the verdict did not fail closed"
    bls_check_unexpected 2>/dev/null && bls__fail "dead stage: bls_check_unexpected passed"
    bls_release
    return 0
}

bls__case_linger() {
    local ctl="$1" start elapsed
    # A command that leaves a setsid daemon (stdio detached) returns at once.
    start="$(date +%s%N)"
    bls_start "$BLS__WARN_ERE" "$BLS__BENIGN_ERE" 'kept'
    # shellcheck disable=SC2016 # expanded by the inner shell
    bls_isolated sh -c 'setsid sleep 5 </dev/null >/dev/null 2>&1 & echo "$!" >"$0/daemon"; echo kept' "$ctl" 2>&1 \
        | tee -p "/dev/fd/$BLS_IN" >/dev/null
    bls_finish
    elapsed=$(( ($(date +%s%N) - start) / 1000000 ))
    kill "$(cat "$ctl/daemon")" 2>/dev/null || true
    (( BLS_COMPLETE == 1 && BLS_KEPT_COUNT == 1 && elapsed < 1000 )) \
        || bls__fail "linger: complete=$BLS_COMPLETE kept=$BLS_KEPT_COUNT verdict after ${elapsed} ms (budget 1000 ms)"
    bls_release
    return 0
}

# Every stage output a process of group $1 holds open stays within its cap.
bls__check_orphan_outputs() {
    local group="$1" label="$2" pid link size cap seen=0
    for pid in $(bls__procs pgrp "$group"); do
        for link in /proc/"$pid"/fd/*; do
            case "$(readlink "$link" 2>/dev/null)" in
                */lt-scan-*/report*) cap="$BLS_REPORT_CAP" ;;
                */lt-scan-*/kept\ *) cap="$BLS_KEEP_CAP" ;;
                */lt-scan-*) cap=4096 ;;
                *) continue ;;
            esac
            size="$(stat -L -c %s "$link" 2>/dev/null || echo 0)"
            seen=1
            (( size <= cap )) || bls__fail "$label: an orphaned stage output holds $size bytes (cap $cap)"
        done
    done
    (( seen == 1 )) || bls__fail "$label: found no orphaned stage output to measure"
}

# Start "$@" in the background as the leader of its own process group (job
# control on for that one fork) and set BLS__GROUP to its PID; fails unless
# the group is its own, so a kill case can never pass without its kill.
bls__start_group() {
    local pgrp
    set -m
    "$@" &
    BLS__GROUP=$!
    set +m
    disown "$BLS__GROUP" 2>/dev/null || true
    pgrp="$(awk '{ print $5 }' "/proc/$BLS__GROUP/stat" 2>/dev/null || true)"
    [[ "$pgrp" == "$BLS__GROUP" ]]
}

# The entries of directory $1, on one line.
bls__entries() {
    find "$1" -mindepth 1 -maxdepth 1 -printf '%f ' 2>/dev/null
}

# kill -9 or SIGTERM ($2) of a runner mid-flood, with TMPDIR a private base.
bls__case_abnormal_exit() {
    local ctl="$1" signal="$2" base="$1/base-$2" pid tries=0
    mkdir -p "$base"
    if ! TMPDIR="$base" bls__start_group bash "$ctl/runner" "${BASH_SOURCE[0]}" "$ctl/$signal" </dev/null >/dev/null 2>&1; then
        bls__fail "$signal: the runner did not start as its own process group"
        return 0
    fi
    pid="$BLS__GROUP"
    while [[ ! -e "$ctl/$signal.started" ]] && (( tries < 50 )); do sleep 0.1; tries=$((tries + 1)); done
    if [[ ! -e "$ctl/$signal.started" ]]; then
        bls__fail "$signal: the runner never started"
    else
        sleep 0.3
        kill "-$signal" "$pid" 2>/dev/null || true
        sleep 0.1
        if [[ "$signal" == KILL ]]; then
            ! kill -0 "$pid" 2>/dev/null || bls__fail "KILL: the runner survived kill -9"
        fi
        [[ -z "$(bls__entries "$base")" ]] || bls__fail "$signal: during the orphan the base holds $(bls__entries "$base")"
        [[ "$signal" != KILL ]] || bls__check_orphan_outputs "$pid" "$signal"
        tries=0
        while [[ ! -e "$ctl/$signal.done" ]] && (( tries < 80 )); do sleep 0.1; tries=$((tries + 1)); done
        sleep 0.1
        [[ -z "$(bls__entries "$base")" ]] || bls__fail "$signal: after the flood the base holds $(bls__entries "$base")"
    fi
    kill -KILL -- "-$pid" 2>/dev/null || true
    return 0
}

bls_self_test() {
    local ctl start elapsed pids=() pid failed=0
    start="$(date +%s%N)"
    ctl="$(mktemp -d "${TMPDIR:-/tmp}/bounded-log-scan-self-test.XXXXXX")"
    # shellcheck disable=SC2064 # expand now: the trap must remove this dir
    trap "rm -rf -- '$ctl'" EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    for tool in setsid stat tee awk; do
        command -v "$tool" >/dev/null 2>&1 || { echo "bounded-log-scan self-test: '$tool' is required" >&2; return 1; }
    done
    tee -p </dev/null >/dev/null 2>&1 || { echo "bounded-log-scan self-test: this tee has no -p" >&2; return 1; }
    # The mid-flood runner: a paced flood of lines matching both branches.
    cat >"$ctl/runner" <<'RUNNER'
set -u
source "$1"
out="$2"
trap 'exit 143' TERM
bls_start '(WARNING|CRITICAL)' '^benign: ' 'CRITICAL'
# shellcheck disable=SC2016
bls_isolated bash -c 'source "$1"; : >"$2.started"; for _ in $(seq 20); do bls__flood "x CRITICAL y" 65536; sleep 0.05; done; : >"$2.done"' bls-flood "$1" "$out" 2>&1 \
    | tee -p "/dev/fd/$BLS_IN" >/dev/null
bls_finish
RUNNER
    (BLS__FAILED=0; bls__case_classification; bls__case_drain_and_wait; exit "$BLS__FAILED") & pids+=($!)
    (BLS__FAILED=0; bls__case_flood; exit "$BLS__FAILED") & pids+=($!)
    (BLS__FAILED=0; bls__case_dead_stage "$ctl"; bls__case_linger "$ctl"; exit "$BLS__FAILED") & pids+=($!)
    (BLS__FAILED=0; bls__case_abnormal_exit "$ctl" KILL; exit "$BLS__FAILED") & pids+=($!)
    (BLS__FAILED=0; bls__case_abnormal_exit "$ctl" TERM; exit "$BLS__FAILED") & pids+=($!)
    for pid in "${pids[@]}"; do
        wait "$pid" || failed=1
    done
    elapsed=$(( ($(date +%s%N) - start) / 1000000 ))
    (( failed == 0 )) || return 1
    echo "bounded-log-scan self-test passed in ${elapsed} ms (classification, 2^20-line floods bounded and counted, drain with exit status, slow stage awaited, dead stage fails closed, lingering daemon, kill -9 and SIGTERM leave nothing)."
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    set -euo pipefail
    case "${1:-}" in
        --self-test) bls_self_test ;;
        *)
            sed -n '3,42p' "$0" | sed 's/^# \{0,1\}//'
            exit 2
            ;;
    esac
fi
