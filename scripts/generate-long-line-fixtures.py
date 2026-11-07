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
    ``model/editor_memory.rs``) crosses its 256 MiB budget, plus two. With
    ``--count`` exactly that many (the lane's small session and draft cases).
``short-code`` [bytes per document]
    ``--count`` (default 2) documents of ``size`` bytes of short lines of 8 to
    71 characters (about 40 on average, like source code), below C6's density
    bound (``V_SLICE`` visible characters per ``R_batch`` = 256 lines, 64
    per line), so LushText installs them in unpaced byte slices and every
    view validates them afterwards: the validation backlog the lane's
    ``save/during-validation`` case saves during (``spawn-completion-priority``).
``markdown`` [bytes]
    An ordinary Markdown document of exactly ``size`` bytes (``.md``):
    headings, paragraphs with inline emphasis, code and links, lists, fenced
    code blocks, tables and block quotes, so the side-by-side preview renders
    every block kind it supports.
``keystroke`` [characters of the edited paragraph]
    One paragraph of exactly ``size`` characters between two runs of short
    lines, with a unique marker at its start, middle and end
    (``KEYSTROKE_MARKERS``), so the lane places the caret by finding a marker
    and types beside it. It measures the cost of one keystroke inside an
    editable paragraph (C2's ``L_EDIT_STALL`` and toolkit-floor exception).

The Python API also writes an ``.editorconfig`` beside the documents
(``editorconfig={"tab_width": 8, ...}``, ``root = true``); the manifest
records it under ``editorconfig``, outside ``files``, so it is never opened.

``--flavor css|script|markup|prose`` picks the giant paragraph's character mix:
the three markup flavours open the sub-language GtkSourceView's html context
runs; ``prose`` is lowercase words, spaces and sentence punctuation with no
markup (and no ``q`` or ``z``, so the keystroke markers stay unique).

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
PROSE_CLASS_WEIGHTS = (
    ("etaoinshrdlucmfwypvbgk", 0.80),
    (" ", 0.17),
    (",.", 0.03),
)
FLAVORS = {
    # opener, class mix: which sub-language GtkSourceView's html context runs.
    "css": ("<style>", MINIFIED_CLASS_WEIGHTS),
    "script": ("<script>", SCRIPT_CLASS_WEIGHTS),
    "markup": ("<div>", MARKUP_CLASS_WEIGHTS),
    # Prose: no opener, no markup.
    "prose": ("", PROSE_CLASS_WEIGHTS),
    # `short-language` only (`chunked-highlight-attach` re-review F1): the
    # worst case for a context-flipping key, no closer anywhere after the
    # cursor: C comments are `//` only, Markdown has no fenced code block (an
    # indented one instead) and no `-->`.
    "no-closers": ("", PROSE_CLASS_WEIGHTS),
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
KINDS = ("paragraph", "minified-html", "dense", "short-html", "session", "eviction", "short-code", "markdown", "keystroke", "short-language", "markers")
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


# The keystroke fixture's caret anchors: no flavour's character mix contains
# both `q` and `z`, so none occurs by chance in the generated text. The short
# paragraph right after the long one starts with its own marker: it is where
# typing must be unaffected by anything a long paragraph turns off
# (`explore-editing-long-paragraphs`).
KEYSTROKE_MARKERS = {"start": "zqzkstartzqz", "middle": "zqzkmiddlezqz", "end": "zqzkendzqz", "short": "zqzkshortzqz"}
KEYSTROKE_SHORT_PARAGRAPH_CHARS = 40
KEYSTROKE_SHORT_LINES = 30
# The start marker must fit the paragraph's first half, the middle and end
# markers its second half, so the paragraph is exactly `size` characters.
KEYSTROKE_MIN_LENGTH = 2 * max(
    len(KEYSTROKE_MARKERS["start"]), len(KEYSTROKE_MARKERS["middle"]) + len(KEYSTROKE_MARKERS["end"])
)


def write_keystroke_document(path: Path, delimiter: str, seed: int, length: int, charset: str, flavor: str = "css") -> dict:
    """One paragraph of exactly ``length`` characters between short lines.

    The paragraph starts with the start marker, holds the middle marker at
    ``length // 2`` and ends with the end marker; a flavour with an opener
    (``<style>`` for css) gets it on its own line just before the paragraph.
    A short paragraph, starting with the short marker, follows it.
    """
    if length < KEYSTROKE_MIN_LENGTH:
        raise ValueError(f"keystroke paragraph must be at least {KEYSTROKE_MIN_LENGTH} characters for its markers")
    rng = SplitMix64(seed)
    chunks = make_chunks(rng, charset, flavor)

    def fill(writer: FixtureWriter, count: int) -> None:
        while count > 0:
            piece = chunks[rng.below(len(chunks))][:count]
            writer.text(piece)
            count -= len(piece)

    def short_lines(writer: FixtureWriter) -> None:
        for _ in range(KEYSTROKE_SHORT_LINES):
            fill(writer, 30 + rng.below(50))
            writer.end_paragraph()

    start, middle, end = (KEYSTROKE_MARKERS[k] for k in ("start", "middle", "end"))
    writer = FixtureWriter(path, delimiter)
    short_lines(writer)
    opener = FLAVORS[flavor][0]
    if opener:
        writer.text(opener)
        writer.end_paragraph()
    half = length // 2
    writer.text(start)
    fill(writer, half - len(start))
    writer.text(middle)
    fill(writer, length - half - len(middle) - len(end))
    writer.text(end)
    writer.end_paragraph()
    writer.text(KEYSTROKE_MARKERS["short"])
    fill(writer, KEYSTROKE_SHORT_PARAGRAPH_CHARS)
    writer.end_paragraph()
    short_lines(writer)
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


MARKERS_TAIL_CHARS = 130


def write_markers(path: Path, delimiter: str, seed: int, pairs: int) -> dict:
    """`pairs` pairs of lines: one holding the find marker and a 130-character
    tail (a long line past the minimap's 120-column warning), then a short
    one, so every marker kind lands on separate, unmerged lines
    (`bin-minimap-markers-per-row` lane cases)."""
    rng = SplitMix64(seed)
    chunks = make_chunks(rng, "ascii")
    writer = FixtureWriter(path, delimiter)
    for _ in range(pairs):
        writer.text(f"{FIND_MARKER} ")
        giant_paragraph(writer, rng, chunks, MARKERS_TAIL_CHARS, marker=False)
        writer.end_paragraph()
        writer.text("z")
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


# `short-code`: line lengths 8 + below(64), so at most 71 and about 40 on
# average; C6's density bound is V_SLICE (16,384) visible characters over
# R_batch (256) lines (`model/buffer_replacement.rs`).
SHORT_CODE_MIN_LINE = 8
SHORT_CODE_LINE_SPREAD = 64
DENSITY_BATCH_LINES = 256
DENSITY_SLICE_CHARS = 16_384


def write_short_code(path: Path, delimiter: str, seed: int, total_bytes: int) -> dict:
    rng = SplitMix64(seed)
    chunks = make_chunks(rng, "ascii")
    writer = FixtureWriter(path, delimiter)
    while writer.stats.bytes + 128 < total_bytes:
        chunk = chunks[rng.below(len(chunks))]
        start = rng.below(len(chunk) - 120)
        writer.text(chunk[start : start + SHORT_CODE_MIN_LINE + rng.below(SHORT_CODE_LINE_SPREAD)])
        writer.end_paragraph()
    return writer.close()


def densest_batch_chars(text: str, lines: int = DENSITY_BATCH_LINES) -> int:
    """The most characters any `lines` consecutive lines hold, delimiters excluded."""
    lengths = [len(line) for line in text.split("\n")]
    window = sum(lengths[:lines])
    best = window
    for index in range(lines, len(lengths)):
        window += lengths[index] - lengths[index - lines]
        best = max(best, window)
    return best


MARKDOWN_WORDS = (
    "editor", "buffer", "paragraph", "preview", "highlight", "layout", "frame", "dispatch", "settle",
    "workspace", "session", "draft", "budget", "slice", "install", "language", "context", "engine",
    "validation", "adjustment", "scroll", "caret", "keyboard", "window", "toolkit", "render", "table",
)
MARKDOWN_BLOCK_KINDS = 6


def markdown_sentence(rng: SplitMix64, words: int) -> str:
    out = []
    for index in range(words):
        word = MARKDOWN_WORDS[rng.below(len(MARKDOWN_WORDS))]
        style = rng.below(40)
        if style == 0:
            word = f"**{word}**"
        elif style == 1:
            word = f"*{word}*"
        elif style == 2:
            word = f"`{word}_{rng.below(100)}`"
        elif style == 3:
            word = f"[{word}](https://example.invalid/{rng.below(1000)})"
        out.append(word.capitalize() if index == 0 else word)
    return " ".join(out) + "."


def write_markdown_block(writer: FixtureWriter, rng: SplitMix64, section: int, fences: bool = True) -> None:
    def line(text: str = "") -> None:
        writer.text(text)
        writer.end_paragraph()

    line(f"## Section {section}: {MARKDOWN_WORDS[section % len(MARKDOWN_WORDS)]}")
    line()
    line(" ".join(markdown_sentence(rng, 8 + rng.below(16)) for _ in range(3 + rng.below(3))))
    line()
    kind = section % MARKDOWN_BLOCK_KINDS
    if kind == 0:
        for item in range(3 + rng.below(4)):
            line(f"- {markdown_sentence(rng, 4 + rng.below(8))}")
            if item % 2:
                line(f"  - {markdown_sentence(rng, 3 + rng.below(5))}")
    elif kind == 1:
        for item in range(3 + rng.below(4)):
            line(f"{item + 1}. {markdown_sentence(rng, 4 + rng.below(8))}")
    elif kind == 2:
        if fences:
            line("```rust")
        for statement in range(4 + rng.below(6)):
            line(f"    let value_{statement} = compute_{rng.below(50)}(&buffer, {rng.below(1000)});")
        if fences:
            line("```")
    elif kind == 3:
        line("| Name | Size | Note |")
        line("| --- | ---: | --- |")
        for _ in range(3 + rng.below(5)):
            line(f"| {MARKDOWN_WORDS[rng.below(len(MARKDOWN_WORDS))]} | {rng.below(100_000)} | {markdown_sentence(rng, 3 + rng.below(4))} |")
    elif kind == 4:
        for _ in range(2 + rng.below(2)):
            line(f"> {markdown_sentence(rng, 6 + rng.below(10))}")
    else:
        line(f"- [ ] {markdown_sentence(rng, 4 + rng.below(6))}")
        line(f"- [x] {markdown_sentence(rng, 4 + rng.below(6))}")
    line()


def write_markdown(path: Path, delimiter: str, seed: int, total_bytes: int, fences: bool = True) -> dict:
    rng = SplitMix64(seed)
    writer = FixtureWriter(path, delimiter)
    writer.text("# Responsiveness lane Markdown fixture")
    writer.end_paragraph()
    writer.end_paragraph()
    section = 0
    # The largest block is well under 2 KiB, so a block never overshoots.
    while writer.stats.bytes + 2048 < total_bytes:
        section += 1
        write_markdown_block(writer, rng, section, fences)
    pad = total_bytes - writer.stats.bytes
    if pad > 0:
        words = (" ".join(MARKDOWN_WORDS) + " ") * (pad // 64 + 2)
        writer.text(words[:pad])
    return writer.close()


def write_editorconfig(out_dir: Path, properties: dict) -> dict:
    """A root `.editorconfig` for every file beside it."""
    body = "root = true\n\n[*]\n" + "".join(f"{key} = {value}\n" for key, value in properties.items())
    path = out_dir / ".editorconfig"
    path.write_text(body, encoding="utf-8")
    return {"path": str(path), "sha256": hashlib.sha256(body.encode("utf-8")).hexdigest(), "properties": dict(properties)}


def eviction_count(per_file_bytes: int) -> int:
    per_estimate = per_file_bytes * ESTIMATE_BYTES_PER_CHAR
    return EDITOR_MEMORY_UPPER_BUDGET_BYTES // per_estimate + 2


def resolve_inside(scope: Path, out_dir: Path) -> Path:
    scope = scope.resolve()
    out_dir = out_dir.resolve()
    if out_dir != scope and scope not in out_dir.parents:
        raise ValueError(f"refusing to write outside the scope: {out_dir} is not inside {scope}")
    return out_dir


# `short-language` (`chunked-highlight-attach`, C6b): short-line source of the
# language the file extension names, so GtkSourceView guesses that language.
# Lines stay under 120 characters; the templates vary names and numbers only.
SHORT_LANGUAGE_EXTS = ("md", "rs", "c", "py", "js", "ts", "yaml", "xml", "css", "sh", "json", "html")


def short_language_line(ext: str, rng: SplitMix64, n: int, closers: bool = True) -> str:
    k = rng.below(100_000)
    word = MARKDOWN_WORDS[rng.below(len(MARKDOWN_WORDS))]
    pick = rng.below(3)
    if ext == "rs":
        return (f"fn {word}_{n}(value: u32) -> u32 {{ value + {k} }} // {word}",
                f"    let {word}_{n}: &str = \"{word} {k}\";",
                f"#[derive(Debug)] struct {word.capitalize()}{n} {{ field: u64 }}")[pick]
    if ext == "c":
        comment = f"/* {word} */" if closers else f"// {word}"
        return (f"static int {word}_{n}(int value) {{ return value + {k}; }} {comment}",
                f"#define {word.upper()}_{n} {k}",
                f"    const char *{word}_{n} = \"{word} {k}\";")[pick]
    if ext == "py":
        return (f"def {word}_{n}(value):",
                f"    return value + {k}  # {word}",
                f"{word}_{n} = \"{word} {k}\"")[pick]
    if ext == "js":
        return (f"function {word}_{n}(value) {{ return value + {k}; }} // {word}",
                f"const {word}_{n} = \"{word} {k}\";",
                f"let {word}List{n} = [{k}, {n}, '{word}'];")[pick]
    if ext == "ts":
        return (f"export function {word}_{n}(value: number): number {{ return value + {k}; }}",
                f"const {word}_{n}: string = \"{word} {k}\";",
                f"interface {word.capitalize()}{n} {{ id: number; name: string }} // {k}")[pick]
    if ext == "yaml":
        return (f"{word}_{n}: \"{word} {k}\"  # note",
                f"  - {word}_{k}",
                f"{word}_{n}_count: {k}")[pick]
    if ext == "xml":
        return f'  <item id="{n}" name="{word}_{k}">{word} {k}</item>'
    if ext == "css":
        return (f".{word}-{n} {{ color: #{k % 0xFFFFFF:06x}; margin: {k % 64}px; }} /* {word} */",
                f"#{word}{n} > p {{ padding: {k % 32}em; }}",
                f"@media (min-width: {k % 2000}px) {{ .{word}-{n} {{ display: none; }} }}")[pick]
    if ext == "sh":
        return (f"function {word}_{n}() {{ echo \"{word} {k}\" \"$1\"; }}  # {word}",
                f"{word.upper()}_{n}=\"{k}\"",
                f"if [ -n \"${{{word.upper()}_{n}}}\" ]; then echo {k}; fi")[pick]
    if ext == "json":
        return f'  {{"id": {n}, "name": "{word}_{k}", "value": {k}, "ok": true}},'
    raise ValueError(f"no short-language template for {ext!r}")


def write_short_language(path: Path, delimiter: str, seed: int, total_bytes: int, ext: str, flavor: str = "css") -> dict:
    closers = flavor != "no-closers"
    if ext == "md":
        return write_markdown(path, delimiter, seed, total_bytes, fences=closers)
    if ext == "html":
        return write_short_html(path, delimiter, seed, total_bytes)
    rng = SplitMix64(seed)
    writer = FixtureWriter(path, delimiter)
    head, tail = {"xml": ('<?xml version="1.0"?>', "</root>"), "json": ("[", "{}]")}.get(ext, (None, None))
    if head:
        writer.text(head)
        writer.end_paragraph()
        if ext == "xml":
            writer.text("<root>")
            writer.end_paragraph()
    reserve = 128 + (len(tail) if tail else 0)
    n = 0
    while writer.stats.bytes + reserve < total_bytes:
        n += 1
        writer.text(short_language_line(ext, rng, n, closers))
        writer.end_paragraph()
    if tail:
        writer.text(tail)
        writer.end_paragraph()
    return writer.close()


def generate(
    kind: str,
    size: int,
    seed: int,
    scope: Path,
    out_dir: Path,
    *,
    ext: str | None = None,
    delimiter: str = "lf",
    charset: str = "ascii",
    flavor: str = "css",
    count: int | None = None,
    editorconfig: dict | None = None,
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
    # short-line markup defaults to .html (syntax on); the responsiveness lane
    # also opens the same bytes as .txt to subtract the plain-text cost.
    ext = ext or {"short-html": "html", "markdown": "md"}.get(kind, "txt")
    stem = f"{kind}-{size}-{charset}-{flavor}-{delimiter}-s{seed}"
    files: list[dict] = []
    if kind == "paragraph":
        files.append(write_paragraph_document(out_dir / f"{stem}.{ext}", separator, seed, size, charset, flavor))
    elif kind == "keystroke":
        files.append(write_keystroke_document(out_dir / f"{stem}.{ext}", separator, seed, size, charset, flavor))
    elif kind == "minified-html":
        files.append(write_minified_html(out_dir / f"{stem}.html", separator, seed, size))
    elif kind == "dense":
        files.append(write_dense(out_dir / f"{stem}.{ext}", separator, seed, size, count or 160))
    elif kind == "markers":
        files.append(write_markers(out_dir / f"{stem}.{ext}", separator, seed, size))
    elif kind == "short-html":
        files.append(write_short_html(out_dir / f"{stem}.{ext}", separator, seed, size))
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
    elif kind == "short-code":
        for index in range(count or 2):
            files.append(write_short_code(out_dir / f"{stem}-doc{index:02}.txt", separator, seed + index, size))
    elif kind == "markdown":
        files.append(write_markdown(out_dir / f"{stem}.{ext}", separator, seed, size))
    elif kind == "short-language":
        if ext not in SHORT_LANGUAGE_EXTS:
            raise ValueError(f"short-language needs ext in {SHORT_LANGUAGE_EXTS}, not {ext!r}")
        files.append(write_short_language(out_dir / f"{stem}.{ext}", separator, seed, size, ext, flavor))
    manifest_editorconfig = write_editorconfig(out_dir, editorconfig) if editorconfig else None
    return {
        "generator_version": GENERATOR_VERSION,
        "kind": kind,
        "size": size,
        "seed": seed,
        "delimiter": delimiter,
        "charset": charset,
        "flavor": flavor,
        "find_marker": FIND_MARKER if kind in ("paragraph", "minified-html", "session", "markers") else None,
        "editorconfig": manifest_editorconfig,
        "keystroke_markers": dict(KEYSTROKE_MARKERS) if kind == "keystroke" else None,
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
        for flavor, ext in (("prose", "txt"), ("css", "html")):
            keyed = generate("keystroke", 5_000, 11, scope, scope / f"k-{flavor}", ext=ext, flavor=flavor)
            lines = Path(keyed["files"][0]["path"]).read_text(encoding="utf-8").split("\n")
            long_index = next((i for i, line in enumerate(lines) if line.startswith(KEYSTROKE_MARKERS["start"])), None)
            expect(long_index is not None, f"keystroke {flavor}: the long paragraph starts with its marker")
            if long_index is not None:
                long_line = lines[long_index]
                expect(len(long_line) == 5_000 and keyed["files"][0]["longest_paragraph"] == 5_000, f"keystroke {flavor}: exact paragraph length")
                expect(long_line.count(KEYSTROKE_MARKERS["middle"]) == 1 and long_line.endswith(KEYSTROKE_MARKERS["end"]), f"keystroke {flavor}: middle and end markers")
                short = lines[long_index + 1]
                expect(short.startswith(KEYSTROKE_MARKERS["short"]) and len(short) == len(KEYSTROKE_MARKERS["short"]) + KEYSTROKE_SHORT_PARAGRAPH_CHARS, f"keystroke {flavor}: the short paragraph follows")
        multibyte = generate("paragraph", 20_000, 3, scope, scope / "m", charset="multibyte")
        mb = multibyte["files"][0]
        expect(mb["longest_paragraph"] == 20_000 and mb["bytes"] > mb["chars"], "multibyte: chars < bytes")
        markers = generate("markers", 64, 5, scope, scope / "markers")
        marker_text = Path(markers["files"][0]["path"]).read_text(encoding="utf-8")
        marker_lines = marker_text.split("\n")
        expect(marker_text.count(FIND_MARKER) == 64, "markers: one find marker per pair")
        expect(markers["files"][0]["paragraphs"] == 128, "markers: two lines per pair")
        expect(len(marker_lines[0]) == len(FIND_MARKER) + 1 + MARKERS_TAIL_CHARS > 120, "markers: the marked line is a long line")
        expect(marker_lines[1] == "z", "markers: a short line separates marked lines")

        for delimiter, separator in DELIMITERS.items():
            result = generate("dense", 1_000, 5, scope, scope / f"d-{delimiter}", delimiter=delimiter, count=12)
            with Path(result["files"][0]["path"]).open(encoding="utf-8", newline="") as handle:
                data = handle.read()
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
        short_txt = generate("short-html", 200_000, 9, scope, scope / "st", ext="txt")
        expect(short["files"][0]["path"].endswith(".html") and short_txt["files"][0]["path"].endswith(".txt") and short_txt["files"][0]["sha256"] == short["files"][0]["sha256"], "short-html: the same bytes as .html (default) and as .txt")

        minified = generate("minified-html", 40_000, 11, scope, scope / "h")
        mf = minified["files"][0]
        expect(mf["longest_paragraph"] == 40_000, f"minified-html: longest is size ({mf['longest_paragraph']})")
        expect(mf["paragraphs"] == SHORT_LINE_COUNT + len(GIANT_PARAGRAPH_RATIOS) + 2, "minified-html: paragraph count")

        session = generate("session", 5_000, 1, scope, scope / "sess", count=3)
        digests = {entry["sha256"] for entry in session["files"]}
        expect(len(session["files"]) == 3 and len(digests) == 3, "session: three distinct documents")

        # The keystroke fixture: each marker once, the edited paragraph exactly
        # its size, every delimiter, both lane flavours, and the minimum.
        for delimiter, separator in DELIMITERS.items():
            for flavor, ext in (("css", "html"), ("prose", "txt")):
                for size in (KEYSTROKE_MIN_LENGTH, 3_500, 50_000):
                    made = generate("keystroke", size, 4, scope, scope / f"k-{delimiter}-{flavor}-{size}", ext=ext, delimiter=delimiter, flavor=flavor)
                    entry = made["files"][0]
                    with Path(entry["path"]).open(encoding="utf-8", newline="") as handle:
                        data = handle.read()
                    label = f"keystroke {delimiter} {flavor} {size}"
                    expect(entry["path"].endswith(f".{ext}"), f"{label}: extension")
                    expect(all(data.count(marker) == 1 for marker in KEYSTROKE_MARKERS.values()), f"{label}: each marker exactly once")
                    paragraphs = data.split(separator)
                    edited = [p for p in paragraphs if KEYSTROKE_MARKERS["start"] in p]
                    expect(len(edited) == 1 and len(edited[0]) == size, f"{label}: the edited paragraph is exactly {size} characters")
                    expect(edited[0].startswith(KEYSTROKE_MARKERS["start"]) and edited[0].endswith(KEYSTROKE_MARKERS["end"]), f"{label}: start and end markers bound it")
                    expect(edited[0].index(KEYSTROKE_MARKERS["middle"]) == size // 2, f"{label}: the middle marker at the middle")
                    expect(entry["longest_paragraph"] == size or size < 80, f"{label}: the longest paragraph")
                    opener_lines = 1 if FLAVORS[flavor][0] else 0
                    expect(entry["paragraphs"] == 2 * KEYSTROKE_SHORT_LINES + opener_lines + 2, f"{label}: paragraph count")
                    others = {"lf": ("\r", "\u2029"), "cr": ("\n", "\u2029"), "crlf": ("\u2029",), "u2029": ("\n", "\r")}[delimiter]
                    expect(not any(other in data for other in others), f"{label}: only its delimiter")
                    if delimiter == "crlf":
                        expect(data.count("\r\n") == data.count("\r") == data.count("\n"), f"{label}: CR and LF only as pairs")
                    if flavor == "prose":
                        expect(not any(c in data for c in "<>") and "zqz" not in data.replace(KEYSTROKE_MARKERS["start"], "").replace(KEYSTROKE_MARKERS["middle"], "").replace(KEYSTROKE_MARKERS["end"], "").replace(KEYSTROKE_MARKERS["short"], ""), f"{label}: prose has no markup and no stray marker text")
        try:
            generate("keystroke", KEYSTROKE_MIN_LENGTH - 1, 4, scope, scope / "k-short")
        except ValueError:
            pass
        else:
            failures.append("keystroke: a length too short for the markers is refused")

        expect(eviction_count(8 * 1024 * 1024) * 8 * 1024 * 1024 * 4 > EDITOR_MEMORY_UPPER_BUDGET_BYTES, "eviction crosses the budget")
        eviction = generate("eviction", 64 * 1024, 2, scope, scope / "e", count=2)
        expect(len(eviction["files"]) == 2 and eviction["files"][0]["longest_paragraph"] < 200, "eviction: short lines")

        code = generate("short-code", 256 * 1024, 6, scope, scope / "sc")
        code_text = Path(code["files"][0]["path"]).read_text(encoding="utf-8")
        expect(len(code["files"]) == 2 and all(f["bytes"] <= 256 * 1024 for f in code["files"]), "short-code: two documents of at most size bytes")
        expect(code["files"][0]["longest_paragraph"] < SHORT_CODE_MIN_LINE + SHORT_CODE_LINE_SPREAD, f"short-code: short lines ({code['files'][0]['longest_paragraph']})")
        expect(densest_batch_chars(code_text) <= DENSITY_SLICE_CHARS, f"short-code: below C6's density bound ({densest_batch_chars(code_text)})")
        expect(generate("short-code", 256 * 1024, 6, scope, scope / "sc2")["files"][0]["sha256"] == code["files"][0]["sha256"], "short-code: deterministic")
        for ext in SHORT_LANGUAGE_EXTS:
            lang = generate("short-language", 64 * 1024, 7, scope, scope / f"sl-{ext}", ext=ext)
            entry = lang["files"][0]
            expect(entry["path"].endswith(f".{ext}"), f"short-language {ext}: extension")
            expect(60 * 1024 <= entry["bytes"] <= 64 * 1024, f"short-language {ext}: size ({entry['bytes']})")
            expect(entry["longest_paragraph"] < 2000, f"short-language {ext}: below L_syntax ({entry['longest_paragraph']})")
            again = generate("short-language", 64 * 1024, 7, scope, scope / f"sl2-{ext}", ext=ext)
            if ext in ("c", "md"):
                worst = generate("short-language", 64 * 1024, 7, scope, scope / f"slw-{ext}", ext=ext, flavor="no-closers")
                body = Path(worst["files"][0]["path"]).read_text(encoding="utf-8")
                expect("*/" not in body and "```" not in body and "-->" not in body, f"short-language {ext} no-closers: no closer anywhere")
            expect(again["files"][0]["sha256"] == entry["sha256"], f"short-language {ext}: deterministic")

        markdown = generate("markdown", 200_000, 4, scope, scope / "md")
        md = markdown["files"][0]
        md_text = Path(md["path"]).read_text(encoding="utf-8")
        expect(md["bytes"] == 200_000 and md["path"].endswith(".md"), f"markdown: exact size, .md ({md['bytes']})")
        expect(md_text.startswith("# ") and "```rust" in md_text and "| --- |" in md_text and "\n> " in md_text and "\n1. " in md_text and "- [x] " in md_text, "markdown: every block kind")
        expect(md["longest_paragraph"] < 2048, f"markdown: ordinary line lengths ({md['longest_paragraph']})")
        expect(generate("markdown", 200_000, 4, scope, scope / "md2")["files"][0]["sha256"] == md["sha256"], "markdown: deterministic")
        expect(markdown["editorconfig"] is None, "no .editorconfig unless asked")
        configured = generate("dense", 710, 5, scope, scope / "ec", count=4, editorconfig={"indent_style": "space", "tab_width": 8})
        ec = configured["editorconfig"]
        ec_text = Path(ec["path"]).read_text(encoding="utf-8") if ec else ""
        expect(ec is not None and Path(ec["path"]).parent == Path(configured["files"][0]["path"]).parent, "editorconfig: beside the fixture")
        expect(ec_text.startswith("root = true") and "tab_width = 8" in ec_text and "[*]" in ec_text, f"editorconfig: root and properties ({ec_text!r})")
        expect(all(not f["path"].endswith(".editorconfig") for f in configured["files"]), "editorconfig: never a document to open")
        small = generate("eviction", 16_000, 2, scope, scope / "small", count=3)
        expect(len(small["files"]) == 3 and all(f["bytes"] <= 16_000 for f in small["files"]), "eviction --count: exactly that many small documents")

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
    parser.add_argument("--ext", choices=("txt",) + SHORT_LANGUAGE_EXTS, help="default: html for short-html, md for markdown, txt otherwise")
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
