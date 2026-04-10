#!/usr/bin/env python3
from __future__ import annotations

import argparse
import pathlib
import re
import selectors
import shutil
import subprocess
import sys
import time
import urllib.parse


def run_checked(cmd: list[str], *, timeout: int | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        check=False,
        text=True,
        capture_output=True,
        timeout=timeout,
    )


def try_gnome_screenshot(output: pathlib.Path) -> tuple[bool, str]:
    binary = shutil.which("gnome-screenshot")
    if not binary:
        return False, "gnome-screenshot not installed"
    result = run_checked([binary, "-f", str(output)])
    if result.returncode == 0 and output.exists() and output.stat().st_size > 0:
        return True, f"saved via gnome-screenshot to {output}"
    return False, (result.stderr or result.stdout or "gnome-screenshot failed").strip()


def try_portal(output: pathlib.Path, timeout_seconds: int) -> tuple[bool, str]:
    if not shutil.which("gdbus"):
        return False, "gdbus not installed"

    call = run_checked(
        [
            "gdbus",
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Screenshot.Screenshot",
            "",
            "{}",
        ]
    )
    if call.returncode != 0:
        return False, (call.stderr or call.stdout or "portal call failed").strip()

    match = re.search(r"\(objectpath '([^']+)'\s*,?\)", call.stdout)
    if not match:
        return False, f"could not parse portal handle from: {call.stdout.strip()}"
    handle = match.group(1)

    monitor = subprocess.Popen(
        [
            "gdbus",
            "monitor",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            handle,
        ],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )

    deadline = time.time() + timeout_seconds
    captured: list[str] = []
    selector = selectors.DefaultSelector()
    if monitor.stdout is None:
        monitor.kill()
        return False, "portal monitor stdout was unavailable"
    selector.register(monitor.stdout, selectors.EVENT_READ)
    try:
        while time.time() < deadline:
            events = selector.select(timeout=0.25)
            for key, _ in events:
                line = key.fileobj.readline()
                if not line:
                    continue
                captured.append(line)
                joined = "".join(captured)
                uri_match = re.search(r"uri': <'([^']+)'>", joined)
                if uri_match:
                    uri = urllib.parse.unquote(uri_match.group(1))
                    if uri.startswith("file://"):
                        source = pathlib.Path(urllib.parse.urlparse(uri).path)
                        if source.exists():
                            shutil.copy2(source, output)
                            return True, f"saved via portal to {output}"
                if "org.freedesktop.portal.Request.Response" in joined:
                    return False, f"portal responded but no file URI was found: {joined.strip()}"
    finally:
        selector.close()
        monitor.kill()
        try:
            monitor.wait(timeout=2)
        except subprocess.TimeoutExpired:
            monitor.terminate()
    joined = "".join(captured).strip()
    if joined:
        return False, f"portal timed out or required approval: {joined}"
    return False, "portal timed out or required approval"


def main() -> int:
    parser = argparse.ArgumentParser(description="Capture a screenshot for GTK debugging.")
    parser.add_argument("output", type=pathlib.Path, help="Destination file path")
    parser.add_argument("--timeout", type=int, default=10, help="Portal wait timeout in seconds")
    args = parser.parse_args()

    args.output.parent.mkdir(parents=True, exist_ok=True)
    if not args.output.suffix:
        args.output = args.output.with_suffix(".png")

    ok, message = try_gnome_screenshot(args.output)
    if ok:
        print(message)
        return 0

    ok, portal_message = try_portal(args.output, args.timeout)
    if ok:
        print(portal_message)
        return 0

    print("screenshot capture failed", file=sys.stderr)
    print(f"- gnome-screenshot: {message}", file=sys.stderr)
    print(f"- portal: {portal_message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
