// SPDX-License-Identifier: MIT OR Apache-2.0

//! A52: a GtkSourceView language parses its definition files when it is
//! first attached to a buffer, and keeps the parsed data **on the language
//! object** for as long as a buffer's engine holds it: a second buffer
//! attaching the same language object while the first holds it parses
//! nothing, and an attach after the last buffer released it parses again
//! (GtkSourceView 5.20 `gtksourcelanguage.c`, `gtk_source_language_parse_file`
//! l. 760–805 and `_gtk_source_language_clear_ctx_data` l. 1027; the engine
//! drops its reference in `gtksourcecontextengine.c` l. 2686, and the last
//! drop clears the language's pointer, l. 2772–2780).
//!
//! The sharing is observed structurally. A language holds its manager only
//! through a weak pointer (`gtksourcelanguage.c` l. 167–169), and parsing
//! needs the manager: without it `gtk_source_language_parse_file` logs a
//! `CRITICAL` and the buffer gets no engine. So the probe parses once through
//! a private manager, drops the manager, and attaches the same language
//! object again: with a buffer still holding the parse, the second attach
//! highlights and logs nothing; once every buffer is gone, the next attach
//! logs the `CRITICAL` (captured on purpose) and highlights nothing.
//!
//! The parse cannot leave the GTK thread. A `GtkSourceBuffer`'s instance
//! init takes the **default** style-scheme manager's `classic` scheme
//! (`gtksourcebuffer.c` l. 510, `_gtk_source_style_scheme_get_default`,
//! `gtksourcestylescheme.c` l. 1666–1674), before any construct property is
//! applied, so even a buffer built with no style scheme reaches the
//! process-wide default manager; the probe builds one on a worker thread (the
//! GTK thread blocked in `join`) and finds the default manager's own `classic`
//! object on it. And attaching a language installs the engine's first-update
//! idle with `g_idle_add_full` (`gtksourcecontextengine.c`
//! `gtk_source_context_engine_attach_buffer` l. 2596, `install_first_update`
//! l. 2399–2414), which attaches to the **global default main context**,
//! whatever thread attached: the probe drains the default context, attaches a
//! language on the GTK thread, and finds a source ready on it.
//!
//! The language **catalog** can be built on a worker: a private
//! `GtkSourceLanguageManager` built there, first in the process and racing the
//! GTK thread's own first manager, loads the same language list as the GTK
//! thread, including the specs bundled as `resource:///` GResources
//! (`_gtk_source_language_new_from_file`, `gtksourcelanguage.c` l. 107–118,
//! reads them with `g_resources_lookup_data`, which resolves on the worker),
//! logs no warning, and answers `guess_language` for a path without a buffer, and a language from a second
//! private manager highlights a short HTML document on the GTK thread with
//! exactly the tags of the default manager's. Handing the worker's manager
//! object itself to the GTK thread is not observed: no gtk-rs object is
//! `Send`, `gio::Task::new` is `unsafe`, and GTK Lush forbids `unsafe` (the
//! A40 precedent), so the handoff is a property of the consumer's
//! thread-confined wrapper, not of the toolkit.
//!
//! The attach cost of each language with a fresh private manager is a
//! recorded magnitude, never a verdict: the first attach (the parse), a
//! second buffer's attach while the first holds the parse, and an attach
//! after both released it.
//!
//! LushText's post-open settle (`bound-post-open-settle`, D2) keeps the
//! attach a GTK dispatch, deferred to a low-priority idle after input-quiet,
//! and records the parse as a toolkit-floor exception against this axiom; its
//! language catalog is built on a worker.

use gtk4::glib;
use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::observation::Recorder;
use crate::probes::text::{LogCapture, time_us};
use crate::session::flush_events;
use crate::{AxiomId, Observation};

/// The languages whose attach cost the probe records.
pub const LANGUAGES: [&str; 4] = ["html", "markdown", "rust", "json"];
/// The short HTML document the highlight comparisons read.
pub const SNIPPET: &str = "<!DOCTYPE html>\n<html><body class=\"page\">\n<!-- a comment -->\n\
                           <a href=\"/p?x=1\" id=\"n1\">link &amp; text</a>\n\
                           <script>var v = \"s\"; function f(x) { return x * 2; }</script>\n\
                           <style>.c { color: red; }</style>\n</body></html>\n";
/// The critical a parse without its manager logs.
pub const MANAGER_GONE_MESSAGE: &str = "language manager was finalized";
/// The path the worker's manager guesses a language for.
pub const GUESSED_PATH: &str = "index.html";

/// One language's attach costs with a fresh private manager, in µs.
#[derive(Clone, Debug)]
pub struct AttachCosts {
    /// The language id.
    pub language: &'static str,
    /// The first attach: the definition parse.
    pub first_us: u128,
    /// A second buffer's attach while the first buffer holds the parse.
    pub shared_us: u128,
    /// An attach after every buffer released the parse (manager alive).
    pub released_us: u128,
}

/// A fresh highlighted buffer with no language.
#[must_use]
pub fn highlighted_buffer() -> sourceview5::Buffer {
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_highlight_syntax(true);
    buffer
}

/// Attach `language` to a fresh buffer, returning the buffer and the
/// microseconds the attach took.
#[must_use]
pub fn timed_attach(language: &sourceview5::Language) -> (sourceview5::Buffer, u128) {
    let buffer = highlighted_buffer();
    let ((), micros) = time_us(|| buffer.set_language(Some(language)));
    (buffer, micros)
}

/// The attach costs of `id` through a private manager made for it alone.
#[must_use]
pub fn attach_costs(id: &'static str) -> Option<AttachCosts> {
    let manager = sourceview5::LanguageManager::new();
    let language = manager.language(id)?;
    let (first, first_us) = timed_attach(&language);
    let (second, shared_us) = timed_attach(&language);
    drop((first, second));
    flush_events();
    let (third, released_us) = timed_attach(&language);
    drop(third);
    flush_events();
    Some(AttachCosts {
        language: id,
        first_us,
        shared_us,
        released_us,
    })
}

/// Describe one tag: its name, or its style for an anonymous style tag.
fn describe_tag(tag: &gtk4::TextTag) -> String {
    if let Some(name) = tag.name() {
        return name.to_string();
    }
    let foreground = if tag.is_foreground_set() {
        tag.foreground_rgba()
            .map_or_else(String::new, |rgba| rgba.to_string())
    } else {
        String::new()
    };
    let weight = if tag.is_weight_set() {
        tag.weight().to_string()
    } else {
        String::new()
    };
    format!("style(fg={foreground},w={weight})")
}

/// Highlight [`SNIPPET`] in `buffer` (which holds a language or not) and
/// describe the tags at every character offset, one entry per offset.
#[must_use]
pub fn snippet_tags(buffer: &sourceview5::Buffer) -> Vec<String> {
    buffer.set_text(SNIPPET);
    let (start, end) = buffer.bounds();
    buffer.ensure_highlight(&start, &end);
    (0..buffer.char_count())
        .map(|offset| {
            let mut tags: Vec<String> = buffer
                .iter_at_offset(offset)
                .tags()
                .iter()
                .map(describe_tag)
                .collect();
            tags.sort();
            tags.join("+")
        })
        .collect()
}

/// Characters of [`SNIPPET`] that carry at least one tag.
#[must_use]
pub fn tagged_characters(tags: &[String]) -> usize {
    tags.iter().filter(|tags| !tags.is_empty()).count()
}

/// What a buffer built on a worker thread (the GTK thread blocked in `join`)
/// reports: the address and id of its style scheme. `None` when the worker
/// panicked.
#[must_use]
pub fn worker_buffer_scheme() -> Option<(Option<usize>, Option<String>)> {
    std::thread::spawn(|| {
        let buffer: sourceview5::Buffer = glib::Object::new();
        let scheme = buffer.style_scheme();
        (
            scheme.as_ref().map(|scheme| scheme.as_ptr().addr()),
            scheme.map(|scheme| scheme.id().to_string()),
        )
    })
    .join()
    .ok()
}

/// What a private language manager built on a worker answered.
#[derive(Clone, Debug)]
pub struct WorkerCatalog {
    /// Its language ids, sorted.
    pub ids: Vec<String>,
    /// Microseconds loading the list took.
    pub list_us: u128,
    /// The id it guessed for [`GUESSED_PATH`].
    pub guess: Option<String>,
    /// Whether `g_resources_lookup_data` found [`RESOURCE_ONLY_SPEC`] on the
    /// worker before the manager was built.
    pub resource_lookup: bool,
    /// The entries `g_resources_enumerate_children` listed under
    /// [`SPEC_RESOURCE_DIR`] on the worker.
    pub resource_children: usize,
}

/// The resource directory of GtkSourceView's bundled language specs.
pub const SPEC_RESOURCE_DIR: &str = "/org/gnome/gtksourceview/language-specs/";
/// A bundled spec the probe looks up directly.
pub const RESOURCE_ONLY_SPEC: &str = "/org/gnome/gtksourceview/language-specs/t2t.lang";

/// Whether [`RESOURCE_ONLY_SPEC`] resolves and how many entries
/// [`SPEC_RESOURCE_DIR`] lists, on the calling thread.
#[must_use]
pub fn spec_resources() -> (bool, usize) {
    (
        gtk4::gio::resources_lookup_data(RESOURCE_ONLY_SPEC, gtk4::gio::ResourceLookupFlags::NONE)
            .is_ok(),
        gtk4::gio::resources_enumerate_children(
            SPEC_RESOURCE_DIR,
            gtk4::gio::ResourceLookupFlags::NONE,
        )
        .map_or(0, |children| children.len()),
    )
}

/// Build a private language manager on a worker thread, load its language
/// list and guess `GUESSED_PATH`; the manager is dropped on the worker.
/// The worker runs alone: the calling thread waits in `join`.
#[must_use]
pub fn worker_catalog() -> Option<WorkerCatalog> {
    spawn_worker_catalog().join().ok()
}

/// Start [`worker_catalog`]'s work on a worker thread and return at once, so
/// the caller can use GtkSourceView concurrently.
#[must_use]
pub fn spawn_worker_catalog() -> std::thread::JoinHandle<WorkerCatalog> {
    std::thread::spawn(|| {
        let (resource_lookup, resource_children) = spec_resources();
        let manager: sourceview5::LanguageManager = glib::Object::new();
        let (ids, list_us) = time_us(|| manager.language_ids());
        let guess = manager
            .guess_language(Some(GUESSED_PATH), None)
            .map(|language| language.id().to_string());
        let mut ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
        ids.sort();
        WorkerCatalog {
            ids,
            list_us,
            guess,
            resource_lookup,
            resource_children,
        }
    })
}

/// Probe A52.
#[must_use]
pub fn probe_a52() -> Observation {
    Recorder::run(AxiomId::new(52), |recorder| {
        crate::probes::text::record_text_toolkit(recorder);

        // The catalog built on a worker first, in a process whose GTK thread
        // has initialized GtkSourceView but used no language manager yet.
        sourceview5::init();
        let capture = LogCapture::start();
        // The worker's first resource access races the GTK thread's own
        // first language-manager use, as an application's startup worker would.
        let handle = spawn_worker_catalog();
        let (gtk_lookup, gtk_children) = spec_resources();
        let mut gtk_ids: Vec<String> = sourceview5::LanguageManager::new()
            .language_ids()
            .iter()
            .map(ToString::to_string)
            .collect();
        gtk_ids.sort();
        let cold = recorder.require(handle.join().ok(), "control: the worker built a manager")?;
        let cold_messages = capture.messages();
        drop(capture);
        let missing: Vec<&String> = gtk_ids.iter().filter(|id| !cold.ids.contains(id)).collect();
        recorder.measure(
            "cold_worker_catalog",
            format!(
                "{} ids (GTK thread {}), missing [{}], resource lookup {} / {} entries on the \
                 worker, {} / {} on the GTK thread",
                cold.ids.len(),
                gtk_ids.len(),
                super::join_entries(&missing),
                cold.resource_lookup,
                cold.resource_children,
                gtk_lookup,
                gtk_children
            ),
        );
        recorder.measure(
            "cold_worker_messages",
            super::join_entries(cold_messages.iter()),
        );
        recorder.axiom(
            cold.ids == gtk_ids && cold.resource_lookup && cold_messages.is_empty(),
            "a private manager built on a worker while the GTK thread builds its own loads the \
             same language list, bundled resource specs included, and logs nothing",
        )?;

        // Costs per language, each through a private manager of its own.
        let mut costs = Vec::new();
        for id in LANGUAGES {
            let cost = recorder.require(
                attach_costs(id),
                "control: GtkSourceView has the html, markdown, rust and json languages",
            )?;
            costs.push(format!(
                "{}: first {} us, shared {} us, after release {} us",
                cost.language, cost.first_us, cost.shared_us, cost.released_us
            ));
        }
        recorder.measure("attach_costs", super::join_entries(&costs));

        // Sharing through the language object.
        let manager = sourceview5::LanguageManager::new();
        let weak_manager = manager.downgrade();
        let language = recorder.require(
            manager.language("html"),
            "control: a private manager has the html language",
        )?;
        let (holder, _) = timed_attach(&language);
        let holder_tags = tagged_characters(&snippet_tags(&holder));
        drop(manager);
        flush_events();
        recorder.measure("holder_tagged_chars", holder_tags);
        recorder.control(
            holder_tags > 0 && weak_manager.upgrade().is_none(),
            "control: the first buffer highlights and the private manager is finalized",
        )?;
        let capture = LogCapture::start();
        let (sharer, _) = timed_attach(&language);
        let shared_tags = tagged_characters(&snippet_tags(&sharer));
        let shared_messages = capture.messages();
        drop((holder, sharer));
        flush_events();
        let (late, _) = timed_attach(&language);
        let late_tags = tagged_characters(&snippet_tags(&late));
        let late_messages = capture.messages();
        drop(capture);
        drop(late);
        flush_events();
        recorder.measure("shared_tagged_chars", shared_tags);
        recorder.measure(
            "shared_messages",
            super::join_entries(shared_messages.iter()),
        );
        recorder.measure("released_tagged_chars", late_tags);
        recorder.measure(
            "released_messages",
            super::join_entries(late_messages.iter()),
        );
        recorder.axiom(
            shared_messages.is_empty() && shared_tags > 0,
            "a second buffer attaching the language while another holds its parse needs no \
             manager and highlights",
        )?;
        recorder.axiom(
            late_tags == 0
                && late_messages.iter().any(|message| {
                    message.level == "critical" && message.message.contains(MANAGER_GONE_MESSAGE)
                }),
            "after the last buffer released the parse, the next attach parses again",
        )?;

        // A buffer built on a worker reaches the default style-scheme manager.
        let classic = recorder.require(
            sourceview5::StyleSchemeManager::default().scheme("classic"),
            "control: the default style-scheme manager has classic",
        )?;
        let _warm = highlighted_buffer();
        let (worker_scheme, worker_scheme_id) =
            recorder.require(worker_buffer_scheme(), "control: the worker built a buffer")?;
        recorder.measure("worker_buffer_scheme", worker_scheme_id.unwrap_or_default());
        recorder.axiom(
            worker_scheme == Some(classic.as_ptr().addr()),
            "a buffer built on a worker takes the default style-scheme manager's classic object",
        )?;

        // Attaching a language adds a source to the default main context.
        let buffer = highlighted_buffer();
        flush_events();
        let context = glib::MainContext::default();
        let pending_before = context.pending();
        let html = recorder.require(
            sourceview5::LanguageManager::default().language("html"),
            "control: the default manager has html",
        )?;
        buffer.set_language(Some(&html));
        let pending_after = context.pending();
        flush_events();
        drop(buffer);
        recorder.measure("pending_before_attach", pending_before);
        recorder.measure("pending_after_attach", pending_after);
        recorder.control(
            !pending_before,
            "control: nothing is ready on the drained default main context",
        )?;
        recorder.axiom(
            pending_after,
            "attaching a language makes a source ready on the default main context",
        )?;

        // The catalog half: a private manager works on a worker, and a second
        // manager's language highlights like the default manager's.
        let catalog = recorder.require(worker_catalog(), "control: the worker built a manager")?;
        let mut default_ids: Vec<String> = sourceview5::LanguageManager::default()
            .language_ids()
            .iter()
            .map(ToString::to_string)
            .collect();
        default_ids.sort();
        recorder.measure("worker_language_ids", catalog.ids.len());
        recorder.measure("worker_list_us", catalog.list_us);
        recorder.measure(
            "worker_guess",
            catalog.guess.clone().unwrap_or_else(|| "none".to_owned()),
        );
        let gtk_thread_manager = sourceview5::LanguageManager::new();
        let (_, gtk_list_us) = time_us(|| gtk_thread_manager.language_ids());
        recorder.measure("gtk_thread_list_us", gtk_list_us);
        recorder.axiom(
            catalog.ids == default_ids && catalog.guess.as_deref() == Some("html"),
            "a private manager on a worker loads the same language list and guesses html",
        )?;
        let second = recorder.require(
            gtk_thread_manager.language("html"),
            "control: a second manager has html",
        )?;
        let (default_buffer, _) = timed_attach(&html);
        let (second_buffer, _) = timed_attach(&second);
        let default_tags = snippet_tags(&default_buffer);
        let second_tags = snippet_tags(&second_buffer);
        recorder.measure("snippet_tagged_chars", tagged_characters(&default_tags));
        let mut tag_sets = default_tags.clone();
        tag_sets.sort();
        tag_sets.dedup();
        recorder.measure("snippet_tag_sets", super::join_entries(&tag_sets));
        recorder.control(
            tagged_characters(&default_tags) > 0,
            "control: the default manager's html highlights the snippet",
        )?;
        recorder.axiom(
            default_tags == second_tags,
            "a language from a second manager highlights exactly as the default manager's",
        )
    })
}
