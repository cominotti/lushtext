#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Keep the GTK axiom ledger and the gtk-lush-axioms crate in agreement.
#
# The ledger markdown is normative; the crate makes each belief observable.
# This check fails when they drift: a ledger id with no catalogue entry (or
# the reverse), a probed entry without its sample or whose probe is named for
# another id, a sample naming an unknown axiom, a ledger row that claims a
# probe the catalogue does not have (or omits one it has), a row missing its
# "Verified against" or "Sample" cell, or a "Pinned by" cell that still names
# the retired LushText-hosted probes (`gtk_axioms::`).

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
LEDGER = REPO_ROOT / ".agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md"
CRATE = REPO_ROOT / "crates/gtk-lush/axioms"
CATALOGUE = CRATE / "src/catalogue.rs"
EXAMPLES = CRATE / "examples"

REQUIRED_COLUMNS = ("Id", "Axiom", "Dependent designs", "Pinned by", "Verified against", "Sample")
RETIRED_PROBE_PREFIX = "gtk_axioms::"
PROBE_MARKER = "**probe**"
NO_SAMPLE_MARKER = "—"


@dataclass(frozen=True)
class LedgerRow:
    number: int
    pinned_by: str
    verified_against: str
    sample: str


@dataclass(frozen=True)
class CatalogueEntry:
    number: int
    name: str
    probe: str | None


def parse_ledger(text: str, errors: list[str]) -> dict[int, LedgerRow]:
    rows: dict[int, LedgerRow] = {}
    header: list[str] | None = None
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped.startswith("|"):
            continue
        cells = [cell.strip() for cell in stripped.strip("|").split("|")]
        if header is None:
            if cells and cells[0] == "Id":
                header = cells
                missing = [column for column in REQUIRED_COLUMNS if column not in header]
                if missing:
                    errors.append(f"ledger table is missing column(s): {', '.join(missing)}")
                    return rows
            continue
        if set("".join(cells)) <= set("-: "):
            continue
        match = re.fullmatch(r"A(\d+)", cells[0])
        if not match:
            continue
        number = int(match.group(1))
        if len(cells) != len(header):
            errors.append(f"A{number}: ledger row has {len(cells)} cells, header has {len(header)}")
            continue
        by_column = dict(zip(header, cells))
        if number in rows:
            errors.append(f"A{number}: ledger lists the id twice")
        rows[number] = LedgerRow(
            number=number,
            pinned_by=by_column["Pinned by"],
            verified_against=by_column["Verified against"],
            sample=by_column["Sample"],
        )
    if header is None:
        errors.append("ledger has no axiom table (a header row starting with `| Id |`)")
    return rows


def parse_catalogue(text: str, errors: list[str]) -> dict[int, CatalogueEntry]:
    entries: dict[int, CatalogueEntry] = {}
    for block in text.split("Axiom {")[1:]:
        id_match = re.search(r"id:\s*AxiomId::new\((\d+)\)", block)
        name_match = re.search(r'name:\s*"([a-z0-9_]+)"', block)
        probe_match = re.search(r"probe:\s*(None|Some\(\s*probes::(probe_a\d+)\s*\))", block)
        if not (id_match and name_match and probe_match):
            continue
        number = int(id_match.group(1))
        if number in entries:
            errors.append(f"A{number}: the catalogue lists the id twice")
        entries[number] = CatalogueEntry(
            number=number,
            name=name_match.group(1),
            probe=probe_match.group(2),
        )
    if not entries:
        errors.append("no catalogue entries found in crates/gtk-lush/axioms/src/catalogue.rs")
    return entries


def check(ledger_text: str, catalogue_text: str, example_stems: set[str]) -> list[str]:
    errors: list[str] = []
    rows = parse_ledger(ledger_text, errors)
    entries = parse_catalogue(catalogue_text, errors)
    if not rows or not entries:
        return errors

    for number in sorted(set(rows) - set(entries)):
        errors.append(f"A{number}: ledger id has no gtk-lush-axioms catalogue entry")
    for number in sorted(set(entries) - set(rows)):
        errors.append(f"A{number}: catalogue entry has no ledger row")

    known_names = {entry.name for entry in entries.values()}
    for stem in sorted(example_stems):
        if re.fullmatch(r"a\d{2}_[a-z0-9_]+", stem) and stem not in known_names:
            errors.append(f"examples/{stem}.rs names no catalogued axiom")

    for number, entry in sorted(entries.items()):
        prefix = f"a{number:02d}_"
        if not entry.name.startswith(prefix):
            errors.append(f"A{number}: catalogue name {entry.name!r} must start with {prefix!r}")
        # rustc already rejects a catalogue probe that does not exist, so the
        # name check is enough to keep `probe_aNN` in step with the id.
        if entry.probe is not None:
            expected_probe = f"probe_a{number:02d}"
            if entry.probe != expected_probe:
                errors.append(f"A{number}: catalogue probe {entry.probe} must be {expected_probe}")
            if entry.name not in example_stems:
                errors.append(f"A{number}: probed entry has no sample examples/{entry.name}.rs")
        row = rows.get(number)
        if row is None:
            continue
        claims_probe = PROBE_MARKER in row.pinned_by
        if claims_probe and entry.probe is None:
            errors.append(f"A{number}: ledger row is pinned by a probe the catalogue does not have")
        if entry.probe is not None and not claims_probe:
            errors.append(f"A{number}: catalogue has a probe but the ledger row does not say {PROBE_MARKER}")
        if RETIRED_PROBE_PREFIX in row.pinned_by:
            errors.append(f"A{number}: Pinned by still names the retired `{RETIRED_PROBE_PREFIX}` probes")
        if not row.verified_against:
            errors.append(f"A{number}: Verified against is empty")
        if entry.probe is not None:
            if f"examples/{entry.name}.rs" not in row.sample:
                errors.append(f"A{number}: Sample must name examples/{entry.name}.rs")
        elif not row.sample.startswith(NO_SAMPLE_MARKER) or len(row.sample) <= len(NO_SAMPLE_MARKER):
            errors.append(f"A{number}: an unprobed row's Sample must be `{NO_SAMPLE_MARKER}` with a reason")
    return errors


def load_tree() -> tuple[str, str, set[str]]:
    return (
        LEDGER.read_text(encoding="utf-8"),
        CATALOGUE.read_text(encoding="utf-8"),
        {path.stem for path in EXAMPLES.glob("*.rs")},
    )


# --- self-test -------------------------------------------------------------

GOOD_LEDGER = """
| Id | Axiom | Dependent designs | Pinned by | Verified against | Sample |
|---|---|---|---|---|---|
| A1 | one | d | **probe**: `gtk_lush_axioms::probe_a01` | GTK 4.22.5 / Adw 1.9.3 (host) | `examples/a01_one.rs` |
| A2 | two | d | **not pinned**: approximate | GTK 4.22.5 / Adw 1.9.3 (host) | — only approximately true |
"""

GOOD_CATALOGUE = """
    Axiom {
        id: AxiomId::new(1),
        name: "a01_one",
        probe: Some(probes::probe_a01),
    },
    Axiom {
        id: AxiomId::new(2),
        name: "a02_two",
        probe: None,
    },
"""

GOOD_EXAMPLES = {"a01_one"}


def self_test() -> int:
    cases: list[tuple[str, dict[str, object], str]] = [
        ("a ledger without the new columns", {"ledger": GOOD_LEDGER.replace(" Verified against | Sample |", "").replace("|---|---|---|---|---|---|", "|---|---|---|---|")}, "missing column"),
        ("a ledger id with no catalogue entry", {"ledger": GOOD_LEDGER + "| A3 | three | d | **evidence** | GTK 4.22.5 | — none |\n"}, "A3: ledger id has no gtk-lush-axioms catalogue entry"),
        ("a catalogue entry with no ledger row", {"catalogue": GOOD_CATALOGUE + '    Axiom {\n        id: AxiomId::new(3),\n        name: "a03_three",\n        probe: None,\n    },\n'}, "A3: catalogue entry has no ledger row"),
        ("a probed entry with no sample", {"examples": set()}, "A1: probed entry has no sample"),
        ("a sample naming an unknown axiom", {"examples": GOOD_EXAMPLES | {"a09_ghost"}}, "examples/a09_ghost.rs names no catalogued axiom"),
        ("a ledger probe the catalogue lacks", {"catalogue": GOOD_CATALOGUE.replace("Some(probes::probe_a01)", "None")}, "A1: ledger row is pinned by a probe the catalogue does not have"),
        ("a catalogue probe the ledger omits", {"ledger": GOOD_LEDGER.replace("**probe**: `gtk_lush_axioms::probe_a01`", "indirectly")}, "does not say **probe**"),
        ("a Pinned by cell naming the retired probes", {"ledger": GOOD_LEDGER.replace("`gtk_lush_axioms::probe_a01`", "`gtk_axioms::test_axiom_a1`")}, "retired `gtk_axioms::` probes"),
        ("a probe named for another axiom", {"catalogue": GOOD_CATALOGUE.replace("Some(probes::probe_a01)", "Some(probes::probe_a02)")}, "catalogue probe probe_a02 must be probe_a01"),
        ("an empty Verified against cell", {"ledger": GOOD_LEDGER.replace("GTK 4.22.5 / Adw 1.9.3 (host) | `examples", " | `examples")}, "A1: Verified against is empty"),
        ("a probed row whose Sample is wrong", {"ledger": GOOD_LEDGER.replace("`examples/a01_one.rs`", "`examples/a01_other.rs`")}, "A1: Sample must name examples/a01_one.rs"),
        ("an unprobed row without a reason", {"ledger": GOOD_LEDGER.replace("— only approximately true", "—")}, "A2: an unprobed row's Sample must be"),
    ]
    baseline = check(GOOD_LEDGER, GOOD_CATALOGUE, GOOD_EXAMPLES)
    failures: list[str] = []
    if baseline:
        failures.append(f"the good fixture must pass, saw: {baseline}")
    for label, overrides, expected in cases:
        errors = check(
            str(overrides.get("ledger", GOOD_LEDGER)),
            str(overrides.get("catalogue", GOOD_CATALOGUE)),
            overrides.get("examples", GOOD_EXAMPLES),  # type: ignore[arg-type]
        )
        if not any(expected in error for error in errors):
            failures.append(f"{label}: expected an error containing {expected!r}, saw {errors}")
    if failures:
        print("check-gtk-axioms self-test failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print(f"check-gtk-axioms self-test passed ({len(cases)} failure cases).")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the checker's own tests")
    arguments = parser.parse_args()
    if arguments.self_test:
        return self_test()
    errors = check(*load_tree())
    if errors:
        print("GTK axiom ledger check failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print("GTK axiom ledger and gtk-lush-axioms agree.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
