#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Self-tests for visual geometry manifests and PNG comparisons."""

from __future__ import annotations

import importlib.util
import json
import tempfile
from pathlib import Path

from visual_geometry_png import PngImage, Rect, clamp_rect, compare_crops, read_png, write_png


REPO_ROOT = Path(__file__).resolve().parents[1]


def load_runner():
    path = REPO_ROOT / "scripts/visual-geometry-smoke.py"
    spec = importlib.util.spec_from_file_location("visual_geometry_smoke", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def rgba_image(rows: list[list[tuple[int, int, int, int]]]) -> tuple[PngImage, list[bytes]]:
    encoded = [
        b"".join(bytes(pixel) for pixel in row)
        for row in rows
    ]
    return (
        PngImage(
            width=len(rows[0]),
            height=len(rows),
            bit_depth=8,
            color_type=6,
            bpp=4,
            rows=encoded,
        ),
        encoded,
    )


def write_rgba(path: Path, rows: list[list[tuple[int, int, int, int]]]) -> None:
    image, encoded_rows = rgba_image(rows)
    write_png(path, image, encoded_rows)


def test_png_exact_and_masked_comparison(tmp: Path) -> None:
    before = tmp / "before.png"
    after = tmp / "after.png"
    black = (0, 0, 0, 255)
    red = (255, 0, 0, 255)
    write_rgba(before, [[black, black], [black, black]])
    write_rgba(after, [[black, red], [black, black]])

    failed = compare_crops(before, after, Rect(0, 0, 2, 2))
    assert failed["status"] == "failed"
    assert failed["diff_pixels"] == 1

    masked = compare_crops(before, after, Rect(0, 0, 2, 2), masks=[Rect(1, 0, 1, 1)])
    assert masked["status"] == "passed"
    assert masked["diff_pixels"] == 0

    clamped = clamp_rect(read_png(before), Rect(-1, 0, 4, 2))
    assert clamped == Rect(0, 0, 2, 2)


def test_manifest_parsing_and_missing_manifest_failure(tmp: Path) -> None:
    runner = load_runner()
    scenario_dir = tmp / "scenarios"
    scenario_dir.mkdir()
    manifest = {
        "schema_version": 1,
        "scenario_id": "self-test",
        "scenario_type": "command-palette-overlay",
        "matrix": {"sizes": [{"id": "tiny", "width": 400, "height": 300}], "color_schemes": ["force-light"]},
        "protected_regions": [{"name": "header", "surface": "header-bar"}],
    }
    (scenario_dir / "self-test.json").write_text(json.dumps(manifest), encoding="utf-8")
    loaded = runner.load_manifests(scenario_dir)
    assert loaded[0]["scenario_id"] == "self-test"
    assert runner.expand_cases(loaded[0])[0]["case_id"] == "self-test--tiny--force-light"

    empty_dir = tmp / "empty"
    empty_dir.mkdir()
    try:
        runner.load_manifests(empty_dir)
    except RuntimeError as exc:
        assert "no visual geometry scenario manifests" in str(exc)
    else:
        raise AssertionError("missing manifests should fail")


def test_skip_summary_shape(tmp: Path) -> None:
    runner = load_runner()
    runner.write_skip_summary(tmp, "missing compositor")
    payload = json.loads((tmp / "summary.json").read_text(encoding="utf-8"))
    assert payload["status"] == "skipped"
    assert payload["skip_reason"] == "missing compositor"
    assert payload["case_count"] == 0


def main() -> int:
    with tempfile.TemporaryDirectory() as directory:
        tmp = Path(directory)
        test_png_exact_and_masked_comparison(tmp)
        test_manifest_parsing_and_missing_manifest_failure(tmp)
        test_skip_summary_shape(tmp)
    print("PASS: visual geometry script self-tests")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
