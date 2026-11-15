#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Prepare the system Flatpak installation of a flatpak-github-actions CI container.

The image `ghcr.io/flathub-infra/flatpak-github-actions` ships its GNOME SDK and
Platform preinstalled in the system installation `/var/lib/flatpak`, inside an
image layer. When Flathub publishes a newer ref, `flatpak-builder
--install-deps-from=flathub` updates it, and flatpak's update renames deployment
directories. On an overlayfs mount whose `redirect_dir` is off, renaming a
directory that lives in a lower layer fails with EXDEV, and the update fails
with `Invalid cross-device link` (followed, in flatpak-builder 1.4, by a
segfault). This helper, run by `.github/actions/flatpak-build` before
flatpak-builder:

  diagnose  prints the installation's storage facts and a rename probe;
  probe     runs the rename probe alone (exit 1 unless it returns ok);
  relocate  copies the installation (hardlinks kept) to $RUNNER_TEMP and
            bind-mounts the copy over /var/lib/flatpak, then re-probes and
            fails unless the probe returns ok.

Paths come only from the environment variables RUNNER_TEMP and
GITHUB_WORKSPACE (never from `${{ }}` contexts), and the copy is refused inside
the workspace, because the Flatpak manifest's `dir` source reads the whole
checkout. The image has no `mount` binary, so the bind mount is mount(2)
through ctypes; it persists across the job's later steps (each a separate
`docker exec` into the same container).

Relocation logs its wall time and `df -h` before and after but never fails
because it was slow: its 90 s budget (design D2) is judged from CI logs.
"""

from __future__ import annotations

import argparse
import ctypes
import errno
import os
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path


INSTALLATION = Path("/var/lib/flatpak")
COPY_NAME = "flatpak-system"
PROBE_SUFFIX = ".rename-probe"
RELOCATION_BUDGET_SECONDS = 90
REQUIRED_TOOLS = ("findmnt", "df", "cp")
MS_BIND = 4096
# Test-only hook: name an errno (for example EROFS) to make the post-relocation
# probe report that failure without renaming anything. It can only force a
# failure; `ok` and unknown names are rejected.
FORCE_PROBE_FAILURE_ENV = "CI_FLATPAK_FORCE_PROBE_FAILURE"


class HelperError(Exception):
    """A failure the step reports as one `::error::` line and exit status 1."""


class ProbeRestoreError(HelperError):
    """The probe renamed its target away and could not rename it back.

    Never swallowed, not even by the diagnostic probe: the installation is
    broken until the sibling is renamed back.
    """


# ---------------------------------------------------------------------------
# Pure helpers (self-tested)
# ---------------------------------------------------------------------------


def errno_name(code: int | None) -> str:
    """Name an errno the way the log reports it (`EXDEV`), or `errno <n>`."""
    if code is None:
        return "unknown error"
    return errno.errorcode.get(code, f"errno {code}")


def is_within(path: Path, root: Path) -> bool:
    """True when `path` is `root` or lies beneath it, after resolving symlinks."""
    resolved = path.resolve()
    base = root.resolve()
    return resolved == base or base in resolved.parents


@dataclass(frozen=True)
class RelocationPlan:
    """What `relocate` will do: `refuse`, `skip` or `relocate`, and why."""

    action: str
    reason: str
    destination: Path | None = None


def plan_relocation(fstype: str, runner_temp: str | None, workspace: str | None) -> RelocationPlan:
    """Decide the relocation from the installation's filesystem and the environment."""
    if not runner_temp:
        return RelocationPlan("refuse", "RUNNER_TEMP is not set")
    if not workspace:
        return RelocationPlan("refuse", "GITHUB_WORKSPACE is not set")
    destination = Path(runner_temp) / COPY_NAME
    if is_within(destination, Path(workspace)):
        return RelocationPlan(
            "refuse",
            f"destination {destination} lies inside GITHUB_WORKSPACE {workspace}; the "
            "manifest's `dir` source would read the copy as build input",
        )
    if fstype != "overlay":
        return RelocationPlan(
            "skip",
            f"{INSTALLATION} is on {fstype or 'an unknown filesystem'}, not overlay; "
            "its directories can be renamed in place",
        )
    return RelocationPlan(
        "relocate",
        f"{INSTALLATION} is on overlay, whose lower-layer directories may not be renamable",
        destination,
    )


def probe_target(installation: Path) -> Path:
    """Choose the directory the probe renames: the first runtime's deployed commit.

    The target is `runtime/<ref>/<arch>/<branch>/<commit>`, the kind of directory
    flatpak renames on update, chosen in sorted order and resolved through the
    branch's `active` link. It is always a real directory, never the `active`
    symlink: renaming a symlink is a lower-layer *file* rename, which overlay
    allows, and would report a false `ok`. A `<commit>.rename-probe` left by an
    interrupted probe is renamed back first.
    """
    runtime = installation / "runtime"
    if not runtime.is_dir():
        raise HelperError(f"{runtime} does not exist; nothing to probe")
    candidates: list[Path] = []
    for ref in sorted(p for p in runtime.iterdir() if p.is_dir()):
        for arch in sorted(p for p in ref.iterdir() if p.is_dir()):
            for branch in sorted(p for p in arch.iterdir() if p.is_dir()):
                candidates.append(branch)
    for branch in candidates:
        restore_interrupted_probe(branch)
        active = branch / "active"
        if active.is_symlink():
            commit = branch / os.readlink(active)
            if commit.is_dir() and not commit.is_symlink():
                return commit
        commits = sorted(
            p
            for p in branch.iterdir()
            if p.is_dir() and not p.is_symlink() and not p.name.endswith(PROBE_SUFFIX)
        )
        if commits:
            return commits[0]
    raise HelperError(f"no deployed runtime commit directory under {runtime} to probe")


def restore_interrupted_probe(branch: Path) -> None:
    """Rename back any `<commit>.rename-probe` whose `<commit>` is missing."""
    for leftover in sorted(branch.glob(f"*{PROBE_SUFFIX}")):
        original = leftover.with_name(leftover.name.removesuffix(PROBE_SUFFIX))
        if original.exists():
            raise HelperError(
                f"both {original} and {leftover} exist; a previous probe left the "
                "installation ambiguous"
            )
        print(f"rename-probe: restoring {leftover} -> {original} left by an interrupted probe")
        os.rename(leftover, original)


def forced_probe_failure() -> str | None:
    """The errno name the test-only hook forces, validated, or None."""
    value = os.environ.get(FORCE_PROBE_FAILURE_ENV)
    if not value:
        return None
    if value not in errno.errorcode.values():
        raise HelperError(
            f"{FORCE_PROBE_FAILURE_ENV}={value!r} is not an errno name; the hook can only "
            "force a failure"
        )
    return value


def rename_probe(installation: Path, *, allow_forced_failure: bool = False) -> tuple[Path, str]:
    """Rename the probe target to its sibling and back; return (target, result).

    The result is `ok` or the errno name of the first rename. If the first
    rename succeeds and the rename back fails, the installation would be left
    broken, so that raises at once naming both paths.
    """
    target = probe_target(installation)
    sibling = target.with_name(target.name + PROBE_SUFFIX)
    if allow_forced_failure:
        forced = forced_probe_failure()
        if forced is not None:
            return target, forced
    try:
        os.rename(target, sibling)
    except OSError as error:
        return target, errno_name(error.errno)
    try:
        os.rename(sibling, target)
    except OSError as error:
        raise ProbeRestoreError(
            f"rename-probe renamed {target} to {sibling} but could not rename it back "
            f"({errno_name(error.errno)}); the installation needs {sibling} renamed to "
            f"{target}"
        ) from error
    return target, "ok"


# ---------------------------------------------------------------------------
# Effects
# ---------------------------------------------------------------------------


def group(title: str) -> None:
    print(f"::group::{title}", flush=True)


def endgroup() -> None:
    print("::endgroup::", flush=True)


def require_tools() -> None:
    missing = [tool for tool in REQUIRED_TOOLS if shutil.which(tool) is None]
    if missing:
        raise HelperError(
            "missing tool(s) in the container image: " + ", ".join(missing)
        )


def run(argv: list[str], *, check: bool = False) -> str:
    """Run a command, echo it and its output, and return its stdout."""
    print(f"$ {' '.join(argv)}", flush=True)
    result = subprocess.run(argv, capture_output=True, text=True, check=False)
    output = (result.stdout + result.stderr).rstrip()
    if output:
        print(output, flush=True)
    if check and result.returncode != 0:
        raise HelperError(f"`{' '.join(argv)}` exited {result.returncode}")
    return result.stdout


def filesystem_of(path: Path) -> str:
    result = subprocess.run(
        ["findmnt", "-no", "FSTYPE", "-T", str(path)],
        capture_output=True,
        text=True,
        check=False,
    )
    return result.stdout.strip().splitlines()[-1] if result.stdout.strip() else ""


def describe_filesystem(path: Path) -> str:
    result = subprocess.run(
        ["findmnt", "-no", "FSTYPE,SOURCE,TARGET", "-T", str(path)],
        capture_output=True,
        text=True,
        check=False,
    )
    lines = result.stdout.strip().splitlines()
    return " ".join(lines[-1].split()) if lines else "unknown"


def env_path(name: str) -> str | None:
    value = os.environ.get(name)
    return value or None


def report_paths() -> list[str]:
    paths = [str(INSTALLATION)]
    runner_temp = env_path("RUNNER_TEMP")
    if runner_temp:
        paths.append(runner_temp)
    paths.append("/tmp")
    return paths


def print_probe(label: str, *, allow_forced_failure: bool = False) -> str:
    try:
        target, result = rename_probe(INSTALLATION, allow_forced_failure=allow_forced_failure)
    except ProbeRestoreError:
        raise
    except HelperError as error:
        print(f"{label} rename-probe: not run ({error})", flush=True)
        return "not run"
    print(f"{label} rename-probe target: {target}", flush=True)
    print(f"rename-probe: {result}", flush=True)
    return result


def diagnose() -> int:
    group("Flatpak installation diagnostics")
    try:
        require_tools()
        print(f"kernel: {os.uname().release}")
        redirect = Path("/sys/module/overlay/parameters/redirect_dir")
        try:
            print(f"overlay redirect_dir: {redirect.read_text(encoding='utf-8').strip()}")
        except OSError as error:
            print(f"overlay redirect_dir: unreadable ({errno_name(error.errno)})")
        image_version = os.environ.get("ImageVersion")
        print(
            "runner image: "
            + (image_version or "not visible in the container (see the job's Runner Image header)")
        )
        print("overlay mounts (/proc/mounts):")
        try:
            overlay = [
                line
                for line in Path("/proc/mounts").read_text(encoding="utf-8").splitlines()
                if line.split(" ", 3)[2:3] == ["overlay"]
            ]
        except OSError as error:
            overlay = [f"unreadable ({errno_name(error.errno)})"]
        for line in overlay or ["none"]:
            print(f"  {line}")
        for path in report_paths():
            run(["findmnt", "-T", path])
        run(["df", "-h", *report_paths()])
        print_probe("pre-relocation")
    except HelperError as error:
        endgroup()
        print(f"::error::diagnose: {error}", flush=True)
        return 1
    endgroup()
    return 0


def probe() -> int:
    group("Flatpak installation rename probe")
    try:
        target, result = rename_probe(INSTALLATION)
        print(f"rename-probe target: {target}")
        print(f"rename-probe: {result}")
    except HelperError as error:
        endgroup()
        print(f"::error::probe: {error}", flush=True)
        return 1
    endgroup()
    return 0 if result == "ok" else 1


def bind_mount(source: Path, target: Path) -> None:
    """mount(2) MS_BIND through libc; the image ships no `mount` binary."""
    libc = ctypes.CDLL("libc.so.6", use_errno=True)
    libc.mount.argtypes = (
        ctypes.c_char_p,
        ctypes.c_char_p,
        ctypes.c_char_p,
        ctypes.c_ulong,
        ctypes.c_void_p,
    )
    if libc.mount(os.fsencode(source), os.fsencode(target), None, MS_BIND, None) != 0:
        code = ctypes.get_errno()
        raise HelperError(
            f"mount(2) MS_BIND {source} -> {target} failed: {errno_name(code)} "
            f"({os.strerror(code)})"
        )


def relocate() -> int:
    group("Relocate the system Flatpak installation off overlayfs")
    try:
        require_tools()
        fstype = filesystem_of(INSTALLATION)
        plan = plan_relocation(fstype, env_path("RUNNER_TEMP"), env_path("GITHUB_WORKSPACE"))
        if plan.action == "refuse":
            raise HelperError(f"refusing to relocate: {plan.reason}")
        if plan.action == "skip":
            print(f"relocation skipped: {plan.reason}")
            endgroup()
            return 0
        assert plan.destination is not None
        destination = plan.destination
        print(f"relocating: {plan.reason}")
        if destination.exists():
            raise HelperError(f"destination {destination} already exists")
        run(["df", "-h", *report_paths()])
        started = time.monotonic()
        run(["cp", "-a", str(INSTALLATION), str(destination)], check=True)
        copied = time.monotonic()
        bind_mount(destination, INSTALLATION)
        mounted = time.monotonic()
        filesystem = describe_filesystem(INSTALLATION)
        print(f"{INSTALLATION} is now on: {filesystem}")
        result = print_probe("post-relocation", allow_forced_failure=True)
        finished = time.monotonic()
        if shutil.which("du"):
            run(["du", "-sh", str(destination)])
        run(["df", "-h", *report_paths()])
        print(
            f"relocation wall time: {finished - started:.1f} s "
            f"(copy {copied - started:.1f} s, mount {mounted - copied:.3f} s, "
            f"probe {finished - mounted:.3f} s; budget {RELOCATION_BUDGET_SECONDS} s, "
            "judged from CI logs, not enforced)"
        )
        if result != "ok":
            raise HelperError(
                f"post-relocation rename-probe returned {result}, not ok; {INSTALLATION} "
                f"is on {filesystem}. flatpak-builder would fail to update dependencies"
            )
        if filesystem.split(" ", 1)[0] == "overlay":
            raise HelperError(
                f"{INSTALLATION} is still on overlay after the bind mount ({filesystem})"
            )
    except HelperError as error:
        endgroup()
        print(f"::error::relocate: {error}", flush=True)
        return 1
    endgroup()
    return 0


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------


def run_self_test() -> None:
    """Exercise the pure helpers and the probe on a temporary (non-overlay) tree."""

    def check(condition: bool, message: str) -> None:
        if not condition:
            raise AssertionError(message)

    with tempfile.TemporaryDirectory(prefix="ci-flatpak-installation-") as tmp:
        root = Path(tmp)
        workspace = root / "work" / "lushtext"
        workspace.mkdir(parents=True)
        runner_temp = root / "_temp"
        runner_temp.mkdir()

        # Planning: off overlay is a skip; on overlay a relocation into RUNNER_TEMP.
        plan = plan_relocation("btrfs", str(runner_temp), str(workspace))
        check(plan.action == "skip" and "btrfs" in plan.reason, f"off-overlay plan: {plan}")
        plan = plan_relocation("overlay", str(runner_temp), str(workspace))
        check(
            plan.action == "relocate" and plan.destination == runner_temp / COPY_NAME,
            f"overlay plan: {plan}",
        )
        # A destination inside the workspace is refused, whatever the filesystem,
        # including through a symlink that resolves into it.
        inside = workspace / "_temp"
        for fstype in ("overlay", "ext4"):
            plan = plan_relocation(fstype, str(inside), str(workspace))
            check(
                plan.action == "refuse" and "inside GITHUB_WORKSPACE" in plan.reason,
                f"inside-workspace plan ({fstype}): {plan}",
            )
        link = root / "link-into-workspace"
        link.symlink_to(workspace)
        plan = plan_relocation("overlay", str(link), str(workspace))
        check(plan.action == "refuse", f"symlinked destination must be refused: {plan}")
        check(
            plan_relocation("overlay", None, str(workspace)).action == "refuse",
            "a missing RUNNER_TEMP must be refused",
        )
        check(
            plan_relocation("overlay", str(runner_temp), "").action == "refuse",
            "a missing GITHUB_WORKSPACE must be refused",
        )
        # A sibling sharing a name prefix is not inside the workspace.
        check(not is_within(root / "work" / "lushtext-other", workspace), "prefix sibling")

        # Errno formatting.
        check(errno_name(errno.EXDEV) == "EXDEV", "EXDEV name")
        check(errno_name(None) == "unknown error", "missing errno")
        check(errno_name(99999).startswith("errno "), "unknown errno")

        # Probe target: the active commit, a real directory, never the symlink.
        installation = root / "flatpak"
        branch_b = installation / "runtime" / "org.gnome.Sdk" / "x86_64" / "51"
        commit_b = branch_b / ("b" * 64)
        commit_b.mkdir(parents=True)
        (branch_b / ("0" * 64)).mkdir()
        (branch_b / "active").symlink_to("b" * 64)
        branch_a = installation / "runtime" / "org.freedesktop.Platform.GL.default" / "x86_64" / "26.08"
        commit_a = branch_a / ("a" * 64)
        commit_a.mkdir(parents=True)
        (commit_a / "files").mkdir()
        (branch_a / "active").symlink_to("a" * 64)
        target = probe_target(installation)
        check(target == commit_a, f"probe target should be the first runtime's active commit: {target}")
        check(not target.is_symlink(), "probe target must not be a symlink")
        # Without an `active` link, the first real commit directory is chosen.
        (branch_a / "active").unlink()
        check(probe_target(installation) == commit_a, "fallback commit choice")
        (branch_a / "active").symlink_to("a" * 64)

        # The probe renames and restores on a non-overlay temporary tree.
        target, result = rename_probe(installation)
        check(result == "ok", f"probe on a temporary directory: {result}")
        check(commit_a.is_dir() and (commit_a / "files").is_dir(), "probe must restore the target")
        check(
            not commit_a.with_name(commit_a.name + PROBE_SUFFIX).exists(),
            "probe must not leave its sibling",
        )

        # An interrupted probe is restored before probing again.
        os.rename(commit_a, commit_a.with_name(commit_a.name + PROBE_SUFFIX))
        target, result = rename_probe(installation)
        check(result == "ok" and commit_a.is_dir(), "interrupted probe must be restored")
        # Both names present is ambiguous and refused.
        commit_a.with_name(commit_a.name + PROBE_SUFFIX).mkdir()
        try:
            probe_target(installation)
        except HelperError as error:
            check("ambiguous" in str(error), f"ambiguous message: {error}")
        else:
            raise AssertionError("both a commit and its probe sibling must be refused")
        commit_a.with_name(commit_a.name + PROBE_SUFFIX).rmdir()

        # A rename back that fails raises at once, naming both paths, and is
        # not swallowed by the diagnostic probe's error handling.
        real_rename = os.rename
        calls: list[int] = []

        def failing_second_rename(src: object, dst: object) -> None:
            calls.append(1)
            if len(calls) == 2:
                raise OSError(errno.EROFS, "forced by the self-test")
            real_rename(src, dst)

        os.rename = failing_second_rename  # type: ignore[assignment]
        try:
            rename_probe(installation)
        except ProbeRestoreError as error:
            check(str(commit_a) in str(error) and PROBE_SUFFIX in str(error), f"restore error: {error}")
        else:
            raise AssertionError("a failed rename back must raise ProbeRestoreError")
        finally:
            os.rename = real_rename  # type: ignore[assignment]
        real_rename(commit_a.with_name(commit_a.name + PROBE_SUFFIX), commit_a)

        # A missing tool is named.
        saved_path = os.environ.get("PATH", "")
        os.environ["PATH"] = str(root / "no-tools")
        try:
            require_tools()
        except HelperError as error:
            check("findmnt" in str(error) and "cp" in str(error), f"missing-tool message: {error}")
        else:
            raise AssertionError("missing tools must fail")
        finally:
            os.environ["PATH"] = saved_path

        # An empty installation has nothing to probe.
        try:
            probe_target(root / "empty")
        except HelperError:
            pass
        else:
            raise AssertionError("an installation without runtimes must be refused")

        # The failure hook only forces failures, and only when asked for.
        saved = os.environ.pop(FORCE_PROBE_FAILURE_ENV, None)
        try:
            os.environ[FORCE_PROBE_FAILURE_ENV] = "EROFS"
            _, result = rename_probe(installation, allow_forced_failure=True)
            check(result == "EROFS", f"forced failure: {result}")
            _, result = rename_probe(installation)
            check(result == "ok", "the hook must not reach a probe that does not allow it")
            for value in ("ok", "not-an-errno"):
                os.environ[FORCE_PROBE_FAILURE_ENV] = value
                try:
                    rename_probe(installation, allow_forced_failure=True)
                except HelperError:
                    pass
                else:
                    raise AssertionError(f"the hook must reject {value!r}")
        finally:
            os.environ.pop(FORCE_PROBE_FAILURE_ENV, None)
            if saved is not None:
                os.environ[FORCE_PROBE_FAILURE_ENV] = saved
    print("ci-flatpak-installation self-test passed")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--self-test", action="store_true", help="run the built-in tests and exit")
    parser.add_argument("command", nargs="?", choices=("diagnose", "probe", "relocate"))
    args = parser.parse_args()
    if args.self_test:
        run_self_test()
        return 0
    if args.command is None:
        parser.error("a command is required (diagnose, probe or relocate)")
    return {"diagnose": diagnose, "probe": probe, "relocate": relocate}[args.command]()


if __name__ == "__main__":
    sys.exit(main())
