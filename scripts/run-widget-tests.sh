#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEMP_SCOPE="$SCRIPT_DIR/with-temp-scope.sh"
# The one way a lane starts a private headless session (the `widget` profile:
# private system bus, no media, no accessibility bus, no portals, and the
# harness's recommended pre-GTK environment set before the bus starts).
HEADLESS_SESSION="$SCRIPT_DIR/headless-session"
# The bounded warning scan beside the live stream (bls_start, bls_finish).
# shellcheck source=scripts/bounded-log-scan.sh
source "$SCRIPT_DIR/bounded-log-scan.sh"

# Paths this script itself creates outside the scoped TMPDIR (self-test
# fixtures), removed on every exit path, including SIGINT and SIGTERM. A
# `( … )` subshell has no traps, so only this top-level shell appends here;
# the warning scan names nothing (its outputs are unlinked descriptors).
OWNED_TEMP_PATHS=()
# shellcheck disable=SC2329 # invoked by the EXIT trap below
remove_owned_temp_paths() {
    if (( ${#OWNED_TEMP_PATHS[@]} > 0 )); then
        rm -rf -- "${OWNED_TEMP_PATHS[@]}"
    fi
}
trap remove_owned_temp_paths EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

RETRIES=0
MONITOR="2560x1600"
# Names the temp scope (`lt-scope-<pid>-<label>-XXXXXX`) and so the session's
# runtime directory; `widget-shards.py parallel` gives each shard a short,
# distinct one.
SCOPE_LABEL=widget
TEST_ARGS=()
SELF_TEST=false
# The command run_headless runs inside the headless environment. Empty means
# the real harness (the temp scope, the session helper, the widget binary);
# only `--self-test -- <command>`, the self-test's own entry point, sets it.
WIDGET_HARNESS_COMMAND=()
# gtk_lush_proof_harness::session::SESSION_HYGIENE_EXIT_CODE: the session
# touched the host (rtkit, bluez, devices, an accessibility bus, a portal).
# Such a run is never retried; a second session would touch the host again.
SESSION_HYGIENE_EXIT_CODE=93
# The private session's own services (mutter, the portal frontend) report the
# services the session does not run; that family lives once in
# smoke_warning_classifiers.py and is read here as POSIX ERE.
SESSION_SERVICES_REGEX="$(python3 -B "$SCRIPT_DIR/smoke_warning_classifiers.py" --print-regex headless-session-services)"
# GTK 4.24 prints `gdk_frame_timings_presented() called on discarded frame.`
# once per process when the compositor discards a popover's unpainted buffer
# while the popover is repositioned: the popover and its toplevel share one
# frame clock, and GDK marks the shared frame discarded from one surface's
# feedback (ledger A134; GTK 4.22 has no such state, and GTK main removed the
# transition in d2595c6b, MR !10242, not in gtk-4-24). It is benign here as
# that exact line from a harness child (`process`) and as nothing else; the
# smoke lanes stay strict. Remove the entry once the shipped toolkit and the
# CI image carry d2595c6b (build.md, "GTK Axioms and Toolkit Updates").
BENIGN_WIDGET_NOISE_REGEX='(dbus-daemon\[[0-9]+\]: .*org\.(freedesktop\.(portal|impl\.portal\.|systemd1)|a11y\.Bus)|\(/usr/libexec/xdg-desktop-portal:.*WARNING \*\*:|\*\* \(xdg-desktop-portal-gtk:.*WARNING \*\*:|Gdk-Message: .*Broken pipe$|^libmutter-Message:|^\*\* Message: .*Obtained a high priority EGL context$|^\*\* \(mutter:[0-9]+\): WARNING \*\*: ([0-9:.]+: )?Skipping layers 1\.\.n of your pipeline since the first layer is sliced\. We don'\''t currently support any multi-texturing with sliced textures but assume layer 0 is the most important to keep$|.*WARNING: Glycin running without sandbox\.( .*)?$|^\(process:[0-9]+\): Gdk-WARNING \*\*: [0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3}: gdk_frame_timings_presented\(\) called on discarded frame\.$)|'"$SESSION_SERVICES_REGEX"
# Cargo diagnostics spell this as a standalone `warning:` token. Requiring a
# line/whitespace boundary keeps `--list` output such as `...visible_warning:
# test` from being mistaken for a toolkit or compiler warning.
WIDGET_WARNING_REGEX='((^|[[:space:]])warning:|WARNING|CRITICAL|Gdk-Message:|Broken pipe|cannot remove|^MESA: error:)'

usage() {
    cat <<'EOF'
Usage: scripts/run-widget-tests.sh [--headless] [--retries N] [--monitor WxH] [--scope-label LABEL] [-- [test-binary-args...]]

Run the LushText widget test harness under a transient Mutter headless compositor.
The harness intentionally has no native/live-display mode.

Options:
  --headless    Always run under `mutter --headless`.
  --retries N   Retry the full harness up to N times after the first failure.
  --monitor WxH Virtual monitor size for headless runs (default: 2560x1600).
  --scope-label LABEL
                Label of the temporary-file scope and session runtime
                directory (default: widget).
  --self-test   Verify warning classification and the temporary-file scope
                without launching GTK.
  -h, --help    Show this help text.

Arguments after `--` are passed to the widget test binary, for example:
  scripts/run-widget-tests.sh -- --exact window::test_primary_menu_button_exists
EOF
}

# Sanitize the harness output as it flows past instead of after the run ends.
# Line-buffered filters keep per-test progress visible in the job log, so a
# cancelled run (for example a GitHub Actions job timeout) still shows how far
# the suite got. `grep` exits 1 when every line was benign noise, which must not
# be mistaken for a harness failure. `-a`: a NUL byte must not turn the rest of
# the job log into one "binary file matches" line.
emit_sanitized_widget_stream() {
    local noise_filter=(cat)
    if [[ -n "${BENIGN_WIDGET_NOISE_REGEX}" ]]; then
        noise_filter=(grep --line-buffered -aEv "$BENIGN_WIDGET_NOISE_REGEX")
    fi
    { "${noise_filter[@]}" || true; } \
        | awk 'NF { print; fflush(); blank = 0; next } !blank { print; fflush(); blank = 1 }'
}

# The verdict of the last scan (bls_finish): its exact count of unexpected
# lines and a bounded excerpt, or a scan that did not finish, fail the run.
check_for_unexpected_widget_warnings() {
    if ! bls_check_unexpected; then
        echo "Error: unexpected warning output during widget tests ($BLS_COUNT unexpected lines${BLS_INCOMPLETE:+; $BLS_INCOMPLETE})." >&2
        return 1
    fi
}

# Classify the lines on stdin through the same stage a real run uses.
check_widget_lines() {
    bls_start "$WIDGET_WARNING_REGEX" "$BENIGN_WIDGET_NOISE_REGEX"
    bls_isolated cat 2>&1 | tee -p "/dev/fd/$BLS_IN" >/dev/null
    bls_finish
    check_for_unexpected_widget_warnings
}

self_test_warning_classification() {
    printf '%s\n' "** (mutter:582): WARNING **: 09:42:29.034: Skipping layers 1..n of your pipeline since the first layer is sliced. We don't currently support any multi-texturing with sliced textures but assume layer 0 is the most important to keep" \
        | check_widget_lines

    if ! python3 -B "$SCRIPT_DIR/smoke_warning_classifiers.py" --print-samples headless-session-services | check_widget_lines; then
        echo "Error: widget warning self-test rejected the shared session-services family." >&2
        return 1
    fi

    if printf '%s\n' "Gtk-WARNING **: allocation failed" | check_widget_lines >/dev/null 2>&1; then
        echo "Error: widget warning self-test accepted an unexpected GTK warning." >&2
        return 1
    fi

    # GTK 4.24's discarded-frame warning (ledger A134) is benign as this exact
    # line from a harness child, and as nothing else. The sample is the line a
    # gate run printed (tab-strip-overflow-navigation, gate-apply-test.log).
    printf '%s\n' "(process:2232206): Gdk-WARNING **: 15:17:27.766: gdk_frame_timings_presented() called on discarded frame." >"$log_file"
    if ! check_for_unexpected_widget_warnings "$log_file"; then
        echo "Error: widget warning self-test rejected GTK 4.24's discarded-frame warning (A134)." >&2
        return 1
    fi
    local near_miss
    for near_miss in \
        "(process:2232206): Gdk-WARNING **: 15:17:27.766: gdk_frame_timings_presented() called on skipped frame." \
        "(process:2232206): Gdk-WARNING **: 15:17:27.766: gdk_frame_timings_presented() called on submitted frame." \
        "(process:2232206): Gdk-WARNING **: 15:17:27.766: gdk_frame_timings_submitted() called on discarded frame." \
        "(process:2232206): Gdk-WARNING **: 15:17:27.766: gdk_frame_timings_discarded() called on already presented frame." \
        "(process:2232206): Gdk-WARNING **: 15:17:27.766: Duplicate call to gdk_frame_timings_presented() with different values: 1 != 2" \
        "(process:2232206): Gtk-WARNING **: 15:17:27.766: gdk_frame_timings_presented() called on discarded frame." \
        "(process:2232206): Gdk-CRITICAL **: 15:17:27.766: gdk_frame_timings_presented() called on discarded frame." \
        "(lushtext:2232206): Gdk-WARNING **: 15:17:27.766: gdk_frame_timings_presented() called on discarded frame." \
        "(process:2232206): Gdk-WARNING **: 15:17:27.766: gdk_frame_timings_presented() called on discarded frame. Also something else" \
        "test window::test_x ... (process:2232206): Gdk-WARNING **: 15:17:27.766: gdk_frame_timings_presented() called on discarded frame."; do
        printf '%s\n' "$near_miss" >"$log_file"
        if check_for_unexpected_widget_warnings "$log_file" >/dev/null 2>&1; then
            echo "Error: widget warning self-test accepted a near miss of the discarded-frame warning: $near_miss" >&2
            return 1
        fi
    done

    # GTK_A11Y=test reaches no AT-SPI bus, so the registry critical the old
    # session printed (its bus could not activate at-spi2-registryd) cannot
    # occur in a widget session; if it comes back, it fails the lane.
    if printf '%s\n' "(process:42): Gtk-CRITICAL **: 14:07:27.435: Unable to register the application: GDBus.Error:org.freedesktop.DBus.Error.NameHasNoOwner: Could not activate remote peer 'org.a11y.atspi.Registry': unit failed" \
        | check_widget_lines >/dev/null 2>&1; then
        echo "Error: widget warning self-test accepted the AT-SPI registry critical." >&2
        return 1
    fi

    # A NUL byte must not cut the live job log: without --text GNU grep prints
    # only "binary file matches" for the whole stream.
    local live
    live="$(printf 'before\nnoise \0 bytes\nafter the NUL\n' | emit_sanitized_widget_stream 2>&1 | tr -d '\0')"
    if [[ "$live" != $'before\nnoise  bytes\nafter the NUL' ]]; then
        echo "Error: widget warning self-test: a NUL byte cut the live stream to '$live'." >&2
        return 1
    fi

    echo "Widget warning classification self-test passed."
    if retry_allowed "$SESSION_HYGIENE_EXIT_CODE" || ! retry_allowed 101; then
        echo "Error: a session-hygiene failure must not be retried, and a test failure may be." >&2
        return 1
    fi
    echo "Widget retry classification self-test passed."
    self_test_memory_report
    self_test_capture_cases
    "$SCRIPT_DIR/bounded-log-scan.sh" --self-test
    "$TEMP_SCOPE" --self-test
}

# Wait up to $2 tenths of a second for the file $1 to exist.
wait_for_file() {
    local tries=0
    while [[ ! -e "$1" ]] && (( tries < $2 )); do
        sleep 0.1
        tries=$((tries + 1))
    done
    [[ -e "$1" ]]
}

# Wait up to $2 tenths of a second for process $1 to exit.
wait_for_exit() {
    local tries=0
    while kill -0 "$1" 2>/dev/null && (( tries < $2 )); do
        sleep 0.1
        tries=$((tries + 1))
    done
    ! kill -0 "$1" 2>/dev/null
}

# A capture case's directory: the fake harness and its private base. The fake
# records the PID of the shell that runs it, floods `$1` chunks of 2^16
# Gtk-CRITICAL lines (pausing `$2` s between them), then lists every named
# file over 64 KiB in the base while its runner still runs.
capture_case_dir() {
    mkdir -p "$1/base"
    cat >"$1/flood" <<'FLOOD'
#!/usr/bin/env bash
ctl="${0%/*}"
echo "$PPID" >"$ctl/parent.tmp" && mv "$ctl/parent.tmp" "$ctl/parent"
for _ in $(seq "$1"); do
    # yes reports EPIPE on stderr where SIGPIPE is ignored (a git hook): drop it.
    yes '(lushtext:1): Gtk-CRITICAL **: 00:00:00.000: gtk_widget_get_parent: assertion failed' 2>/dev/null | head -n 65536
    sleep "$2"
done
find "$ctl/base" -type f -size +64k >"$ctl/big"
touch "$ctl/done"
FLOOD
    chmod +x "$1/flood"
}

# (a) A flood of 2^20 lines: no capture over 64 KiB exists while it runs, the
# run fails, and the report counts every line.
capture_case_flood() {
    local dir="$1" status failed=0
    capture_case_dir "$dir"
    set +e
    TMPDIR="$dir/base" "$0" --self-test -- "$dir/flood" 16 0 2>&1 >/dev/null | tail -c 65536 >"$dir/err"
    status="${PIPESTATUS[0]}"
    set -e
    if [[ -s "$dir/big" ]]; then
        echo "Error: capture case (a): a capture over 64 KiB existed during the flood: $(head -n 3 "$dir/big" | tr '\n' ' ')" >&2
        failed=1
    fi
    if (( status == 0 )); then
        echo "Error: capture case (a): a flood of unexpected lines passed." >&2
        failed=1
    fi
    if ! grep -q '1048576 unexpected' "$dir/err"; then
        echo "Error: capture case (a): the report does not count the 1048576 unexpected lines: $(tail -c 300 "$dir/err")" >&2
        failed=1
    fi
    if [[ -n "$(bls__entries "$dir/base")" ]]; then
        echo "Error: capture case (a): the base gained $(bls__entries "$dir/base")" >&2
        failed=1
    fi
    return "$failed"
}

# (b) kill -9 of the runner mid-flood: the base gains no entry while the
# orphaned pipeline streams, nor after it ends.
capture_case_kill() {
    local dir="$1" failed=0
    capture_case_dir "$dir"
    if ! TMPDIR="$dir/base" bls__start_group "$0" --self-test -- "$dir/flood" 12 0.05 </dev/null >/dev/null 2>&1; then
        echo "Error: capture case (b): the runner did not start as its own process group." >&2
        failed=1
    elif wait_for_file "$dir/parent" 50; then
        sleep 0.3
        kill -KILL "$BLS__GROUP" 2>/dev/null || true
        if ! wait_for_exit "$BLS__GROUP" 20; then
            echo "Error: capture case (b): the runner survived kill -9." >&2
            failed=1
        fi
        if [[ -n "$(bls__entries "$dir/base")" ]]; then
            echo "Error: capture case (b): after kill -9 the base holds $(bls__entries "$dir/base")" >&2
            failed=1
        fi
        wait_for_file "$dir/done" 100 || true
        sleep 0.2
        if [[ -n "$(bls__entries "$dir/base")" ]]; then
            echo "Error: capture case (b): after the orphan ended the base holds $(bls__entries "$dir/base")" >&2
            failed=1
        fi
    else
        echo "Error: capture case (b): the fake harness never started." >&2
        failed=1
    fi
    kill -KILL -- "-$BLS__GROUP" 2>/dev/null || true
    return "$failed"
}

# (c) SIGTERM to the subshell that runs the command, mid-run: no scan entry
# remains once the runner has finished.
capture_case_term() {
    local dir="$1" failed=0
    capture_case_dir "$dir"
    if ! TMPDIR="$dir/base" bls__start_group "$0" --self-test -- "$dir/flood" 12 0.05 </dev/null >/dev/null 2>&1; then
        echo "Error: capture case (c): the runner did not start as its own process group." >&2
        failed=1
    elif wait_for_file "$dir/parent" 50; then
        sleep 0.2
        kill -TERM "$(cat "$dir/parent")" 2>/dev/null || true
        wait_for_exit "$BLS__GROUP" 100 || true
        sleep 0.2
        if [[ -n "$(bls__entries "$dir/base")" ]]; then
            echo "Error: capture case (c): after SIGTERM to the subshell the base holds $(bls__entries "$dir/base")" >&2
            failed=1
        fi
    else
        echo "Error: capture case (c): the fake harness never started." >&2
        failed=1
    fi
    kill -KILL -- "-$BLS__GROUP" 2>/dev/null || true
    return "$failed"
}

# The capture cases (bound-widget-log-capture), side by side: run_headless,
# and so run_with_widget_log in the same place a real run uses, over a fake
# flooding command, with TMPDIR set to a private base that must gain no
# entry. The flood never reaches this output: the live stream goes to
# /dev/null and the runner's stderr is cut to its tail.
self_test_capture_cases() {
    local ctl failed=0 pid pids=()
    ctl="$(mktemp -d)"
    OWNED_TEMP_PATHS+=("$ctl")
    capture_case_flood "$ctl/a" & pids+=($!)
    capture_case_kill "$ctl/b" & pids+=($!)
    capture_case_term "$ctl/c" & pids+=($!)
    for pid in "${pids[@]}"; do
        wait "$pid" || failed=1
    done
    rm -rf -- "$ctl"
    (( failed == 0 )) || return 1
    echo "Widget capture self-test passed (a flood bounded and counted, kill -9 and SIGTERM leave nothing)."
}

self_test_memory_report() {
    local fixture report failed=0

    # The memory report, read from a fixture cgroup directory rather than by
    # provoking the OOM killer: a clean scope passes, an OOM kill fails loudly.
    fixture="$(mktemp -d)"
    OWNED_TEMP_PATHS+=("$fixture")
    printf '%s\n' 1073741824 >"$fixture/memory.peak"
    printf '%s\n' 'low 0' 'high 0' 'max 3' 'oom 1' 'oom_kill 0' >"$fixture/memory.events"
    if [[ "$(report_memory_scope "$fixture" 2>&1)" != "widget-memory: peak 1024 MiB, oom_kill 0" ]]; then
        echo "Error: the memory report misread a clean scope." >&2
        failed=1
    fi
    printf '%s\n' 'low 0' 'high 0' 'max 3' 'oom 1' 'oom_kill 2' >"$fixture/memory.events"
    if report="$(report_memory_scope "$fixture" 2>&1)" || [[ "$report" != *OOM-KILL:* ]]; then
        echo "Error: the memory report accepted a scope with OOM kills: $report" >&2
        failed=1
    fi
    rm -rf -- "$fixture"
    (( failed == 0 )) || return 1
    echo "Widget memory report self-test passed."
}

# Report the memory scope this runner ran in: its peak, and whether the kernel
# OOM-killed anything in it. Under `with-memory-cap.sh` the scope uses
# `OOMPolicy=continue`, so an OOM kill removes only the largest process
# (usually the current test child), which the harness may then retry and pass
# as FLAKY; failing here keeps an OOM from ever being green. Reported only when
# the runner is under `with-memory-cap.sh` (LUSHTEXT_MEMORY_CAP_KIB), so CI's
# uncapped output is unchanged; nested in place, the scope is the outer one.
report_memory_scope() {
    local dir="$1" peak oom_kills
    [[ -r "$dir/memory.peak" && -r "$dir/memory.events" ]] || return 0
    read -r peak <"$dir/memory.peak"
    oom_kills="$(awk '$1 == "oom_kill" { print $2 }' "$dir/memory.events")"
    echo "widget-memory: peak $(( peak / 1048576 )) MiB, oom_kill ${oom_kills:-0}"
    if (( ${oom_kills:-0} > 0 )); then
        echo "OOM-KILL: the kernel killed ${oom_kills} process(es) in this run's memory scope; raise nothing, find what grew." >&2
        return 1
    fi
}

memory_scope_dir() {
    local cgroup
    cgroup="$(sed -n 's/^0:://p' /proc/self/cgroup 2>/dev/null)"
    [[ -n "$cgroup" ]] && printf '/sys/fs/cgroup%s\n' "$cgroup"
}

# Run "$@" with its output scanned for warnings beside the live stream. The
# stage holds an exact count and at most 64 KiB of unexpected lines, names
# nothing on disk, and is started and awaited in this shell (`wait` reaches
# only this shell's children), so this runs in the top-level shell, never in
# a `( … )` subshell. `tee -p` keeps the live stream going if the stage dies,
# which then fails the verdict closed. `pipefail` is already set, so the
# harness exit code still propagates.
run_with_widget_log() {
    local status=0
    if ! bls_start "$WIDGET_WARNING_REGEX" "$BENIGN_WIDGET_NOISE_REGEX"; then
        echo "Error: $BLS__START_ERROR; not running the widget tests." >&2
        return 1
    fi
    bls_isolated "$@" 2>&1 | tee -p "/dev/fd/$BLS_IN" | emit_sanitized_widget_stream || status=$?
    bls_finish

    if ! check_for_unexpected_widget_warnings; then
        return 1
    fi
    return "$status"
}

# The headless environment and the harness, in a subshell that creates and
# registers nothing: the exports stay out of this shell's later attempts.
# shellcheck disable=SC2329 # run by run_headless through run_with_widget_log
headless_widget_harness() (
    export GDK_BACKEND=wayland
    export LUSHTEXT_WIDGET_HEADLESS_RUNNER=1
    export LUSHTEXT_WIDGET_HEADLESS_MONITOR="$MONITOR"
    # Cairo keeps widget tests on GTK's CPU fallback renderer, so headless
    # containers do not probe missing Mesa/EGL devices; the session keeps
    # a caller's GSK_RENDERER when a renderer bug is being chased.
    unset DISPLAY WAYLAND_DISPLAY
    "${WIDGET_HARNESS_COMMAND[@]}"
)

run_headless() {
    if (( ${#WIDGET_HARNESS_COMMAND[@]} == 0 )); then
        # The temp scope gives the whole harness a private TMPDIR and a short
        # private XDG_RUNTIME_DIR, releases the runtime directory only once
        # the session's services have exited, and fails the run if any test
        # process left a temporary entry behind. The session helper runs in
        # that directory and judges, at teardown, what the session touched.
        WIDGET_HARNESS_COMMAND=(
            "$TEMP_SCOPE" --session-runtime "$SCOPE_LABEL" --
            "$HEADLESS_SESSION" run --profile widget --inherit-runtime-dir --monitor "$MONITOR" --
            cargo test -p lushtext --test widget -- "${TEST_ARGS[@]}"
        )
    fi
    run_with_widget_log headless_widget_harness
}

# Whether a failed attempt may be rerun: never after a session-hygiene verdict.
retry_allowed() {
    local status="$1"
    (( status != SESSION_HYGIENE_EXIT_CODE ))
}

run_once() {
    echo "Running widget tests under mutter --headless..."
    run_headless
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --headless)
            shift
            ;;
        --auto|--native)
            echo "Error: '$1' was removed; widget tests are headless-only." >&2
            exit 1
            ;;
        --retries)
            [[ $# -lt 2 ]] && { echo "Error: --retries requires a value." >&2; exit 1; }
            RETRIES="$2"
            shift 2
            ;;
        --monitor)
            [[ $# -lt 2 ]] && { echo "Error: --monitor requires a value." >&2; exit 1; }
            MONITOR="$2"
            shift 2
            ;;
        --scope-label)
            [[ $# -lt 2 ]] && { echo "Error: --scope-label requires a value." >&2; exit 1; }
            SCOPE_LABEL="$2"
            shift 2
            ;;
        --self-test)
            SELF_TEST=true
            shift
            ;;
        --)
            shift
            TEST_ARGS=("$@")
            break
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Error: unknown argument '$1'." >&2
            usage
            exit 1
            ;;
    esac
done

if ! [[ "$RETRIES" =~ ^[0-9]+$ ]]; then
    echo "Error: --retries must be a non-negative integer." >&2
    exit 1
fi

if [[ "$SELF_TEST" == true && ${#TEST_ARGS[@]} -eq 0 ]]; then
    self_test_warning_classification
    exit 0
fi

status=0
if [[ "$SELF_TEST" == true ]]; then
    # The self-test's own entry point: run_headless over a fake command,
    # exactly as a real run would, then the same memory report.
    WIDGET_HARNESS_COMMAND=("${TEST_ARGS[@]}")
    run_headless || status=$?
else
    attempt=1
    max_attempts=$((RETRIES + 1))
    while (( attempt <= max_attempts )); do
        if run_once; then
            status=0
            break
        else
            status=$?
        fi

        if (( attempt == max_attempts )); then
            break
        fi
        if ! retry_allowed "$status"; then
            echo "Widget test run failed its session-hygiene check (exit $status); not retrying." >&2
            break
        fi

        echo "Widget test run failed on attempt $attempt/$max_attempts; retrying..." >&2
        attempt=$((attempt + 1))
    done
fi

if [[ -n "${LUSHTEXT_MEMORY_CAP_KIB:-}" ]] && scope_dir="$(memory_scope_dir)"; then
    if ! report_memory_scope "$scope_dir"; then
        (( status != 0 )) || status=1
    fi
fi
exit "$status"
