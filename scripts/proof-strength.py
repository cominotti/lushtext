#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Measure how much of each Kani-checked module the proofs pin, by mutation.

cargo-mutants generates the mutants but can only use `cargo test` or nextest as
its oracle, so this driver applies each mutant itself and runs the module's
Kani harnesses (the `ORACLES` table of `scripts/kani-shards.py`) against it. The
tests arm is cargo-mutants itself over the same module, and `report` joins the
two on the mutant name. openspec/changes/measure-proof-strength-with-mutation,
design D1-D7.

Usage:
  scripts/proof-strength.py list     [--module M|all] [--tier ci|all]
  scripts/proof-strength.py baseline [--module M|all] [--tier ci|all]
  scripts/proof-strength.py run      [--module M|all] [--tier ci|all] [--shard k/n] [--mutant NAME]
  scripts/proof-strength.py tests    [--module M|all] [--shard k/n]
  scripts/proof-strength.py report   [--module M|all] [--markdown FILE]
  scripts/proof-strength.py measure  [--module M|all] [--tier ci|all] [--shard k/n] [--arm tests|kani|both]
  scripts/proof-strength.py clean
  scripts/proof-strength.py --self-test

Isolation: every Kani run happens in a disposable detached worktree
(`target/proof-strength/wt`, at `--rev`, default HEAD), so uncommitted edits in
the developer tree are neither mutated nor checked. The tests arm runs
cargo-mutants against that worktree, which copies it before mutating. `clean`
removes the worktree. Results live outside it, one JSON file per mutant under
`target/proof-strength/results/<rev>/<module>/`, so a restart with the same
revision and oracle table skips every finished mutant.

Kani classes: killed (a harness failed; `should_panic` kills, where the mutant
removed a pinned counterexample, are flagged), vacuity (every harness verified
but a cover satisfied on the baseline became unsatisfiable), timeout, unviable
(the mutant does not compile under Kani), and survived. Only `killed` counts as
a kill. A harness that provably cannot reach the mutated function (its name is
absent from the harness's baseline symbol table, see `reach_tokens`) is skipped,
because its program, and so its verdict, is unchanged.

Resource use: Kani runs one harness batch at a time, never in parallel, and the
tests arm runs only after the Kani arm (they never overlap).
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import signal
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
STATE = REPO_ROOT / "target/proof-strength"
WORKTREE = STATE / "wt"
RESULTS = STATE / "results"
FEATURES = "lushtext-core/property-tests"
# Harnesses whose recorded seconds sum to at most this share one `cargo kani`
# invocation (one compile) with `--fail-fast`; each costlier harness runs alone,
# so the list stays cheapest first where it matters.
BATCH_SECONDS = 30.0
# Per-harness timeout: max(60 s, 3 x the baseline seconds), design D3.
MIN_TIMEOUT = 60.0
TIMEOUT_FACTOR = 3.0
BASELINE_MIN_TIMEOUT = 600.0
BASELINE_TIMEOUT_FACTOR = 10.0
# Allowance for the `cargo kani` compile on top of the harness timeouts.
COMPILE_ALLOWANCE = 900.0
TESTS_TIMEOUT = 300
# Peak resident set (KiB) of each `cargo kani` run since the last reset.
PEAKS: list[int] = []
TESTS_JOBS = 2
TESTS_BUILD_JOBS = 8
TESTS_THREADS = 8


def load_shards():
    spec = importlib.util.spec_from_file_location("kani_shards", REPO_ROOT / "scripts/kani-shards.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules["kani_shards"] = module
    spec.loader.exec_module(module)
    return module


KS = load_shards()


def git(*args: str, cwd: Path = REPO_ROOT, check: bool = True) -> str:
    return subprocess.run(["git", *args], cwd=cwd, check=check, capture_output=True, text=True).stdout.strip()


def slug(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()[:16]


def module_slug(module: str) -> str:
    return module.replace("crates/", "").replace("/", "__")


def table_hash(module: str) -> str:
    """Identity of one module's oracle entry: a changed harness list, order,
    scope, or flag invalidates that module's persisted results."""
    oracle = KS.ORACLES[module]
    payload = {
        "package": oracle.package,
        "functions": list(oracle.functions),
        "harnesses": [(h.name, h.tier, list(kani_flags(oracle.package, h.name))) for h in oracle.harnesses],
    }
    return slug(json.dumps(payload, sort_keys=True))


def kani_flags(package: str, harness: str) -> tuple[str, ...]:
    owners = [s for s in KS.SHARDS.values() if s.owns(package, harness)]
    return owners[0].kani_flags if owners else ()


def cargo_mutants_version() -> str:
    return subprocess.run(["cargo", "mutants", "--version"], capture_output=True, text=True, check=True).stdout.strip()


def kani_version() -> str:
    out = subprocess.run(["cargo", "kani", "--version"], capture_output=True, text=True, check=True).stdout
    return out.splitlines()[0].strip()


# --- worktree -----------------------------------------------------------------


def resolve_rev(rev: str) -> str:
    return git("rev-parse", "--verify", f"{rev}^{{commit}}")


def ensure_worktree(rev: str, worktree: Path = WORKTREE) -> Path:
    """A detached, clean worktree at `rev`. It is disposable: stray edits in it
    (an interrupted mutant) are discarded, never the developer tree's."""
    if not (worktree / ".git").exists():
        worktree.parent.mkdir(parents=True, exist_ok=True)
        git("worktree", "add", "--detach", str(worktree), rev)
    git("checkout", "--force", "--detach", rev, cwd=worktree)
    git("checkout", "--", ".", cwd=worktree)
    dirty = git("status", "--porcelain", "--untracked-files=no", cwd=worktree)
    if dirty:
        raise SystemExit(f"proof-strength: worktree {worktree} is not clean:\n{dirty}")
    return worktree


def remove_worktree(worktree: Path = WORKTREE) -> None:
    if (worktree / ".git").exists():
        git("worktree", "remove", "--force", str(worktree))
    git("worktree", "prune")


def apply_mutant(worktree: Path, mutant: dict) -> None:
    """Apply one cargo-mutants diff. Its `+++` line is a description, not a
    path, so the target file is named explicitly."""
    subprocess.run(
        ["patch", "--forward", "--silent", "--no-backup-if-mismatch", mutant["file"]],
        cwd=worktree,
        input=mutant["diff"],
        text=True,
        check=True,
        capture_output=True,
    )


def restore_mutant(worktree: Path, mutant: dict) -> None:
    git("checkout", "--", mutant["file"], cwd=worktree)
    if git("status", "--porcelain", "--untracked-files=no", cwd=worktree):
        raise SystemExit(f"proof-strength: restoring {mutant['name']} left the worktree dirty")


# --- mutant population --------------------------------------------------------


def derived_config(worktree: Path, module: str) -> Path:
    """The repository's cargo-mutants config with `examine_globs` narrowed to one
    module: the same exclude_re entries, timeouts, and test tool, and no
    whole-scope field-deletion floor. Both arms list through it."""
    text = (worktree / ".cargo/mutants.toml").read_text(encoding="utf-8")
    narrowed, count = re.subn(r"(?ms)^examine_globs = \[.*?^\]", f'examine_globs = ["{module}"]', text)
    if count != 1:
        raise SystemExit("proof-strength: .cargo/mutants.toml has no single examine_globs list")
    path = STATE / "config" / f"{module_slug(module)}.toml"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(narrowed, encoding="utf-8")
    return path


def list_json(worktree: Path, args: list[str]) -> list[dict]:
    out = subprocess.run(
        ["cargo", "mutants", "--list", "--json", "--workspace", *args],
        cwd=worktree,
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return json.loads(out)


def in_default_scope(worktree: Path, module: str) -> bool:
    text = (worktree / ".cargo/mutants.toml").read_text(encoding="utf-8")
    import tomllib

    config = tomllib.loads(text)
    from fnmatch import fnmatch

    def matches(glob: str) -> bool:
        return fnmatch(module, glob.replace("**/", "*").replace("/**", "/*"))

    return any(matches(g) for g in config.get("examine_globs", [])) and not any(
        matches(g) for g in config.get("exclude_globs", [])
    )


@dataclass
class Population:
    module: str
    mutants: list[dict]
    outside_scope: list[str]
    floor: int | None


def function_name(mutant: dict) -> str | None:
    function = mutant.get("function")
    return function.get("function_name") if function else None


def split_scope(mutants: list[dict], module: str, functions: tuple[str, ...]) -> tuple[list[dict], list[str], int]:
    """Keep the module's own mutants (dropping any other file's, the floor) and,
    for a function-scoped oracle, only those in the listed functions."""
    own = [m for m in mutants if m["file"] == module]
    floor = len(mutants) - len(own)
    if not functions:
        return own, [], floor
    inside = [m for m in own if function_name(m) in functions]
    outside = [m["name"] for m in own if function_name(m) not in functions]
    return inside, outside, floor


def population(worktree: Path, module: str) -> Population:
    oracle = KS.ORACLES[module]
    listed = list_json(worktree, ["--config", str(derived_config(worktree, module))])
    mutants, outside, _ = split_scope(listed, module, oracle.functions)
    floor = None
    if in_default_scope(worktree, module):
        escaped = re.escape(module)
        repo_listed = list_json(worktree, ["--re", f"^{escaped}:"])
        floor = split_scope(repo_listed, module, ())[2]
    return Population(module, mutants, outside, floor)


def shard_select(names: list[str], shard: str | None) -> set[str]:
    """A stable partition: sorted names, index modulo n."""
    if not shard:
        return set(names)
    k, n = (int(x) for x in shard.split("/"))
    if not 0 <= k < n:
        raise SystemExit(f"proof-strength: shard {shard} is not k/n with 0 <= k < n")
    return {name for index, name in enumerate(sorted(names)) if index % n == k}


# --- Kani runs ----------------------------------------------------------------

START_RE = re.compile(r"^Checking harness (\S+?)\.\.\.$")
VERDICT_RE = re.compile(r"^VERIFICATION:- (\S+)(.*)$")
TIME_RE = re.compile(r"^Verification Time: ([0-9.]+)s$")
CHECK_RE = re.compile(r"^Check \d+: (\S+)$")
STATUS_RE = re.compile(r"^- Status: (\S+)$")
COMPILE_ERROR_RE = re.compile(r"^error(\[E\d+\])?: |could not compile", re.M)


@dataclass
class HarnessRun:
    name: str
    verdict: str | None = None
    detail: str = ""
    seconds: float | None = None
    timed_out: bool = False
    covers: dict[str, str] = field(default_factory=dict)

    @property
    def verified(self) -> bool:
        return self.verdict == "SUCCESSFUL" and not self.timed_out

    @property
    def should_panic_kill(self) -> bool:
        return "at least one was expected" in self.detail


def parse_kani(lines: list[str]) -> list[HarnessRun]:
    runs: list[HarnessRun] = []
    check: str | None = None
    for raw in lines:
        line = raw.strip()
        if match := START_RE.match(line):
            runs.append(HarnessRun(match.group(1)))
            check = None
        elif not runs:
            continue
        elif match := VERDICT_RE.match(line):
            runs[-1].verdict = match.group(1)
            runs[-1].detail = match.group(2).strip()
        elif match := TIME_RE.match(line):
            runs[-1].seconds = float(match.group(1))
        elif line.startswith("CBMC timed out"):
            runs[-1].timed_out = True
        elif match := CHECK_RE.match(line):
            check = match.group(1) if ".cover." in match.group(1) else None
        elif check and (match := STATUS_RE.match(line)):
            runs[-1].covers[check] = match.group(1)
            check = None
    return runs


def harness_timeout(seconds: float, baseline: bool = False) -> float:
    """3x the recorded seconds, at least a minute. The baseline itself is the
    measurement, so it only guards against a hang: 10x, at least ten minutes."""
    if baseline:
        return max(BASELINE_MIN_TIMEOUT, BASELINE_TIMEOUT_FACTOR * seconds)
    return max(MIN_TIMEOUT, TIMEOUT_FACTOR * seconds)


def batches(harnesses: list, package: str) -> list[list]:
    """Consecutive cheap harnesses with the same shard flags share one run."""
    groups: list[list] = []
    for harness in harnesses:
        flags = kani_flags(package, harness.name)
        last = groups[-1] if groups else None
        if (
            last
            and kani_flags(package, last[0].name) == flags
            and sum(h.seconds for h in last) + harness.seconds <= BATCH_SECONDS
        ):
            last.append(harness)
        else:
            groups.append([harness])
    return groups


def run_kani(
    worktree: Path, package: str, harnesses: list, log: Path, baseline: bool = False
) -> tuple[int | None, list[HarnessRun], str]:
    """One `cargo kani` over `harnesses` with a per-harness timeout and an outer
    wall-clock guard. Returns (exit status or None on the outer timeout, the
    parsed harness runs, the full output)."""
    per = max(harness_timeout(h.seconds, baseline) for h in harnesses)
    command = [
        "cargo",
        "kani",
        "-p",
        package,
        "--target-dir",
        str(worktree / "target/kani"),
        *kani_flags(package, harnesses[0].name),
        "-Z",
        "unstable-options",
        "--harness-timeout",
        f"{int(per)}s",
        "--fail-fast",
        "--exact",
    ]
    for harness in harnesses:
        command += ["--harness", harness.name]
    guard = COMPILE_ALLOWANCE + sum(harness_timeout(h.seconds, baseline) for h in harnesses)
    log.parent.mkdir(parents=True, exist_ok=True)
    expired = threading.Event()
    with open(log, "w", encoding="utf-8") as handle:
        handle.write(" ".join(command) + "\n")
        handle.flush()
        child = subprocess.Popen(command, cwd=worktree, stdout=handle, stderr=subprocess.STDOUT, start_new_session=True)

        def kill() -> None:
            expired.set()
            os.killpg(child.pid, signal.SIGKILL)

        timer = threading.Timer(guard, kill)
        timer.start()
        try:
            # wait4 folds every waited-for descendant (cargo-kani, kani-driver,
            # CBMC) into the child's rusage, so ru_maxrss is CBMC's peak.
            _, wait_status, usage = os.wait4(child.pid, 0)
        finally:
            timer.cancel()
        child.returncode = os.waitstatus_to_exitcode(wait_status)
    status: int | None = None if expired.is_set() else child.returncode
    output = log.read_text(encoding="utf-8", errors="replace")
    PEAKS.append(usage.ru_maxrss)
    return status, parse_kani(output.splitlines()), output


# --- reachability -------------------------------------------------------------

TOKEN_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


def reach_tokens(worktree: Path, package: str, harness: str) -> set[str] | None:
    """Every identifier in the pretty names of the harness's goto symbol table,
    from the newest `--only-codegen` output, or None when it cannot be found."""
    segments = harness.split("::")
    mangled = "".join(f"{len(s)}{s}" for s in segments)
    crate = package.replace("-", "_")
    build = worktree / "target/kani/kani/x86_64-unknown-linux-gnu/debug/build" / package
    candidates = sorted(
        build.glob(f"*/out/{crate}-*__*{mangled}.pretty_name_map.json"),
        key=lambda p: p.stat().st_mtime,
        reverse=True,
    )
    if not candidates:
        return None
    names = json.loads(candidates[0].read_text(encoding="utf-8"))
    tokens: set[str] = set()
    for key, value in names.items():
        if isinstance(value, str):
            tokens.update(TOKEN_RE.findall(value))
        if isinstance(key, str):
            tokens.update(TOKEN_RE.findall(key))
    return tokens


def mutated_token(mutant: dict, source: str | None) -> str | None:
    """The identifier a harness must reach for this mutant to change its
    program, or None when the mutant must always be run: a mutant outside any
    function (a constant) or inside a `const fn`, which rustc may evaluate at
    compile time so the goto program never names it."""
    name = function_name(mutant)
    if not name:
        return None
    if source is not None:
        line = mutant["function"]["span"]["start"]["line"]
        lines = source.splitlines()
        header = " ".join(lines[line - 1 : line + 2]) if 0 < line <= len(lines) else ""
        if re.search(r"\bconst\s+fn\b", header):
            return None
    tokens = TOKEN_RE.findall(name.split("::")[-1])
    return tokens[0] if tokens else None


# --- baseline -----------------------------------------------------------------


def results_dir(rev: str, module: str) -> Path:
    return RESULTS / rev / module_slug(module)


def baseline_path(rev: str) -> Path:
    return RESULTS / rev / "baseline.json"


def load_baseline(rev: str) -> dict:
    path = baseline_path(rev)
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else {"harnesses": {}}


def baseline(worktree: Path, rev: str, modules: list[str], tier: str) -> dict:
    """Every oracle harness verifies on the unmutated tree; records each one's
    seconds, satisfied covers, and reachable identifiers. Aborts on a failure."""
    record = load_baseline(rev)
    record["kani_version"] = kani_version()
    by_package: dict[str, list] = {}
    for module in modules:
        oracle = KS.ORACLES[module]
        for harness in oracle.ordered(tier):
            if harness.name not in record["harnesses"]:
                bucket = by_package.setdefault(oracle.package, [])
                if harness.name not in [h.name for h in bucket]:
                    bucket.append(harness)
    for package, harnesses in by_package.items():
        print(f"proof-strength: codegen for {package} (reachability)", flush=True)
        subprocess.run(
            ["cargo", "kani", "-p", package, "--target-dir", str(worktree / "target/kani"), "--only-codegen"],
            cwd=worktree,
            check=True,
            stdout=subprocess.DEVNULL,
        )
        reach = {h.name: reach_tokens(worktree, package, h.name) for h in harnesses}
        for group in batches(sorted(harnesses, key=lambda h: h.seconds), package):
            names = ", ".join(h.name.split("::")[-1] for h in group)
            print(f"proof-strength: baseline {names}", flush=True)
            start = time.monotonic()
            PEAKS.clear()
            status, runs, output = run_kani(worktree, package, group, RESULTS / rev / "baseline-logs" / f"{slug(names)}.log", baseline=True)
            wall = time.monotonic() - start
            by_name = {run.name: run for run in runs}
            for harness in group:
                run = by_name.get(harness.name)
                if run is None or not run.verified:
                    raise SystemExit(
                        f"proof-strength: baseline harness {harness.name} did not verify "
                        f"(status {status}, verdict {run.verdict if run else 'none'}); see the baseline log"
                    )
                record["harnesses"][harness.name] = {
                    "seconds": run.seconds,
                    "covers": run.covers,
                    "reach": sorted(reach[harness.name]) if reach[harness.name] is not None else None,
                    "batch_wall_seconds": round(wall, 1),
                    "batch_peak_gib": round(max(PEAKS, default=0) / 1024 / 1024, 2),
                }
            baseline_path(rev).parent.mkdir(parents=True, exist_ok=True)
            baseline_path(rev).write_text(json.dumps(record, indent=1) + "\n", encoding="utf-8")
    return record


# --- classification -----------------------------------------------------------


@dataclass
class Outcome:
    name: str
    klass: str
    harness: str | None = None
    should_panic: bool = False
    seconds: float = 0.0
    ran: list[str] = field(default_factory=list)
    skipped_unreached: list[str] = field(default_factory=list)
    timeouts: list[str] = field(default_factory=list)


def classify(name: str, runs_by_batch: list[tuple[int | None, list[HarnessRun], str]], covers: dict[str, dict]) -> Outcome:
    """The mutant's class from its batches' results, in order (design D3)."""
    outcome = Outcome(name, "survived")
    vacuous: str | None = None
    for status, runs, output in runs_by_batch:
        if not runs:
            if status is None:
                outcome.timeouts.append("<compile>")
                continue
            if status != 0 and COMPILE_ERROR_RE.search(output):
                outcome.klass = "unviable"
                return outcome
            raise RuntimeError(f"cargo kani produced no harness result (status {status})")
        for run in runs:
            outcome.ran.append(run.name)
            if run.timed_out:
                outcome.timeouts.append(run.name)
            elif run.verdict != "SUCCESSFUL":
                outcome.klass = "killed"
                outcome.harness = run.name
                outcome.should_panic = run.should_panic_kill
                return outcome
            else:
                for check, before in covers.get(run.name, {}).items():
                    if before == "SATISFIED" and run.covers.get(check) != "SATISFIED" and vacuous is None:
                        vacuous = run.name
        if status is None:
            outcome.timeouts.append("<wall-clock>")
    if outcome.timeouts:
        outcome.klass = "timeout"
    elif vacuous:
        outcome.klass = "vacuity"
        outcome.harness = vacuous
    return outcome


def result_path(rev: str, module: str, name: str) -> Path:
    return results_dir(rev, module) / f"{slug(name)}.json"


def finished(rev: str, module: str, name: str, tier: str) -> bool:
    path = result_path(rev, module, name)
    if not path.exists():
        return False
    record = json.loads(path.read_text(encoding="utf-8"))
    if record.get("table_hash") != table_hash(module):
        return False
    # A ci-tier survivor is re-run by an `all`-tier run over the local harnesses.
    return not (tier == "all" and record.get("tier") == "ci" and record["class"] in ("survived", "timeout"))


def run_module(worktree: Path, rev: str, module: str, tier: str, shard: str | None, only: str | None) -> None:
    oracle = KS.ORACLES[module]
    pop = population(worktree, module)
    base = load_baseline(rev)
    missing = [h.name for h in oracle.ordered(tier) if h.name not in base["harnesses"]]
    if missing:
        raise SystemExit(f"proof-strength: no baseline for {missing}; run `baseline` first")
    covers = {name: entry["covers"] for name, entry in base["harnesses"].items()}
    selected = shard_select([m["name"] for m in pop.mutants], shard)
    todo = [m for m in pop.mutants if m["name"] in selected and (only is None or m["name"] == only)]
    source = (worktree / module).read_text(encoding="utf-8")
    versions = {"kani_version": base["kani_version"], "cargo_mutants_version": cargo_mutants_version()}
    for index, mutant in enumerate(todo, 1):
        name = mutant["name"]
        if finished(rev, module, name, tier):
            continue
        prior = None
        path = result_path(rev, module, name)
        if path.exists():
            prior = json.loads(path.read_text(encoding="utf-8"))
        token = mutated_token(mutant, source)
        harnesses = oracle.ordered(tier)
        if prior and prior.get("table_hash") == table_hash(module):
            # Extending a ci-tier survivor: only the harnesses it has not met.
            harnesses = [h for h in harnesses if h.name not in prior.get("ran", []) + prior.get("skipped_unreached", [])]
        reached, unreached = [], []
        for harness in harnesses:
            reach = base["harnesses"][harness.name].get("reach")
            (unreached if token and reach is not None and token not in reach else reached).append(harness)
        print(f"[{module_slug(module)} {index}/{len(todo)}] {name}", flush=True)
        start = time.monotonic()
        PEAKS.clear()
        apply_mutant(worktree, mutant)
        try:
            batch_results = []
            for group in batches(reached, oracle.package):
                log = results_dir(rev, module) / "logs" / f"{slug(name)}-{slug(group[0].name)}.log"
                result = run_kani(worktree, oracle.package, group, log)
                batch_results.append(result)
                partial = classify(name, batch_results, covers)
                if partial.klass in ("killed", "unviable"):
                    break
            outcome = classify(name, batch_results, covers)
        finally:
            restore_mutant(worktree, mutant)
        outcome.seconds = round(time.monotonic() - start, 1) + (prior or {}).get("seconds", 0.0)
        outcome.skipped_unreached = [h.name for h in unreached] + (prior or {}).get("skipped_unreached", [])
        outcome.ran = (prior or {}).get("ran", []) + outcome.ran
        outcome.timeouts = (prior or {}).get("timeouts", []) + outcome.timeouts
        if outcome.klass == "survived" and outcome.timeouts:
            outcome.klass = "timeout"
        record = {
            **{k: v for k, v in asdict(outcome).items() if k != "klass"},
            "class": outcome.klass,
            "tier": tier,
            "rev": rev,
            "module": module,
            "function": function_name(mutant),
            "table_hash": table_hash(module),
            "peak_gib": round(max([*PEAKS, int((prior or {}).get("peak_gib", 0) * 1024 * 1024)]) / 1024 / 1024, 2),
            **versions,
        }
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(record, indent=1) + "\n", encoding="utf-8")
        killer = f" by {outcome.harness.split('::')[-1]}" if outcome.harness else ""
        print(f"    -> {outcome.klass}{killer} ({outcome.seconds} s)", flush=True)


# --- tests arm ----------------------------------------------------------------


def tests_dir(rev: str, module: str) -> Path:
    return RESULTS / rev / "tests" / module_slug(module)


def run_tests(worktree: Path, rev: str, module: str, shard: str | None) -> None:
    """cargo-mutants over the module through the narrowed config, with the
    property tests enabled (design D5)."""
    oracle = KS.ORACLES[module]
    pop = population(worktree, module)
    out = tests_dir(rev, module)
    if shard:
        out = out / f"shard-{shard.replace('/', '-of-')}"
    meta_path = out / "meta.json"
    if meta_path.exists() and (out / "mutants.out/outcomes.json").exists():
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        if meta.get("rev") == rev and meta.get("finished"):
            return
    out.mkdir(parents=True, exist_ok=True)
    command = [
        "cargo",
        "mutants",
        "--config",
        str(derived_config(worktree, module)),
        "--workspace",
        "--test-workspace=true",
        "--test-tool",
        "nextest",
        "--no-shuffle",
        "--timeout",
        str(TESTS_TIMEOUT),
        "--features",
        FEATURES,
        "--jobs",
        str(TESTS_JOBS),
        "--output",
        str(out),
        "-d",
        str(worktree),
    ]
    if oracle.functions:
        for mutant in pop.mutants:
            command += ["--re", f"^{re.escape(mutant['name'])}$"]
        # cargo-mutants lets field-deletion mutants through any `--re`; exclude
        # the out-of-scope ones by name so the arm runs exactly the population.
        for name in pop.outside_scope:
            command += ["--exclude-re", f"^{re.escape(name)}$"]
    if shard:
        command += ["--shard", shard]
    env = {**os.environ, "CARGO_BUILD_JOBS": str(TESTS_BUILD_JOBS), "NEXTEST_TEST_THREADS": str(TESTS_THREADS)}
    meta = {
        "rev": rev,
        "module": module,
        "cargo_mutants_version": cargo_mutants_version(),
        "features": FEATURES,
        "command": command,
        "finished": False,
    }
    meta_path.write_text(json.dumps(meta, indent=1) + "\n", encoding="utf-8")
    print(f"proof-strength: tests arm for {module}", flush=True)
    start = time.monotonic()
    status = subprocess.run(command, cwd=REPO_ROOT, env=env, check=False).returncode
    # cargo-mutants exits 2 when mutants were missed and 3 on timeouts: results, not errors.
    if status not in (0, 2, 3):
        raise SystemExit(f"proof-strength: cargo mutants failed with status {status} for {module}")
    meta.update(finished=True, status=status, wall_seconds=round(time.monotonic() - start, 1))
    meta_path.write_text(json.dumps(meta, indent=1) + "\n", encoding="utf-8")


def tests_outcomes(rev: str, module: str) -> tuple[dict[str, str], list[dict]]:
    root = tests_dir(rev, module)
    outcomes: dict[str, str] = {}
    metas = []
    for meta_path in sorted(root.rglob("meta.json")):
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        metas.append(meta)
        data = json.loads((meta_path.parent / "mutants.out/outcomes.json").read_text(encoding="utf-8"))
        for item in data["outcomes"]:
            scenario = item["scenario"]
            if isinstance(scenario, dict) and "Mutant" in scenario:
                outcomes[scenario["Mutant"]["name"]] = item["summary"]
    return outcomes, metas


# --- report -------------------------------------------------------------------


def join_problems(kani: dict[str, dict], metas: list[dict], rev: str) -> list[str]:
    """Refuse to join arms measured on different revisions or tool versions."""
    problems = []
    versions = {r.get("cargo_mutants_version") for r in kani.values()}
    for meta in metas:
        if meta.get("rev") != rev:
            problems.append(f"tests arm measured at {meta.get('rev')}, Kani arm at {rev}")
        if not meta.get("finished"):
            problems.append("tests arm did not finish")
        if versions and meta.get("cargo_mutants_version") not in versions:
            problems.append(f"cargo-mutants {meta.get('cargo_mutants_version')} vs {sorted(versions)}")
    for record in kani.values():
        if record.get("rev") != rev:
            problems.append(f"Kani result {record['name']} is from {record.get('rev')}")
    return problems


def module_report(rev: str, module: str, pop: Population) -> dict:
    kani: dict[str, dict] = {}
    for path in results_dir(rev, module).glob("*.json"):
        record = json.loads(path.read_text(encoding="utf-8"))
        if record.get("table_hash") == table_hash(module):
            kani[record["name"]] = record
    tests, metas = tests_outcomes(rev, module)
    problems = join_problems(kani, metas, rev)
    names = [m["name"] for m in pop.mutants]
    row = {
        "module": module,
        "mutants": len(names),
        "floor": pop.floor,
        "outside_scope": len(pop.outside_scope),
        "kani_measured": sum(1 for n in names if n in kani),
        "tests_measured": sum(1 for n in names if n in tests),
        "problems": problems,
    }

    def kani_class(n: str) -> str | None:
        return kani[n]["class"] if n in kani else None

    kani_kill = {n for n in names if kani_class(n) == "killed"}
    tests_kill = {n for n in names if tests.get(n) == "CaughtMutant"}
    row.update(
        kani_unviable=sum(1 for n in names if kani_class(n) == "unviable"),
        tests_unviable=sum(1 for n in names if tests.get(n) == "Unviable"),
        tests_timeouts=sum(1 for n in names if tests.get(n) == "Timeout"),
        tests_only=len(tests_kill - kani_kill),
        kani_only=len(kani_kill - tests_kill),
        kani_only_should_panic=sum(1 for n in kani_kill - tests_kill if kani[n].get("should_panic")),
        should_panic_kills=sum(1 for n in kani_kill if kani[n].get("should_panic")),
        both=len(kani_kill & tests_kill),
        either=len(kani_kill | tests_kill),
        kani_kills=len(kani_kill),
        tests_kills=len(tests_kill),
        vacuity=sum(1 for n in names if kani_class(n) == "vacuity"),
        kani_timeouts=sum(1 for n in names if kani_class(n) == "timeout"),
        kani_survivors=sum(1 for n in names if kani_class(n) == "survived"),
    )
    row["survivors"] = [
        {
            "name": n,
            "class": kani[n]["class"],
            "tests": tests.get(n),
            "unreached": not kani[n].get("ran"),
            "tier": kani[n].get("tier"),
        }
        for n in names
        if kani_class(n) in ("survived", "timeout", "vacuity")
    ]
    return row


def markdown(rows: list[dict], rev: str, versions: dict) -> str:
    lines = [
        f"Revision `{rev[:8]}`, {versions.get('kani', '?')}, {versions.get('cargo_mutants', '?')}, "
        f"tests arm features `{FEATURES}`.",
        "",
        "| Module | Mutants | Floor | Outside scope | Unviable (Kani / tests) | Tests-only kills | "
        "Kani-only kills (should_panic) | Both | Either | Vacuity | Kani timeouts | Kani survivors |",
        "|---|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for row in rows:
        floor = "n/a" if row["floor"] is None else row["floor"]
        lines.append(
            f"| `{row['module'].split('/src/')[-1]}` | {row['mutants']} | {floor} | {row['outside_scope']} "
            f"| {row['kani_unviable']} / {row['tests_unviable']} | {row['tests_only']} "
            f"| {row['kani_only']} ({row['kani_only_should_panic']}) | {row['both']} | {row['either']} "
            f"| {row['vacuity']} | {row['kani_timeouts']} | {row['kani_survivors']} |"
        )
    lines.append("")
    for row in rows:
        if row["problems"]:
            lines.append(f"- `{row['module']}`: " + "; ".join(sorted(set(row["problems"]))))
        for survivor in row["survivors"]:
            flag = " (no oracle harness reaches it)" if survivor["unreached"] else ""
            lines.append(f"- {survivor['class']}: `{survivor['name']}`, tests: {survivor['tests']}{flag}")
    return "\n".join(lines) + "\n"


# --- self-test ----------------------------------------------------------------


def self_test() -> None:
    # Diff application and restore on a fixture repository.
    with tempfile.TemporaryDirectory() as scratch:
        repo = Path(scratch)
        git("init", "-q", cwd=repo)
        git("config", "user.email", "t@example.invalid", cwd=repo)
        git("config", "user.name", "t", cwd=repo)
        git("config", "commit.gpgsign", "false", cwd=repo)
        (repo / "src").mkdir()
        original = "fn f() -> i32 {\n    1 + 2\n}\n"
        (repo / "src/m.rs").write_text(original)
        git("add", "src/m.rs", cwd=repo)
        git("commit", "-q", "-m", "fixture", cwd=repo)
        mutant = {
            "name": "src/m.rs:2:7: replace + with -",
            "file": "src/m.rs",
            "diff": "--- src/m.rs\n+++ replace + with -\n@@ -1,3 +1,3 @@\n fn f() -> i32 {\n-    1 + 2\n+    1 - 2\n }\n",
        }
        apply_mutant(repo, mutant)
        assert (repo / "src/m.rs").read_text() == "fn f() -> i32 {\n    1 - 2\n}\n"
        restore_mutant(repo, mutant)
        assert (repo / "src/m.rs").read_text() == original

    # The floor post-filter and function scope.
    listed = [
        {"name": "a.rs:1:1: x in f", "file": "a.rs", "function": {"function_name": "f"}},
        {"name": "a.rs:2:1: y in g", "file": "a.rs", "function": {"function_name": "g"}},
        {"name": "a.rs:3:1: const", "file": "a.rs", "function": None},
        {"name": "b.rs:9:1: delete field z", "file": "b.rs", "function": {"function_name": "h"}},
    ]
    own, outside, floor = split_scope(listed, "a.rs", ())
    assert [m["name"] for m in own] == ["a.rs:1:1: x in f", "a.rs:2:1: y in g", "a.rs:3:1: const"] and floor == 1
    own, outside, floor = split_scope(listed, "a.rs", ("f",))
    assert [m["name"] for m in own] == ["a.rs:1:1: x in f"] and len(outside) == 2 and floor == 1

    # Outcome classes from canned Kani output.
    def out(*lines: str) -> list[HarnessRun]:
        return parse_kani(list(lines))

    verified = out(
        "Checking harness m::kani_proofs::h...",
        "Check 1: m::kani_proofs::h.cover.1",
        "\t - Status: SATISFIED",
        "Check 2: m::kani_proofs::h.assertion.1",
        "\t - Status: SUCCESS",
        "VERIFICATION:- SUCCESSFUL",
        "Verification Time: 1.5s",
    )
    assert verified[0].verified and verified[0].seconds == 1.5
    assert verified[0].covers == {"m::kani_proofs::h.cover.1": "SATISFIED"}
    covers = {"m::kani_proofs::h": {"m::kani_proofs::h.cover.1": "SATISFIED"}}
    assert classify("x", [(0, verified, "")], covers).klass == "survived"
    failed = out("Checking harness m::kani_proofs::h...", "VERIFICATION:- FAILED", "Verification Time: 0.1s")
    killed = classify("x", [(0, verified, ""), (1, failed, "")], covers)
    assert killed.klass == "killed" and killed.harness == "m::kani_proofs::h" and not killed.should_panic
    panicked = out(
        "Checking harness m::kani_proofs::p...",
        "VERIFICATION:- FAILED (encountered no panics, but at least one was expected)",
    )
    assert classify("x", [(1, panicked, "")], {}).should_panic
    timed_out = out("Checking harness m::kani_proofs::h...", "CBMC timed out. You may want to rerun", "VERIFICATION:- FAILED")
    assert classify("x", [(1, timed_out, "")], covers).klass == "timeout"
    assert classify("x", [(None, [], "")], covers).klass == "timeout"
    later_kill = classify("x", [(1, timed_out, ""), (1, failed, "")], covers)
    assert later_kill.klass == "killed", later_kill
    compile_error = "error[E0308]: mismatched types\nerror: could not compile `lushtext-core`\n"
    assert classify("x", [(101, [], compile_error)], covers).klass == "unviable"
    vacuous = out(
        "Checking harness m::kani_proofs::h...",
        "Check 1: m::kani_proofs::h.cover.1",
        "\t - Status: UNSATISFIABLE",
        "VERIFICATION:- SUCCESSFUL",
    )
    vacuity = classify("x", [(0, vacuous, "")], covers)
    assert vacuity.klass == "vacuity" and vacuity.harness == "m::kani_proofs::h"
    try:
        classify("x", [(1, [], "no harness output")], covers)
        raise AssertionError("an unexplained empty run must be an error")
    except RuntimeError:
        pass

    # Batching keeps cheap harnesses together, costly ones alone, and flags apart.
    H = KS.OracleHarness
    groups = batches([H("a", 1, "ci"), H("b", 2, "ci"), H("c", 40, "ci"), H("d", 1, "ci")], "no-such-package")
    assert [[h.name for h in g] for g in groups] == [["a", "b"], ["c"], ["d"]]

    # Reachability skip: never for a constant or a const fn.
    source = "const X: i32 = 1;\npub const fn k() -> i32 {\n    2\n}\nfn g() {}\n"
    assert mutated_token({"function": None}, source) is None
    assert mutated_token({"function": {"function_name": "k", "span": {"start": {"line": 2}}}}, source) is None
    assert mutated_token({"function": {"function_name": "T::g", "span": {"start": {"line": 5}}}}, source) == "g"
    impl = {"function": {"function_name": "<impl Clone for U<'_, F>>::clone", "span": {"start": {"line": 5}}}}
    assert mutated_token(impl, source) == "clone"

    # Shard partition: stable, disjoint, and complete.
    names = [f"m{i}" for i in range(11)]
    parts = [shard_select(list(reversed(names)), f"{k}/3") for k in range(3)]
    assert parts == [shard_select(names, f"{k}/3") for k in range(3)]
    assert set().union(*parts) == set(names) and sum(len(p) for p in parts) == len(names)

    # Resume: a finished result with the current table hash is skipped, a stale
    # hash is not, and an all-tier run re-opens a ci-tier survivor.
    global RESULTS
    saved = RESULTS
    with tempfile.TemporaryDirectory() as scratch:
        RESULTS = Path(scratch)
        module = next(iter(KS.ORACLES))
        path = result_path("r", module, "n")
        path.parent.mkdir(parents=True)
        path.write_text(json.dumps({"table_hash": table_hash(module), "class": "killed", "tier": "ci"}))
        assert finished("r", module, "n", "ci") and finished("r", module, "n", "all")
        path.write_text(json.dumps({"table_hash": table_hash(module), "class": "survived", "tier": "ci"}))
        assert finished("r", module, "n", "ci") and not finished("r", module, "n", "all")
        path.write_text(json.dumps({"table_hash": "stale", "class": "killed", "tier": "ci"}))
        assert not finished("r", module, "n", "ci")
    RESULTS = saved

    # The join refuses mismatched revisions and tool versions.
    kani = {"n": {"name": "n", "rev": "r1", "cargo_mutants_version": "cargo-mutants 27.0.0"}}
    good = [{"rev": "r1", "finished": True, "cargo_mutants_version": "cargo-mutants 27.0.0"}]
    assert join_problems(kani, good, "r1") == []
    assert join_problems(kani, [{**good[0], "rev": "r2"}], "r1")
    assert join_problems(kani, [{**good[0], "cargo_mutants_version": "cargo-mutants 28.0.0"}], "r1")
    assert join_problems(kani, [{**good[0], "finished": False}], "r1")
    print("proof-strength self-test passed")


# --- main ---------------------------------------------------------------------


def selected_modules(module: str) -> list[str]:
    if module == "all":
        return list(KS.ORACLES)
    if module not in KS.ORACLES:
        raise SystemExit(f"proof-strength: {module} is not an oracle module; known: {', '.join(KS.ORACLES)}")
    return [module]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", nargs="?", choices=["list", "baseline", "run", "tests", "report", "measure", "clean"])
    parser.add_argument("--module", default="all")
    parser.add_argument("--tier", choices=["ci", "all"], default="all")
    parser.add_argument("--shard")
    parser.add_argument("--arm", choices=["tests", "kani", "both"], default="both")
    parser.add_argument("--rev", default="HEAD")
    parser.add_argument("--mutant", help="run: only this mutant name")
    parser.add_argument("--markdown", help="report: also write the Markdown table here")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        if args.command is None:
            return 0
    if args.command is None:
        parser.error("a command is required")
    if args.command == "clean":
        remove_worktree()
        return 0
    rev = resolve_rev(args.rev)
    modules = selected_modules(args.module)
    worktree = ensure_worktree(rev)
    if args.command == "list":
        for module in modules:
            pop = population(worktree, module)
            floor = "n/a (outside the default mutation scope)" if pop.floor is None else pop.floor
            print(f"{module}: {len(pop.mutants)} mutants, floor {floor}, outside oracle scope {len(pop.outside_scope)}")
            for harness in KS.ORACLES[module].ordered(args.tier):
                print(f"  harness {harness.name} ({harness.tier}, {harness.seconds} s)")
        return 0
    if args.command in ("baseline", "measure") and args.arm in ("kani", "both"):
        baseline(worktree, rev, modules, args.tier)
    if args.command in ("run", "measure") and args.arm in ("kani", "both"):
        for module in modules:
            run_module(worktree, rev, module, args.tier, args.shard, args.mutant)
    if args.command == "tests" or (args.command == "measure" and args.arm in ("tests", "both")):
        for module in modules:
            run_tests(worktree, rev, module, args.shard)
    if args.command in ("report", "measure"):
        rows = [module_report(rev, module, population(worktree, module)) for module in modules]
        text = markdown(rows, rev, {"kani": load_baseline(rev).get("kani_version"), "cargo_mutants": cargo_mutants_version()})
        print(text)
        if args.markdown:
            Path(args.markdown).write_text(text, encoding="utf-8")
        (RESULTS / rev / "report.json").write_text(json.dumps(rows, indent=1) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
