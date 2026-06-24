#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Generate and validate Blueprint-authored GTK templates.

set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
UI_DIR="$ROOT_DIR/resources/ui"
CONTRACT_FILE="$UI_DIR/template-contract.json"
BLUEPRINT_COMPILER_BIN=${BLUEPRINT_COMPILER:-blueprint-compiler}
BLUEPRINT_TYPELIB_PATH=${BLUEPRINT_TYPELIB_PATH:-/usr/lib64/girepository-1.0}
BLUEPRINT_GIR_PATH=${BLUEPRINT_GIR_PATH:-/usr/share/gir-1.0}

usage() {
    cat <<'EOF'
Usage: scripts/blueprint-templates.sh <command>

Commands:
  generate  Compile resources/ui/*.blp into matching resources/ui/*.ui files
  drift     Fail when committed .ui files differ from compiled .blp output
  audit     Check generated .ui files against the template contract and Rust bindings
  lint      Run advisory grouped Blueprint lint triage on resources/ui/*.blp
  check     Run drift and audit

Environment:
  BLUEPRINT_COMPILER       Override the blueprint-compiler executable
  BLUEPRINT_TYPELIB_PATH   Override GI typelib path (default: /usr/lib64/girepository-1.0)
  BLUEPRINT_GIR_PATH       Override GIR path (default: /usr/share/gir-1.0)
EOF
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

require_blueprint_compiler() {
    if ! command -v "$BLUEPRINT_COMPILER_BIN" >/dev/null 2>&1; then
        cat >&2 <<EOF
error: blueprint-compiler is required for UI template generation and drift checks.

Install it in Fedora/Toolbx and CI with:
  sudo dnf install blueprint-compiler

Or set BLUEPRINT_COMPILER to an explicit executable path.
Expected tool source: Fedora blueprint-compiler package, currently 0.20.x.
Ordinary Cargo, Meson, Flatpak, and Snap runtime builds still consume committed .ui files.
EOF
        exit 127
    fi
}

ensure_blueprint_sources() {
    set -- "$UI_DIR"/*.blp
    [ -e "$1" ] || die "no Blueprint sources found under resources/ui/*.blp"
}

blueprint_version() {
    "$BLUEPRINT_COMPILER_BIN" --version 2>/dev/null || printf 'unknown\n'
}

print_blueprint_context() {
    label=$1
    printf 'Blueprint compiler: %s\n' "$(blueprint_version)"
    printf 'Blueprint %s templates:\n' "$label"
    for blp_file in "$UI_DIR"/*.blp; do
        printf '  %s\n' "${blp_file#$ROOT_DIR/}"
    done
}

compile_one_raw() {
    blp_file=$1
    ui_file=$2

    if [ -d "$BLUEPRINT_TYPELIB_PATH" ] && [ -d "$BLUEPRINT_GIR_PATH" ]; then
        "$BLUEPRINT_COMPILER_BIN" compile \
            --typelib-path "$BLUEPRINT_TYPELIB_PATH" \
            --gir-path "$BLUEPRINT_GIR_PATH" \
            --output "$ui_file" \
            "$blp_file"
    else
        "$BLUEPRINT_COMPILER_BIN" compile --output "$ui_file" "$blp_file"
    fi
}

classify_compile_output() {
    blp_file=$1
    output_file=$2

    python3 - "$ROOT_DIR" "$blp_file" "$output_file" <<'PY'
from __future__ import annotations

import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
blp_file = Path(sys.argv[2])
output_file = Path(sys.argv[3])

text = output_file.read_text(encoding="utf-8", errors="replace")
if not text.strip():
    raise SystemExit(0)

clean = re.sub(r"\x1b\[[0-9;]*m", "", text)
clean = re.sub(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)", "", clean)
warning_positions = [match.start() for match in re.finditer(r"^warning: ", clean, re.MULTILINE)]
if not warning_positions:
    print(clean, file=sys.stderr, end="" if clean.endswith("\n") else "\n")
    raise SystemExit(1)

warning_positions.append(len(clean))
rel_path = blp_file.resolve().relative_to(root).as_posix()
print(f"error: unclassified blueprint-compiler warnings in {rel_path}", file=sys.stderr)
print("Known warning policy currently accepts no Blueprint compiler warnings.", file=sys.stderr)
for start, end in zip(warning_positions, warning_positions[1:]):
    block = clean[start:end].strip()
    print("", file=sys.stderr)
    print(block, file=sys.stderr)
raise SystemExit(1)
PY
}

compile_one() {
    blp_file=$1
    ui_file=$2
    output_file=$(mktemp)

    if compile_one_raw "$blp_file" "$ui_file" >"$output_file" 2>&1; then
        status=0
    else
        status=$?
    fi

    if [ "$status" -ne 0 ]; then
        cat "$output_file" >&2
        rm -f "$output_file"
        return "$status"
    fi

    if classify_compile_output "$blp_file" "$output_file"; then
        classify_status=0
    else
        classify_status=$?
    fi
    rm -f "$output_file"
    return "$classify_status"
}

generate_templates() {
    require_blueprint_compiler
    ensure_blueprint_sources
    print_blueprint_context "generation"

    for blp_file in "$UI_DIR"/*.blp; do
        ui_file=${blp_file%.blp}.ui
        printf 'Generating %s\n' "${ui_file#$ROOT_DIR/}"
        compile_one "$blp_file" "$ui_file"
    done
}

check_drift() {
    require_blueprint_compiler
    ensure_blueprint_sources
    print_blueprint_context "drift-check"

    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT INT TERM

    failed=0
    for blp_file in "$UI_DIR"/*.blp; do
        base=$(basename "$blp_file" .blp)
        committed_ui="$UI_DIR/$base.ui"
        generated_ui="$tmp_dir/$base.ui"
        compile_one "$blp_file" "$generated_ui"

        if ! cmp -s "$committed_ui" "$generated_ui"; then
            printf 'Blueprint drift detected: %s\n' "${committed_ui#$ROOT_DIR/}" >&2
            diff -u "$committed_ui" "$generated_ui" >&2 || true
            failed=1
        fi
    done

    [ "$failed" -eq 0 ] || die "generated .ui output is stale; run scripts/blueprint-templates.sh generate"
}

audit_contract() {
    printf 'Blueprint contract baseline: %s\n' "${CONTRACT_FILE#$ROOT_DIR/}"
    python3 "$ROOT_DIR/scripts/check-ui-template-contract.py" --baseline "$CONTRACT_FILE"
}

lint_blueprints() {
    require_blueprint_compiler
    ensure_blueprint_sources
    print_blueprint_context "lint"

    output_file=$(mktemp)
    if "$BLUEPRINT_COMPILER_BIN" lint "$UI_DIR"/*.blp >"$output_file" 2>&1; then
        lint_status=0
    else
        lint_status=$?
    fi

    if python3 - "$ROOT_DIR" "$output_file" "$lint_status" <<'PY'
from __future__ import annotations

import collections
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])
output_file = Path(sys.argv[2])
lint_status = int(sys.argv[3])

raw = output_file.read_text(encoding="utf-8", errors="replace")
clean = re.sub(r"\x1b\[[0-9;]*m", "", raw)
clean = re.sub(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)", "", clean)

diagnostics: list[tuple[str, str, str, str]] = []
current: tuple[str, str, str] | None = None
for line in clean.splitlines():
    severity_match = re.match(r"^(warning|error): .* \[([a-z0-9_]+)\]$", line)
    if severity_match:
        current = (severity_match.group(1), severity_match.group(2), "")
        continue
    location_match = re.match(r"^at (.+?) line \d+ column \d+:", line)
    if current and location_match:
        severity, rule, _ = current
        location = location_match.group(1)
        location_path = Path(location)
        if not location_path.is_absolute():
            location_path = root / location_path
        try:
            rel = location_path.resolve().relative_to(root).as_posix()
        except ValueError:
            rel = location
        diagnostics.append((rule, severity, rel, location))
        current = None

promoted_zero_rules = {
    "missing_descriptive_text": "promoted: images need descriptive text or accessible-role=presentation",
    "use_unicode": "promoted: visible text should use Unicode punctuation such as ellipsis",
}

accepted_policy = {
    "adjustment_prop_order": {
        "resources/ui/preferences.blp": (
            4,
            "classified: lower/upper/value is normalized; compiler 0.20.4 still warns when increment properties are present",
        ),
    },
    "avoid_all_caps": {
        "resources/ui/status-bar.blp": (
            2,
            "classified: compact technical status labels such as LF and UTF-8 are intentionally uppercase",
        ),
    },
    "scrollable_parent": {
        "resources/ui/editor-page.blp": (
            2,
            "classified: composite-template children are owned by editor geometry, not standalone scroll containers",
        ),
        "resources/ui/window.blp": (
            8,
            "classified: shell children own layout, overlay, and secondary-surface geometry",
        ),
    },
    "translate_display_string": {
        "resources/ui/info-bar.blp": (
            2,
            "classified: empty labels are runtime-populated inline alert text",
        ),
        "resources/ui/search-panel.blp": (
            4,
            "classified: remaining labels are symbolic search toggles or the .gitignore filename token",
        ),
        "resources/ui/status-bar.blp": (
            3,
            "classified: remaining labels are EditorConfig, LF, and UTF-8 technical tokens",
        ),
        "resources/ui/window.blp": (
            2,
            "classified: remaining strings are LushText brand titles",
        ),
    },
    "use_adw_bin": {
        "resources/ui/info-bar.blp": (
            1,
            "classified: alert_box carries alert role, CSS, and wrapping layout semantics",
        ),
        "resources/ui/search-panel.blp": (
            1,
            "classified: footer_box remains a GtkBox template child until structural proof is added",
        ),
        "resources/ui/status-bar.blp": (
            1,
            "classified: message_area_box owns full-lane status pulse CSS and tests",
        ),
        "resources/ui/window.blp": (
            1,
            "classified: editor_box participates in Adwaita preview-mode layout visibility",
        ),
    },
}

if lint_status == 0 and not diagnostics:
    print("Blueprint lint advisory summary: no diagnostics reported.")
    raise SystemExit(0)

if not diagnostics:
    print(clean, file=sys.stderr, end="" if clean.endswith("\n") else "\n")
    raise SystemExit(lint_status)

by_rule: dict[str, collections.Counter[str]] = collections.defaultdict(collections.Counter)
severities: dict[str, set[str]] = collections.defaultdict(set)
for rule, severity, rel, _location in diagnostics:
    by_rule[rule][rel] += 1
    severities[rule].add(severity)

print("Blueprint lint policy summary:")
print("rule\tseverity\tcount\tfiles\tpolicy")
unknown_findings: list[str] = []
promoted_regressions: list[str] = []
excess_findings: list[str] = []
error_rules: list[str] = []
for rule in sorted(by_rule):
    files = ", ".join(f"{path} x{count}" for path, count in sorted(by_rule[rule].items()))
    severity_text = ",".join(sorted(severities[rule]))
    count = sum(by_rule[rule].values())
    if "error" in severities[rule]:
        error_rules.append(rule)

    rule_policy_parts: list[str] = []
    if rule in promoted_zero_rules:
        rule_policy_parts.append(promoted_zero_rules[rule])
        promoted_regressions.append(rule)

    accepted_files = accepted_policy.get(rule, {})
    for path, path_count in sorted(by_rule[rule].items()):
        accepted = accepted_files.get(path)
        if accepted is None:
            if rule not in promoted_zero_rules:
                unknown_findings.append(f"{rule} in {path}")
            continue
        max_count, rationale = accepted
        if path_count > max_count:
            excess_findings.append(f"{rule} in {path}: {path_count} > {max_count}")
        rule_policy_parts.append(f"{path}: <= {max_count}; {rationale}")

    rule_policy = "; ".join(rule_policy_parts) if rule_policy_parts else "unclassified"
    print(f"{rule}\t{severity_text}\t{count}\t{files}\t{rule_policy}")

if promoted_regressions:
    print(
        "error: promoted Blueprint lint rules regressed: "
        + ", ".join(sorted(set(promoted_regressions))),
        file=sys.stderr,
    )
    raise SystemExit(1)
if unknown_findings:
    print(f"error: unclassified Blueprint lint findings: {', '.join(sorted(unknown_findings))}", file=sys.stderr)
    raise SystemExit(1)
if excess_findings:
    print(f"error: Blueprint lint findings exceeded accepted policy: {', '.join(sorted(excess_findings))}", file=sys.stderr)
    raise SystemExit(1)
if error_rules:
    print(f"error: Blueprint lint reported errors for classified advisory rules: {', '.join(sorted(error_rules))}", file=sys.stderr)
    raise SystemExit(1)

promoted_names = ", ".join(sorted(promoted_zero_rules))
print(f"Blueprint lint promoted rules are clean: {promoted_names}.")
print("Blueprint lint remains advisory for documented exceptions: all current warnings are classified.")
PY
    then
        result=0
    else
        result=$?
    fi
    rm -f "$output_file"
    return "$result"
}

command=${1:-}
case "$command" in
    generate)
        generate_templates
        ;;
    drift)
        check_drift
        ;;
    audit)
        audit_contract
        ;;
    lint)
        lint_blueprints
        ;;
    check)
        check_drift
        audit_contract
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
