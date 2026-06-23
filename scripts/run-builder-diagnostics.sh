#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_SMOKE_ARTIFACT_DIR:-build/smoke/builder-diagnostics}"
PROVIDER="${LUSHTEXT_BUILDER_DIAGNOSTICS_PROVIDER:-auto}"
IMAGE="${LUSHTEXT_BUILDER_DIAGNOSTICS_IMAGE:-ghcr.io/cominotti/lushtext-builder-diagnostics-runtime:gnome-50-debug}"
CONTAINER_RUNNER="${LUSHTEXT_BUILDER_DIAGNOSTICS_CONTAINER:-}"
REQUIRED_RUNTIME="${LUSHTEXT_BUILDER_DIAGNOSTICS_REQUIRED:-0}"
MANIFEST="$REPO_ROOT/scripts/builder-diagnostics-coverage.json"

usage() {
    cat <<'EOF'
Usage: scripts/run-builder-diagnostics.sh [options]

Run LushText GtkBuilder diagnostics with GTK_DEBUG=builder,builder-objects,
classify emitted diagnostics, and preserve artifacts.

Options:
  --artifact-dir DIR       Artifact directory (default: build/smoke/builder-diagnostics)
  --provider MODE         auto, host, or container
  --image IMAGE           Reusable debug GTK runtime image for container mode
  --container-runner CMD  podman or docker
  --manifest PATH         Coverage manifest
  --required-runtime      Fail instead of skipping when debug GTK is unavailable
  -h, --help              Show this help text

Environment:
  LUSHTEXT_BUILDER_DIAGNOSTICS_PROVIDER=auto|host|container
  LUSHTEXT_BUILDER_DIAGNOSTICS_IMAGE=ghcr.io/...:tag-or-digest
  LUSHTEXT_BUILDER_DIAGNOSTICS_CONTAINER=podman|docker
  LUSHTEXT_BUILDER_DIAGNOSTICS_REQUIRED=1
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact-dir)
            [[ $# -lt 2 ]] && smoke_fail "--artifact-dir requires a value"
            ARTIFACT_DIR="$2"
            shift 2
            ;;
        --provider)
            [[ $# -lt 2 ]] && smoke_fail "--provider requires a value"
            PROVIDER="$2"
            shift 2
            ;;
        --image)
            [[ $# -lt 2 ]] && smoke_fail "--image requires a value"
            IMAGE="$2"
            shift 2
            ;;
        --container-runner)
            [[ $# -lt 2 ]] && smoke_fail "--container-runner requires a value"
            CONTAINER_RUNNER="$2"
            shift 2
            ;;
        --manifest)
            [[ $# -lt 2 ]] && smoke_fail "--manifest requires a value"
            MANIFEST="$2"
            shift 2
            ;;
        --required-runtime)
            REQUIRED_RUNTIME=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            smoke_fail "unknown argument: $1"
            ;;
    esac
done

case "$PROVIDER" in
    auto|host|container) ;;
    *) smoke_fail "--provider must be auto, host, or container" ;;
esac

choose_container_runner() {
    if [[ -n "$CONTAINER_RUNNER" ]]; then
        command -v "$CONTAINER_RUNNER" >/dev/null 2>&1 || return 1
        printf '%s\n' "$CONTAINER_RUNNER"
        return 0
    fi
    if command -v podman >/dev/null 2>&1; then
        printf '%s\n' podman
        return 0
    fi
    if command -v docker >/dev/null 2>&1; then
        printf '%s\n' docker
        return 0
    fi
    return 1
}

host_debug_supported() {
    command -v gtk4-builder-tool >/dev/null 2>&1 || return 1
    local tmp
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN
    cat >"$tmp/probe.ui" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <object class="GtkBox" id="root"/>
</interface>
EOF
    GTK_DEBUG=help gtk4-builder-tool validate "$tmp/probe.ui" >"$tmp/out" 2>"$tmp/err" || true
    if grep -q "GTK_DEBUG set but ignored" "$tmp/err"; then
        return 1
    fi
    grep -Eq '(^|[[:space:]])builder(-objects)?([[:space:]]|$)' "$tmp/out" "$tmp/err"
}

container_path_for_repo_path() {
    local path="$1"
    if [[ "$path" == "$REPO_ROOT"/* ]]; then
        printf '/workspace/%s\n' "${path#"$REPO_ROOT"/}"
    elif [[ "$path" = /* ]]; then
        smoke_fail "container mode only supports paths inside the repository: $path"
    else
        printf '%s\n' "$path"
    fi
}

run_inside_container() {
    local runner="$1"
    local artifact_dir="$2"
    local container_artifact_dir
    local container_manifest
    container_artifact_dir="$(container_path_for_repo_path "$artifact_dir")"
    container_manifest="$(container_path_for_repo_path "$MANIFEST")"
    local uid_gid
    uid_gid="$(id -u):$(id -g)"
    mkdir -p "$artifact_dir"
    local script_args=(
        /workspace/scripts/run-builder-diagnostics.sh
        --provider host
        --artifact-dir "$container_artifact_dir"
        --manifest "$container_manifest"
    )
    if [[ "$REQUIRED_RUNTIME" == "1" ]]; then
        script_args+=(--required-runtime)
    fi
    echo "Running builder diagnostics through ${runner} image ${IMAGE}"
    exec "$runner" run --rm \
        --user "$uid_gid" \
        --workdir /workspace \
        --volume "$REPO_ROOT:/workspace:Z" \
        --env LUSHTEXT_BUILDER_DIAGNOSTICS_PROVIDER=host \
        --env LUSHTEXT_BUILDER_DIAGNOSTICS_IN_CONTAINER=1 \
        --env LUSHTEXT_BUILDER_DIAGNOSTICS_REQUIRED="$REQUIRED_RUNTIME" \
        "$IMAGE" \
        "${script_args[@]}"
}

if [[ "$PROVIDER" == "container" ]]; then
    runner="$(choose_container_runner)" || smoke_skip "No container runner available for builder diagnostics runtime."
    run_inside_container "$runner" "$ARTIFACT_DIR"
fi

if [[ "$PROVIDER" == "auto" && -z "${LUSHTEXT_BUILDER_DIAGNOSTICS_IN_CONTAINER:-}" ]]; then
    if host_debug_supported; then
        PROVIDER=host
    elif runner="$(choose_container_runner)"; then
        run_inside_container "$runner" "$ARTIFACT_DIR"
    fi
fi

smoke_require_command python3
[[ -f "$MANIFEST" ]] || smoke_fail "coverage manifest is missing: $MANIFEST"

ARTIFACT_DIR="$(smoke_artifact_dir "$ARTIFACT_DIR")"
rm -rf "$ARTIFACT_DIR/capability" "$ARTIFACT_DIR/standalone" "$ARTIFACT_DIR/runtime"
smoke_write_environment_report "$ARTIFACT_DIR/environment.txt"

args=(
    "$REPO_ROOT/scripts/builder-diagnostics.py"
    --artifact-dir "$ARTIFACT_DIR"
    --manifest "$MANIFEST"
    --provider "$PROVIDER"
    --image "$IMAGE"
)
if [[ "$REQUIRED_RUNTIME" == "1" ]]; then
    args+=(--required-runtime)
fi

python3 "${args[@]}"
