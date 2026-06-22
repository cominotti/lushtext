#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run grouped advisory Rust lint probes and verify policy classification."""

from __future__ import annotations

import argparse
import fnmatch
import json
import os
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_POLICY = REPO_ROOT / "scripts" / "lint-advisory-policy.toml"


@dataclass(frozen=True)
class Finding:
    code: str
    file: str
    line: int
    column: int
    message: str


@dataclass(frozen=True)
class Rule:
    code: str
    classification: str
    rationale: str
    path_globs: tuple[str, ...]
    max_count: int | None

    def matches(self, finding: Finding) -> bool:
        if self.code != finding.code:
            return False
        if not self.path_globs:
            return True
        return any(fnmatch.fnmatch(finding.file, glob) for glob in self.path_globs)


@dataclass(frozen=True)
class Probe:
    name: str
    command: tuple[str, ...]
    env: dict[str, str] | None = None


def normalize_path(path: str) -> str:
    if not path:
        return "<unknown>"
    path = path.replace("\\", "/")
    if "/target/" in path and "/out/" in path:
        return f"<generated>/{Path(path).name}"
    if path.startswith("/rustc/"):
        return f"<rustc>/{Path(path).name}"

    path_obj = Path(path)
    if path_obj.is_absolute():
        try:
            return path_obj.relative_to(REPO_ROOT).as_posix()
        except ValueError:
            return path_obj.as_posix()
    return path


def load_policy(policy_path: Path) -> list[Rule]:
    try:
        policy = tomllib.loads(policy_path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        print(f"Policy file not found: {policy_path}", file=sys.stderr)
        raise SystemExit(2) from None

    if policy.get("schema") != 1:
        print("Unsupported advisory lint policy schema", file=sys.stderr)
        raise SystemExit(2)

    valid_classifications = {
        "blocking_candidate",
        "must_stay_zero",
        "accepted_advisory",
        "generated_code_noise",
        "resolved_policy_exception",
    }
    rules = []
    for raw_rule in policy.get("rules", []):
        classification = raw_rule["classification"]
        if classification not in valid_classifications:
            print(f"Unknown classification for {raw_rule['code']}: {classification}", file=sys.stderr)
            raise SystemExit(2)
        rules.append(
            Rule(
                code=raw_rule["code"],
                classification=classification,
                rationale=raw_rule["rationale"],
                path_globs=tuple(raw_rule.get("path_globs", [])),
                max_count=raw_rule.get("max_count"),
            )
        )
    return rules


def probes() -> list[Probe]:
    cargo_base = (
        "cargo",
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--message-format=json",
        "--color=never",
        "--",
    )
    return [
        Probe(
            "clippy-broad",
            cargo_base
            + (
                "-W",
                "clippy::pedantic",
                "-W",
                "clippy::nursery",
                "-W",
                "clippy::cargo",
            ),
        ),
        Probe(
            "clippy-design",
            cargo_base
            + (
                "-W",
                "clippy::cognitive_complexity",
                "-W",
                "clippy::too_many_lines",
                "-W",
                "clippy::too_many_arguments",
                "-W",
                "clippy::type_complexity",
                "-W",
                "clippy::struct_excessive_bools",
                "-W",
                "clippy::fn_params_excessive_bools",
                "-W",
                "clippy::implicit_hasher",
                "-W",
                "clippy::multiple_crate_versions",
                "-W",
                "clippy::print_stdout",
                "-W",
                "clippy::print_stderr",
                "-W",
                "clippy::panic",
                "-W",
                "clippy::expect_used",
                "-W",
                "clippy::indexing_slicing",
            ),
        ),
        Probe(
            "clippy-numeric",
            cargo_base
            + (
                "-W",
                "clippy::default_numeric_fallback",
                "-W",
                "clippy::float_arithmetic",
                "-W",
                "clippy::integer_division",
                "-W",
                "clippy::large_digit_groups",
                "-W",
                "clippy::decimal_bitwise_operands",
                "-W",
                "clippy::lossy_float_literal",
                "-W",
                "clippy::unused_rounding",
                "-W",
                "clippy::mixed_case_hex_literals",
                "-W",
                "clippy::zero_prefixed_literal",
                "-W",
                "clippy::unusual_byte_groupings",
                "-W",
                "clippy::inconsistent_digit_grouping",
            ),
        ),
        Probe(
            "rustc-advisory",
            (
                "cargo",
                "check",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--message-format=json",
                "--color=never",
            ),
            {
                "RUSTFLAGS": (
                    "-W future-incompatible "
                    "-W rust-2024-compatibility "
                    "-W unused-qualifications "
                    "-W unreachable-pub "
                    "-W unused-crate-dependencies "
                    "-W missing-debug-implementations "
                    "-W missing-docs "
                    "-W unsafe-code"
                )
            },
        ),
    ]


def parse_json_messages(output: str, probe: Probe) -> set[Finding]:
    findings: set[Finding] = set()
    for line_number, line in enumerate(output.splitlines(), start=1):
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError as error:
            print(
                f"{probe.name}: unparseable JSON on stdout line {line_number}: {error}",
                file=sys.stderr,
            )
            raise SystemExit(2) from None
        if event.get("reason") != "compiler-message":
            continue

        message = event.get("message", {})
        code = (message.get("code") or {}).get("code")
        if not code:
            continue

        spans = message.get("spans") or []
        primary = next((span for span in spans if span.get("is_primary")), None)
        span = primary or (spans[0] if spans else {})
        findings.add(
            Finding(
                code=code,
                file=normalize_path(span.get("file_name", "")),
                line=int(span.get("line_start") or 0),
                column=int(span.get("column_start") or 0),
                message=" ".join(str(message.get("message", "")).split()),
            )
        )
    return findings


def run_probe(probe: Probe) -> set[Finding]:
    env = os.environ.copy()
    if probe.env:
        env.update(probe.env)

    completed = subprocess.run(
        probe.command,
        cwd=REPO_ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    findings = parse_json_messages(completed.stdout, probe)
    if completed.returncode != 0:
        print(f"{probe.name} failed with exit code {completed.returncode}", file=sys.stderr)
        if completed.stderr:
            print(completed.stderr, file=sys.stderr)
        raise SystemExit(completed.returncode)
    return findings


def classify_finding(finding: Finding, rules: list[Rule]) -> Rule | None:
    matches = [rule for rule in rules if rule.matches(finding)]
    if not matches:
        return None
    if len(matches) == 1:
        return matches[0]

    distinct = {(rule.classification, rule.max_count, rule.rationale) for rule in matches}
    if len(distinct) != 1:
        print(
            f"Conflicting advisory rules for {finding.code} at {finding.file}:{finding.line}",
            file=sys.stderr,
        )
        for rule in matches:
            print(f"  {rule.classification}: {rule.rationale}", file=sys.stderr)
        raise SystemExit(2)
    return matches[0]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    args = parser.parse_args()

    rules = load_policy(args.policy)
    all_findings: set[Finding] = set()
    for probe in probes():
        print(f"running {probe.name}...", file=sys.stderr)
        all_findings.update(run_probe(probe))

    classified: dict[Finding, Rule] = {}
    unclassified = []
    for finding in sorted(all_findings, key=lambda item: (item.code, item.file, item.line, item.message)):
        rule = classify_finding(finding, rules)
        if rule is None:
            unclassified.append(finding)
        else:
            classified[finding] = rule

    if unclassified:
        print("Unclassified advisory lint categories or paths:", file=sys.stderr)
        for finding in unclassified:
            print(
                f"  {finding.code}\t{finding.file}:{finding.line}\t{finding.message}",
                file=sys.stderr,
            )
        return 1

    counts: dict[str, int] = {}
    first: dict[str, Finding] = {}
    rule_counts: dict[Rule, int] = {rule: 0 for rule in rules}
    classifications: dict[str, str] = {}
    for finding, rule in classified.items():
        counts[finding.code] = counts.get(finding.code, 0) + 1
        classifications[finding.code] = rule.classification
        rule_counts[rule] += 1
        current_first = first.get(finding.code)
        if current_first is None or (finding.file, finding.line, finding.message) < (
            current_first.file,
            current_first.line,
            current_first.message,
        ):
            first[finding.code] = finding

    print("count\tclassification\tcode\tfirst_file\tfirst_line\tfirst_message")
    for code in sorted(counts, key=lambda item: (-counts[item], item)):
        finding = first[code]
        print(
            f"{counts[code]}\t{classifications[code]}\t{code}\t"
            f"{finding.file}\t{finding.line}\t{finding.message}"
        )

    failures = []
    for rule, count in rule_counts.items():
        max_count = 0 if rule.classification == "must_stay_zero" and rule.max_count is None else rule.max_count
        if max_count is not None and count > max_count:
            failures.append((rule, count, max_count))

    if failures:
        print("Advisory lint max-count policy failed:", file=sys.stderr)
        for rule, count, max_count in failures:
            print(
                f"  {rule.code}: {count} finding(s), max {max_count} "
                f"({rule.classification}; {rule.rationale})",
                file=sys.stderr,
            )
        return 1

    print("Advisory lint policy passed: all findings are classified.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
