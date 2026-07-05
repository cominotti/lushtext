#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Live typing smoke for active-line editor glyph clipping."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
APP_ID = "dev.cominotti.lushtext"
REQUIRED_COMMANDS = (
    "dbus-run-session",
    "gsettings",
    "magick",
    "xdotool",
    "xvfb-run",
    "xwd",
)
# Calibrated geometry for the bracket-clipping repro. The fixed 1200x760 window
# and crop keep the first active editor line stable under Xvfb/cairo; the dark
# threshold and horizontal-run floor distinguish Adwaita Mono bracket caps from
# background and caret noise without requiring exact glyph matching.
WINDOW_SIZE = "1200x760"
EDITOR_FOCUS_CLICKS = ((390, 116), (390, 150))
WINDOW_CROP = (350, 86, 300, 72)
DARK_PIXEL_THRESHOLD = 150
MIN_TOP_HORIZONTAL_RUN = 4
# The threshold image is diagnostic only; pass/fail uses raw grayscale pixels.
ARTIFACT_THRESHOLD_PERCENT = "58%"
# Treat the top 45% of detected glyph ink as the bracket cap band. This catches
# missing upper strokes while tolerating font anti-aliasing and cursor position.
TOP_INK_BAND_RATIO = 0.45
# Bound the outer Xvfb/D-Bus wrapper too, because inner subprocess timeouts do
# not fire if session startup itself wedges before the Python child runs.
OUTER_TIMEOUT_SECONDS = 60


@dataclass
class CropAnalysis:
    """Pixel evidence for one active-line screenshot crop, mirrored into summary.json."""

    path: str
    crop_path: str
    threshold_path: str
    dark_pixels: int
    bbox: list[int] | None
    max_horizontal_run: int
    top_half_horizontal_run: int
    passed: bool


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-dir", type=Path, default=Path("build/smoke/editor-glyph-live"))
    parser.add_argument("--binary", type=Path, default=REPO_ROOT / "target/debug/lushtext")
    parser.add_argument("--internal-run", action="store_true", help=argparse.SUPPRESS)
    return parser.parse_args()


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def run(
    command: list[str],
    *,
    env: dict[str, str] | None = None,
    cwd: Path = REPO_ROOT,
    timeout: float = 10.0,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )
    if check and result.returncode != 0:
        quoted = " ".join(command)
        raise RuntimeError(
            f"`{quoted}` failed with status {result.returncode}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def require_tooling(binary: Path) -> list[str]:
    missing = [
        f"missing required command: {command}"
        for command in REQUIRED_COMMANDS
        if shutil.which(command) is None
    ]
    if not binary.is_file() or not os.access(binary, os.X_OK):
        missing.append(f"LushText debug binary is missing or not executable: {binary}")
    return missing


def reset_artifact_dir(artifact_dir: Path) -> None:
    """Replace the artifact directory after rejecting dangerous broad paths."""

    resolved = artifact_dir.resolve()
    forbidden = {Path("/"), Path.home().resolve(), REPO_ROOT.resolve(), REPO_ROOT.parent.resolve()}
    if resolved in forbidden:
        raise RuntimeError(f"refusing to reset unsafe artifact dir: {resolved}")
    if artifact_dir.exists():
        shutil.rmtree(artifact_dir)
    artifact_dir.mkdir(parents=True, exist_ok=True)


def outer_run(args: argparse.Namespace) -> int:
    binary = args.binary.resolve()
    missing_capabilities = require_tooling(binary)
    reset_artifact_dir(args.artifact_dir)
    if missing_capabilities:
        skip_reason = "; ".join(missing_capabilities)
        (args.artifact_dir / "summary.json").write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "status": "unsupported-host",
                    "skip_reason": skip_reason,
                    "missing_capabilities": missing_capabilities,
                    "lane": "editor-glyph-live-smoke",
                    "created_at": now_iso(),
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        print(f"SKIP: editor glyph live smoke unsupported: {skip_reason}")
        return 0

    command = [
        "xvfb-run",
        "-a",
        "-s",
        f"-screen 0 {WINDOW_SIZE}x24 -nolisten tcp",
        "dbus-run-session",
        "--",
        sys.executable,
        str(Path(__file__).resolve()),
        "--internal-run",
        "--artifact-dir",
        str(args.artifact_dir.resolve()),
        "--binary",
        str(binary),
    ]
    process = subprocess.Popen(command, cwd=REPO_ROOT, text=True, start_new_session=True)
    try:
        return process.wait(timeout=OUTER_TIMEOUT_SECONDS)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=5)
        (args.artifact_dir / "summary.json").write_text(
            json.dumps(
                {
                    "status": "error",
                    "schema_version": 1,
                    "failure_reason": f"outer smoke wrapper timed out after {OUTER_TIMEOUT_SECONDS}s",
                    "lane": "editor-glyph-live-smoke",
                    "created_at": now_iso(),
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        print(
            f"editor glyph live smoke timed out after {OUTER_TIMEOUT_SECONDS}s",
            file=sys.stderr,
        )
        return 1


def smoke_env(runtime_root: Path) -> dict[str, str]:
    """Build an isolated GTK runtime so the smoke never reads the user's state."""

    env = os.environ.copy()
    # X11/Xvfb gives deterministic window capture with xwd; cairo keeps pixels
    # stable across hosts; keyfile GSettings points the app at temporary XDG
    # directories instead of the user's real preferences and session data.
    env.update(
        {
            "GDK_BACKEND": "x11",
            "GSK_RENDERER": "cairo",
            "GSETTINGS_BACKEND": "keyfile",
            "GSETTINGS_SCHEMA_DIR": str(REPO_ROOT / "data"),
            "HOME": str(runtime_root / "home"),
            "NO_AT_BRIDGE": "1",
            "XDG_CACHE_HOME": str(runtime_root / "cache"),
            "XDG_CONFIG_HOME": str(runtime_root / "config"),
            "XDG_DATA_HOME": str(runtime_root / "data"),
            "XDG_STATE_HOME": str(runtime_root / "state"),
        }
    )
    for key in ("cache", "config", "data", "home", "state"):
        (runtime_root / key).mkdir(parents=True, exist_ok=True)
    return env


def set_gsettings(env: dict[str, str]) -> None:
    def configured(key: str, default: str) -> str:
        env_key = f"LUSHTEXT_EDITOR_GLYPH_{key.upper().replace('-', '_')}"
        return os.environ.get(env_key, default)

    # String values include GVariant CLI quotes because this map is passed
    # directly to `gsettings set`.
    settings = {
        "bookmark-gutter-visible": "false",
        "color-scheme": configured("color-scheme", "'force-light'"),
        "custom-font": configured("custom-font", "'Adwaita Mono 11'"),
        "highlight-current-line": configured("highlight-current-line", "true"),
        "properties-sidebar-visible": "false",
        "show-line-numbers": "false",
        "show-minimap": "false",
        "style-scheme": configured("style-scheme", "'Adwaita'"),
        "tab-content-opacity": configured("tab-content-opacity", "1.0"),
        "use-system-font": configured("use-system-font", "false"),
        "window-height": "760",
        "window-maximized": "false",
        "window-width": "1200",
        "word-wrap": "false",
        "workspace-sidebar-visible": "true",
        "workspace-sidebar-width-fraction": "0.3",
        "zoom-level": "100",
    }
    for key, value in settings.items():
        run(["gsettings", "set", APP_ID, key, value], env=env)


def wait_for_window(app: subprocess.Popen[str], env: dict[str, str]) -> str:
    deadline = time.monotonic() + 10
    last_error = ""
    while time.monotonic() < deadline:
        for command in (
            ["xdotool", "search", "--onlyvisible", "--pid", str(app.pid)],
            ["xdotool", "search", "--onlyvisible", "--name", "LushText"],
        ):
            result = run(command, env=env, timeout=2.0, check=False)
            if result.returncode == 0:
                window_ids = [line.strip() for line in result.stdout.splitlines() if line.strip()]
                if window_ids:
                    return window_ids[-1]
            last_error = result.stderr.strip()
        if app.poll() is not None:
            raise RuntimeError(f"LushText exited before a window appeared: {app.returncode}")
        time.sleep(0.1)
    raise RuntimeError(f"timed out waiting for LushText window: {last_error}")


def capture_window(window_id: str, output_png: Path, env: dict[str, str]) -> None:
    xwd_path = output_png.with_suffix(".xwd")
    run(["xwd", "-silent", "-id", window_id, "-out", str(xwd_path)], env=env)
    run(["magick", str(xwd_path), str(output_png)], env=env)
    xwd_path.unlink(missing_ok=True)


def grayscale_crop_bytes(png_path: Path, crop: tuple[int, int, int, int], env: dict[str, str]) -> bytes:
    x, y, width, height = crop
    result = subprocess.run(
        [
            "magick",
            str(png_path),
            "-crop",
            f"{width}x{height}+{x}+{y}",
            "+repage",
            "-colorspace",
            "Gray",
            "-depth",
            "8",
            "gray:-",
        ],
        env=env,
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=10.0,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"failed to read grayscale crop for {png_path}: {result.stderr.decode('utf-8', 'replace')}"
        )
    expected = width * height
    if len(result.stdout) != expected:
        raise RuntimeError(
            f"unexpected grayscale crop length for {png_path}: {len(result.stdout)} != {expected}"
        )
    return result.stdout


def row_max_run(raw: bytes, width: int, row: int) -> int:
    best = 0
    current = 0
    offset = row * width
    for column in range(width):
        if raw[offset + column] < DARK_PIXEL_THRESHOLD:
            current += 1
            best = max(best, current)
        else:
            current = 0
    return best


def analyze_capture(
    png_path: Path,
    name: str,
    artifact_dir: Path,
    env: dict[str, str],
) -> CropAnalysis:
    """Write crop artifacts and decide whether upper bracket ink is present."""

    x, y, width, height = WINDOW_CROP
    crop_path = artifact_dir / f"{name}-active-line-crop.png"
    threshold_path = artifact_dir / f"{name}-active-line-threshold.png"
    run(["magick", str(png_path), "-crop", f"{width}x{height}+{x}+{y}", "+repage", str(crop_path)], env=env)
    run(
        [
            "magick",
            str(crop_path),
            "-colorspace",
            "Gray",
            "-threshold",
            ARTIFACT_THRESHOLD_PERCENT,
            str(threshold_path),
        ],
        env=env,
    )

    raw = grayscale_crop_bytes(png_path, WINDOW_CROP, env)
    dark_points = [
        (column, row)
        for row in range(height)
        for column in range(width)
        if raw[row * width + column] < DARK_PIXEL_THRESHOLD
    ]
    if not dark_points:
        return CropAnalysis(
            path=str(png_path),
            crop_path=str(crop_path),
            threshold_path=str(threshold_path),
            dark_pixels=0,
            bbox=None,
            max_horizontal_run=0,
            top_half_horizontal_run=0,
            passed=False,
        )

    min_x = min(point[0] for point in dark_points)
    max_x = max(point[0] for point in dark_points)
    min_y = min(point[1] for point in dark_points)
    max_y = max(point[1] for point in dark_points)
    top_limit = min(max_y, min_y + max(1, int((max_y - min_y + 1) * TOP_INK_BAND_RATIO)))
    row_runs = [row_max_run(raw, width, row) for row in range(height)]
    max_horizontal_run = max(row_runs)
    top_half_horizontal_run = max(row_runs[min_y : top_limit + 1])
    return CropAnalysis(
        path=str(png_path),
        crop_path=str(crop_path),
        threshold_path=str(threshold_path),
        dark_pixels=len(dark_points),
        bbox=[min_x, min_y, max_x, max_y],
        max_horizontal_run=max_horizontal_run,
        top_half_horizontal_run=top_half_horizontal_run,
        passed=top_half_horizontal_run >= MIN_TOP_HORIZONTAL_RUN,
    )


def terminate(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    process.send_signal(signal.SIGTERM)
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def inner_run(args: argparse.Namespace) -> int:
    """Run the isolated GUI session and write before/after typing artifacts."""

    args.artifact_dir.mkdir(parents=True, exist_ok=True)
    runtime_root = Path(tempfile.mkdtemp(prefix="lushtext-editor-glyph."))
    env = smoke_env(runtime_root)
    fixture = runtime_root / "glyph-live-smoke.txt"
    fixture.write_text("", encoding="utf-8")
    app_log = (args.artifact_dir / "lushtext.log").open("w", encoding="utf-8")
    app: subprocess.Popen[str] | None = None
    summary: dict[str, object] = {
        "schema_version": 1,
        "created_at": now_iso(),
        "lane": "editor-glyph-live-smoke",
        "crop": {"x": WINDOW_CROP[0], "y": WINDOW_CROP[1], "width": WINDOW_CROP[2], "height": WINDOW_CROP[3]},
        "min_top_horizontal_run": MIN_TOP_HORIZONTAL_RUN,
        "typed_text": "[[[[[[[[",
    }
    try:
        set_gsettings(env)
        app = subprocess.Popen(
            [str(args.binary), str(fixture)],
            cwd=REPO_ROOT,
            env=env,
            text=True,
            stdout=app_log,
            stderr=subprocess.STDOUT,
        )
        window_id = wait_for_window(app, env)
        summary["window_id"] = window_id
        # Xvfb has no window manager, so _NET_ACTIVE_WINDOW may be unsupported.
        # Direct focus plus editor-body clicks are the real input path.
        run(["xdotool", "windowactivate", "--sync", window_id], env=env, check=False)
        run(["xdotool", "windowfocus", "--sync", window_id], env=env)
        for click_x, click_y in EDITOR_FOCUS_CLICKS:
            run(
                [
                    "xdotool",
                    "mousemove",
                    "--window",
                    window_id,
                    str(click_x),
                    str(click_y),
                    "click",
                    "1",
                ],
                env=env,
            )
        run(["xdotool", "type", "--clearmodifiers", "--delay", "35", "[[[[[[[["], env=env)
        time.sleep(0.25)
        before_png = args.artifact_dir / "typed-before-enter.png"
        capture_window(window_id, before_png, env)
        run(["xdotool", "key", "Return"], env=env)
        time.sleep(0.25)
        after_png = args.artifact_dir / "typed-after-enter.png"
        capture_window(window_id, after_png, env)

        before = analyze_capture(before_png, "typed-before-enter", args.artifact_dir, env)
        after = analyze_capture(after_png, "typed-after-enter", args.artifact_dir, env)
        summary["before_enter"] = asdict(before)
        summary["after_enter"] = asdict(after)
        # The pre-Enter active line is the bug oracle; the post-Enter assertion
        # keeps the detector honest by proving the same crop sees full glyph ink
        # after GTK performs the redraw that used to mask the defect.
        passed = before.passed and after.passed
        summary["status"] = "passed" if passed else "failed"
        if not passed:
            summary["reason"] = (
                "active line lacks top horizontal bracket strokes before Enter"
                if not before.passed
                else "post-Enter reference crop did not expose full bracket strokes"
            )
        (args.artifact_dir / "summary.json").write_text(
            json.dumps(summary, indent=2) + "\n", encoding="utf-8"
        )
        if passed:
            print("editor glyph live smoke passed")
            return 0
        print(f"editor glyph live smoke failed; see {args.artifact_dir}", file=sys.stderr)
        return 1
    except Exception as error:
        summary["status"] = "error"
        summary["failure_reason"] = str(error)
        (args.artifact_dir / "summary.json").write_text(
            json.dumps(summary, indent=2) + "\n", encoding="utf-8"
        )
        print(f"editor glyph live smoke error: {error}", file=sys.stderr)
        return 1
    finally:
        if app is not None:
            terminate(app)
        app_log.close()
        shutil.rmtree(runtime_root, ignore_errors=True)


def main() -> int:
    args = parse_args()
    if args.internal_run:
        return inner_run(args)
    return outer_run(args)


if __name__ == "__main__":
    sys.exit(main())
