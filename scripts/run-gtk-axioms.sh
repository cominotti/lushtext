#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run the gtk-lush-axioms probes and samples.
#
#   scripts/run-gtk-axioms.sh all
#       The probe binary (one isolated test per probed axiom, headless), then
#       every sample's --check in one private headless session. This is
#       `make gtk-axioms`, and the CI `gtk-axioms` job. Probes marked slow in
#       the catalogue run only with GTK_LUSH_AXIOMS_INCLUDE_SLOW=1 (local
#       `make gtk-axioms` and the nightly responsiveness workflow set it; the
#       pull-request job does not); otherwise the probe runner and the sample
#       print `skipped (slow): <id>`.
#   scripts/run-gtk-axioms.sh sample <id> [--check]
#       One sample. With --check it runs headless and exits with the verdict
#       (0 holds, 1 violated, 2 fixture invalid). Without it the sample opens
#       its interactive window on the current desktop session, for a person to
#       watch. This is `make gtk-axiom-sample AXIOM=<id> [CHECK=1]`.
#   scripts/run-gtk-axioms.sh runtimes [--only <ids>] [<sdk-ref>...]
#       LOCAL ONLY. Build the samples inside each installed GNOME SDK (default
#       org.gnome.Sdk//50 (the source floor), org.gnome.Sdk//51 (the shipped
#       runtime) and org.gnome.Sdk//master, each with the
#       rust-stable extension) and run every --check against that SDK's own
#       GTK and Libadwaita, under a private headless session on the host. The
#       gtk-rs feature flags bind a binary to the SDK it was built against, so
#       each SDK gets its own target directory. An SDK that is not installed
#       is skipped and reported; nothing is installed. This is
#       `make gtk-axioms-runtimes`. `--only A28,A79` builds and checks only
#       the named samples (an unknown id fails the run), for interleaved
#       re-test rounds. The sandbox has no session bus, so a sample whose
#       fixture drives the compositor's RemoteDesktop (A59-A61, A64, A97)
#       prints `skipped (needs RemoteDesktop): <id>` there (the run sets
#       GTK_LUSH_AXIOMS_SANDBOX=1; the host lane never does). After each SDK
#       it prints a summary: every sample's verdict or skip reason, and the
#       SDK's toolkit versions.
#   scripts/run-gtk-axioms.sh sample <id> --screenshot <png>
#       The interactive window under a private headless session, captured to
#       <png> after it settles. Evidence for review only; never committed.
#   scripts/run-gtk-axioms.sh --self-test
#       Check the warning classification, the result count (slow and
#       RemoteDesktop skips) and the --only selection, without GTK.
#
# Probes and --check samples never touch the developer's live desktop: they run
# under `dbus-run-session -- mutter --headless`, with the environment of
# scripts/run-widget-tests.sh.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLES_DIR="$REPO_ROOT/crates/gtk-lush/axioms/examples"
UNSUPPORTED_HOST_EXIT_CODE=77
MONITOR="${GTK_AXIOMS_MONITOR:-1280x1024}"
# Any warning or critical is a defect here, as in the widget lane, whatever
# process or log domain printed it: a GLib-style "** (prog:pid): WARNING **"
# from a helper (xdg-dbus-proxy logs as "(process:pid)", with no domain), a
# Broken pipe, a library's "*** BUG ***" report (pixman's invalid-rectangle
# line, which bypasses GLib logging), or a runtime directory the session could
# not remove all fail the lane. The private session's own mutter and dbus-daemon chatter is not ours;
# BENIGN_NOISE_REGEX names exactly those lines, taken from the widget runner's
# list of what the same headless session prints on the hosts and CI images.
WARNING_REGEX='(WARNING|CRITICAL|\*\*\* BUG \*\*\*|Broken pipe|cannot remove|^MESA: error:)'
BENIGN_NOISE_REGEX='(^dbus-daemon\[[0-9]+\]: |^libmutter-Message:|^\*\* Message: .*Obtained a high priority EGL context$|^\*\* \(mutter:[0-9]+\): WARNING \*\*: ([0-9:.]+: )?Skipping layers 1\.\.n of your pipeline since the first layer is sliced\. |^\(mutter:[0-9]+\): mutter-WARNING \*\*: .*Failed to acquire org\.freedesktop\.locale1 proxy: Could not connect: No such file or directory$|^\(mutter:[0-9]+\): libmutter-WARNING \*\*: .*Failed to connect to colord daemon: Could not connect: No such file or directory$)'

# An SDK sandbox gets no D-Bus proxy and no document portal: the sample checks
# need neither, and each is a Flatpak-side helper racing a sample's exit. With
# the session bus proxied, every sample's GIO gvfs client connects through the
# sandbox's xdg-dbus-proxy, which intermittently logged "Error writing
# credentials to socket: Error sending message: Broken pipe" when a sample
# exited mid-handshake; the document portal left its FUSE mount in the private
# runtime directory, which then could not be removed.
SANDBOX_FLAGS=(--no-session-bus --no-a11y-bus --no-documents-portal)

usage() {
    sed -n '4,41p' "$0" | sed 's/^# \{0,1\}//'
}

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "UNSUPPORTED-HOST: '$1' is required to run GTK axiom samples headless." >&2
        exit "$UNSUPPORTED_HOST_EXIT_CODE"
    fi
}

# The one list of the samples' headless environment: exported on the host,
# passed as --env flags into an SDK sandbox. A caller's GSK_RENDERER wins.
HEADLESS_ENV=(
    GTK_LUSH_AXIOMS_HEADLESS=1
    NO_AT_BRIDGE=1
    GTK_A11Y=none
    ADW_DISABLE_PORTAL=1
    GDK_DEBUG=no-portals
    GTK_USE_PORTAL=0
    GTK_IM_MODULE=gtk-im-context-simple
    "GSK_RENDERER=${GSK_RENDERER:-cairo}"
    "GTK_LUSH_AXIOMS_INCLUDE_SLOW=${GTK_LUSH_AXIOMS_INCLUDE_SLOW:-0}"
    GDK_BACKEND=wayland
)

export_headless_env() {
    export "${HEADLESS_ENV[@]}"
    unset DISPLAY WAYLAND_DISPLAY
}

# The example stem for an axiom id such as A9, a9, 9, or A09.
example_for() {
    local raw="${1#[Aa]}"
    if ! [[ "$raw" =~ ^[0-9]+$ ]]; then
        echo "unknown axiom id: $1" >&2
        return 2
    fi
    local padded
    padded="$(printf 'a%02d_' "$((10#$raw))")"
    local match
    match="$(find "$EXAMPLES_DIR" -maxdepth 1 -name "${padded}*.rs" -printf '%f\n' | sed 's/\.rs$//')"
    if [[ -z "$match" ]]; then
        echo "axiom $1 has no sample in crates/gtk-lush/axioms/examples (see the ledger row)" >&2
        return 2
    fi
    printf '%s\n' "$match"
}

# Every sample, whatever the number of digits in its id: the pattern once read
# `a[0-9][0-9]_*`, which silently skipped every sample from A100 on (A100-A106
# ran in no lane until `adopt-gnome-51`); the self-test checks the count.
all_examples() {
    find "$EXAMPLES_DIR" -maxdepth 1 -regextype posix-extended -regex '.*/a[0-9]+_[^/]*\.rs' \
        -printf '%f\n' | sed 's/\.rs$//' | sort -V
}

# The example stems for a comma-separated id list (`runtimes --only`), in the
# given order and once each. An unknown id fails the run, as `sample` does; so
# does a list naming no id at all.
selected_examples() {
    local raw id example seen=" " count=0
    IFS=',' read -r -a raw <<<"$1"
    for id in "${raw[@]}"; do
        id="${id//[[:space:]]/}"
        [[ -n "$id" ]] || continue
        example="$(example_for "$id")" || return 2
        if [[ "$seen" != *" $example "* ]]; then
            seen+="$example "
            printf '%s\n' "$example"
            count=$((count + 1))
        fi
    done
    if [[ "$count" -eq 0 ]]; then
        echo "--only needs at least one axiom id, such as --only A28,A79" >&2
        return 2
    fi
}

# Build the named example, or every example with no argument, and record the
# target directory that example_binary reads.
build_examples() {
    local targets=(--examples)
    if [[ $# -gt 0 ]]; then
        targets=(--example "$1")
    fi
    cargo build -p gtk-lush-axioms "${targets[@]}" --quiet
    TARGET_DIR="$(target_dir)"
}

target_dir() {
    cargo metadata --format-version 1 --no-deps \
        | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
}

example_binary() {
    printf '%s/debug/examples/%s\n' "$TARGET_DIR" "$1"
}

# Run "$@" inside a private headless session. The temp scope provides a short
# runtime directory (Wayland socket paths are length-limited), removes it only
# once the session's services have exited, and fails the run if the session
# left any temporary entry behind.
run_headless() {
    require_command dbus-run-session
    require_command mutter
    "$REPO_ROOT/scripts/with-temp-scope.sh" --session-runtime axioms -- \
        dbus-run-session -- \
        mutter --headless --wayland --no-x11 --virtual-monitor "$MONITOR" -- "$@"
}

scan_warnings() {
    local log_file="$1"
    local found
    found="$(grep -E "$WARNING_REGEX" "$log_file" | grep -Ev "$BENIGN_NOISE_REGEX" || true)"
    if [[ -n "$found" ]]; then
        printf '%s\n' "$found" >&2
        echo "Error: unexpected warnings while running GTK axiom probes." >&2
        return 1
    fi
}

# The benign session lines must pass, and every defect class must fail,
# including the ones seen from the SDK sandbox's helpers.
self_test() {
    local log_file line
    log_file="$(mktemp)"
    local benign=(
        'libmutter-Message: 02:16:19.694: Running Mutter (using mutter 50.4) as a Wayland display server'
        '** Message: 02:16:19.858: Obtained a high priority EGL context'
        "dbus-daemon[3411502]: [session uid=1000 pid=3411502 pidfd=5] Activated service 'org.freedesktop.systemd1' failed: Process org.freedesktop.systemd1 exited with status 1"
        '(mutter:582): mutter-WARNING **: 09:42:29.034: Failed to acquire org.freedesktop.locale1 proxy: Could not connect: No such file or directory'
        '{"axiom":"A1","verdict":"holds","measured":{},"gtk":"4.22.2","adw":"1.9.0"}'
    )
    local defects=(
        '** (process:3411594): WARNING **: 02:16:21.611: Error writing credentials to socket: Error sending message: Broken pipe'
        "rm: cannot remove '/tmp/gtk-axioms-zcE6Sm/doc': Is a directory"
        'Gdk-Message: 02:16:21.611: Error flushing display: Broken pipe'
        '(a01_realized_rows_are_capped:12): Gtk-CRITICAL **: 02:16:21.611: gtk_widget_snapshot_child: assertion failed'
        '(process:12): GLib-GIO-WARNING **: 02:16:21.611: Error creating IO channel'
        '(process:12): Adwaita-WARNING **: 02:16:21.611: unknown style class'
        '(mutter:582): mutter-WARNING **: 09:42:29.034: an unlisted compositor warning'
        '*** BUG ***'
        'In pixman_region32_init_rect: Invalid rectangle passed *** BUG ***'
    )
    local failed=0
    printf '%s\n' "${benign[@]}" >"$log_file"
    if ! scan_warnings "$log_file"; then
        echo "Error: GTK axiom warning self-test rejected benign session output." >&2
        failed=1
    fi
    for line in "${defects[@]}"; do
        printf '%s\n' "$line" >"$log_file"
        if scan_warnings "$log_file" >/dev/null 2>&1; then
            echo "Error: GTK axiom warning self-test accepted: $line" >&2
            failed=1
        fi
    done
    # A slow sample skipped by design counts as a result and is not a
    # warning; anything else that only looks like one does not count.
    printf '%s\n' '{"axiom":"A1","verdict":"holds","measured":{},"gtk":"4.22.5","adw":"1.9.3"}' \
        'skipped (slow): A29' 'skipped (slow): A29 extra' 'not skipped (slow): A3' >"$log_file"
    if ! scan_warnings "$log_file" || [[ "$(count_sample_results "$log_file")" -ne 2 ]]; then
        echo "Error: GTK axiom self-test miscounted a slow sample skipped by design." >&2
        failed=1
    fi
    # A sample whose fixture needs RemoteDesktop, skipped inside an SDK
    # sandbox (which has no session bus), likewise counts as a result.
    printf '%s\n' '{"axiom":"A1","verdict":"holds"}' 'skipped (needs RemoteDesktop): A59' \
        'skipped (needs RemoteDesktop): A59 extra' 'skipped (needs a session bus): A60' >"$log_file"
    if ! scan_warnings "$log_file" || [[ "$(count_sample_results "$log_file")" -ne 2 ]]; then
        echo "Error: GTK axiom self-test miscounted a sample skipped for lack of RemoteDesktop." >&2
        failed=1
    fi
    # `runtimes --only` selects exactly the named samples, in the given order
    # and once each, refuses an unknown or empty id, and the run then expects
    # one result per selected sample, not one per sample on disk.
    local selected
    if ! selected="$(selected_examples 'A28, a79,29,A28')" \
        || [[ "$(printf '%s\n' "$selected" | sed 's/_.*//' | paste -sd,)" != 'a28,a79,a29' ]]; then
        echo "Error: GTK axiom self-test misparsed the --only list 'A28, a79,29,A28': $selected" >&2
        failed=1
    fi
    if selected_examples 'A28,A9999' >/dev/null 2>&1; then
        echo "Error: GTK axiom self-test accepted the unknown --only id A9999." >&2
        failed=1
    fi
    if selected_examples ',' >/dev/null 2>&1 || selected_examples '' >/dev/null 2>&1; then
        echo "Error: GTK axiom self-test accepted an empty --only list." >&2
        failed=1
    fi
    printf '%s\n' '{"axiom":"A28","verdict":"holds"}' '{"axiom":"A79","verdict":"holds"}' >"$log_file"
    if ! expect_sample_results "$log_file" 2 2>/dev/null || expect_sample_results "$log_file" 3 2>/dev/null; then
        echo "Error: GTK axiom self-test miscounted the results of a selected subset." >&2
        failed=1
    fi
    rm -f "$log_file"
    if [[ "$failed" -ne 0 ]]; then
        return 1
    fi
    local listed on_disk
    listed="$(all_examples | wc -l)"
    on_disk="$(find "$EXAMPLES_DIR" -maxdepth 1 -name 'a*.rs' | wc -l)"
    if [[ "$listed" -ne "$on_disk" ]] || ! all_examples | grep -qx 'a1[0-9][0-9]_.*'; then
        echo "Error: the sample list has $listed of the $on_disk samples on disk (three-digit ids included?)." >&2
        exit 1
    fi
    echo "GTK axiom warning classification, slow-skip and sample-list self-test passed."
}

# Run "$@" with its output teed to the log file $1, failing on a nonzero exit
# or on a toolkit warning in the log.
run_logged() {
    local log_file="$1"
    shift
    local status
    set +e
    "$@" 2>&1 | tee "$log_file"
    status="${PIPESTATUS[0]}"
    set -e
    if [[ "$status" -ne 0 ]]; then
        return "$status"
    fi
    scan_warnings "$log_file"
}

# Run every sample binary with --check, one after another, inside whatever
# runs this script: the host session or an SDK sandbox. It expands inside
# that shell, so it is single-quoted here.
# shellcheck disable=SC2016
SAMPLE_CHECK_LOOP='
    failed=0
    for binary in "$@"; do
        code=0
        "$binary" --check || code=$?
        if [ "$code" -ne 0 ]; then
            echo "FAILED: $(basename "$binary") --check exited $code" >&2
            failed=1
        fi
    done
    exit "$failed"
'

# How many sample results a log holds: one observation line per sample that
# ran, plus one `skipped (slow): <id>` line per slow sample skipped by design
# and, in an SDK sandbox only, one `skipped (needs RemoteDesktop): <id>` line
# per sample whose fixture drives the compositor's RemoteDesktop.
SAMPLE_RESULT_REGEX='^(\{"axiom"|skipped \((slow|needs RemoteDesktop)\): A[0-9]+$)'
count_sample_results() {
    grep -cE "$SAMPLE_RESULT_REGEX" "$1" || true
}

# Run the sample checks through "$@" (a command that ends where the loop's
# arguments begin), logged to $1, and require one result per binary in
# SAMPLE_BINARIES.
check_samples() {
    local log_file="$1"
    shift
    if ! run_logged "$log_file" run_headless "$@" -c "$SAMPLE_CHECK_LOOP" gtk-axiom-samples \
        "${SAMPLE_BINARIES[@]}"; then
        echo "Error: at least one sample --check did not hold." >&2
        return 1
    fi
    expect_sample_results "$log_file" "${#SAMPLE_BINARIES[@]}"
}

# Fail unless the log $1 holds exactly $2 sample results.
expect_sample_results() {
    local checked
    checked="$(count_sample_results "$1")"
    if [[ "$checked" -ne "$2" ]]; then
        echo "Error: expected $2 sample results, saw $checked." >&2
        return 1
    fi
}

run_all() {
    LOG_DIR="$(mktemp -d)"
    trap 'rm -rf "$LOG_DIR"' EXIT
    export_headless_env

    # Build the samples first so a sample that does not compile fails before
    # the probe run rather than after it.
    build_examples
    SAMPLE_BINARIES=()
    local example
    while IFS= read -r example; do
        SAMPLE_BINARIES+=("$(example_binary "$example")")
    done < <(all_examples)

    echo "==> Probe binary (gtk-lush-adoption-lab axiom_probes)"
    if ! run_logged "$LOG_DIR/probes.log" "$REPO_ROOT/scripts/with-temp-scope.sh" axiom-probes -- \
        cargo test -p gtk-lush-adoption-lab --test axiom_probes; then
        echo "Error: the probe binary failed." >&2
        return 1
    fi

    echo "==> Sample --check runs"
    check_samples "$LOG_DIR/samples.log" bash
    local skipped
    skipped="$(grep -cE '^skipped \(slow\): ' "$LOG_DIR/samples.log" || true)"
    echo "GTK axioms: probe binary passed; ${#SAMPLE_BINARIES[@]} sample checks held ($skipped slow skipped by design)."
}

run_sample() {
    local id="$1"
    local mode="${2:-}"
    local example
    example="$(example_for "$id")"
    build_examples "$example"
    local binary
    binary="$(example_binary "$example")"
    case "$mode" in
        --check)
            export_headless_env
            run_headless "$binary" --check
            ;;
        --screenshot)
            local output="${3:?--screenshot needs an output path}"
            export_headless_env
            unset GTK_LUSH_AXIOMS_HEADLESS
            export GTK_LUSH_AXIOMS_SCREENSHOT="$output"
            mkdir -p "$(dirname "$output")"
            run_headless "$binary"
            ;;
        "")
            # Interactive: deliberately the current desktop session.
            exec "$binary"
            ;;
        *)
            usage >&2
            return 2
            ;;
    esac
}

run_runtimes() {
    require_command flatpak
    local only="" examples=()
    if [[ "${1:-}" == "--only" ]]; then
        only="${2:?--only needs a comma-separated list of axiom ids}"
        shift 2
    elif [[ "${1:-}" == --only=* ]]; then
        only="${1#--only=}"
        shift
    fi
    if [[ -n "$only" ]]; then
        local selection
        selection="$(selected_examples "$only")" || return 2
        mapfile -t examples <<<"$selection"
    else
        mapfile -t examples < <(all_examples)
    fi
    local refs=("$@")
    if [[ ${#refs[@]} -eq 0 ]]; then
        refs=(org.gnome.Sdk//50 org.gnome.Sdk//51 org.gnome.Sdk//master)
    fi
    LOG_DIR="$(mktemp -d)"
    trap 'rm -rf "$LOG_DIR"' EXIT
    # Only here: the sandbox has no session bus, so a sample whose fixture
    # drives RemoteDesktop reports a skip instead of fixture-invalid. The host
    # lane (`all`, `sample`) never sets it.
    local env_flags=("${HEADLESS_ENV[@]/#/--env=}" --env=GTK_LUSH_AXIOMS_SANDBOX=1)
    local failed=0 ran=0 ref
    for ref in "${refs[@]}"; do
        if ! flatpak info "$ref" >/dev/null 2>&1; then
            echo "SKIPPED: $ref is not installed (install it and org.freedesktop.Sdk.Extension.rust-stable to check it)."
            continue
        fi
        local slug="${ref##*//}"
        local target="target/gtk-axioms-sdk-${slug}"
        echo "==> $ref: building the samples inside the SDK ($target)"
        local build_targets="--examples"
        if [[ -n "$only" ]]; then
            build_targets="$(printf -- '--example %s ' "${examples[@]}")"
        fi
        if ! flatpak run --command=bash --filesystem=home --share=network "${SANDBOX_FLAGS[@]}" "$ref" -c \
            "source /usr/lib/sdk/rust-stable/enable.sh && cd '$REPO_ROOT' && CARGO_TARGET_DIR='$target' cargo build -p gtk-lush-axioms $build_targets --quiet"; then
            echo "FAILED: $ref could not build the samples." >&2
            failed=1
            continue
        fi
        SAMPLE_BINARIES=()
        local example
        for example in "${examples[@]}"; do
            SAMPLE_BINARIES+=("$REPO_ROOT/$target/debug/examples/$example")
        done
        echo "==> $ref: ${#SAMPLE_BINARIES[@]} sample --check run(s) against the SDK's toolkit"
        unset DISPLAY WAYLAND_DISPLAY
        # One sandbox per SDK runs every sample, each still its own process.
        if ! check_samples "$LOG_DIR/runtime-${slug}.log" \
            flatpak run --command=bash --filesystem=home --socket=wayland --nosocket=x11 \
            "${SANDBOX_FLAGS[@]}" "${env_flags[@]}" "$ref"; then
            echo "FAILED: sample checks on $ref." >&2
            failed=1
        fi
        runtime_summary "$ref" "$LOG_DIR/runtime-${slug}.log" "${examples[@]}"
        # The loop expands inside the SDK's shell, so it is single-quoted.
        # shellcheck disable=SC2016
        echo "  SDK modules: $(flatpak run --command=bash "${SANDBOX_FLAGS[@]}" "$ref" -c \
            'for module in gtk4 libadwaita-1 pango gtksourceview-5 glib-2.0; do printf "%s %s; " "$module" "$(pkg-config --modversion "$module")"; done' \
            2>/dev/null || echo unknown)"
        ran=$((ran + 1))
    done
    echo "GTK axiom runtimes: checked $ran SDK(s)."
    return "$failed"
}

# The per-SDK summary: one line per selected sample with its verdict or skip
# reason, then the toolkit versions the observations report. A sample that
# printed no result line is `no result` (it crashed or was refused).
runtime_summary() {
    local ref="$1" log_file="$2"
    shift 2
    echo "==> $ref: summary"
    python3 - "$log_file" "$@" <<'PY'
import json
import re
import sys

log_file, examples = sys.argv[1], sys.argv[2:]
results, versions = {}, set()
with open(log_file, encoding="utf-8", errors="replace") as handle:
    for line in handle:
        line = line.rstrip("\n")
        skip = re.match(r"^skipped \((slow|needs RemoteDesktop)\): (A[0-9]+)$", line)
        if skip:
            results[skip.group(2)] = f"skipped ({skip.group(1)})"
            continue
        if line.startswith('{"axiom"'):
            try:
                observation = json.loads(line)
            except json.JSONDecodeError:
                continue
            results[observation["axiom"]] = observation.get("verdict", "?")
            versions.add(
                ", ".join(
                    f"{key} {observation[key]}"
                    for key in ("gtk", "adw", "pango", "sourceview")
                    if key in observation
                )
            )
counts = {}
for example in examples:
    axiom = "A" + str(int(re.match(r"a([0-9]+)_", example).group(1)))
    verdict = results.get(axiom, "no result")
    counts[verdict] = counts.get(verdict, 0) + 1
    print(f"  {axiom:<5} {verdict}")
print("  toolkit: " + ("; ".join(sorted(versions)) or "no observation reported versions"))
print("  totals: " + ", ".join(f"{count} {verdict}" for verdict, count in sorted(counts.items())))
PY
}

main() {
    case "${1:-}" in
        all) run_all ;;
        runtimes)
            shift
            run_runtimes "$@"
            ;;
        sample)
            shift
            [[ $# -ge 1 ]] || { usage >&2; exit 2; }
            run_sample "$@"
            ;;
        --self-test) self_test ;;
        -h | --help) usage ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
}

main "$@"
