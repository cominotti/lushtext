#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Pure-Python PNG crop comparison helpers for visual geometry smoke tests."""

from __future__ import annotations

import argparse
import json
import struct
import zlib
from dataclasses import dataclass
from pathlib import Path


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


@dataclass(frozen=True)
class PngImage:
    width: int
    height: int
    bit_depth: int
    color_type: int
    bpp: int
    rows: list[bytes]


@dataclass(frozen=True)
class Rect:
    x: int
    y: int
    width: int
    height: int

    @classmethod
    def from_mapping(cls, value: dict[str, int]) -> "Rect":
        return cls(
            x=int(value["x"]),
            y=int(value["y"]),
            width=int(value["width"]),
            height=int(value["height"]),
        )

    def to_dict(self) -> dict[str, int]:
        return {
            "x": self.x,
            "y": self.y,
            "width": self.width,
            "height": self.height,
        }


def read_chunks(data: bytes) -> tuple[dict[str, int], bytes]:
    if not data.startswith(PNG_SIGNATURE):
        raise ValueError("not a PNG file")

    offset = len(PNG_SIGNATURE)
    ihdr: dict[str, int] | None = None
    idat_parts: list[bytes] = []
    while offset < len(data):
        if offset + 8 > len(data):
            raise ValueError("truncated PNG chunk header")
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        chunk_type = data[offset + 4 : offset + 8]
        offset += 8
        chunk_data = data[offset : offset + length]
        offset += length + 4
        if len(chunk_data) != length:
            raise ValueError("truncated PNG chunk data")
        if chunk_type == b"IHDR":
            width, height, bit_depth, color_type, _compression, _filter, _interlace = struct.unpack(
                ">IIBBBBB", chunk_data
            )
            ihdr = {
                "width": width,
                "height": height,
                "bit_depth": bit_depth,
                "color_type": color_type,
            }
        elif chunk_type == b"IDAT":
            idat_parts.append(chunk_data)
        elif chunk_type == b"IEND":
            break

    if ihdr is None:
        raise ValueError("missing IHDR")
    if not idat_parts:
        raise ValueError("missing IDAT")
    return ihdr, b"".join(idat_parts)


def bytes_per_pixel(color_type: int) -> int:
    channels_by_type = {0: 1, 2: 3, 4: 2, 6: 4}
    try:
        return channels_by_type[color_type]
    except KeyError as exc:
        raise ValueError(f"unsupported PNG color type {color_type}") from exc


def paeth(left: int, above: int, upper_left: int) -> int:
    estimate = left + above - upper_left
    dist_left = abs(estimate - left)
    dist_above = abs(estimate - above)
    dist_upper_left = abs(estimate - upper_left)
    if dist_left <= dist_above and dist_left <= dist_upper_left:
        return left
    if dist_above <= dist_upper_left:
        return above
    return upper_left


def unfilter_rows(raw: bytes, width: int, height: int, bpp: int) -> list[bytes]:
    stride = width * bpp
    rows: list[bytes] = []
    offset = 0
    previous = bytes(stride)
    for _ in range(height):
        if offset + stride + 1 > len(raw):
            raise ValueError("decompressed image data is truncated")
        filter_type = raw[offset]
        offset += 1
        encoded = raw[offset : offset + stride]
        offset += stride
        decoded = bytearray(stride)
        for index, value in enumerate(encoded):
            left = decoded[index - bpp] if index >= bpp else 0
            above = previous[index]
            upper_left = previous[index - bpp] if index >= bpp else 0
            if filter_type == 0:
                decoded[index] = value
            elif filter_type == 1:
                decoded[index] = (value + left) & 0xFF
            elif filter_type == 2:
                decoded[index] = (value + above) & 0xFF
            elif filter_type == 3:
                decoded[index] = (value + ((left + above) // 2)) & 0xFF
            elif filter_type == 4:
                decoded[index] = (value + paeth(left, above, upper_left)) & 0xFF
            else:
                raise ValueError(f"unsupported PNG filter {filter_type}")
        previous = bytes(decoded)
        rows.append(previous)
    return rows


def read_png(path: Path) -> PngImage:
    ihdr, compressed = read_chunks(path.read_bytes())
    if ihdr["bit_depth"] != 8:
        raise ValueError(f"unsupported PNG bit depth {ihdr['bit_depth']}")
    bpp = bytes_per_pixel(ihdr["color_type"])
    rows = unfilter_rows(zlib.decompress(compressed), ihdr["width"], ihdr["height"], bpp)
    return PngImage(
        width=ihdr["width"],
        height=ihdr["height"],
        bit_depth=ihdr["bit_depth"],
        color_type=ihdr["color_type"],
        bpp=bpp,
        rows=rows,
    )


def png_chunk(kind: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


def write_png(path: Path, image: PngImage, rows: list[bytes]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    height = len(rows)
    width = len(rows[0]) // image.bpp if rows else 0
    raw = b"".join(b"\x00" + row for row in rows)
    ihdr = struct.pack(">IIBBBBB", width, height, 8, image.color_type, 0, 0, 0)
    path.write_bytes(
        PNG_SIGNATURE
        + png_chunk(b"IHDR", ihdr)
        + png_chunk(b"IDAT", zlib.compress(raw))
        + png_chunk(b"IEND", b"")
    )


def validate_rect(image: PngImage, rect: Rect) -> None:
    if rect.width <= 0 or rect.height <= 0:
        raise ValueError(f"invalid empty crop: {rect}")
    if rect.x < 0 or rect.y < 0:
        raise ValueError(f"crop starts outside image: {rect}")
    if rect.x + rect.width > image.width or rect.y + rect.height > image.height:
        raise ValueError(f"crop exceeds image bounds {image.width}x{image.height}: {rect}")


def clamp_rect(image: PngImage, rect: Rect) -> Rect:
    x0 = max(0, rect.x)
    y0 = max(0, rect.y)
    x1 = min(image.width, rect.x + rect.width)
    y1 = min(image.height, rect.y + rect.height)
    clamped = Rect(x=x0, y=y0, width=x1 - x0, height=y1 - y0)
    validate_rect(image, clamped)
    return clamped


def crop_rows(image: PngImage, rect: Rect) -> list[bytes]:
    validate_rect(image, rect)
    start = rect.x * image.bpp
    end = (rect.x + rect.width) * image.bpp
    return [row[start:end] for row in image.rows[rect.y : rect.y + rect.height]]


def pixel_masked(x: int, y: int, masks: list[Rect]) -> bool:
    return any(mask.x <= x < mask.x + mask.width and mask.y <= y < mask.y + mask.height for mask in masks)


def compare_crops(
    before_path: Path,
    after_path: Path,
    before_rect: Rect,
    after_rect: Rect | None = None,
    masks: list[Rect] | None = None,
    artifact_prefix: Path | None = None,
) -> dict[str, object]:
    before = read_png(before_path)
    after = read_png(after_path)
    after_rect = after_rect or before_rect
    masks = masks or []
    if before.bpp != after.bpp or before.color_type != after.color_type:
        raise ValueError("PNG color formats differ")
    if before_rect.width != after_rect.width or before_rect.height != after_rect.height:
        raise ValueError("crop sizes differ")

    before_rows = crop_rows(before, before_rect)
    after_rows = crop_rows(after, after_rect)
    diff_pixels = 0
    compared_pixels = 0
    first_difference: dict[str, int] | None = None
    for y, (before_row, after_row) in enumerate(zip(before_rows, after_rows, strict=True)):
        for x in range(before_rect.width):
            if pixel_masked(x, y, masks):
                continue
            compared_pixels += 1
            start = x * before.bpp
            end = start + before.bpp
            if before_row[start:end] != after_row[start:end]:
                diff_pixels += 1
                if first_difference is None:
                    first_difference = {"x": x, "y": y}

    if artifact_prefix is not None:
        write_png(artifact_prefix.with_name(f"{artifact_prefix.name}-before.png"), before, before_rows)
        write_png(artifact_prefix.with_name(f"{artifact_prefix.name}-after.png"), after, after_rows)

    return {
        "status": "passed" if diff_pixels == 0 else "failed",
        "before_rect": before_rect.to_dict(),
        "after_rect": after_rect.to_dict(),
        "mask_rects": [mask.to_dict() for mask in masks],
        "compared_pixels": compared_pixels,
        "diff_pixels": diff_pixels,
        "first_difference": first_difference,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--rect-json", required=True)
    parser.add_argument("--after-rect-json")
    parser.add_argument("--mask-json", action="append", default=[])
    parser.add_argument("--artifact-prefix", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    before_rect = Rect.from_mapping(json.loads(args.rect_json))
    after_rect = (
        Rect.from_mapping(json.loads(args.after_rect_json))
        if args.after_rect_json
        else None
    )
    masks = [Rect.from_mapping(json.loads(mask)) for mask in args.mask_json]
    result = compare_crops(
        args.before,
        args.after,
        before_rect,
        after_rect,
        masks,
        args.artifact_prefix,
    )
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
