#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Shared helpers for host-dependent smoke lanes. These helpers keep skip
# behavior explicit so unsupported desktop, portal, or packaging environments do
# not look like verified coverage.

set -euo pipefail

smoke_repo_root() {
    cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd
}

smoke_skip() {
    echo "SKIP: $*"
    exit 0
}

smoke_fail() {
    echo "FAIL: $*" >&2
    exit 1
}

# Assert a test or bench log actually ran something, not just that it exited 0.
#
# `cargo test`, `cargo bench`, and the widget harness all exit 0 when their
# filter matches nothing, so every literal test name, module path, and bench
# group name in a smoke lane is a string-keyed gate that **fails open**: a rename
# turns an advertised proof into a no-op that still reports success. This is not
# hypothetical — `run-performance-smoke.sh` kept reporting a green
# "exact mixed search-event turn-budget proof" for three days after its module
# was renamed from `runtime` to `execution`. Check the log's content.
#
# `pattern` defaults to libtest's own summary with a non-zero count. Pass an
# explicit pattern for runners that do not print it, such as Criterion.
smoke_assert_ran() {
    local log_path="$1"
    local label="$2"
    local pattern="${3:-test result: ok\. [1-9]}"

    if ! grep -qE "${pattern}" "${log_path}"; then
        tail -n 120 "${log_path}" >&2 || true
        smoke_fail "${label} matched nothing; its filter is stale."
    fi
}

smoke_require_command() {
    local command_name="$1"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        smoke_skip "'${command_name}' is not installed."
    fi
}

smoke_artifact_dir() {
    local requested="$1"
    mkdir -p "$requested"
    cd "$requested" && pwd
}

smoke_timestamp_utc() {
    date -u +"%Y-%m-%dT%H:%M:%SZ"
}

smoke_prepare_isolated_state() {
    local parent_dir="$1"
    local state_dir
    state_dir="$(mktemp -d "${parent_dir%/}/state.XXXXXX")"

    mkdir -p "$state_dir/cache" "$state_dir/config" "$state_dir/data" "$state_dir/runtime"
    chmod 700 "$state_dir/runtime"

    export XDG_CACHE_HOME="$state_dir/cache"
    export XDG_CONFIG_HOME="$state_dir/config"
    export XDG_DATA_HOME="$state_dir/data"
    export XDG_RUNTIME_DIR="$state_dir/runtime"
    export GSETTINGS_BACKEND="${GSETTINGS_BACKEND:-keyfile}"
    export GSETTINGS_SCHEMA_DIR="${GSETTINGS_SCHEMA_DIR:-$(smoke_repo_root)/data}"

    printf '%s\n' "$state_dir"
}

smoke_write_environment_report() {
    local report_path="$1"

    {
        echo "timestamp=$(smoke_timestamp_utc)"
        echo "repo=$(smoke_repo_root)"
        echo "kernel=$(uname -srmo 2>/dev/null || true)"
        echo "gsk_renderer=${GSK_RENDERER:-}"
        echo "gdk_backend=${GDK_BACKEND:-}"
        echo "wayland_display=${WAYLAND_DISPLAY:-}"
        echo "display=${DISPLAY:-}"
        echo "xdg_session_type=${XDG_SESSION_TYPE:-}"
        echo "gsettings_backend=${GSETTINGS_BACKEND:-}"
        if command -v pkg-config >/dev/null 2>&1; then
            for package in gtk4 libadwaita-1 gtksourceview-5; do
                if pkg-config --exists "$package"; then
                    echo "${package}_version=$(pkg-config --modversion "$package")"
                fi
            done
        fi
        if command -v flatpak >/dev/null 2>&1; then
            echo "flatpak=$(flatpak --version 2>/dev/null || true)"
        fi
        if command -v snap >/dev/null 2>&1; then
            echo "snap=$(snap version 2>/dev/null | tr '\n' ';' || true)"
            echo
        fi
    } >"$report_path"
}

smoke_create_text_fixture() {
    local output_path="$1"

    mkdir -p "$(dirname "$output_path")"
    cat >"$output_path" <<'EOF'
LushText smoke fixture

This file is intentionally tiny. It gives visual, accessibility, portal, and
performance smoke lanes a stable document to open without touching user data.

needle one
needle two
needle three
EOF
}
