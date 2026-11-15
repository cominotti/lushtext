#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Mirror slots for local in-place mutation runs, and the merge of their reports.

Design D2 and D5 of openspec/changes/reduce-build-disk-writes; the caller is
`scripts/run-mutants.sh` in mirror mode (`MUTANTS_MIRROR=1`).

Usage:
  scripts/mutants-mirror.py sync <checkout> <slot-src>
  scripts/mutants-mirror.py merge <checkout> <status>:<slot-output-dir>...
  scripts/mutants-mirror.py scan <mutants.out>
  scripts/mutants-mirror.py --self-test

sync: make <slot-src> hold exactly the checkout's tracked and untracked,
non-ignored files (`git ls-files --cached --others --exclude-standard`, minus
tracked files deleted in the working tree), uncommitted edits included, and
never `.git`. A file is rewritten only when its bytes differ, so an unchanged
file keeps its mtime and Cargo's freshness; a rewritten one gets a new mtime.
The permission bits are mirrored, a mode-only difference is applied without a
rewrite, and a symlink is mirrored as a symlink with the same target, never
followed; a symlink whose target resolves outside the slot is refused, so an
in-place mutant cannot write through a link into the checkout. Anything else in
the slot is removed. A file an interrupted in-place run left mutated differs
from the checkout's bytes, so the next sync restores it.

merge: join the `mutants.out` of each slot (`<dir>/mutants.out`, each run with
`--output <dir>`) into `<checkout>/mutants.out`, after rotating the previous
report to `mutants.out.old` the way cargo-mutants does. Slots' `log/` and
`diff/` names collide (each run restarts the `_001` counters and writes its own
`log/baseline.log`), so every slot's files are kept under `log/slot-<k>/` and
`diff/slot-<k>/`, and every `log_path`/`diff_path` in `outcomes.json` is
rewritten to match. Counters are summed, start/end times are the earliest and
latest, the outcome and mutant lists and the `*.txt` lists are concatenated,
`debug.log` gets one header per slot, and `lock.json` is dropped. A slot whose
shard was empty wrote no `outcomes.json` and contributes nothing. The exit
status follows cargo-mutants 27 (probed): a slot status outside {0, 2, 3} wins
(the first one), else 3 if any slot timed out, else 2 if any missed, else 0.

scan (also done by merge): list in `address-space-caught.txt` every caught
mutant whose log shows an allocation failure, the signature of the per-test
address-space limit (`scripts/test-address-space-runner.sh`), and print the
count, so triage can confirm each one.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURE = HERE / "mutants-mirror-fixture.json"
COUNTERS = ("total_mutants", "missed", "caught", "timeout", "unviable", "success")
RESULT_STATUSES = (0, 2, 3)
# What a refused allocation looks like in a test log once RLIMIT_AS is hit:
# Rust's allocation-failure abort, GLib's g_malloc abort, a refused thread
# stack, and ENOMEM from mmap and friends.
ADDRESS_SPACE_PATTERNS = re.compile(
    r"memory allocation of \d+ bytes failed"
    r"|failed to allocate \d+ bytes"
    r"|failed to spawn thread"
    r"|Cannot allocate memory"
    r"|\bENOMEM\b"
    r"|os error 12\b"
)
ADDRESS_SPACE_LIST = "address-space-caught.txt"


class MirrorError(Exception):
    pass


# --- sync ---------------------------------------------------------------------


def checkout_files(checkout: Path) -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(checkout), "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        check=True,
        capture_output=True,
    ).stdout
    seen: dict[str, None] = {}
    for raw in out.split(b"\0"):
        if raw:
            path = os.fsdecode(raw)
            # Tracked but deleted in the working tree: absent from a copy too.
            if os.path.lexists(checkout / path):
                seen.setdefault(path)
    return list(seen)


def inside(root: Path, candidate: str) -> bool:
    root_s = os.path.normpath(str(root))
    cand = os.path.normpath(candidate)
    return cand == root_s or cand.startswith(root_s + os.sep)


def remove_any(path: Path) -> None:
    if path.is_symlink() or not path.is_dir():
        path.unlink()
    else:
        shutil.rmtree(path)


def ensure_parent(slot: Path, rel: str) -> None:
    """Create the slot's parent directories of `rel` as real directories,
    replacing any symlink or file in the way so nothing is written through it."""
    current = slot
    for part in Path(rel).parent.parts:
        current = current / part
        if current.is_symlink() or (current.exists() and not current.is_dir()):
            current.unlink()
        if not current.exists():
            current.mkdir()


def same_bytes(a: Path, b: Path) -> bool:
    try:
        if a.stat().st_size != b.stat().st_size:
            return False
    except FileNotFoundError:
        return False
    with a.open("rb") as fa, b.open("rb") as fb:
        while True:
            ca, cb = fa.read(1 << 20), fb.read(1 << 20)
            if ca != cb:
                return False
            if not ca:
                return True


def sync(checkout: Path, slot: Path) -> dict[str, int]:
    checkout = checkout.resolve()
    slot.mkdir(parents=True, exist_ok=True)
    slot = slot.resolve()
    counts = dict.fromkeys(("written", "mode", "links", "removed", "unchanged"), 0)
    wanted = checkout_files(checkout)
    # Refuse every escaping symlink before touching the slot.
    for rel in wanted:
        src = checkout / rel
        if src.is_symlink():
            target = os.readlink(src)
            resolved = os.path.join(os.path.dirname(os.path.join(str(slot), rel)), target)
            if os.path.isabs(target) or not inside(slot, resolved):
                raise MirrorError(f"refusing symlink {rel} -> {target}: it resolves outside the slot")
    for rel in wanted:
        src, dst = checkout / rel, slot / rel
        ensure_parent(slot, rel)
        if src.is_symlink():
            target = os.readlink(src)
            if dst.is_symlink() and os.readlink(dst) == target:
                counts["unchanged"] += 1
                continue
            if os.path.lexists(dst):
                remove_any(dst)
            os.symlink(target, dst)
            counts["links"] += 1
            continue
        if not src.is_file():
            continue  # a submodule gitlink or other non-file: cargo-mutants copies none either
        mode = stat.S_IMODE(src.stat().st_mode)
        if dst.is_symlink() or (dst.exists() and not dst.is_file()):
            remove_any(dst)
        if dst.is_file() and same_bytes(src, dst):
            if stat.S_IMODE(dst.stat().st_mode) != mode:
                os.chmod(dst, mode)
                counts["mode"] += 1
            else:
                counts["unchanged"] += 1
            continue
        # A new inode via rename: never writes through a link, and gives a new mtime.
        fd, tmp = tempfile.mkstemp(dir=dst.parent, prefix=".mirror-")
        try:
            with os.fdopen(fd, "wb") as out, src.open("rb") as inp:
                shutil.copyfileobj(inp, out, 1 << 20)
            os.chmod(tmp, mode)
            os.replace(tmp, dst)
        except BaseException:
            Path(tmp).unlink(missing_ok=True)
            raise
        counts["written"] += 1
    keep = set(wanted)
    keep_dirs = {str(p) for rel in wanted for p in Path(rel).parents if str(p) != "."}
    for root, dirs, files in os.walk(slot, topdown=False):
        rel_root = os.path.relpath(root, slot)
        for name in files + [d for d in dirs if os.path.islink(os.path.join(root, d))]:
            rel = os.path.normpath(os.path.join(rel_root, name))
            if rel not in keep:
                os.unlink(os.path.join(root, name))
                counts["removed"] += 1
        for name in dirs:
            path = os.path.join(root, name)
            rel = os.path.normpath(os.path.join(rel_root, name))
            if not os.path.islink(path) and rel not in keep_dirs and not os.listdir(path):
                os.rmdir(path)
    return counts


# --- merge --------------------------------------------------------------------


def address_space_caught(report: Path) -> list[str]:
    """Caught mutants whose log shows an allocation failure."""
    outcomes = report / "outcomes.json"
    if not outcomes.exists():
        return []
    names = []
    for item in json.loads(outcomes.read_text(encoding="utf-8"))["outcomes"]:
        scenario = item["scenario"]
        if item.get("summary") != "CaughtMutant" or not isinstance(scenario, dict):
            continue
        log = report / (item.get("log_path") or "")
        if log.is_file() and ADDRESS_SPACE_PATTERNS.search(log.read_text(encoding="utf-8", errors="replace")):
            names.append(scenario["Mutant"]["name"])
    return names


def write_address_space_list(report: Path) -> int:
    names = address_space_caught(report)
    (report / ADDRESS_SPACE_LIST).write_text("".join(f"{n}\n" for n in names), encoding="utf-8")
    print(
        f"mutants-mirror: {len(names)} caught mutant(s) hit the per-test address-space limit"
        + (f"; triage each one in {report / ADDRESS_SPACE_LIST}" if names else "")
    )
    return len(names)


def merged_status(statuses: list[int], timeouts: int, missed: int) -> int:
    for status in statuses:
        if status not in RESULT_STATUSES:
            return status
    if timeouts or 3 in statuses:
        return 3
    if missed or 2 in statuses:
        return 2
    return 0


def rewrite(path: str | None, slot: str) -> str | None:
    if not path:
        return path
    head, _, tail = path.partition("/")
    return f"{head}/{slot}/{tail}" if head in ("log", "diff") and tail else path


def merge(checkout: Path, slots: list[tuple[int, Path]]) -> int:
    """`slots` is (cargo-mutants exit status, output dir) per slot, in slot order."""
    dest = checkout / "mutants.out"
    staging = Path(tempfile.mkdtemp(dir=checkout, prefix=".mutants.out.merge-"))
    try:
        merged: dict = {k: 0 for k in COUNTERS}
        merged["outcomes"] = []
        versions, starts, ends = set(), [], []
        mutants: list = []
        texts: dict[str, list[str]] = {}
        debug: list[str] = []
        for status, out_dir in slots:
            slot = out_dir.name
            report = out_dir / "mutants.out"
            if not report.is_dir():
                if status in RESULT_STATUSES:
                    raise MirrorError(f"{report} is missing although the slot exited {status}")
                # The slot failed before writing a report; its status decides the run.
                print(f"mutants-mirror: {slot} wrote no report (exit status {status})", file=sys.stderr)
                continue
            for sub in ("log", "diff"):
                if (report / sub).is_dir():
                    shutil.copytree(report / sub, staging / sub / slot)
            outcomes = report / "outcomes.json"
            if outcomes.exists():  # an empty shard writes none
                data = json.loads(outcomes.read_text(encoding="utf-8"))
                versions.add(data.get("cargo_mutants_version"))
                for key in COUNTERS:
                    merged[key] += int(data.get(key, 0))
                if data.get("start_time"):
                    starts.append(data["start_time"])
                if data.get("end_time"):
                    ends.append(data["end_time"])
                for item in data["outcomes"]:
                    item["log_path"] = rewrite(item.get("log_path"), slot)
                    item["diff_path"] = rewrite(item.get("diff_path"), slot)
                    merged["outcomes"].append(item)
            if (report / "mutants.json").exists():
                mutants += json.loads((report / "mutants.json").read_text(encoding="utf-8"))
            for txt in sorted(report.glob("*.txt")):
                if txt.name != ADDRESS_SPACE_LIST:
                    texts.setdefault(txt.name, []).append(txt.read_text(encoding="utf-8"))
            if (report / "debug.log").exists():
                debug.append(f"===== {slot} (exit status {status}) =====\n")
                debug.append((report / "debug.log").read_text(encoding="utf-8", errors="replace"))
        if len(versions) > 1:
            raise MirrorError(f"slots ran different cargo-mutants versions: {sorted(map(str, versions))}")
        if versions:
            merged["cargo_mutants_version"] = versions.pop()
        if starts:
            merged["start_time"] = min(starts)
        if ends:
            merged["end_time"] = max(ends)
        order = ["outcomes", *COUNTERS, "start_time", "end_time", "cargo_mutants_version"]
        merged = {k: merged[k] for k in order if k in merged}
        (staging / "outcomes.json").write_text(json.dumps(merged, indent=2) + "\n", encoding="utf-8")
        (staging / "mutants.json").write_text(json.dumps(mutants, indent=2) + "\n", encoding="utf-8")
        for name, parts in texts.items():
            (staging / name).write_text("".join(parts), encoding="utf-8")
        (staging / "debug.log").write_text("".join(debug), encoding="utf-8")
        old = checkout / "mutants.out.old"
        if dest.exists():
            if old.exists():
                shutil.rmtree(old)
            dest.rename(old)
        staging.rename(dest)
    except BaseException:
        shutil.rmtree(staging, ignore_errors=True)
        raise
    statuses = [status for status, _ in slots]
    print(
        f"mutants-mirror: merged {len(slots)} slot(s) into {dest}: {merged['total_mutants']} mutants, "
        f"{merged['missed']} missed, {merged['caught']} caught, {merged['timeout']} timeouts, {merged['unviable']} unviable"
    )
    write_address_space_list(dest)
    return merged_status(statuses, merged["timeout"], merged["missed"])


# --- self-test ----------------------------------------------------------------


def fixture_repo(path: Path) -> None:
    git = ["git", "-C", str(path)]
    subprocess.run(["git", "init", "-q", str(path)], check=True)
    for key, value in (("user.name", "t"), ("user.email", "t@example.invalid"), ("commit.gpgsign", "false")):
        subprocess.run([*git, "config", key, value], check=True)


def commit_all(path: Path) -> None:
    subprocess.run(["git", "-C", str(path), "add", "-A"], check=True)
    subprocess.run(["git", "-C", str(path), "commit", "-q", "-m", "fixture"], check=True)


def materialize(out_dir: Path, entry: dict, slot: str, log_text: dict[str, str]) -> None:
    report = out_dir / "mutants.out"
    report.mkdir(parents=True)
    for rel in entry["files"]:
        path = report / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        if rel == "outcomes.json":
            path.write_text(json.dumps(entry["outcomes"]), encoding="utf-8")
        elif rel == "mutants.json":
            path.write_text(json.dumps(entry["mutants"]), encoding="utf-8")
        elif rel.endswith(".txt"):
            path.write_text(entry["txt"][rel], encoding="utf-8")
        else:
            path.write_text(log_text.get(rel, f"{slot}:{rel}\n"), encoding="utf-8")


def self_test() -> None:
    # A hook-run self-test inherits the commit's git environment.
    for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE", "GIT_PREFIX", "GIT_OBJECT_DIRECTORY", "GIT_COMMON_DIR"):
        os.environ.pop(name, None)
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        co, slot = root / "checkout", root / "slot"
        co.mkdir()
        fixture_repo(co)
        (co / ".gitignore").write_text("ignored.txt\n")
        (co / "src").mkdir()
        (co / "src/lib.rs").write_text("fn a() {}\n")
        (co / "src/gone.rs").write_text("gone\n")
        (co / "run.sh").write_text("#!/bin/sh\n")
        os.chmod(co / "run.sh", 0o755)
        (co / "flip.sh").write_text("#!/bin/sh\n")
        (co / "rules").mkdir()
        (co / "rules/a.md").write_text("rule\n")
        os.symlink("rules", co / "rules-link")  # a directory symlink, like .claude/rules
        os.symlink("src/lib.rs", co / "lib-link")
        commit_all(co)
        (co / "src/gone.rs").unlink()  # deleted tracked file
        (co / "src/new.rs").write_text("untracked\n")  # untracked, not ignored
        (co / "ignored.txt").write_text("ignored\n")
        (co / "src/lib.rs").write_text("fn a() { /* uncommitted */ }\n")

        counts = sync(co, slot)
        assert (slot / "src/lib.rs").read_text() == "fn a() { /* uncommitted */ }\n"
        assert (slot / "src/new.rs").exists(), "untracked non-ignored file missing"
        assert not (slot / "ignored.txt").exists(), "ignored file copied"
        assert not (slot / "src/gone.rs").exists(), "deleted tracked file copied"
        assert not (slot / ".git").exists(), ".git copied"
        assert os.access(slot / "run.sh", os.X_OK) and not os.access(slot / "flip.sh", os.X_OK)
        assert (slot / "rules-link").is_symlink() and os.readlink(slot / "rules-link") == "rules"
        assert (slot / "lib-link").is_symlink() and os.readlink(slot / "lib-link") == "src/lib.rs"
        assert counts["links"] == 2, counts

        # Unchanged files keep their mtime; a changed file gets a new one.
        stamp = 1_000_000_000
        for rel in ("src/lib.rs", "src/new.rs"):
            os.utime(slot / rel, (stamp, stamp))
        (co / "src/new.rs").write_text("changed\n")
        sync(co, slot)
        assert (slot / "src/lib.rs").stat().st_mtime == stamp, "an unchanged file was rewritten"
        assert (slot / "src/new.rs").stat().st_mtime != stamp and (slot / "src/new.rs").read_text() == "changed\n"

        # Extra files go; a mutated file heals; a mode-only change applies without a rewrite.
        (slot / "extra.rs").write_text("stale\n")
        (slot / "stale-dir/deep").mkdir(parents=True)
        (slot / "stale-dir/deep/x").write_text("x\n")
        (slot / "src/lib.rs").write_text("fn a() { /* ~ changed by cargo-mutants ~ */ }\n")
        os.chmod(co / "flip.sh", 0o755)
        os.utime(slot / "flip.sh", (stamp, stamp))
        counts = sync(co, slot)
        assert not (slot / "extra.rs").exists() and not (slot / "stale-dir").exists()
        assert (slot / "src/lib.rs").read_text() == "fn a() { /* uncommitted */ }\n", "a mutated slot file was not restored"
        assert os.access(slot / "flip.sh", os.X_OK) and (slot / "flip.sh").stat().st_mtime == stamp
        assert counts["mode"] == 1 and counts["removed"] == 2, counts

        # Nothing is written through a symlink: a slot path that is a link to the
        # checkout where the checkout has a real file is replaced, not followed.
        (slot / "src/new.rs").unlink()
        os.symlink(co / "src/new.rs", slot / "src/new.rs")
        before = (co / "src/new.rs").read_text()
        (co / "src/new.rs").write_text("edited again\n")
        sync(co, slot)
        assert not (slot / "src/new.rs").is_symlink() and (slot / "src/new.rs").read_text() == "edited again\n"
        assert before == "changed\n"
        # A link whose target changed is re-pointed, and a real directory where a link belongs is replaced.
        shutil.rmtree(slot / "rules")
        (slot / "rules-link").unlink()
        (slot / "rules-link").mkdir()
        (slot / "rules-link/a.md").write_text("in a real dir\n")
        sync(co, slot)
        assert (slot / "rules-link").is_symlink() and (slot / "rules/a.md").read_text() == "rule\n"

        # A symlink resolving outside the slot is refused before anything is written.
        os.symlink("../../outside", co / "src/escape")
        (co / "src/lib.rs").write_text("fn a() { /* after the refusal */ }\n")
        try:
            sync(co, slot)
            raise AssertionError("an escaping symlink was mirrored")
        except MirrorError as refusal:
            assert "outside the slot" in str(refusal), refusal
        assert not os.path.lexists(slot / "src/escape")
        assert (slot / "src/lib.rs").read_text() == "fn a() { /* uncommitted */ }\n"
        (co / "src/escape").unlink()
        os.symlink("/etc/passwd", co / "src/abs")
        try:
            sync(co, slot)
            raise AssertionError("an absolute symlink was mirrored")
        except MirrorError:
            pass
        (co / "src/abs").unlink()

    # Merge, pinned to real cargo-mutants 27.0.0 output (two shards and an empty one).
    fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))["slots"]
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        co = root / "checkout"
        co.mkdir()
        (co / "mutants.out").mkdir()
        (co / "mutants.out/previous").write_text("previous report\n")
        (co / "mutants.out.old").mkdir()
        oom_log = "thread 'x' panicked\nmemory allocation of 123 bytes failed\n"
        slots = []
        for k, key in ((1, "s0"), (2, "s1"), (3, "empty")):
            out = co / "mutants.out.slots" / f"slot-{k}"
            logs = {"log/src__lib.rs_line_1_col_23.log": oom_log} if key == "s0" else {}
            if key == "s1":
                logs = {"log/src__lib.rs_line_3_col_37.log": "assertion failed\n"}
            materialize(out, fixture[key], f"slot-{k}", logs)
            slots.append(({"s0": 0, "s1": 2, "empty": 0}[key], out))
        # A slot that failed before writing a report: its status wins, the rest still merge.
        broken = co / "broken"
        broken.mkdir()
        (broken / "slot-9").mkdir()
        assert merge(broken, slots + [(4, broken / "slot-9")]) == 4
        assert (broken / "mutants.out/outcomes.json").is_file()
        status = merge(co, slots)
        merged = co / "mutants.out"
        assert status == 2, status
        assert (co / "mutants.out.old/previous").read_text() == "previous report\n", "previous report not rotated"
        data = json.loads((merged / "outcomes.json").read_text())
        names = [o["scenario"]["Mutant"]["name"] for o in data["outcomes"] if isinstance(o["scenario"], dict)]
        expected = [m["name"] for key in ("s0", "s1") for m in fixture[key]["mutants"]]
        assert sorted(names) == sorted(expected) and len(set(names)) == len(names) == 12, names
        assert [o["scenario"] for o in data["outcomes"]].count("Baseline") == 2
        for key in COUNTERS:
            assert data[key] == sum(fixture[s]["outcomes"][key] for s in ("s0", "s1")), key
        assert data["start_time"] == min(fixture[s]["outcomes"]["start_time"] for s in ("s0", "s1"))
        assert data["end_time"] == max(fixture[s]["outcomes"]["end_time"] for s in ("s0", "s1"))
        assert data["cargo_mutants_version"] == "27.0.0"
        for item in data["outcomes"]:
            assert (merged / item["log_path"]).is_file(), item["log_path"]
            assert item["diff_path"] is None or (merged / item["diff_path"]).is_file(), item["diff_path"]
        # The colliding names survive under per-slot directories.
        assert (merged / "log/slot-1/baseline.log").read_text() == "slot-1:log/baseline.log\n"
        assert (merged / "log/slot-2/baseline.log").read_text() == "slot-2:log/baseline.log\n"
        assert (merged / "log/slot-1/src__lib.rs_line_3_col_37.log").is_file()
        assert (merged / "log/slot-2/src__lib.rs_line_3_col_37.log").read_text() == "assertion failed\n"
        assert (merged / "diff/slot-2/src__lib.rs_line_3_col_37_001.diff").is_file()
        assert len(json.loads((merged / "mutants.json").read_text())) == 12
        assert (merged / "missed.txt").read_text() == fixture["s1"]["txt"]["missed.txt"]
        assert not (merged / "lock.json").exists()
        assert (merged / "debug.log").read_text().count("===== slot-") == 3
        assert (merged / ADDRESS_SPACE_LIST).read_text() == "src/lib.rs:1:23: replace one -> i32 with 0\n"
        assert not list(co.glob(".mutants.out.merge-*")), "merge staging left behind"

    # The address-space scan: every pattern is caught, a plain assertion is not.
    for text, hit in (
        ("memory allocation of 123 bytes failed\n", True),
        ("GLib-ERROR **: failed to allocate 123 bytes\n", True),
        ("failed to spawn thread: Os { code: 11 }\n", True),
        ("Cannot allocate memory (os error 12)\n", True),
        ("assertion `left == right` failed\nos error 120\n", False),
    ):
        with tempfile.TemporaryDirectory() as scratch:
            report = Path(scratch)
            (report / "log").mkdir()
            (report / "log/m.log").write_text(text)
            outcome = {"scenario": {"Mutant": {"name": "m"}}, "summary": "CaughtMutant", "log_path": "log/m.log"}
            (report / "outcomes.json").write_text(json.dumps({"outcomes": [outcome]}))
            assert address_space_caught(report) == (["m"] if hit else []), text

    # Exit-status precedence (cargo-mutants 27: timeouts over missed).
    assert merged_status([0, 0], 0, 0) == 0
    assert merged_status([2, 0], 1, 1) == 3
    assert merged_status([2, 3], 0, 0) == 3
    assert merged_status([0, 2], 0, 1) == 2
    assert merged_status([2, 4, 70], 0, 1) == 4
    assert merged_status([0, 1], 0, 0) == 1
    print("mutants-mirror self-test: ok")


def main(argv: list[str]) -> int:
    if argv == ["--self-test"]:
        self_test()
        return 0
    try:
        if len(argv) == 3 and argv[0] == "sync":
            counts = sync(Path(argv[1]), Path(argv[2]))
            print("mutants-mirror: synced " + ", ".join(f"{v} {k}" for k, v in counts.items()))
            return 0
        if len(argv) >= 3 and argv[0] == "merge":
            slots = []
            for spec in argv[2:]:
                status, _, path = spec.partition(":")
                slots.append((int(status), Path(path)))
            return merge(Path(argv[1]), slots)
        if len(argv) == 2 and argv[0] == "scan":
            write_address_space_list(Path(argv[1]))
            return 0
    except MirrorError as error:
        print(f"mutants-mirror: {error}", file=sys.stderr)
        return 70
    print(__doc__.split("\n\n")[1], file=sys.stderr)
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
