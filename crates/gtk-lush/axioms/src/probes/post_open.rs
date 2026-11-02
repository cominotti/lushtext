// SPDX-License-Identifier: MIT OR Apache-2.0

//! The fixtures the post-open settle probes (A67–A73, LushText's
//! `bound-post-open-settle`) and their samples share: short-line HTML and
//! dense-line documents, a reference-sized source view without a language,
//! timed main-context dispatches, a priority bracket that isolates the
//! dispatches of one idle priority, and a PDF export that counts the pages a
//! `GtkSourcePrintCompositor` prints.
//!
//! Durations recorded through these helpers are values, never verdict
//! inputs: a probe decides only from what dispatched, in which order, and what
//! it changed.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use gtk4::glib;
use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::probes::text::{Generator, ParagraphKind, ascii_word, paragraph};

/// Characters on every line of the dense fixture: the responsiveness lane's
/// dense reference length.
pub const DENSE_LINE_CHARS: usize = 710;
/// Lines in the dense fixture, as in the lane (`--count` default 160).
pub const DENSE_LINES: usize = 160;
/// The window the dense probes present their view in: the lane's monitor.
pub const REFERENCE_WINDOW: (i32, i32) = (1280, 800);
/// Half of LushText's `B_SETTLE_US` (16,000 µs, owned by
/// `crates/lushtext-core/src/model/responsiveness_budget.rs`, which this crate
/// cannot depend on): the budget `V_SLICE` is derived against.
pub const HALF_SETTLE_US: u128 = 8_000;
/// `GTK_TEXT_VIEW_PRIORITY_VALIDATE` (`GDK_PRIORITY_REDRAW + 5`), the priority
/// of `GtkTextView`'s incremental validation idle.
pub const VALIDATE_PRIORITY: i32 = 125;
/// Most iterations any timed drain runs before giving up.
pub const MAX_ITERATIONS: usize = 200_000;

/// A short-line HTML document of about `total_bytes` bytes: a doctype line,
/// then markup lines of 40 to 119 characters, each ending in `\n`.
#[must_use]
pub fn short_html(total_bytes: usize, seed: u64) -> String {
    let mut generator = Generator::new(seed);
    let mut text = String::with_capacity(total_bytes + 128);
    text.push_str("<!DOCTYPE html><html><body>\n");
    while text.len() + 120 < total_bytes {
        let target = 40 + usize::try_from(generator.below(80)).unwrap_or(0);
        let class = generator.below(400);
        let id = generator.below(10_000);
        let mut line = match generator.below(4) {
            0 => format!("<a href=\"/p/{id}?c={class}\" class=\"l{class}\">"),
            1 => format!("<script>var v{id}={class};</script><b>"),
            _ => format!("<div class=\"c{class}\" id=\"n{id}\"><span>"),
        };
        while line.len() + 20 < target {
            line.push_str(&ascii_word(&mut generator));
        }
        line.push_str("</span></div>");
        line.truncate(119);
        text.push_str(&line);
        text.push('\n');
    }
    text.push_str("</body></html>\n");
    text
}

/// The longest line the synthetic short-line documents hold, in bytes.
pub const SHORT_LINE_MAX: usize = 119;

/// A few words of the generator's ASCII vocabulary, at most `max` bytes.
fn words(generator: &mut Generator, max: usize) -> String {
    let mut text = String::new();
    loop {
        let word = ascii_word(generator);
        if text.len() + word.len() > max {
            break;
        }
        text.push_str(&word);
    }
    text.trim_end().to_owned()
}

/// A short-line Markdown document of about `total_bytes` bytes: headings,
/// paragraphs with emphasis, inline code and links, list items, block quotes
/// and fenced code blocks, every line at most [`SHORT_LINE_MAX`] bytes.
#[must_use]
pub fn short_markdown(total_bytes: usize, seed: u64) -> String {
    let mut generator = Generator::new(seed);
    let mut text = String::with_capacity(total_bytes + 256);
    while text.len() + 4 * SHORT_LINE_MAX < total_bytes {
        let line = match generator.below(10) {
            0 => format!("## {}", words(&mut generator, 50)),
            1 | 2 => format!(
                "- {} *{}* `{}`",
                words(&mut generator, 40),
                words(&mut generator, 20),
                words(&mut generator, 15)
            ),
            3 => format!("> {}", words(&mut generator, 80)),
            4 => {
                let body = words(&mut generator, 60);
                format!("```rust\nlet x = \"{body}\";\n```")
            }
            5 => format!(
                "[{}](https://example.com/p/{}) {}",
                words(&mut generator, 20),
                generator.below(10_000),
                words(&mut generator, 40)
            ),
            _ => format!(
                "{} **{}** {}",
                words(&mut generator, 50),
                words(&mut generator, 15),
                words(&mut generator, 30)
            ),
        };
        text.push_str(&line);
        text.push_str("\n\n");
    }
    text
}

/// A short-line Rust document of about `total_bytes` bytes: functions with
/// comments, `let` statements, string and numeric literals and a `match`,
/// every line at most [`SHORT_LINE_MAX`] bytes, braces balanced.
#[must_use]
pub fn short_rust(total_bytes: usize, seed: u64) -> String {
    let mut generator = Generator::new(seed);
    let mut text = String::with_capacity(total_bytes + 512);
    let mut function = 0u64;
    while text.len() + 12 * SHORT_LINE_MAX < total_bytes {
        function += 1;
        let k = generator.below(1_000);
        text.push_str(&format!("/// {}\n", words(&mut generator, 80)));
        text.push_str(&format!(
            "pub fn f{function}(x: u32, name: &str) -> Option<u32> {{\n"
        ));
        text.push_str(&format!("    // {}\n", words(&mut generator, 70)));
        text.push_str(&format!("    let value = x.wrapping_mul({k}) + 0x{k:x};\n"));
        text.push_str(&format!(
            "    let label = format!(\"{{name}}: {}\");\n",
            words(&mut generator, 40)
        ));
        text.push_str("    match value % 3 {\n");
        text.push_str(&format!("        0 => Some(value + {k}),\n"));
        text.push_str("        1 if label.is_empty() => None,\n");
        text.push_str("        _ => Some(value),\n");
        text.push_str("    }\n}\n\n");
    }
    text
}

/// A short-line JSON document of about `total_bytes` bytes: an array of flat
/// objects, one per line, every line at most [`SHORT_LINE_MAX`] bytes.
#[must_use]
pub fn short_json(total_bytes: usize, seed: u64) -> String {
    let mut generator = Generator::new(seed);
    let mut text = String::with_capacity(total_bytes + 256);
    text.push_str("[\n");
    let mut id = 0u64;
    while text.len() + 2 * SHORT_LINE_MAX < total_bytes {
        id += 1;
        let line = format!(
            "  {{\"id\": {id}, \"name\": \"{}\", \"ok\": {}, \"v\": {}.{}, \"tags\": [\"{}\"]}},\n",
            words(&mut generator, 30),
            generator.below(2) == 0,
            generator.below(1_000),
            generator.below(100),
            words(&mut generator, 15)
        );
        text.push_str(&line);
    }
    text.push_str("  {\"id\": 0}\n]\n");
    text
}

/// Repeat `block(generator, index)` until the text is about `total_bytes`
/// long; `head` and `tail` wrap it.
fn blocks(
    total_bytes: usize,
    seed: u64,
    head: &str,
    tail: &str,
    mut block: impl FnMut(&mut Generator, u64) -> String,
) -> String {
    let mut generator = Generator::new(seed);
    let mut text = String::with_capacity(total_bytes + 1024);
    text.push_str(head);
    let mut index = 0u64;
    while text.len() + 12 * SHORT_LINE_MAX + tail.len() < total_bytes {
        index += 1;
        let chunk = block(&mut generator, index);
        text.push_str(&chunk);
    }
    text.push_str(tail);
    text
}

/// A short-line C document of about `total_bytes` bytes.
#[must_use]
pub fn short_c(total_bytes: usize, seed: u64) -> String {
    blocks(
        total_bytes,
        seed,
        "#include <stdio.h>\n#include <stdlib.h>\n\n",
        "",
        |g, i| {
            let k = g.below(1_000);
            format!(
                "/* {} */\nstatic int\nf{i} (int x, const char *name)\n{{\n  int value = x * {k} + 0x{k:x};\n  \
             // {}\n  if (value % 3 == 0)\n    printf (\"%s: {}\\n\", name);\n  return value;\n}}\n\n",
                words(g, 70),
                words(g, 60),
                words(g, 30)
            )
        },
    )
}

/// A short-line Python document of about `total_bytes` bytes.
#[must_use]
pub fn short_python(total_bytes: usize, seed: u64) -> String {
    blocks(
        total_bytes,
        seed,
        "import os\nimport sys\n\n\n",
        "",
        |g, i| {
            let k = g.below(1_000);
            format!(
                "def f{i}(x: int, name: str) -> int:\n    \"\"\"{}\"\"\"\n    # {}\n    value = x * {k} + 0x{k:x}\n    \
             if value % 3 == 0:\n        print(f\"{{name}}: {}\")\n    return value\n\n\n",
                words(g, 70),
                words(g, 60),
                words(g, 30)
            )
        },
    )
}

/// A short-line JavaScript document of about `total_bytes` bytes.
#[must_use]
pub fn short_javascript(total_bytes: usize, seed: u64) -> String {
    blocks(total_bytes, seed, "'use strict';\n\n", "", |g, i| {
        let k = g.below(1_000);
        format!(
            "// {}\nfunction f{i}(x, name) {{\n  const value = x * {k} + 0x{k:x};\n  \
             const label = `${{name}}: {}`;\n  if (value % 3 === 0) {{\n    return {{ label, value, ok: true }};\n  }}\n  \
             return null; /* {} */\n}}\n\n",
            words(g, 70),
            words(g, 40),
            words(g, 30)
        )
    })
}

/// A short-line TypeScript document of about `total_bytes` bytes.
#[must_use]
pub fn short_typescript(total_bytes: usize, seed: u64) -> String {
    blocks(total_bytes, seed, "", "", |g, i| {
        let k = g.below(1_000);
        format!(
            "/** {} */\nexport interface Item{i} {{\n  id: number;\n  name: string;\n}}\n\n\
             export function f{i}(x: number, name: string): Item{i} | null {{\n  const value: number = x * {k};\n  \
             // {}\n  return value % 3 === 0 ? {{ id: value, name: `${{name}} {}` }} : null;\n}}\n\n",
            words(g, 70),
            words(g, 60),
            words(g, 20)
        )
    })
}

/// A short-line YAML document of about `total_bytes` bytes.
#[must_use]
pub fn short_yaml(total_bytes: usize, seed: u64) -> String {
    blocks(total_bytes, seed, "---\nitems:\n", "", |g, i| {
        format!(
            "  # {}\n  - id: {i}\n    name: \"{}\"\n    enabled: {}\n    ratio: {}.{}\n    tags: [{}, {}]\n    \
             note: >\n      {}\n",
            words(g, 70),
            words(g, 30),
            g.below(2) == 0,
            g.below(100),
            g.below(100),
            words(g, 10),
            words(g, 10),
            words(g, 80)
        )
    })
}

/// A short-line XML document of about `total_bytes` bytes.
#[must_use]
pub fn short_xml(total_bytes: usize, seed: u64) -> String {
    blocks(
        total_bytes,
        seed,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<items>\n",
        "</items>\n",
        |g, i| {
            format!(
                "  <!-- {} -->\n  <item id=\"{i}\" enabled=\"{}\">\n    <name>{}</name>\n    \
                 <value unit=\"ms\">{}</value>\n    <note><![CDATA[{}]]></note>\n  </item>\n",
                words(g, 70),
                g.below(2) == 0,
                words(g, 40),
                g.below(10_000),
                words(g, 60)
            )
        },
    )
}

/// A short-line CSS document of about `total_bytes` bytes.
#[must_use]
pub fn short_css(total_bytes: usize, seed: u64) -> String {
    blocks(total_bytes, seed, "@charset \"utf-8\";\n\n", "", |g, i| {
        let k = g.below(1_000);
        format!(
            "/* {} */\n.c{i} > .item:hover, #n{k} a[href^=\"/p\"] {{\n  color: #{:06x};\n  \
             margin: {}px {}em 0 auto;\n  font-family: \"{}\", sans-serif;\n  transition: opacity 0.{}s ease-in;\n}}\n\n",
            words(g, 70),
            g.below(0x0100_0000),
            g.below(40),
            g.below(4),
            words(g, 20),
            g.below(10)
        )
    })
}

/// A short-line shell document of about `total_bytes` bytes.
#[must_use]
pub fn short_sh(total_bytes: usize, seed: u64) -> String {
    blocks(total_bytes, seed, "#!/bin/sh\nset -eu\n\n", "", |g, i| {
        let k = g.below(1_000);
        format!(
            "# {}\nf{i}() {{\n  value=$(( $1 * {k} ))\n  name=\"${{2:-default}}\"\n  \
             if [ \"$value\" -gt {k} ]; then\n    echo \"$name: {}\" >&2\n  fi\n  for x in a b c; do printf '%s\\n' \"$x\"; done\n}}\n\n",
            words(g, 70),
            words(g, 30)
        )
    })
}

/// The synthetic short-line document of `language` (`html`, `markdown`,
/// `rust`, `json`, `c`, `python3`, `js`, `typescript`, `yaml`, `xml`, `css`
/// or `sh`), about `total_bytes` bytes, or `None` for another id.
#[must_use]
pub fn short_document(language: &str, total_bytes: usize, seed: u64) -> Option<String> {
    match language {
        "html" => Some(short_html(total_bytes, seed)),
        "markdown" => Some(short_markdown(total_bytes, seed)),
        "rust" => Some(short_rust(total_bytes, seed)),
        "json" => Some(short_json(total_bytes, seed)),
        "c" => Some(short_c(total_bytes, seed)),
        "python3" | "python" => Some(short_python(total_bytes, seed)),
        "js" => Some(short_javascript(total_bytes, seed)),
        "typescript" => Some(short_typescript(total_bytes, seed)),
        "yaml" => Some(short_yaml(total_bytes, seed)),
        "xml" => Some(short_xml(total_bytes, seed)),
        "css" => Some(short_css(total_bytes, seed)),
        "sh" => Some(short_sh(total_bytes, seed)),
        _ => None,
    }
}

/// `lines` lines of exactly `chars` ASCII characters each (words separated
/// by single spaces), each ending in `\n`.
#[must_use]
pub fn dense_text(lines: usize, chars: usize, seed: u64) -> String {
    let mut text = String::with_capacity(lines * (chars + 1));
    for line in 0..lines {
        text.push_str(&paragraph(
            ParagraphKind::Ascii,
            chars,
            seed.wrapping_add(line as u64),
        ));
        text.push('\n');
    }
    text
}

/// A `GtkSourceView` with no language, monospace, in a `GtkScrolledWindow`:
/// the view a dense document is edited in.
#[derive(Clone, Debug)]
pub struct DenseView {
    /// The scrolled window to present.
    pub scrolled: gtk4::ScrolledWindow,
    /// The view under observation.
    pub view: sourceview5::View,
    /// The view's buffer.
    pub buffer: sourceview5::Buffer,
}

impl DenseView {
    /// An empty view wrapping with `wrap` (`WrapMode::None` for no wrap).
    #[must_use]
    pub fn new(wrap: gtk4::WrapMode) -> Self {
        let buffer = sourceview5::Buffer::new(None);
        buffer.set_highlight_syntax(false);
        let view = sourceview5::View::with_buffer(&buffer);
        view.set_wrap_mode(wrap);
        view.set_monospace(true);
        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&view)
            .hexpand(true)
            .vexpand(true)
            .build();
        Self {
            scrolled,
            view,
            buffer,
        }
    }

    /// The horizontal and vertical adjustments' `upper`.
    #[must_use]
    pub fn uppers(&self) -> (f64, f64) {
        (
            self.scrolled.hadjustment().upper(),
            self.scrolled.vadjustment().upper(),
        )
    }

    /// Append `text` at the end of the buffer.
    pub fn append(&self, text: &str) {
        let mut end = self.buffer.end_iter();
        self.buffer.insert(&mut end, text);
    }
}

/// Whether `font` (the widget's own CSS font when `None`) lays out at a fixed
/// pitch in `widget`'s Pango context: ten `i` as wide as ten `M`.
///
/// The probes that read columns or line widths from character counts assume
/// the "Monospace" family resolves to a fixed-pitch face, as it does in every
/// GNOME runtime. A host without one (a minimal container whose only font is
/// a sans face) resolves "Monospace" to a proportional face; the probes make
/// this a control step, so such a host reports an invalid fixture rather than
/// a violated axiom.
#[must_use]
pub fn fixed_pitch(widget: &impl IsA<gtk4::Widget>, font: Option<&pango::FontDescription>) -> bool {
    let layout = widget.create_pango_layout(None);
    layout.set_font_description(font);
    let width = |text: &str| {
        layout.set_text(text);
        layout.size().0
    };
    width("iiiiiiiiii") == width("MMMMMMMMMM")
}

/// The control step every monospace-dependent probe takes.
pub const FIXED_PITCH_CONTROL: &str =
    "control: the Monospace font is fixed-pitch (install a monospace font, such as Adwaita Mono)";

/// Counts `after-paint` emissions of one frame clock while it is alive.
pub struct FrameCounter {
    clock: gtk4::gdk::FrameClock,
    handler: Option<glib::SignalHandlerId>,
    count: Rc<Cell<u32>>,
}

impl std::fmt::Debug for FrameCounter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FrameCounter")
            .field("count", &self.count.get())
            .finish_non_exhaustive()
    }
}

impl FrameCounter {
    /// Count the frames of `widget`'s frame clock, or `None` before it is
    /// realized.
    #[must_use]
    pub fn watch(widget: &impl IsA<gtk4::Widget>) -> Option<Self> {
        let clock = widget.frame_clock()?;
        let count = Rc::new(Cell::new(0));
        let counter = Rc::clone(&count);
        let handler = clock.connect_after_paint(move |_| counter.set(counter.get() + 1));
        Some(Self {
            clock,
            handler: Some(handler),
            count,
        })
    }

    /// Frames painted so far.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.count.get()
    }
}

impl Drop for FrameCounter {
    fn drop(&mut self) {
        if let Some(handler) = self.handler.take() {
            self.clock.disconnect(handler);
        }
    }
}

/// One main-context iteration a probe timed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimedDispatch {
    /// Microseconds the iteration took.
    pub micros: u128,
    /// Frames painted inside it.
    pub frames: u32,
}

/// Time one non-blocking iteration of the default main context. `None` when
/// nothing was ready.
#[must_use]
pub fn timed_iteration(frames: Option<&FrameCounter>) -> Option<TimedDispatch> {
    let before = frames.map_or(0, FrameCounter::count);
    let started = Instant::now();
    let dispatched = glib::MainContext::default().iteration(false);
    let micros = started.elapsed().as_micros();
    dispatched.then(|| TimedDispatch {
        micros,
        frames: frames.map_or(0, FrameCounter::count) - before,
    })
}

/// Time every iteration until nothing is ready, at most `max`.
#[must_use]
pub fn timed_drain(max: usize, frames: Option<&FrameCounter>) -> Vec<TimedDispatch> {
    let mut dispatches = Vec::new();
    while dispatches.len() < max {
        match timed_iteration(frames) {
            Some(dispatch) => dispatches.push(dispatch),
            None => break,
        }
    }
    dispatches
}

/// One iteration [`watch_until_quiet`] timed, with whether the watched value
/// changed in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WatchedDispatch {
    /// The iteration.
    pub dispatch: TimedDispatch,
    /// Whether `read` returned a different value after it.
    pub changed: bool,
}

/// Drive the default main context (sleeping a millisecond when nothing is
/// ready, so frames and timeouts arrive) until `read` has not changed and no
/// iteration took a millisecond or more for `quiet`, or `budget` ran out.
#[must_use]
pub fn watch_until_quiet(
    frames: Option<&FrameCounter>,
    quiet: std::time::Duration,
    budget: std::time::Duration,
    mut read: impl FnMut() -> f64,
) -> Vec<WatchedDispatch> {
    let deadline = Instant::now() + budget;
    let mut last_activity = Instant::now();
    let mut value = read();
    let mut watched = Vec::new();
    while Instant::now() < deadline && last_activity.elapsed() < quiet {
        match timed_iteration(frames) {
            Some(dispatch) => {
                let now = read();
                let changed = (now - value).abs() > f64::EPSILON;
                value = now;
                if changed || dispatch.micros >= 1_000 {
                    last_activity = Instant::now();
                }
                watched.push(WatchedDispatch { dispatch, changed });
            }
            None => std::thread::sleep(std::time::Duration::from_millis(1)),
        }
    }
    watched
}

/// The frameless iterations of `watched` that changed the value: their
/// count, longest duration and total duration, µs.
#[must_use]
pub fn changing_dispatches(watched: &[WatchedDispatch]) -> (usize, u128, u128) {
    let changing: Vec<u128> = watched
        .iter()
        .filter(|w| w.changed && w.dispatch.frames == 0)
        .map(|w| w.dispatch.micros)
        .collect();
    (
        changing.len(),
        changing.iter().copied().max().unwrap_or_default(),
        changing.iter().sum(),
    )
}

/// The longest frameless iteration of `watched`, µs.
#[must_use]
pub fn longest_frameless(watched: &[WatchedDispatch]) -> u128 {
    watched
        .iter()
        .filter(|w| w.dispatch.frames == 0)
        .map(|w| w.dispatch.micros)
        .max()
        .unwrap_or_default()
}

/// What [`bracketed`] observed.
#[derive(Clone, Debug, Default)]
pub struct Bracketed {
    /// Every iteration between the opening and the closing marker, with
    /// what `after_each` returned for it.
    pub inside: Vec<(TimedDispatch, f64)>,
    /// Whether both markers dispatched before the iteration cap.
    pub closed: bool,
}

impl Bracketed {
    /// The inside iterations that painted no frame.
    #[must_use]
    pub fn frameless(&self) -> Vec<(TimedDispatch, f64)> {
        self.inside
            .iter()
            .copied()
            .filter(|(dispatch, _)| dispatch.frames == 0)
            .collect()
    }
}

/// Isolate the dispatches of sources at `priority`: arm a one-shot idle at
/// `priority - 1` and one at `priority + 1`, then iterate until the second
/// one has run. The iterations in between dispatched only sources of
/// `priority` or higher (a frame is flagged by `frames`); `after_each` reads a
/// value after each of them (an adjustment's `upper`, say).
#[must_use]
pub fn bracketed(
    priority: i32,
    frames: Option<&FrameCounter>,
    mut after_each: impl FnMut() -> f64,
) -> Bracketed {
    let opened = Rc::new(Cell::new(false));
    let closed = Rc::new(Cell::new(false));
    {
        let opened = Rc::clone(&opened);
        glib::idle_add_local_full(glib::Priority::from(priority - 1), move || {
            opened.set(true);
            glib::ControlFlow::Break
        });
    }
    {
        let closed = Rc::clone(&closed);
        glib::idle_add_local_full(glib::Priority::from(priority + 1), move || {
            closed.set(true);
            glib::ControlFlow::Break
        });
    }
    let mut result = Bracketed::default();
    let mut iterations = 0;
    while !closed.get() && iterations < MAX_ITERATIONS {
        iterations += 1;
        let was_open = opened.get();
        let Some(dispatch) = timed_iteration(frames) else {
            break;
        };
        if was_open && !closed.get() {
            result.inside.push((dispatch, after_each()));
        }
    }
    result.closed = closed.get();
    result
}

/// The median of `values` (the upper one of an even count), or 0 when empty.
#[must_use]
pub fn median(values: &[u128]) -> u128 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() / 2).copied().unwrap_or_default()
}

/// How many pages a `GtkSourcePrintCompositor` for `buffer` prints, exported
/// to a PDF in a private directory under the process's temporary directory
/// (removed afterwards). `None` when the export fails.
#[must_use]
pub fn printed_pages(buffer: &sourceview5::Buffer) -> Option<i32> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "gtk-lush-axioms-print-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).ok()?;
    let compositor = sourceview5::PrintCompositor::new(buffer);
    let operation = gtk4::PrintOperation::new();
    operation.set_export_filename(dir.join("print.pdf"));
    let pages = Rc::new(Cell::new(None::<i32>));
    {
        let (compositor, pages) = (compositor.clone(), Rc::clone(&pages));
        operation.connect_paginate(move |operation, context| {
            if compositor.paginate(context) {
                let count = compositor.n_pages();
                operation.set_n_pages(count);
                pages.set(Some(count));
                true
            } else {
                false
            }
        });
    }
    operation.connect_draw_page(move |_, context, page| compositor.draw_page(context, page));
    let outcome = operation.run(gtk4::PrintOperationAction::Export, None::<&gtk4::Window>);
    let _ = std::fs::remove_dir_all(&dir);
    outcome.ok().and_then(|_| pages.get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_documents_are_deterministic_short_lined_and_sized() {
        for language in [
            "html",
            "markdown",
            "rust",
            "json",
            "c",
            "python3",
            "js",
            "typescript",
            "yaml",
            "xml",
            "css",
            "sh",
        ] {
            let text = short_document(language, 64 * 1024, 7).unwrap_or_default();
            assert_eq!(Some(&text), short_document(language, 64 * 1024, 7).as_ref());
            assert!(
                text.len() > 60 * 1024 && text.len() <= 66 * 1024,
                "{language}: {}",
                text.len()
            );
            assert!(
                text.lines().all(|line| line.len() <= SHORT_LINE_MAX),
                "{language} has a line over {SHORT_LINE_MAX} bytes"
            );
        }
        assert!(short_document("cobol", 1024, 7).is_none());
    }
}
