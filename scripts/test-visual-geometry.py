#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Self-tests for visual geometry manifests and PNG comparisons."""

from __future__ import annotations

import importlib.util
import json
import tempfile
from pathlib import Path

from visual_geometry_png import (
    PngImage,
    Rect,
    clamp_rect,
    compare_crops,
    detect_pixel_anchor,
    read_png,
    write_png,
)


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

    malformed = {**manifest, "pixel_anchors": [{"name": "edge"}]}
    (scenario_dir / "malformed.json").write_text(json.dumps(malformed), encoding="utf-8")
    try:
        runner.load_manifests(scenario_dir)
    except RuntimeError as exc:
        assert "declares pixel_anchors without invariant_id" in str(exc)
    else:
        raise AssertionError("pixel anchors without invariant_id should fail")

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


def test_reported_minimap_native_highlight_fixture_regression(tmp: Path) -> None:
    good = REPO_ROOT / "build/diagnostics/minimap-viewport-pixel-anchors/ok-workspace-sidebar.png"
    bad = REPO_ROOT / "build/diagnostics/minimap-viewport-pixel-anchors/issue-no-sidebar.png"
    if not good.is_file() or not bad.is_file():
        # Prefer captured regression fixtures when available, but synthesize the
        # same color signature so self-tests stay hermetic in clean checkouts.
        good = tmp / "reported-good-signature.png"
        bad = tmp / "reported-bad-signature.png"
        bg = (29, 29, 32, 255)
        fill = (61, 61, 63, 255)
        edge = (150, 150, 151, 255)
        cyan = (51, 178, 164, 255)
        good_rows = [[bg for _ in range(40)] for _ in range(12)]
        bad_rows = [[bg for _ in range(40)] for _ in range(12)]
        for x in range(8, 21):
            good_rows[3][x] = edge
        for x in range(8, 21):
            bad_rows[3][x] = fill
        for x in range(8, 29):
            good_rows[4][x] = fill
            bad_rows[4][x] = fill
        for x in range(4, 24):
            good_rows[6][x] = cyan
            bad_rows[6][x] = cyan
        write_rgba(good, good_rows)
        write_rgba(bad, bad_rows)
        crop = Rect(0, 0, 40, 12)
    else:
        crop = Rect(1580, 90, 158, 30)

    good_image = read_png(good)
    bad_image = read_png(bad)
    good_edge = detect_pixel_anchor(
        good_image,
        "minimap-native-viewport-top-edge",
        crop,
        "native-minimap-viewport-top-edge-row",
        12,
    )
    good_content = detect_pixel_anchor(
        good_image,
        "minimap-first-content-row",
        crop,
        "minimap-content-row",
        20,
    )
    bad_edge = detect_pixel_anchor(
        bad_image,
        "minimap-native-viewport-top-edge",
        crop,
        "native-minimap-viewport-top-edge-row",
        12,
    )
    bad_content = detect_pixel_anchor(
        bad_image,
        "minimap-first-content-row",
        crop,
        "minimap-content-row",
        20,
    )

    assert good_edge.status == "passed"
    assert good_content.status == "passed"
    assert good_edge.row_y is not None and good_content.row_y is not None
    assert good_edge.row_y - good_content.row_y < 0
    assert bad_content.status == "passed"
    assert bad_edge.status != "passed" or bad_edge.row_y != good_edge.row_y


def test_pixel_anchor_detectors_and_drift_regression(tmp: Path) -> None:
    runner = load_runner()
    before = tmp / "before-anchors.png"
    after = tmp / "after-anchors.png"
    bg = (29, 29, 32, 255)
    edge = (78, 78, 78, 255)
    fill = (60, 60, 63, 255)
    cyan = (0, 190, 180, 255)
    orange = (245, 132, 28, 255)

    before_rows = [[bg for _ in range(12)] for _ in range(10)]
    after_rows = [[bg for _ in range(12)] for _ in range(10)]
    for x in range(2, 10):
        before_rows[1][x] = edge
        after_rows[2][x] = edge
    for x in range(2, 10):
        before_rows[3][x] = fill
        after_rows[4][x] = fill
    for x in range(2, 10):
        before_rows[6][x] = edge
        after_rows[7][x] = edge
    for x in range(2, 7):
        before_rows[8][x] = cyan
        after_rows[8][x] = cyan
    for x in range(8, 11):
        before_rows[9][x] = orange
        after_rows[9][x] = orange
    write_rgba(before, before_rows)
    write_rgba(after, after_rows)

    image = read_png(before)
    native_edge_detection = detect_pixel_anchor(
        image,
        "minimap-native-viewport-top-edge",
        Rect(2, 0, 8, 4),
        "native-minimap-viewport-top-edge-row",
        6,
    )
    edge_detection = detect_pixel_anchor(
        image, "legacy-neutral-edge", Rect(2, 0, 8, 4), "horizontal-neutral-edge-row", 6
    )
    content_detection = detect_pixel_anchor(
        image, "minimap-first-content-row", Rect(2, 7, 8, 3), "minimap-content-row", 4
    )
    fill_detection = detect_pixel_anchor(
        image, "minimap-viewport-fill", Rect(2, 2, 8, 3), "viewport-highlight-fill-row", 6
    )
    bottom_edge_detection = detect_pixel_anchor(
        image, "minimap-viewport-bottom-edge", Rect(2, 5, 8, 3), "horizontal-neutral-edge-row", 6
    )
    generic_detection = detect_pixel_anchor(
        image, "generic-row", Rect(0, 8, 12, 1), "non-background-row", 4
    )
    search_marker_detection = detect_pixel_anchor(
        image, "minimap-search-marker-row", Rect(0, 8, 12, 2), "minimap-search-marker-row", 2
    )
    assert native_edge_detection.status == "passed"
    assert native_edge_detection.row_y == 1
    assert edge_detection.status == "passed"
    assert content_detection.status == "passed"
    assert content_detection.row_y == 8
    assert fill_detection.status == "passed"
    assert fill_detection.row_y == 3
    assert bottom_edge_detection.status == "passed"
    assert bottom_edge_detection.row_y == 6
    assert generic_detection.status == "passed"
    assert generic_detection.row_y == 8
    assert search_marker_detection.status == "passed"
    assert search_marker_detection.row_y == 9
    try:
        detect_pixel_anchor(image, "unknown", Rect(0, 0, 2, 2), "unknown-detector", 1)
    except ValueError as exc:
        assert "unsupported pixel anchor detector" in str(exc)
    else:
        raise AssertionError("unsupported detector should fail")

    unchanged_header = compare_crops(before, after, Rect(0, 0, 2, 1))
    assert unchanged_header["status"] == "passed"

    manifest = {
        "invariant_id": "native-minimap-highlight-anchors",
        "pixel_anchors": [
            {
                "name": "minimap-native-viewport-top-edge",
                "crop_surface": "minimap-shell",
                "detector": "native-minimap-viewport-top-edge-row",
                "min_pixels": 6,
                "max_screen_y_delta": 0,
                "min_row_offset": 2,
            },
        ],
        "relative_pixel_anchors": [],
    }
    snapshot = {
        "window": {
            "visual_geometry": {
                "surfaces": [
                    {
                        "name": "minimap-shell",
                        "visible": True,
                        "rect": {"x": 0, "y": 0, "width": 12, "height": 10},
                    },
                ],
                "pixel_anchors": [
                    {
                        "name": "minimap-viewport-top-edge",
                        "surface": "minimap-native-viewport",
                        "visible": True,
                        "rect": {"x": 2, "y": 1, "width": 8, "height": 1},
                    },
                    {
                        "name": "minimap-first-content-row",
                        "surface": "minimap-source-map",
                        "visible": True,
                        "rect": {"x": 2, "y": 8, "width": 8, "height": 1},
                    },
                ],
            }
        }
    }
    report = runner.evaluate_pixel_anchors(
        {"manifest": manifest},
        {"snapshot": snapshot, "screenshot": before},
        {"snapshot": snapshot, "screenshot": after},
        tmp,
    )
    assert report["status"] == "failed"
    assert report["anchors"][0]["screen_y_delta"] == 1
    assert report["anchors"][0]["before_row_offset"] == 1
    assert report["anchors"][0]["after_row_offset"] == 2
    assert report["anchors"][0]["app_geometry"]["screen_y_delta"] == 0
    assert report["app_vs_rendered_disagreements"][0]["status"] == "app-vs-rendered-anchor-disagreement"
    assert (tmp / report["anchors"][0]["artifacts"]["before_crop"]).is_file()
    assert (tmp / report["anchors"][0]["artifacts"]["after_crop"]).is_file()
    assert report["relationships"] == []

    missing_snapshot = {
        "window": {
            "visual_geometry": {
                "surfaces": []
            }
        }
    }
    try:
        runner.evaluate_pixel_anchors(
            {"manifest": manifest},
            {"snapshot": missing_snapshot, "screenshot": before},
            {"snapshot": snapshot, "screenshot": after},
            tmp,
        )
    except RuntimeError as exc:
        assert "visual surface not found" in str(exc)
    else:
        raise AssertionError("missing declared pixel anchor should fail")


def test_animation_frame_anchor_drift_report(tmp: Path) -> None:
    runner = load_runner()
    before = tmp / "animation-before.png"
    stable = tmp / "animation-stable.png"
    drifted = tmp / "animation-drifted.png"
    bg = (29, 29, 32, 255)
    edge = (78, 78, 78, 255)
    cyan = (0, 190, 180, 255)

    before_rows = [[bg for _ in range(12)] for _ in range(10)]
    stable_rows = [[bg for _ in range(12)] for _ in range(10)]
    drifted_rows = [[bg for _ in range(12)] for _ in range(10)]
    for x in range(2, 10):
        before_rows[1][x] = edge
        stable_rows[1][x] = edge
        drifted_rows[2][x] = edge
    for x in range(2, 7):
        before_rows[4][x] = cyan
        stable_rows[4][x] = cyan
        drifted_rows[4][x] = cyan
    write_rgba(before, before_rows)
    write_rgba(stable, stable_rows)
    write_rgba(drifted, drifted_rows)

    manifest = {
        "scenario_type": "minimap-sidebar",
        "invariant_id": "native-minimap-highlight-anchors",
        "pixel_anchors": [
            {
                "name": "minimap-native-viewport-top-edge",
                "crop_surface": "minimap-shell",
                "detector": "native-minimap-viewport-top-edge-row",
                "min_pixels": 6,
            },
        ],
        "relative_pixel_anchors": [],
    }
    snapshot = {
        "window": {
            "visual_geometry": {
                "surfaces": [
                    {
                        "name": "workspace-sidebar",
                        "visible": True,
                        "rect": {"x": 0, "y": 0, "width": 2, "height": 10},
                    },
                    {
                        "name": "editor-viewport",
                        "visible": True,
                        "rect": {"x": 2, "y": 0, "width": 10, "height": 10},
                    },
                    {
                        "name": "source-view",
                        "visible": True,
                        "rect": {"x": 2, "y": 0, "width": 8, "height": 10},
                    },
                    {
                        "name": "minimap-shell",
                        "visible": True,
                        "rect": {"x": 0, "y": 0, "width": 12, "height": 10},
                    },
                    {
                        "name": "minimap-source-map",
                        "visible": True,
                        "rect": {"x": 0, "y": 0, "width": 12, "height": 10},
                    },
                    {
                        "name": "minimap-native-viewport",
                        "visible": True,
                        "rect": {"x": 2, "y": 1, "width": 8, "height": 2},
                    },
                    {
                        "name": "minimap-marker-strip",
                        "visible": True,
                        "rect": {"x": 10, "y": 0, "width": 2, "height": 10},
                    },
                ],
                "pixel_anchors": [
                    {
                        "name": "minimap-viewport-top-edge",
                        "surface": "minimap-native-viewport",
                        "visible": True,
                        "rect": {"x": 2, "y": 1, "width": 8, "height": 1},
                    },
                    {
                        "name": "minimap-first-content-row",
                        "surface": "minimap-source-map",
                        "visible": True,
                        "rect": {"x": 2, "y": 4, "width": 8, "height": 1},
                    },
                ],
                "scroll_anchors": [],
                "native_minimap": {"visible": True},
            }
        }
    }
    snapshot_path = tmp / "animation-before-snapshot.json"
    snapshot_path.write_text(json.dumps(snapshot), encoding="utf-8")
    case = {"manifest": manifest}
    crops = tmp / "animation" / "crops"
    crops.mkdir(parents=True)
    baseline = runner.detect_animation_baseline(
        case,
        {"snapshot": snapshot, "snapshot_path": snapshot_path, "screenshot": before},
        manifest["pixel_anchors"],
        crops,
    )

    stable_report = runner.evaluate_animation_frame(
        case,
        0,
        16,
        snapshot,
        read_png(stable),
        stable,
        tmp / "animation/frames/frame-000-geometry-snapshot.json",
        manifest["pixel_anchors"],
        baseline,
        crops,
        0,
        mapped_sample_elapsed_ms=15,
        sample_skew_ms=1,
        sidebar_phase="intermediate",
    )
    assert stable_report["status"] == "passed"
    assert stable_report["max_row_drift"] == 0

    drift_report = runner.evaluate_animation_frame(
        case,
        1,
        32,
        snapshot,
        read_png(drifted),
        drifted,
        tmp / "animation/frames/frame-001-geometry-snapshot.json",
        manifest["pixel_anchors"],
        baseline,
        crops,
        0,
        mapped_sample_elapsed_ms=31,
        sample_skew_ms=1,
        sidebar_phase="intermediate",
    )
    assert drift_report["status"] == "failed"
    assert drift_report["max_row_drift"] == 1
    assert drift_report["anchors"][0]["diagnostics"][0]["status"] == (
        "animation-app-vs-rendered-anchor-disagreement"
    )

    report_dir = tmp / "summary-case" / "animation"
    report_dir.mkdir(parents=True)
    (report_dir / "animation-report.json").write_text(
        json.dumps(
            {
                "status": "failed",
                "capture_mode": "stream",
                "invariant_id": "native-minimap-animation-highlight-anchors",
                "sampled_frame_count": 2,
                "geometry_sample_count": 2,
                "intermediate_geometry_sample_count": 2,
                "mapped_intermediate_frame_count": 2,
                "sample_interval_ms": 16,
                "max_sample_skew_ms": 80,
                "max_sample_skew_observed_ms": 1,
                "phase_sequence": ["intermediate"],
                "max_row_drift": 1,
                "frames": [stable_report, drift_report],
                "failures": runner.summarize_animation_failures([drift_report]),
            }
        ),
        encoding="utf-8",
    )
    summary = runner.animation_evidence_summary(tmp / "summary-case")
    assert summary["status"] == "failed"
    assert summary["sampled_frame_count"] == 2
    assert summary["mapped_intermediate_frame_count"] == 2
    assert summary["max_sample_skew_observed_ms"] == 1
    assert summary["frames"][1]["anchors"][0]["row_delta_from_baseline"] == 1


def test_animation_timestamp_mapping_rejects_stale_pairs() -> None:
    runner = load_runner()
    samples = [
        {"elapsed_ms": 10, "sidebar_phase": "shown", "snapshot": {}},
        {"elapsed_ms": 40, "sidebar_phase": "intermediate", "snapshot": {}},
        {"elapsed_ms": 70, "sidebar_phase": "hidden", "snapshot": {}},
    ]

    sample, skew = runner.animation_sample_for_frame(samples, 46, 20)
    assert sample["elapsed_ms"] == 40
    assert skew == 6

    stale_sample, stale_skew = runner.animation_sample_for_frame(samples, 130, 20)
    assert stale_sample is None
    assert stale_skew == 60

    assert runner.animation_phase_sequence(samples) == ["shown", "intermediate", "hidden"]


def geometry_snapshot(sidebar_x: int, sidebar_width: int, editor_x: int) -> dict[str, object]:
    return {
        "window": {
            "visual_geometry": {
                "surfaces": [
                    {
                        "name": "workspace-sidebar",
                        "visible": True,
                        "rect": {"x": sidebar_x, "y": 10, "width": sidebar_width, "height": 90},
                    },
                    {
                        "name": "editor-viewport",
                        "visible": True,
                        "rect": {"x": editor_x, "y": 10, "width": 500, "height": 90},
                    },
                    {
                        "name": "source-view",
                        "visible": True,
                        "rect": {"x": editor_x, "y": 10, "width": 450, "height": 90},
                    },
                    {
                        "name": "minimap-shell",
                        "visible": True,
                        "rect": {"x": editor_x + 450, "y": 10, "width": 50, "height": 90},
                    },
                    {
                        "name": "minimap-source-map",
                        "visible": True,
                        "rect": {"x": editor_x + 450, "y": 10, "width": 40, "height": 90},
                    },
                    {
                        "name": "minimap-native-viewport",
                        "visible": True,
                        "rect": {"x": editor_x + 450, "y": 10, "width": 40, "height": 20},
                    },
                    {
                        "name": "minimap-marker-strip",
                        "visible": True,
                        "rect": {"x": editor_x + 490, "y": 10, "width": 10, "height": 90},
                    },
                ],
                "pixel_anchors": [],
                "scroll_anchors": [],
            }
        }
    }


def test_final_sidebar_geometry_rejects_mid_animation(tmp: Path) -> None:
    runner = load_runner()
    visible = geometry_snapshot(sidebar_x=0, sidebar_width=360, editor_x=360)
    overlay_visible = geometry_snapshot(sidebar_x=0, sidebar_width=360, editor_x=0)
    hidden = geometry_snapshot(sidebar_x=-360, sidebar_width=360, editor_x=0)
    mid_animation = geometry_snapshot(sidebar_x=-180, sidebar_width=360, editor_x=180)

    assert runner.sidebar_final_geometry_matches(visible, True)[0]
    assert runner.sidebar_final_geometry_matches(overlay_visible, True, True)[0]
    assert not runner.sidebar_final_geometry_matches(overlay_visible, True, False)[0]
    assert runner.sidebar_final_geometry_matches(hidden, False)[0]
    assert not runner.sidebar_final_geometry_matches(mid_animation, True)[0]
    assert not runner.sidebar_final_geometry_matches(mid_animation, False)[0]

    sample_dir = tmp / "samples"
    sample_dir.mkdir()
    runner.write_final_geometry_samples(
        sample_dir,
        "after",
        False,
        [
            runner.final_geometry_sample(mid_animation, False, False, "workspace-sidebar x=-180"),
        ],
        "failed",
    )
    payload = json.loads((sample_dir / "after-final-geometry-samples.json").read_text())
    assert payload["status"] == "failed"
    assert payload["samples"][0]["surfaces"][0]["name"] == "workspace-sidebar"


def test_compact_overlay_sidebar_transition_keeps_editor_geometry() -> None:
    runner = load_runner()
    hidden_snapshot = geometry_snapshot(sidebar_x=-360, sidebar_width=360, editor_x=0)
    visible_snapshot = geometry_snapshot(sidebar_x=0, sidebar_width=360, editor_x=0)
    hidden_sidebar = runner.rect_for(hidden_snapshot, "workspace-sidebar")
    visible_sidebar = runner.rect_for(visible_snapshot, "workspace-sidebar")
    editor = runner.rect_for(visible_snapshot, "editor-viewport")

    assert runner.compact_overlay_sidebar_transition(
        "show", editor, editor, hidden_sidebar, visible_sidebar
    )
    assert runner.compact_overlay_sidebar_transition(
        "hide", editor, editor, visible_sidebar, hidden_sidebar
    )
    assert runner.compact_overlay_allowed({"size": {"width": 837}})
    assert not runner.compact_overlay_allowed({"size": {"width": 1100}})


def main() -> int:
    with tempfile.TemporaryDirectory() as directory:
        tmp = Path(directory)
        test_png_exact_and_masked_comparison(tmp)
        test_manifest_parsing_and_missing_manifest_failure(tmp)
        test_skip_summary_shape(tmp)
        test_reported_minimap_native_highlight_fixture_regression(tmp)
        test_pixel_anchor_detectors_and_drift_regression(tmp)
        test_animation_frame_anchor_drift_report(tmp)
        test_animation_timestamp_mapping_rejects_stale_pairs()
        test_final_sidebar_geometry_rejects_mid_animation(tmp)
        test_compact_overlay_sidebar_transition_keeps_editor_geometry()
    print("PASS: visual geometry script self-tests")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
