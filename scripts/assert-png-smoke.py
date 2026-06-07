#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Coarse PNG assertions for host-dependent visual smoke captures."""

from __future__ import annotations

import argparse
import struct
import sys
import zlib
from pathlib import Path


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Assert coarse PNG smoke invariants.")
    parser.add_argument("path", type=Path)
    parser.add_argument("--max-width", type=int, required=True)
    parser.add_argument("--max-height", type=int, required=True)
    parser.add_argument("--min-width", type=int, default=200)
    parser.add_argument("--min-height", type=int, default=150)
    parser.add_argument("--min-unique-pixels", type=int, default=16)
    parser.add_argument("--require-top-band-detail", action="store_true")
    parser.add_argument("--require-bottom-band-detail", action="store_true")
    parser.add_argument("--band-fraction", type=float, default=0.08)
    parser.add_argument("--min-band-unique-pixels", type=int, default=8)
    return parser.parse_args()


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
    channels_by_type = {
        0: 1,  # grayscale
        2: 3,  # RGB
        4: 2,  # grayscale + alpha
        6: 4,  # RGBA
    }
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


def unique_pixel_count(rows: list[bytes], bpp: int, limit: int) -> int:
    unique: set[bytes] = set()
    for row in rows:
        for offset in range(0, len(row), bpp):
            unique.add(row[offset : offset + bpp])
            if len(unique) >= limit:
                return len(unique)
    return len(unique)


def unique_pixel_count_in_band(
    rows: list[bytes], bpp: int, start: int, end: int, limit: int
) -> int:
    unique: set[bytes] = set()
    for row in rows[start:end]:
        for offset in range(0, len(row), bpp):
            unique.add(row[offset : offset + bpp])
            if len(unique) >= limit:
                return len(unique)
    return len(unique)


def assert_band_detail(
    rows: list[bytes],
    *,
    bpp: int,
    name: str,
    start: int,
    end: int,
    minimum: int,
) -> str:
    unique_pixels = unique_pixel_count_in_band(rows, bpp, start, end, minimum)
    if unique_pixels < minimum:
        raise ValueError(
            f"{name} band appears blank or too uniform: {unique_pixels} unique pixels"
        )
    return f"{name}-band>={minimum}"


def main() -> int:
    args = parse_args()
    data = args.path.read_bytes()
    ihdr, compressed = read_chunks(data)
    width = ihdr["width"]
    height = ihdr["height"]
    if ihdr["bit_depth"] != 8:
        raise ValueError(f"unsupported PNG bit depth {ihdr['bit_depth']}")
    if not (args.min_width <= width <= args.max_width):
        raise ValueError(f"width {width} outside expected bounds")
    if not (args.min_height <= height <= args.max_height):
        raise ValueError(f"height {height} outside expected bounds")

    bpp = bytes_per_pixel(ihdr["color_type"])
    rows = unfilter_rows(zlib.decompress(compressed), width, height, bpp)
    unique_pixels = unique_pixel_count(rows, bpp, args.min_unique_pixels)
    if unique_pixels < args.min_unique_pixels:
        raise ValueError(
            f"capture appears blank or too uniform: {unique_pixels} unique pixels"
        )
    band_rows = max(1, int(height * args.band_fraction))
    band_assertions: list[str] = []
    if args.require_top_band_detail:
        band_assertions.append(
            assert_band_detail(
                rows,
                bpp=bpp,
                name="top",
                start=0,
                end=band_rows,
                minimum=args.min_band_unique_pixels,
            )
        )
    if args.require_bottom_band_detail:
        band_assertions.append(
            assert_band_detail(
                rows,
                bpp=bpp,
                name="bottom",
                start=max(0, height - band_rows),
                end=height,
                minimum=args.min_band_unique_pixels,
            )
        )

    detail = f">={args.min_unique_pixels} unique pixels"
    if band_assertions:
        detail += ", " + ", ".join(band_assertions)
    print(f"PASS: {args.path} {width}x{height}, {detail}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        raise SystemExit(1)
