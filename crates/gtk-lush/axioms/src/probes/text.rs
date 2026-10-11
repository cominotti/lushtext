// SPDX-License-Identifier: MIT OR Apache-2.0

//! The text fixtures the long-line probes (A23–A34) and their samples share:
//! a deterministic paragraph builder, a text view in a scrolled window, a
//! source view with the HTML language, a log-capture guard, and a probe input
//! method.
//!
//! None of them depends on a LushText or GTK Lush widget; they are stock
//! `GtkTextView`, `GtkSourceView` and `GtkIMContext` objects configured the
//! way an editor configures them.

use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Instant;

use gtk4::glib;
use gtk4::prelude::*;
/// The shape of the characters [`paragraph`] produces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParagraphKind {
    /// Lower-case ASCII words separated by single spaces.
    Ascii,
    /// Words mixing ASCII with two-, three- and four-byte UTF-8 characters.
    Multibyte,
    /// One long run of minified HTML with inline script, the shape of a
    /// minified web page: dense tags and attributes, no newline.
    MinifiedMarkup,
}

/// A small deterministic generator (xorshift64*), so the same seed always
/// builds the same paragraph on every host.
struct Generator(u64);

impl Generator {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A value in `0..bound` (`bound` > 0).
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    /// One of `items` (not empty), chosen uniformly.
    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        let bound = u64::try_from(items.len()).unwrap_or(u64::MAX);
        items[usize::try_from(self.below(bound)).unwrap_or(0)]
    }
}

const ASCII_LETTERS: &[u8] = b"etaoinshrdlucmfwypvbgkjqxz";
const MULTIBYTE_CHARACTERS: &[char] = &['é', 'ж', 'ß', '漢', '字', '😀', 'ç', 'Ω', '→', '🦀'];
const MARKUP_TAGS: &[&str] = &["div", "span", "a", "p", "li", "td", "em", "b"];

/// A paragraph of exactly `chars` characters with no line terminator,
/// deterministic in `kind`, `chars` and `seed`.
#[must_use]
pub fn paragraph(kind: ParagraphKind, chars: usize, seed: u64) -> String {
    let mut generator = Generator::new(seed);
    let mut text = String::with_capacity(chars.saturating_mul(2));
    let mut count = 0usize;
    while count < chars {
        let piece = match kind {
            ParagraphKind::Ascii => ascii_word(&mut generator),
            ParagraphKind::Multibyte => multibyte_word(&mut generator),
            ParagraphKind::MinifiedMarkup => markup_chunk(&mut generator),
        };
        for character in piece.chars() {
            if count == chars {
                break;
            }
            text.push(character);
            count += 1;
        }
    }
    text
}

fn ascii_word(generator: &mut Generator) -> String {
    let length = 2 + generator.below(8);
    let mut word: String = (0..length)
        .map(|_| char::from(generator.pick(ASCII_LETTERS)))
        .collect();
    word.push(' ');
    word
}

fn multibyte_word(generator: &mut Generator) -> String {
    let mut word = ascii_word(generator);
    word.pop();
    let multibyte = generator.pick(MULTIBYTE_CHARACTERS);
    word.push(multibyte);
    word.push(' ');
    word
}

fn markup_chunk(generator: &mut Generator) -> String {
    let tag = generator.pick(MARKUP_TAGS);
    let class = generator.below(400);
    let id = generator.below(10_000);
    let text = ascii_word(generator);
    match generator.below(6) {
        0 => format!(
            "<script>var v{id}={class};function f{id}(x){{return x*{class}+\"{}\"}}</script>",
            text.trim_end()
        ),
        1 => format!("<a href=\"/p/{id}?c={class}\" class=\"l{class}\">{text}</a>"),
        _ => format!("<{tag} class=\"c{class}\" id=\"n{id}\">{text}</{tag}>"),
    }
}

/// A `GtkTextView` inside a `GtkScrolledWindow`, the way an editor hosts it.
#[derive(Clone, Debug)]
pub struct TextViewFixture {
    /// The scrolled window to present.
    pub scrolled: gtk4::ScrolledWindow,
    /// The view under observation.
    pub view: gtk4::TextView,
    /// The view's buffer.
    pub buffer: gtk4::TextBuffer,
}

impl TextViewFixture {
    /// A monospace view wrapping with `wrap` (`WrapMode::None` for no wrap).
    #[must_use]
    pub fn new(wrap: gtk4::WrapMode) -> Self {
        let buffer = gtk4::TextBuffer::new(None);
        let view = gtk4::TextView::with_buffer(&buffer);
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
}

/// A `GtkSourceView` with the HTML language inside a `GtkScrolledWindow`.
#[derive(Clone, Debug)]
pub struct SourceViewFixture {
    /// The scrolled window to present.
    pub scrolled: gtk4::ScrolledWindow,
    /// The view under observation.
    pub view: sourceview5::View,
    /// The view's buffer, highlighting HTML.
    pub buffer: sourceview5::Buffer,
}

impl SourceViewFixture {
    /// The fixture, or `None` when GtkSourceView has no `html` language
    /// definition on this host.
    #[must_use]
    pub fn html(wrap: gtk4::WrapMode) -> Option<Self> {
        let language = sourceview5::LanguageManager::default().language("html")?;
        let buffer = sourceview5::Buffer::with_language(&language);
        sourceview5::prelude::BufferExt::set_highlight_syntax(&buffer, true);
        let view = sourceview5::View::with_buffer(&buffer);
        view.set_wrap_mode(wrap);
        view.set_monospace(true);
        let scrolled = gtk4::ScrolledWindow::builder()
            .child(&view)
            .hexpand(true)
            .vexpand(true)
            .build();
        Some(Self {
            scrolled,
            view,
            buffer,
        })
    }
}

/// An `invisible` tag added to `buffer`'s tag table.
#[must_use]
pub fn invisible_tag(buffer: &gtk4::TextBuffer) -> gtk4::TextTag {
    let tag = gtk4::TextTag::builder().invisible(true).build();
    buffer.tag_table().add(&tag);
    tag
}

/// A character count as a `GtkTextBuffer` offset, saturating at `i32::MAX`.
#[must_use]
pub fn offset(chars: usize) -> i32 {
    i32::try_from(chars).unwrap_or(i32::MAX)
}

/// The running Pango and GtkSourceView versions, `major.minor.micro`, which
/// every text probe records beside the GTK and Libadwaita versions an
/// observation carries.
#[must_use]
pub fn text_toolkit_versions() -> (String, String) {
    (
        pango::version_string().to_string(),
        format!(
            "{}.{}.{}",
            sourceview5::major_version(),
            sourceview5::minor_version(),
            sourceview5::micro_version()
        ),
    )
}

/// Record [`text_toolkit_versions`] as the probe's first two values.
pub(crate) fn record_text_toolkit(recorder: &mut crate::observation::Recorder) {
    let (pango, sourceview) = text_toolkit_versions();
    recorder.measure("pango", pango);
    recorder.measure("gtksourceview", sourceview);
}

/// Microseconds `work` took.
pub fn time_us<T>(work: impl FnOnce() -> T) -> (T, u128) {
    let start = Instant::now();
    let value = work();
    (value, start.elapsed().as_micros())
}

/// One message the [`LogCapture`] guard kept.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedLog {
    /// `critical`, `warning`, `message`, `info`, `debug` or `error`.
    pub level: &'static str,
    /// The log domain, empty when there is none.
    pub domain: String,
    /// The message text.
    pub message: String,
}

impl std::fmt::Display for CapturedLog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}-{}: {}",
            self.domain, self.level, self.message
        )
    }
}

/// The messages kept while a [`LogCapture`] is alive; `None` when none is.
static CAPTURED: Mutex<Option<Vec<CapturedLog>>> = Mutex::new(None);
static WRITER: OnceLock<()> = OnceLock::new();

/// Keeps every warning and critical GLib logs while it is alive, instead of
/// printing them, so a probe can provoke a toolkit `CRITICAL` on purpose and
/// read it back as an observed value.
///
/// GLib accepts one structured-log writer per process and has no call to
/// restore a previous one, so the first guard installs a forwarding writer
/// once: while no guard is alive it hands every message to GLib's default
/// writer unchanged, exactly what happened before; while one is alive it keeps
/// warnings and criticals and forwards everything else. Dropping the guard
/// therefore restores the previous log handling. Debug, info and message
/// levels are never captured.
#[derive(Debug)]
pub struct LogCapture {
    _private: (),
}

impl LogCapture {
    /// Start capturing.
    #[must_use]
    pub fn start() -> Self {
        WRITER.get_or_init(|| {
            glib::log_set_writer_func(|level, fields| {
                if matches!(level, glib::LogLevel::Critical | glib::LogLevel::Warning) {
                    let mut captured = CAPTURED.lock().unwrap_or_else(PoisonError::into_inner);
                    if let Some(messages) = captured.as_mut() {
                        messages.push(captured_log(level, fields));
                        return glib::LogWriterOutput::Handled;
                    }
                }
                glib::log_writer_default(level, fields)
            });
        });
        CAPTURED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(Vec::new());
        Self { _private: () }
    }

    /// The messages kept so far.
    #[must_use]
    pub fn messages(&self) -> Vec<CapturedLog> {
        CAPTURED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_default()
    }
}

impl Drop for LogCapture {
    fn drop(&mut self) {
        CAPTURED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

fn captured_log(level: glib::LogLevel, fields: &[glib::LogField<'_>]) -> CapturedLog {
    let field = |key: &str| {
        fields
            .iter()
            .find(|field| field.key() == key)
            .and_then(glib::LogField::value_str)
            .unwrap_or_default()
            .to_owned()
    };
    CapturedLog {
        level: match level {
            glib::LogLevel::Error => "error",
            glib::LogLevel::Critical => "critical",
            glib::LogLevel::Warning => "warning",
            glib::LogLevel::Message => "message",
            glib::LogLevel::Info => "info",
            glib::LogLevel::Debug => "debug",
        },
        domain: field("GLIB_DOMAIN"),
        message: field("MESSAGE"),
    }
}

/// The `gtk-im-module` id the probe input method registers under. It is
/// selected only through a view's `im-module` property, never through
/// `GTK_IM_MODULE` or the display's `gtk-im-module` setting.
pub const PROBE_IM_ID: &str = "gtk-lush-axioms-probe";

/// One surrounding-text request the probe input method made.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImRequest {
    /// What armed it: `focus-in`, `cursor-location`, `filter-keypress` or
    /// `reset`.
    pub trigger: &'static str,
    /// Whether the client answered `retrieve-surrounding`.
    pub handled: bool,
    /// Characters of surrounding text the client supplied, or `None` when it
    /// supplied none.
    pub surrounding_chars: Option<usize>,
    /// Microseconds the client spent answering.
    pub micros: u128,
}

thread_local! {
    static IM_REQUESTS: std::cell::RefCell<Vec<ImRequest>> = const { std::cell::RefCell::new(Vec::new()) };
    static LAST_SURROUNDING: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Take every request the probe input method has recorded on this thread.
#[must_use]
pub fn take_im_requests() -> Vec<ImRequest> {
    IM_REQUESTS.with(|requests| std::mem::take(&mut *requests.borrow_mut()))
}

mod im_imp {
    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct ProbeImContext;

    #[glib::object_subclass]
    impl ObjectSubclass for ProbeImContext {
        const NAME: &str = "GtkLushAxiomsProbeIMContext";
        type Type = super::ProbeImContext;
        type ParentType = gtk4::IMContext;
    }

    impl ObjectImpl for ProbeImContext {}

    impl ProbeImContext {
        /// Ask the client for surrounding text, as an IBus engine with the
        /// surrounding-text capability does, and record the answer.
        fn request(&self, trigger: &'static str) {
            super::LAST_SURROUNDING.with(|last| last.set(None));
            let (handled, micros) =
                super::time_us(|| self.obj().emit_by_name::<bool>("retrieve-surrounding", &[]));
            let surrounding_chars =
                super::LAST_SURROUNDING
                    .with(std::cell::Cell::take)
                    .or_else(|| {
                        handled
                            .then(|| self.obj().surrounding_with_selection())
                            .flatten()
                            .map(|(text, _, _)| text.chars().count())
                    });
            super::IM_REQUESTS.with(|requests| {
                requests.borrow_mut().push(super::ImRequest {
                    trigger,
                    handled,
                    surrounding_chars,
                    micros,
                });
            });
        }
    }

    impl IMContextImpl for ProbeImContext {
        fn focus_in(&self) {
            self.request("focus-in");
        }

        fn set_cursor_location(&self, _area: &gtk4::gdk::Rectangle) {
            self.request("cursor-location");
        }

        fn filter_keypress(&self, _event: &gtk4::gdk::Event) -> bool {
            self.request("filter-keypress");
            false
        }

        fn reset(&self) {
            self.request("reset");
        }

        // The only surrounding-text vfunc gtk4-rs 0.11.3 lets a subclass
        // override (set_surrounding_with_selection is not wrapped there);
        // GTK's default set_surrounding_with_selection routes to it.
        fn set_surrounding(&self, text: &str, _cursor_index: i32) {
            super::LAST_SURROUNDING.with(|last| last.set(Some(text.chars().count())));
        }
    }
}

glib::wrapper! {
    /// A `GtkIMContext` that, on focus-in, cursor-location updates, key
    /// filtering and reset, emits `retrieve-surrounding` and records how long
    /// the client took and how many characters it supplied (never the text).
    pub struct ProbeImContext(ObjectSubclass<im_imp::ProbeImContext>)
        @extends gtk4::IMContext;
}

/// Register [`ProbeImContext`] on the `gtk-im-module` extension point as
/// [`PROBE_IM_ID`] (once per process), so a view can select it with
/// `set_im_module(Some(PROBE_IM_ID))`. Returns whether the extension point
/// lists it afterwards.
pub fn register_probe_im() -> bool {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        gtk4::gio::IOExtensionPoint::implement(
            "gtk-im-module",
            ProbeImContext::static_type(),
            PROBE_IM_ID,
            0,
        );
    });
    gtk4::gio::IOExtensionPoint::lookup("gtk-im-module")
        .and_then(|point| point.extension_by_name(PROBE_IM_ID))
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraphs_are_deterministic_and_exactly_sized() {
        for kind in [
            ParagraphKind::Ascii,
            ParagraphKind::Multibyte,
            ParagraphKind::MinifiedMarkup,
        ] {
            let first = paragraph(kind, 5_000, 7);
            assert_eq!(first, paragraph(kind, 5_000, 7));
            assert_ne!(first, paragraph(kind, 5_000, 8));
            assert_eq!(first.chars().count(), 5_000);
            assert!(!first.contains('\n'));
        }
        assert!(
            paragraph(ParagraphKind::Multibyte, 2_000, 1).len() > 2_000,
            "multibyte paragraphs carry multibyte characters"
        );
        assert!(paragraph(ParagraphKind::MinifiedMarkup, 2_000, 1).contains("<script>"));
    }
}
