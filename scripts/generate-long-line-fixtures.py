#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Deterministic synthetic long-line fixtures for the responsiveness lane.

Every fixture is produced from a kind, a size and a seed, so the same inputs
always yield byte-identical output and an artifact can record the seed and the
SHA-256 instead of the bytes. Nothing here reads, copies or excerpts a user
file: the ``minified-html`` kind reproduces only the *shape* of a profiled
real-world minified page (its paragraph-length distribution and its character
class mix, recorded as the numeric constants below), never its content.

Kinds (``--size`` unit in brackets):

``paragraph`` [characters]
    One giant paragraph of exactly ``size`` characters (minified CSS/JS-like
    token mix, or a multibyte mix with ``--charset multibyte``) and nothing
    else, so every caret placement lands inside it. The same bytes serve ``.html`` (syntax on) and
    ``.txt`` (syntax off), so an ``.html`` minus ``.txt`` difference isolates
    highlighting. A unique find marker sits at 97 % of the giant paragraph.
``minified-html`` [characters of the longest paragraph]
    A whole document shaped like the profiled page: eight giant paragraphs whose
    lengths keep the reference ratios, plus several hundred short markup lines.
``dense`` [characters per line]
    ``--count`` lines (default 160) of exactly ``size`` characters each: many
    lines just under a threshold, filling at least three viewports.
``short-html`` [bytes]
    Ordinary short-line HTML (about 80 characters per line) of ``size`` bytes.
``session`` [characters per document]
    ``--count`` (default 3) ``paragraph`` documents with distinct seeds.
``eviction`` [bytes per document]
    Enough short-line text documents of ``size`` bytes each that LushText's
    conservative live-editor estimate (four bytes per character,
    ``model/editor_memory.rs``) crosses its 256 MiB budget, plus two.

``--delimiter lf|cr|crlf|u2029`` rewrites every paragraph separator, so the
same paragraph sets can be split by CR-only, CRLF or U+2029.

The generator writes only inside ``--scope`` (the run's temporary scope) and
refuses an output directory outside it. It prints one JSON manifest with the
seed, the size and each file's SHA-256 and paragraph structure.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

GENERATOR_VERSION = 1

# LushText's live-editor upper budget (model/editor_memory.rs,
# EDITOR_MEMORY_UPPER_BUDGET_BYTES) and its per-character upper bound.
EDITOR_MEMORY_UPPER_BUDGET_BYTES = 256 * 1024 * 1024
ESTIMATE_BYTES_PER_CHAR = 4

FIND_MARKER = "zqzneedlezqz"
FIND_MARKER_POSITION_PERMILLE = 970

# Shape of the profiled minified page (5.12 MB, 447 paragraphs, median 398
# characters, eight paragraphs over 10k). Ratios of the eight giant paragraphs to
# the longest one, longest first.
GIANT_PARAGRAPH_RATIOS = (1.0, 0.5958, 0.3157, 0.3115, 0.2542, 0.0663, 0.0515, 0.0217)
SHORT_LINE_COUNT = 439
SHORT_LINE_MEDIAN = 398
SHORT_LINE_MAX = 4407
# Character-class mix measured over the longest giant paragraph (fractions of
# its characters); the letters exclude q and z so the find marker stays unique.
MINIFIED_CLASS_WEIGHTS = (
    ("abcdefghijklmnoprstuvwxyABCDEFGHIJKLMNOPRSTUVWXY", 0.6918),
    ("-", 0.1019),
    ("0123456789", 0.0769),
    (":", 0.0291),
    (";", 0.0203),
    (".", 0.0181),
    (",", 0.0130),
    (" ", 0.0122),
    ("()", 0.0134),
    ("{}", 0.0130),
    ("#", 0.0043),
    ("/", 0.0018),
    ("%", 0.0015),
    ('"', 0.0007),
    ("_", 0.0006),
    ("@", 0.0003),
)
# Character-class mixes measured over the profiled page's longest script line
# (JSON-like data: quotes, backslash escapes, digits, brackets) and its longest
# markup line (attributes, tags). Fractions only; letters exclude q and z.
SCRIPT_CLASS_WEIGHTS = (
    ("abcdefghijklmnoprstuvwxyABCDEFGHIJKLMNOPRSTUVWXY", 0.4947),
    ("0123456789", 0.2413),
    (",", 0.0752),
    ('"', 0.0521),
    ("\\", 0.0418),
    (" ", 0.0205),
    (".", 0.0099),
    ("[]", 0.0136),
    ("-", 0.0067),
    ("/", 0.0062),
    (":", 0.0059),
    ("=", 0.0052),
    ("_", 0.0048),
    (";", 0.0043),
    ("()", 0.0068),
    ("<>", 0.0066),
    ("%?!&{}+|'#", 0.0040),
)
MARKUP_CLASS_WEIGHTS = (
    ("abcdefghijklmnoprstuvwxyABCDEFGHIJKLMNOPRSTUVWXY", 0.5909),
    ("0123456789", 0.1450),
    ('"', 0.0524),
    (" ", 0.0503),
    ("-", 0.0374),
    ("=", 0.0269),
    ("<>", 0.0394),
    (".", 0.0191),
    ("/", 0.0117),
    (":", 0.0097),
    (";", 0.0087),
    (",", 0.0024),
    ("#()_%&", 0.0056),
)
FLAVORS = {
    # opener, class mix: which sub-language GtkSourceView's html context runs.
    "css": ("<style>", MINIFIED_CLASS_WEIGHTS),
    "script": ("<script>", SCRIPT_CLASS_WEIGHTS),
    "markup": ("<div>", MARKUP_CLASS_WEIGHTS),
}
MULTIBYTE_CLASS_WEIGHTS = (
    ("abcdefghijklmnoprstuvwxy", 0.40),
    ("éèàçßøñüöΩλπ", 0.25),  # two-byte UTF-8
    ("中文字体编辑器长行段落", 0.20),  # three-byte UTF-8
    (" ", 0.12),
    ("😀🧪", 0.03),  # four-byte UTF-8
)
SHORT_HTML_TAGS = ("div", "span", "p", "li", "a", "td", "section", "em")

DELIMITERS = {"lf": "\n", "cr": "\r", "crlf": "\r\n", "u2029": " "}
KINDS = ("paragraph", "minified-html", "dense", "short-html", "session", "eviction")
CHUNK_CHARS = 16 * 1024
CHUNK_COUNT = 48
TABLE_SIZE = 4096


class SplitMix64:
    """A tiny PRNG whose output is fixed by this file, not by the Python version."""

    MASK = (1 << 64) - 1

    def __init__(self, seed: int) -> None:
        self.state = seed & self.MASK

    def next(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & self.MASK
        value = self.state
        value = ((value ^ (value >> 30)) * 0xBF58476D1CE4E5B9) & self.MASK
        value = ((value ^ (value >> 27)) * 0x94D049BB133111EB) & self.MASK
        return value ^ (value >> 31)

    def below(self, bound: int) -> int:
        return self.next() % bound


def character_table(weights: tuple[tuple[str, float], ...]) -> list[str]:
    """A TABLE_SIZE-slot sampling table matching the class weights."""
    total = sum(weight for _, weight in weights)
    table: list[str] = []
    for characters, weight in weights:
        slots = max(len(characters), round(TABLE_SIZE * weight / total))
        for index in range(slots):
            table.append(characters[index % len(characters)])
    return table


SCRIPT_ESCAPES = ('\\"', "\\/", "\\u00e9", "\\n")
SCRIPT_LETTERS = "abcdefghijklmnoprstuvwxyABCDEFGHIJKLMNOPRSTUVWXY"


def script_chunk(rng: SplitMix64) -> str:
    """JSON-like script data with balanced strings and backslash escapes.

    Per-character sampling left quote parity to chance and made the
    highlighting cost swing tenfold between seeds; whole tokens keep every
    string closed, so the cost is stable. The token mix is calibrated, not
    copied: GtkSourceView's html context takes about 0.75 s for 8k characters
    of it (seeds 7-9, GtkSourceView 5.20) and reaches the 2000 ms cut-off by
    16k, never cheaper than the profiled page's costliest script line
    (0.69 s at 8k, cut-off by 32k). Its densities therefore run above the
    profile's (about 37 % digits, 10 % quotes, 3.5 % backslashes).
    """
    parts: list[str] = []
    size = 0
    while size < CHUNK_CHARS:
        roll = rng.below(100)
        if roll < 28:
            word = "".join(SCRIPT_LETTERS[rng.below(len(SCRIPT_LETTERS))] for _ in range(2 + rng.below(9)))
            if rng.below(10) < 8:
                cut = rng.below(len(word) + 1)
                word = word[:cut] + SCRIPT_ESCAPES[rng.below(len(SCRIPT_ESCAPES))] + word[cut:]
            token = '"' + word + '"'
        elif roll < 84:
            token = str(rng.below(10 ** (2 + rng.below(5))))
        elif roll < 90:
            token = "[" if rng.below(2) else "]"
        elif roll < 94:
            token = ":"
        elif roll < 97:
            token = " "
        else:
            token = "(" + "".join(SCRIPT_LETTERS[rng.below(len(SCRIPT_LETTERS))] for _ in range(3)) + ")"
        parts.append(token + ",")
        size += len(token) + 1
    return "".join(parts)[:CHUNK_CHARS]


def make_chunks(rng: SplitMix64, charset: str, flavor: str = "css") -> list[str]:
    if charset == "ascii" and flavor == "script":
        return [script_chunk(rng) for _ in range(CHUNK_COUNT)]
    weights = FLAVORS[flavor][1] if charset == "ascii" else MULTIBYTE_CLASS_WEIGHTS
    table = character_table(weights)
    size = len(table)
    return ["".join(table[rng.below(size)] for _ in range(CHUNK_CHARS)) for _ in range(CHUNK_COUNT)]


@dataclass
class FileStats:
    path: Path
    bytes: int = 0
    chars: int = 0
    paragraphs: int = 0
    longest_paragraph: int = 0
    current: int = 0
    digest: "hashlib._Hash" = field(default_factory=hashlib.sha256)


class FixtureWriter:
    """Streams text to a file, hashing it and tracking paragraph lengths.

    Paragraphs are separated by the chosen delimiter only (the generator never
    emits any other separator), which is what GTK counts as a paragraph.
    """

    def __init__(self, path: Path, delimiter: str) -> None:
        self.stats = FileStats(path=path)
        self.delimiter = delimiter
        self.handle = path.open("wb")

    def text(self, text: str) -> None:
        data = text.encode("utf-8")
        self.handle.write(data)
        self.stats.digest.update(data)
        self.stats.bytes += len(data)
        self.stats.chars += len(text)
        self.stats.current += len(text)

    def end_paragraph(self) -> None:
        self.stats.longest_paragraph = max(self.stats.longest_paragraph, self.stats.current)
        self.stats.paragraphs += 1
        self.stats.current = 0
        data = self.delimiter.encode("utf-8")
        self.handle.write(data)
        self.stats.digest.update(data)
        self.stats.bytes += len(data)
        self.stats.chars += len(self.delimiter)

    def close(self) -> dict:
        if self.stats.current:
            self.stats.longest_paragraph = max(self.stats.longest_paragraph, self.stats.current)
            self.stats.paragraphs += 1
        self.handle.close()
        return {
            "name": self.stats.path.name,
            "path": str(self.stats.path),
            "bytes": self.stats.bytes,
            "chars": self.stats.chars,
            "paragraphs": self.stats.paragraphs,
            "longest_paragraph": self.stats.longest_paragraph,
            "sha256": self.stats.digest.hexdigest(),
        }


def giant_paragraph(writer: FixtureWriter, rng: SplitMix64, chunks: list[str], length: int, marker: bool) -> None:
    """Write exactly ``length`` characters with no separator, marker at 97 %."""
    marker_at = length * FIND_MARKER_POSITION_PERMILLE // 1000 if marker and length > len(FIND_MARKER) * 4 else -1
    written = 0
    while written < length:
        if marker_at >= 0 and written <= marker_at < written + CHUNK_CHARS:
            head = marker_at - written
            piece = chunks[rng.below(len(chunks))][:head] + FIND_MARKER
            piece = piece[: length - written]
            writer.text(piece)
            written += len(piece)
            marker_at = -1
            continue
        piece = chunks[rng.below(len(chunks))][: length - written]
        writer.text(piece)
        written += len(piece)


def short_markup_line(rng: SplitMix64, chunks: list[str], length: int) -> str:
    tag = SHORT_HTML_TAGS[rng.below(len(SHORT_HTML_TAGS))]
    opening = f'<{tag} class="c{rng.below(997)}">'
    closing = f"</{tag}>"
    body_len = max(0, length - len(opening) - len(closing))
    chunk = chunks[rng.below(len(chunks))]
    start = rng.below(len(chunk) - body_len) if body_len < len(chunk) else 0
    body = chunk[start : start + body_len].replace("<", " ").replace(">", " ")
    return (opening + body + closing)[:length] if length >= len(opening) + len(closing) else opening[:length]


def short_line_length(rng: SplitMix64) -> int:
    """Log-uniform around the reference median, capped at the reference max."""
    exponent = (rng.below(2001) - 1000) / 1000.0  # [-1, 1]
    return max(8, min(SHORT_LINE_MAX, int(SHORT_LINE_MEDIAN * math.pow(4.0, exponent))))


def write_paragraph_document(path: Path, delimiter: str, seed: int, length: int, charset: str, flavor: str = "css") -> dict:
    """Exactly one paragraph of ``length`` characters and its terminator.

    The giant paragraph is the whole document, so document start and end
    (Control+Home/End) and any click in the text area put the caret inside it,
    with wrap on or off. It opens with ``<style>`` so ``.html`` highlighting
    runs the CSS sub-context the profiled page's giant lines exercise
    (``--flavor css``); ``script`` opens ``<script>`` over a JSON-like data mix
    and ``markup`` opens ``<div>`` over an attribute-heavy mix, the two flavours
    the profiled page's costliest lines have.
    """
    rng = SplitMix64(seed)
    chunks = make_chunks(rng, charset, flavor)
    writer = FixtureWriter(path, delimiter)
    opener = FLAVORS[flavor][0] if length > 64 else ""
    writer.text(opener)
    giant_paragraph(writer, rng, chunks, length - len(opener), marker=True)
    writer.end_paragraph()
    return writer.close()


def write_minified_html(path: Path, delimiter: str, seed: int, longest: int) -> dict:
    rng = SplitMix64(seed)
    chunks = make_chunks(rng, "ascii")
    writer = FixtureWriter(path, delimiter)
    giants = [max(1, int(longest * ratio)) for ratio in GIANT_PARAGRAPH_RATIOS]
    # Spread the giants through the short lines, as the reference does.
    slots = sorted(rng.below(SHORT_LINE_COUNT) for _ in giants)
    giant_index = 0
    writer.text("<!DOCTYPE html>")
    writer.end_paragraph()
    for line_number in range(SHORT_LINE_COUNT):
        while giant_index < len(giants) and slots[giant_index] == line_number:
            writer.text("<style>" if giant_index % 2 else "<script>")
            giant_paragraph(writer, rng, chunks, giants[giant_index] - 8, marker=giant_index == 0)
            writer.end_paragraph()
            giant_index += 1
        writer.text(short_markup_line(rng, chunks, short_line_length(rng)))
        writer.end_paragraph()
    writer.text("</html>")
    writer.end_paragraph()
    return writer.close()


def write_dense(path: Path, delimiter: str, seed: int, line_length: int, count: int) -> dict:
    rng = SplitMix64(seed)
    chunks = make_chunks(rng, "ascii")
    writer = FixtureWriter(path, delimiter)
    for _ in range(count):
        giant_paragraph(writer, rng, chunks, line_length, marker=False)
        writer.end_paragraph()
    return writer.close()


def write_short_html(path: Path, delimiter: str, seed: int, total_bytes: int) -> dict:
    rng = SplitMix64(seed)
    chunks = make_chunks(rng, "ascii")
    writer = FixtureWriter(path, delimiter)
    writer.text("<!DOCTYPE html><html><body>")
    writer.end_paragraph()
    tail = "</body></html>"
    while writer.stats.bytes + 100 + len(tail) < total_bytes:
        writer.text(short_markup_line(rng, chunks, 40 + rng.below(80)))
        writer.end_paragraph()
    pad = total_bytes - writer.stats.bytes - len(tail)
    if pad > 0:
        writer.text("<p>" + "x" * max(0, pad - 7) + "</p>" if pad >= 7 else " " * pad)
    writer.text(tail)
    return writer.close()


def write_short_text(path: Path, delimiter: str, seed: int, total_bytes: int) -> dict:
    rng = SplitMix64(seed)
    chunks = make_chunks(rng, "ascii")
    writer = FixtureWriter(path, delimiter)
    while writer.stats.bytes + 128 < total_bytes:
        chunk = chunks[rng.below(len(chunks))]
        start = rng.below(len(chunk) - 120)
        writer.text(chunk[start : start + 40 + rng.below(80)])
        writer.end_paragraph()
    return writer.close()


def eviction_count(per_file_bytes: int) -> int:
    per_estimate = per_file_bytes * ESTIMATE_BYTES_PER_CHAR
    return EDITOR_MEMORY_UPPER_BUDGET_BYTES // per_estimate + 2


def resolve_inside(scope: Path, out_dir: Path) -> Path:
    scope = scope.resolve()
    out_dir = out_dir.resolve()
    if out_dir != scope and scope not in out_dir.parents:
        raise ValueError(f"refusing to write outside the scope: {out_dir} is not inside {scope}")
    return out_dir


def generate(
    kind: str,
    size: int,
    seed: int,
    scope: Path,
    out_dir: Path,
    *,
    ext: str = "txt",
    delimiter: str = "lf",
    charset: str = "ascii",
    flavor: str = "css",
    count: int | None = None,
    name: str | None = None,
) -> dict:
    if kind not in KINDS:
        raise ValueError(f"unknown kind {kind!r}")
    if flavor not in FLAVORS:
        raise ValueError(f"unknown flavor {flavor!r}")
    if delimiter not in DELIMITERS:
        raise ValueError(f"unknown delimiter {delimiter!r}")
    if size <= 0:
        raise ValueError("size must be positive")
    out_dir = resolve_inside(scope, out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    separator = DELIMITERS[delimiter]
    stem = name or f"{kind}-{size}-{charset}-{flavor}-{delimiter}-s{seed}"
    files: list[dict] = []
    if kind == "paragraph":
        files.append(write_paragraph_document(out_dir / f"{stem}.{ext}", separator, seed, size, charset, flavor))
    elif kind == "minified-html":
        files.append(write_minified_html(out_dir / f"{stem}.html", separator, seed, size))
    elif kind == "dense":
        files.append(write_dense(out_dir / f"{stem}.{ext}", separator, seed, size, count or 160))
    elif kind == "short-html":
        files.append(write_short_html(out_dir / f"{stem}.html", separator, seed, size))
    elif kind == "session":
        for index in range(count or 3):
            files.append(
                write_paragraph_document(
                    out_dir / f"{stem}-doc{index}.{ext}", separator, seed + 1000 * (index + 1), size, charset, flavor
                )
            )
    elif kind == "eviction":
        for index in range(count or eviction_count(size)):
            files.append(write_short_text(out_dir / f"{stem}-doc{index:02}.txt", separator, seed + index, size))
    return {
        "generator_version": GENERATOR_VERSION,
        "kind": kind,
        "size": size,
        "seed": seed,
        "delimiter": delimiter,
        "charset": charset,
        "flavor": flavor,
        "find_marker": FIND_MARKER if kind in ("paragraph", "minified-html", "session") else None,
        "files": files,
    }


# --- self-test ---------------------------------------------------------------


def self_test() -> int:
    failures: list[str] = []

    def expect(condition: bool, label: str) -> None:
        if not condition:
            failures.append(label)

    with tempfile.TemporaryDirectory(prefix="lt-fixture-selftest-") as raw:
        scope = Path(raw)
        first = generate("paragraph", 50_000, 7, scope, scope / "a", ext="html")
        second = generate("paragraph", 50_000, 7, scope, scope / "b", ext="html")
        expect(first["files"][0]["sha256"] == second["files"][0]["sha256"], "same inputs give the same digest")
        other = generate("paragraph", 50_000, 8, scope, scope / "c", ext="html")
        expect(first["files"][0]["sha256"] != other["files"][0]["sha256"], "another seed gives other bytes")
        expect(first["files"][0]["longest_paragraph"] == 50_000, "paragraph: longest paragraph is the size")
        text = Path(first["files"][0]["path"]).read_text(encoding="utf-8")
        expect(text.count(FIND_MARKER) == 1, "paragraph: one find marker")
        expect("\r" not in text and " " not in text, "paragraph: lf only")

        for flavor, opener in (("script", "<script>"), ("markup", "<div>")):
            flavored = generate("paragraph", 9_000, 7, scope, scope / f"f-{flavor}", ext="html", flavor=flavor)
            body = Path(flavored["files"][0]["path"]).read_text(encoding="utf-8")
            expect(body.startswith(opener) and flavored["files"][0]["longest_paragraph"] == 9_000, f"{flavor} flavour opener and length")
            expect(flavored["files"][0]["sha256"] != first["files"][0]["sha256"], f"{flavor} flavour differs from css")
        multibyte = generate("paragraph", 20_000, 3, scope, scope / "m", charset="multibyte")
        mb = multibyte["files"][0]
        expect(mb["longest_paragraph"] == 20_000 and mb["bytes"] > mb["chars"], "multibyte: chars < bytes")

        for delimiter, separator in DELIMITERS.items():
            result = generate("dense", 1_000, 5, scope, scope / f"d-{delimiter}", delimiter=delimiter, count=12)
            data = Path(result["files"][0]["path"]).read_text(encoding="utf-8", newline="")
            expect(result["files"][0]["paragraphs"] == 12, f"dense {delimiter}: 12 paragraphs")
            expect(result["files"][0]["longest_paragraph"] == 1_000, f"dense {delimiter}: line length")
            expect(data.count(separator) == 12, f"dense {delimiter}: separator count")
            if delimiter == "u2029":
                expect("\n" not in data and "\r" not in data, "u2029: no LF/CR")
            elif delimiter == "cr":
                expect("\n" not in data and " " not in data, "cr: CR only")

        short = generate("short-html", 200_000, 9, scope, scope / "s")
        expect(short["files"][0]["bytes"] == 200_000, f"short-html: total size exact ({short['files'][0]['bytes']})")
        expect(short["files"][0]["longest_paragraph"] < 400, "short-html: short lines")

        minified = generate("minified-html", 40_000, 11, scope, scope / "h")
        mf = minified["files"][0]
        expect(mf["longest_paragraph"] == 40_000, f"minified-html: longest is size ({mf['longest_paragraph']})")
        expect(mf["paragraphs"] == SHORT_LINE_COUNT + len(GIANT_PARAGRAPH_RATIOS) + 2, "minified-html: paragraph count")

        session = generate("session", 5_000, 1, scope, scope / "sess", count=3)
        digests = {entry["sha256"] for entry in session["files"]}
        expect(len(session["files"]) == 3 and len(digests) == 3, "session: three distinct documents")

        expect(eviction_count(8 * 1024 * 1024) * 8 * 1024 * 1024 * 4 > EDITOR_MEMORY_UPPER_BUDGET_BYTES, "eviction crosses the budget")
        eviction = generate("eviction", 64 * 1024, 2, scope, scope / "e", count=2)
        expect(len(eviction["files"]) == 2 and eviction["files"][0]["longest_paragraph"] < 200, "eviction: short lines")

        try:
            generate("paragraph", 100, 1, scope, scope.parent / "outside-scope")
        except ValueError:
            pass
        else:
            failures.append("refuses an output directory outside the scope")
        try:
            generate("paragraph", 100, 1, scope, scope / ".." / "escape")
        except ValueError:
            pass
        else:
            failures.append("refuses a ../ escape")

    if failures:
        print("generate-long-line-fixtures self-test failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print("generate-long-line-fixtures self-test passed.")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--kind", choices=KINDS)
    parser.add_argument("--size", type=int)
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--scope", type=Path, help="the run's temporary scope (default $TMPDIR)")
    parser.add_argument("--out", type=Path, help="output directory inside the scope")
    parser.add_argument("--ext", default="txt", choices=("txt", "html"))
    parser.add_argument("--delimiter", default="lf", choices=tuple(DELIMITERS))
    parser.add_argument("--charset", default="ascii", choices=("ascii", "multibyte"))
    parser.add_argument("--flavor", default="css", choices=tuple(FLAVORS))
    parser.add_argument("--count", type=int)
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    if args.kind is None or args.size is None or args.out is None:
        parser.error("--kind, --size and --out are required")
    scope = args.scope or Path(os.environ.get("TMPDIR", tempfile.gettempdir()))
    try:
        manifest = generate(
            args.kind,
            args.size,
            args.seed,
            scope,
            args.out,
            ext=args.ext,
            delimiter=args.delimiter,
            charset=args.charset,
            flavor=args.flavor,
            count=args.count,
        )
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    print(json.dumps(manifest, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
