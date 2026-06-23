#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

ARTIFACT_DIR="${1:-/tmp/lushtext-builder-diagnostics-runtime-self-check}"
mkdir -p "$ARTIFACT_DIR"

PROBE_UI="$ARTIFACT_DIR/debug-probe.ui"
cat >"$PROBE_UI" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <object class="GtkBox" id="root"/>
</interface>
EOF

{
    echo "runtime_refs=${LUSHTEXT_BUILDER_DIAGNOSTICS_RUNTIME_REFS:-unknown}"
    echo "prefix=${LUSHTEXT_GTK_DEBUG_PREFIX:-}"
    echo "path=${PATH}"
    echo "pkg_config_path=${PKG_CONFIG_PATH:-}"
    echo "ld_library_path=${LD_LIBRARY_PATH:-}"
    if command -v pkg-config >/dev/null 2>&1; then
        for package in gtk4 libadwaita-1 gtksourceview-5; do
            if pkg-config --exists "$package"; then
                echo "${package}_version=$(pkg-config --modversion "$package")"
            else
                echo "${package}_version=missing"
            fi
        done
    fi
    if command -v blueprint-compiler >/dev/null 2>&1; then
        echo "blueprint_compiler=$(blueprint-compiler --version 2>/dev/null || true)"
    fi
} >"$ARTIFACT_DIR/runtime.txt"

if ! command -v gtk4-builder-tool >/dev/null 2>&1; then
    echo "gtk4-builder-tool is missing" >&2
    exit 1
fi

if ! GTK_DEBUG=help gtk4-builder-tool validate "$PROBE_UI" \
    >"$ARTIFACT_DIR/gtk-debug-help.stdout" \
    2>"$ARTIFACT_DIR/gtk-debug-help.stderr"; then
    echo "GTK_DEBUG=help probe failed" >&2
    exit 1
fi

if grep -q "GTK_DEBUG set but ignored" "$ARTIFACT_DIR/gtk-debug-help.stderr"; then
    echo "GTK debug channels are unavailable in this runtime" >&2
    exit 1
fi

if ! grep -Eq '(^|[[:space:]])builder(-objects)?([[:space:]]|$)' "$ARTIFACT_DIR/gtk-debug-help.stdout"; then
    echo "GTK_DEBUG=help did not list builder diagnostics" >&2
    exit 1
fi

echo "builder diagnostics runtime self-check passed" >"$ARTIFACT_DIR/result.txt"
