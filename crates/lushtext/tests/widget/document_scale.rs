// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for the document scale policy (`document-scale-policy`, C1):
//! the GTK paragraph boundaries, the load-time gates, the edit-time paragraph
//! watch, the minimap's retire-and-rebuild, and the reduced-features notice.
//!
//! Every fixture is synthetic. Thresholds are read from
//! `lushtext_core::model::document_scale`, never restated.

use crate::common::{ensure_gtk_init, fixture, flush_events, test_application, wait_until};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::config::keys;
use lushtext_core::model::document_scale::{
    DocumentScale, L_INVISIBLES, L_MINIMAP, L_PROTECT, L_SEARCH_HIGHLIGHT, L_SYNTAX,
    LocalHistoryAvailability, R_BATCH, ReducedFeatures, S_MINIMAP, V_BATCH,
};
use lushtext_core::model::paragraph_boundary::DocumentProfile;
use lushtext_core::services::notifications::{InlineActionNotification, InlineNotificationStyle};
use lushtext_core::ui::editor_page::document_scale::REDUCED_FEATURES_NOTICE_TITLE;
use lushtext_core::ui::editor_page::{
    EditorLoadState, InlineAlertLifetime, LushtextEditorPage, MinimapAvailability,
};
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// A policy constant as a `usize` length.
fn n(value: u64) -> usize {
    usize::try_from(value).expect("policy constants fit usize")
}

/// Category of the minimap's modified-since-save source marks.
const MODIFIED_MARK_CATEGORY: &str = "lushtext-minimap-modified";

fn enable_minimap(enabled: bool) -> gio::Settings {
    let settings = lushtext_core::ui::settings_writes::shared();
    settings
        .set_boolean(keys::SHOW_MINIMAP, enabled)
        .expect("set minimap preference");
    settings
}

fn present(page: &LushtextEditorPage) -> gtk4::ApplicationWindow {
    let app = test_application();
    let window = gtk4::ApplicationWindow::builder()
        .application(&app)
        .default_width(1000)
        .default_height(600)
        .child(page)
        .build();
    window.present();
    flush_events();
    wait_until(Duration::from_secs(5), || {
        page.source_view().is_mapped() && page.source_view().visible_rect().height() > 0
    });
    window
}

fn text_of(page: &LushtextEditorPage) -> String {
    let buffer = page.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), true)
        .to_string()
}

fn load_and_wait(page: &LushtextEditorPage, path: &std::path::Path) {
    page.load_file_async(path);
    wait_until(Duration::from_secs(20), || {
        page.load_evidence().load_state == EditorLoadState::Loaded && !page.is_saving()
    });
    flush_events();
}

fn insert_at_end(page: &LushtextEditorPage, text: &str) {
    let buffer = page.buffer();
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, text);
}

fn insert_at_offset(page: &LushtextEditorPage, offset: i32, text: &str) {
    let buffer = page.buffer();
    let mut at = buffer.iter_at_offset(offset);
    buffer.insert(&mut at, text);
}

fn delete_range(page: &LushtextEditorPage, start: i32, end: i32) {
    let buffer = page.buffer();
    let mut start = buffer.iter_at_offset(start);
    let mut end = buffer.iter_at_offset(end);
    buffer.delete(&mut start, &mut end);
}

/// Whether any tag of the buffer's language (the context engine's
/// `context-classes:` tags or its style tags) toggles anywhere in the buffer.
fn has_context_tags(page: &LushtextEditorPage) -> bool {
    let buffer = page.buffer();
    let mut found = false;
    buffer.tag_table().foreach(|tag| {
        let name = tag.name().map(|name| name.to_string()).unwrap_or_default();
        let is_context_tag = name.contains("context-classes") || name.contains(':');
        if is_context_tag {
            let mut iter = buffer.start_iter();
            if iter.toggles_tag(Some(tag)) || iter.forward_to_tag_toggle(Some(tag)) {
                found = true;
            }
        }
    });
    found
}

/// Captures every GLib critical logged while it lives (warnings are the
/// widget runner's job; the headless session logs benign ones).
struct CriticalCapture;

static CAPTURED: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);
static WRITER: std::sync::OnceLock<()> = std::sync::OnceLock::new();

impl CriticalCapture {
    fn start() -> Self {
        WRITER.get_or_init(|| {
            glib::log_set_writer_func(|level, fields| {
                // The headless session's missing accessibility bus is benign
                // (the widget runner allowlists the same line).
                if level == glib::LogLevel::Critical
                    && !fields.iter().any(|field| {
                        field
                            .value_str()
                            .is_some_and(|value| value.contains("org.a11y.atspi.Registry"))
                    })
                {
                    let field = |key: &str| {
                        fields
                            .iter()
                            .find(|field| field.key() == key)
                            .and_then(|field| field.value_str().map(str::to_owned))
                            .unwrap_or_default()
                    };
                    if let Some(messages) = CAPTURED
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .as_mut()
                    {
                        messages.push(format!("{}: {}", field("GLIB_DOMAIN"), field("MESSAGE")));
                    }
                }
                glib::log_writer_default(level, fields)
            });
        });
        CAPTURED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace(Vec::new());
        Self
    }

    #[expect(
        clippy::unused_self,
        reason = "reading through the live guard ties the read to the capture's lifetime"
    )]
    fn messages(&self) -> Vec<String> {
        CAPTURED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .unwrap_or_default()
    }
}

impl Drop for CriticalCapture {
    fn drop(&mut self) {
        CAPTURED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }
}

/// Drain several frames of the page's window.
fn drain_frames(page: &LushtextEditorPage, frames: u32) {
    let Some(clock) = page.frame_clock() else {
        flush_events();
        return;
    };
    let target = clock.frame_counter() + i64::from(frames);
    wait_until(Duration::from_secs(5), || {
        page.queue_draw();
        clock.frame_counter() >= target
    });
}

// ─── 1. GTK paragraph boundaries ─────────────────────────────────────────

#[test]
fn test_minimap_marks_cover_lines_inserted_with_gtk_paragraph_delimiters() {
    ensure_gtk_init();
    let _settings = enable_minimap(true);
    let page = LushtextEditorPage::new();
    page.buffer().set_text("first");
    page.buffer().set_modified(false);
    flush_events();

    // Two CR-only lines and one U+2029 line added after line 0.
    insert_at_end(&page, "\rsecond\rthird\u{2029}fourth");
    let buffer = page.buffer();
    assert_eq!(buffer.line_count(), 4);
    for line in 0..4 {
        assert!(
            !buffer
                .source_marks_at_line(line, Some(MODIFIED_MARK_CATEGORY))
                .is_empty(),
            "line {line} inserted with a GTK paragraph delimiter must be marked modified"
        );
    }
}

#[test]
fn test_cr_crlf_and_paragraph_separator_documents_round_trip_byte_identically() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("round-trip tempdir");
    // Above the synchronous threshold, so the sliced install runs.
    let bodies = [
        ("cr", format!("{}\r", "c".repeat(999)).repeat(1_200)),
        ("crlf", format!("{}\r\n", "d".repeat(998)).repeat(1_200)),
        ("ps", format!("{}\u{2029}", "p".repeat(997)).repeat(1_200)),
        (
            "mixed",
            "alpha\rbeta\r\ngamma\u{2029}delta\n\u{2028}epsilon\u{85}\r".repeat(30_000),
        ),
    ];
    for (name, body) in bodies {
        let path = dir.path().join(format!("{name}.txt"));
        fixture::write_text(&path, &body);
        let page = LushtextEditorPage::new();
        load_and_wait(&page, &path);
        assert_eq!(
            text_of(&page),
            body,
            "{name}: load installs the decoded text"
        );

        // Save writes the same bytes back (the detected line ending is kept).
        // A mixed document is normalized on save by the existing mixed
        // line-ending policy, so its save is not a byte-identity case.
        if name == "mixed" {
            continue_with_replacement(&body, name);
            continue;
        }
        let saved = Rc::new(Cell::new(false));
        let saved_flag = Rc::clone(&saved);
        page.buffer().set_modified(true);
        page.save_file_async(move |result| {
            result.expect("save succeeds");
            saved_flag.set(true);
        });
        wait_until(Duration::from_secs(20), || saved.get());
        assert_eq!(
            fixture::read_bytes(&path),
            body.as_bytes(),
            "{name}: save writes the bytes back unchanged"
        );

        continue_with_replacement(&body, name);
    }
}

/// A buffer replacement (the draft-recovery installer) installs `body`
/// byte-identically.
fn continue_with_replacement(body: &str, name: &str) {
    let replacement = LushtextEditorPage::new();
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    replacement.replace_buffer_for_test(
        body.to_string(),
        1,
        Rc::new(Cell::new(true)),
        Rc::clone(&outcomes),
    );
    wait_until(Duration::from_secs(20), || !outcomes.borrow().is_empty());
    assert_eq!(text_of(&replacement), body, "{name}: replacement");
}

// ─── 4. Save and identity republication ─────────────────────────────────

#[test]
fn test_saved_small_document_keeps_gated_syntax_after_rename() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("rename tempdir");
    let path = dir.path().join("long.html");
    fixture::write_text(&path, &format!("<p>{}</p>\n", "w".repeat(3 * n(L_SYNTAX))));
    let page = LushtextEditorPage::new();
    load_and_wait(&page, &path);
    assert!(!page.document_scale().syntax_enabled);
    assert!(page.buffer().language().is_none());

    // Delete the long paragraph and save a tiny document.
    page.buffer().set_text("<p>short</p>\n");
    let saved = Rc::new(Cell::new(false));
    let saved_flag = Rc::clone(&saved);
    page.save_file_async(move |result| {
        result.expect("save succeeds");
        saved_flag.set(true);
    });
    wait_until(Duration::from_secs(10), || saved.get());
    assert!(
        !page.document_scale().syntax_enabled,
        "a save never re-enables a gated presentation feature"
    );

    // Rename (identity republication) must not reapply the language.
    page.set_file_path(&dir.path().join("renamed.html"));
    flush_events();
    assert!(page.buffer().language().is_none());
    assert!(!page.buffer().is_highlight_syntax());
}

#[test]
fn test_save_below_ten_megabytes_restores_full_local_history() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("history tempdir");
    let path = dir.path().join("shrunk.txt");
    fixture::write_text(&path, "small\n");
    let page = LushtextEditorPage::new();
    load_and_wait(&page, &path);
    // Promote it to the save-only history tier, as a 10 MB+ load would.
    page.apply_loaded_content_for_test("small\n", 10_000_001);
    assert_eq!(
        page.document_scale().local_history,
        LocalHistoryAvailability::SaveOnly
    );
    assert!(!page.document_scale().syntax_enabled);

    page.buffer().set_text("smaller\n");
    let saved = Rc::new(Cell::new(false));
    let saved_flag = Rc::clone(&saved);
    page.save_file_async(move |result| {
        result.expect("save succeeds");
        saved_flag.set(true);
    });
    wait_until(Duration::from_secs(10), || saved.get());
    let scale = page.document_scale();
    assert_eq!(scale.local_history, LocalHistoryAvailability::Full);
    assert!(!scale.syntax_enabled, "presentation features stay off");
}

// ─── 5. Load-time gating ─────────────────────────────────────────────────

#[test]
fn test_giant_paragraph_file_loads_without_language_or_context_tags() {
    ensure_gtk_init();
    let capture = CriticalCapture::start();
    let dir = tempfile::tempdir().expect("giant paragraph tempdir");
    let path = dir.path().join("minified.html");
    let body = format!(
        "<html>{}</html>\n<p>tail</p>\n",
        "<div class=\"x\">y</div>".repeat(n(L_SYNTAX))
    );
    fixture::write_text(&path, &body);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    load_and_wait(&page, &path);
    for _ in 0..10 {
        drain_frames(&page, 2);
        flush_events();
    }
    let scale = page.document_scale();
    assert!(!scale.syntax_enabled && !scale.bracket_matching_enabled);
    assert!(page.buffer().language().is_none());
    assert!(!page.buffer().is_highlight_syntax());
    assert!(!page.buffer().is_highlight_matching_brackets());
    assert!(!has_context_tags(&page));
    assert!(capture.messages().is_empty(), "{:?}", capture.messages());
}

#[test]
fn test_gated_search_session_keeps_counts_without_highlighting() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    page.buffer().set_text(&format!(
        "{}\nneedle\n",
        "x".repeat(n(L_SEARCH_HIGHLIGHT) + 1)
    ));
    assert!(!page.document_scale().search_highlight_allowed);
    page.search_bar().attach(&page.buffer(), page.source_view());
    let context = page
        .search_bar()
        .search_context()
        .expect("attached session");
    assert!(!context.is_highlight());
    context.settings().set_search_text(Some("needle"));
    wait_until(Duration::from_secs(5), || context.occurrences_count() == 1);
}

#[test]
fn test_invisibles_are_forced_off_for_gated_documents() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    page.set_invisible_characters_mode(
        lushtext_core::model::encoding::InvisibleCharactersMode::All,
    );
    page.buffer()
        .set_text(&format!("{}\n", "z".repeat(n(L_INVISIBLES) + 1)));
    assert!(!page.document_scale().invisibles_allowed);
    assert!(!page.source_view().space_drawer().enables_matrix());
    // The per-document mode is kept for the other documents.
    assert_eq!(
        page.invisible_characters_mode(),
        lushtext_core::model::encoding::InvisibleCharactersMode::All
    );
}

// ─── 6. The edit-time watch ─────────────────────────────────────────────

#[test]
fn test_paste_creating_a_giant_paragraph_downgrades_within_the_emission() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    page.buffer().set_language(
        sourceview5::LanguageManager::default()
            .language("html")
            .as_ref(),
    );
    page.buffer().set_highlight_syntax(true);
    page.buffer().set_text("<p>short</p>\n");
    assert!(page.document_scale().syntax_enabled);

    // An after-handler connected later than the watch observes the state
    // before the emission returns.
    let observed = Rc::new(RefCell::new(None));
    let observer = Rc::clone(&observed);
    let handler = page
        .buffer()
        .connect_local("insert-text", true, move |values| {
            let buffer = values[0].get::<sourceview5::Buffer>().ok()?;
            observer.replace(Some((
                buffer.language().is_none(),
                buffer.is_highlight_syntax(),
            )));
            None
        });
    insert_at_end(
        &page,
        &format!("<div>{}</div>", "<b>v</b>".repeat(n(L_SYNTAX))),
    );
    page.buffer().disconnect(handler);
    assert_eq!(*observed.borrow(), Some((true, false)));
    for _ in 0..5 {
        drain_frames(&page, 2);
    }
    assert!(!has_context_tags(&page));
    assert!(!page.document_scale().syntax_enabled);
}

#[test]
fn test_joining_lines_downgrades_in_the_delete_range_emission() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let half = n(L_SYNTAX / 2 + 1);
    page.buffer()
        .set_text(&format!("{}\n{}", "a".repeat(half), "b".repeat(half)));
    assert!(page.document_scale().syntax_enabled);
    let offset = i32::try_from(half).expect("offset fits i32");
    delete_range(&page, offset, offset + 1);
    assert!(!page.document_scale().syntax_enabled);
    assert_eq!(
        page.document_scale_evidence().longest_paragraph_chars,
        2 * half as u64
    );
}

#[test]
fn test_a_merge_raising_only_density_is_recomputed() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    // R_BATCH paragraphs that sum exactly to V_BATCH, then one separator line
    // and one more paragraph: deleting the separator line makes the last
    // paragraph join the window without any paragraph growing.
    let per_line = V_BATCH / R_BATCH as u64;
    let line = "d".repeat(n(per_line));
    let mut text = vec![line; R_BATCH - 1].join("\n");
    text.push_str("\n\n");
    text.push_str(&"d".repeat(n(per_line + 1)));
    page.buffer().set_text(&text);
    let before = page.document_scale_evidence();
    assert!(!before.scale.protected_view);
    // Delete the empty separator line: a line removed, no paragraph merged.
    let separator = text.find("\n\n").expect("separator line");
    let offset = i32::try_from(separator + 1).expect("offset fits i32");
    delete_range(&page, offset, offset + 1);
    let after = page.document_scale_evidence();
    assert!(after.watch_window_steps > before.watch_window_steps);
    assert!(
        after.scale.protected_view,
        "density over V_BATCH sets protected_view"
    );
}

#[test]
fn test_edits_between_cr_and_lf_follow_the_buffer_lines() {
    ensure_gtk_init();
    // The buffer's lines depend on its edit history (ledger A42): deleting
    // the text between `\r` and `\n` leaves "aa\r" and "\n" as separate lines,
    // so no line is removed and nothing grows.
    let page = LushtextEditorPage::new();
    page.buffer().set_text("aa\rX\nbb");
    let before = page.document_scale_evidence();
    delete_range(&page, 3, 4);
    assert_eq!(page.buffer().line_count(), 3);
    let after_delete = page.document_scale_evidence();
    assert_eq!(after_delete.watch_window_steps, before.watch_window_steps);
    assert_eq!(after_delete.watch_line_reads, before.watch_line_reads);

    // Typing between a `\r` and an `\n` adds no line but turns the CRLF
    // ending into LF ("aa\rY", four characters, `\r` included): a window
    // step, because the line grew by more than the typed character.
    let page = LushtextEditorPage::new();
    page.buffer().set_text(&format!("aa\r\n{}", "b".repeat(2)));
    let before = page.document_scale_evidence();
    insert_at_offset(&page, 3, "Y");
    assert_eq!(page.buffer().line_count(), 2);
    let after_insert = page.document_scale_evidence();
    assert_eq!(
        after_insert.watch_window_steps,
        before.watch_window_steps + 1
    );
    assert_eq!(after_insert.longest_paragraph_chars, 4);

    // Deleting the `\n` of a CRLF removes a line start: the lines join and
    // the watch recomputes the windows.
    let page = LushtextEditorPage::new();
    page.buffer().set_text("aa\r\nbb");
    let before = page.document_scale_evidence();
    delete_range(&page, 3, 4);
    assert_eq!(page.buffer().line_count(), 1);
    let after = page.document_scale_evidence();
    assert_eq!(after.watch_window_steps, before.watch_window_steps + 1);
    assert_eq!(after.longest_paragraph_chars, 5);
}

#[test]
fn test_fast_keystrokes_cannot_cross_v_batch_unseen() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let per_line = n(V_BATCH / R_BATCH as u64);
    let mut lines = vec!["e".repeat(per_line); R_BATCH];
    // Leave ten characters of room in the only full window.
    lines[0] = "e".repeat(per_line - 10);
    page.buffer().set_text(&lines.join("\n"));
    let start = page.document_scale_evidence();
    assert!(!start.scale.protected_view);
    for keystroke in 1..=11 {
        insert_at_offset(&page, 0, "k");
        let now = page.document_scale_evidence();
        if keystroke <= 10 {
            assert!(!now.scale.protected_view, "keystroke {keystroke}");
        } else {
            assert!(now.scale.protected_view, "the crossing keystroke sets it");
            assert_eq!(now.watch_fast_steps, start.watch_fast_steps + 10);
            assert_eq!(now.watch_window_steps, start.watch_window_steps + 1);
        }
    }
}

#[test]
fn test_edit_watch_and_load_profile_agree_at_the_threshold() {
    ensure_gtk_init();
    for delimiter in ["\n", "\r", "\r\n", "\u{2029}"] {
        let page = LushtextEditorPage::new();
        let tail = format!("{delimiter}next");
        page.buffer().set_text(&tail);
        // Type up to exactly L_SYNTAX characters into the first paragraph.
        insert_at_offset(&page, 0, &"t".repeat(n(L_SYNTAX) - 1));
        insert_at_offset(&page, 0, "t");
        let text = text_of(&page);
        let load_decision =
            DocumentScale::for_profile(DocumentProfile::of_text(&text).summary()).syntax_enabled;
        assert!(
            page.document_scale().syntax_enabled,
            "{delimiter:?} at L_SYNTAX"
        );
        assert!(load_decision);
        insert_at_offset(&page, 0, "t");
        let text = text_of(&page);
        assert!(
            !page.document_scale().syntax_enabled,
            "{delimiter:?} at L_SYNTAX + 1"
        );
        assert!(
            !DocumentScale::for_profile(DocumentProfile::of_text(&text).summary()).syntax_enabled
        );
    }
}

#[test]
fn test_deleting_the_giant_line_and_saving_restores_nothing() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("delete-and-save tempdir");
    let path = dir.path().join("note.txt");
    fixture::write_text(&path, "short\n");
    let page = LushtextEditorPage::new();
    load_and_wait(&page, &path);
    insert_at_end(&page, &"m".repeat(n(L_MINIMAP) + 1));
    let reduced = page.document_scale();
    assert!(!reduced.minimap_allowed && !reduced.syntax_enabled);
    page.buffer().set_text("short\n");
    let saved = Rc::new(Cell::new(false));
    let saved_flag = Rc::clone(&saved);
    page.save_file_async(move |result| {
        result.expect("save succeeds");
        saved_flag.set(true);
    });
    wait_until(Duration::from_secs(10), || saved.get());
    let after = page.document_scale();
    assert!(!after.minimap_allowed && !after.syntax_enabled && !after.search_highlight_allowed);
}

#[test]
fn test_watch_reads_per_keystroke_are_constant() {
    ensure_gtk_init();
    for line_count in [10usize, 10_000] {
        let page = LushtextEditorPage::new();
        page.buffer().set_text(&"line of text\n".repeat(line_count));
        let middle = i32::try_from(line_count / 2 * 13).expect("offset fits i32");
        let before = page.document_scale_evidence();
        for _ in 0..20 {
            insert_at_offset(&page, middle, "q");
        }
        let after = page.document_scale_evidence();
        // Ordinary typing stays on the fast path: one line read per keystroke.
        assert_eq!(after.watch_fast_steps - before.watch_fast_steps, 20);
        assert_eq!(after.watch_line_reads - before.watch_line_reads, 20);
        // A keystroke that adds a line reads at most 2·R_BATCH lines.
        let reads_before = after.watch_line_reads;
        insert_at_offset(&page, middle, "\n");
        let reads = page.document_scale_evidence().watch_line_reads - reads_before;
        assert!(reads <= 2 * R_BATCH as u64 + 2, "{line_count}: {reads}");
    }
}

#[test]
fn test_install_slices_do_no_watch_work() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("install tempdir");
    let path = dir.path().join("big.txt");
    fixture::write_text(&path, &"slice line\n".repeat(150_000));
    let page = LushtextEditorPage::new();
    let before = page.document_scale_evidence();
    load_and_wait(&page, &path);
    let after = page.document_scale_evidence();
    assert_eq!(after.watch_line_reads, before.watch_line_reads);
    assert_eq!(after.watch_fast_steps, before.watch_fast_steps);
    assert_eq!(after.watch_window_steps, before.watch_window_steps);
    assert_eq!(after.longest_paragraph_chars, 10, "the worker profile");
}

#[test]
fn test_crossing_l_protect_changes_no_text_or_modified_state() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    page.buffer().set_text("seed\n");
    page.buffer().set_modified(false);
    let addition = "p".repeat(n(L_PROTECT) + 1);
    insert_at_end(&page, &addition);
    assert!(page.document_scale().protected_view);
    assert_eq!(text_of(&page), format!("seed\n{addition}"));
    assert!(
        page.buffer().is_modified(),
        "only the edit itself modified the buffer"
    );
    assert!(
        page.document_scale()
            .reduced
            .describe()
            .is_some_and(|text| text.contains("Protected View"))
    );
}

// ─── 8. Minimap retire-and-rebuild ──────────────────────────────────────

#[test]
fn test_minimap_retirement_after_scrolling_emits_no_critical() {
    ensure_gtk_init();
    let _settings = enable_minimap(true);
    let capture = CriticalCapture::start();
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    page.buffer()
        .set_text(&"scrolled line of text\n".repeat(4_000));
    wait_until(Duration::from_secs(5), || {
        page.minimap_evidence().projection_attached
    });
    drain_frames(&page, 3);

    // Scroll so the map arms its update tick, then suspend the projection in
    // the same turn, before that tick can run (the path that used to clear
    // the live map's view; ledger A32).
    let vadj = page.source_view().vadjustment().expect("editor adjustment");
    vadj.set_value(vadj.upper() / 2.0);
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    page.replace_buffer_for_test(
        "replaced\n".repeat(10),
        1,
        Rc::new(Cell::new(true)),
        Rc::clone(&outcomes),
    );
    for _ in 0..10 {
        drain_frames(&page, 2);
        flush_events();
    }
    wait_until(Duration::from_secs(5), || !outcomes.borrow().is_empty());
    let criticals: Vec<String> = capture
        .messages()
        .into_iter()
        .filter(|message| message.contains("Gtk") || message.contains("GLib-GObject"))
        .collect();
    assert!(criticals.is_empty(), "{criticals:?}");
}

#[test]
fn test_giant_paragraph_document_never_attaches_a_map() {
    ensure_gtk_init();
    let _settings = enable_minimap(true);
    let dir = tempfile::tempdir().expect("minimap tempdir");
    // Below and above the synchronous install threshold.
    for (name, repeat) in [("direct", 1usize), ("chunked", 60)] {
        let path = dir.path().join(format!("{name}.txt"));
        let giant = format!("{}\n", "n".repeat(n(L_MINIMAP) + 1));
        fixture::write_text(&path, &giant.repeat(repeat));
        let page = LushtextEditorPage::new();
        let _window = present(&page);
        // At the first byte of the load install, no map may project.
        let attached_at_install = Rc::new(Cell::new(None));
        let observer = Rc::clone(&attached_at_install);
        let page_weak = page.downgrade();
        let handler = page.buffer().connect_insert_text(move |_, _, _| {
            if observer.get().is_none()
                && let Some(page) = page_weak.upgrade()
            {
                observer.set(Some(page.minimap_evidence().projection_attached));
            }
        });
        load_and_wait(&page, &path);
        page.buffer().disconnect(handler);
        assert_eq!(attached_at_install.get(), Some(false), "{name}");
        flush_events();
        wait_until(Duration::from_secs(5), || {
            !page.minimap_evidence().work_pending
        });
        let evidence = page.minimap_evidence();
        assert!(
            !evidence.projection_attached && !evidence.map_present,
            "{name}"
        );
        assert_eq!(page.minimap_availability(), MinimapAvailability::Reduced);
    }
}

#[test]
fn test_an_edit_retires_the_map_in_the_same_emission() {
    ensure_gtk_init();
    let _settings = enable_minimap(true);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    page.buffer().set_text("short\n");
    wait_until(Duration::from_secs(5), || {
        page.minimap_evidence().projection_attached
    });
    let observed = Rc::new(Cell::new(None));
    let observer = Rc::clone(&observed);
    let page_weak = page.downgrade();
    let handler = page.buffer().connect_local("insert-text", true, move |_| {
        if let Some(page) = page_weak.upgrade() {
            observer.set(Some(page.minimap_evidence().map_present));
        }
        None
    });
    insert_at_end(&page, &"r".repeat(n(L_MINIMAP) + 1));
    page.buffer().disconnect(handler);
    assert_eq!(observed.get(), Some(false));
    flush_events();
    wait_until(Duration::from_secs(5), || {
        !page.minimap_evidence().work_pending
    });
    assert!(!page.minimap_evidence().map_present, "never re-attached");
}

#[test]
fn test_rapid_minimap_toggling_keeps_at_most_one_map_without_warnings() {
    ensure_gtk_init();
    let settings = enable_minimap(true);
    let capture = CriticalCapture::start();
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    page.buffer().set_text(&"toggle line\n".repeat(3_000));
    wait_until(Duration::from_secs(5), || {
        page.minimap_evidence().projection_attached
    });
    let vadj = page.source_view().vadjustment().expect("editor adjustment");
    for step in 0..12 {
        vadj.set_value(f64::from(step) * 40.0);
        settings
            .set_boolean(keys::SHOW_MINIMAP, step % 2 == 1)
            .expect("toggle minimap");
        drain_frames(&page, 1);
        let shell = page.imp().minimap_overlay.get();
        let mut maps = 0;
        let mut child = shell.first_child();
        while let Some(widget) = child {
            if widget.is::<sourceview5::Map>() {
                maps += 1;
            }
            child = widget.next_sibling();
        }
        assert!(maps <= 1, "step {step}: {maps} maps attached");
    }
    settings
        .set_boolean(keys::SHOW_MINIMAP, true)
        .expect("leave minimap on");
    wait_until(Duration::from_secs(5), || {
        page.minimap_evidence().projection_attached
    });
    assert!(capture.messages().is_empty(), "{:?}", capture.messages());
}

#[test]
fn test_load_suspension_retires_and_rebuilds_the_map() {
    ensure_gtk_init();
    let _settings = enable_minimap(true);
    let dir = tempfile::tempdir().expect("rebuild tempdir");
    let path = dir.path().join("chunked.txt");
    fixture::write_text(&path, &"rebuild line\n".repeat(150_000));
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    wait_until(Duration::from_secs(5), || {
        page.minimap_evidence().projection_attached
    });
    let built_before = page.minimap_evidence().maps_built;
    load_and_wait(&page, &path);
    wait_until(Duration::from_secs(10), || {
        page.minimap_evidence().projection_attached
    });
    let evidence = page.minimap_evidence();
    assert!(evidence.maps_built > built_before, "a fresh map was built");
    assert_eq!(page.minimap_availability(), MinimapAvailability::Visible);
}

#[test]
fn test_rebuilt_map_matches_a_constructed_map() {
    ensure_gtk_init();
    let _settings = enable_minimap(true);
    let page = LushtextEditorPage::new();
    let _window = present(&page);
    page.buffer().set_text("constructed\n");
    wait_until(Duration::from_secs(5), || {
        page.minimap_evidence().projection_attached
    });
    drain_frames(&page, 3);
    wait_until(Duration::from_secs(5), || {
        !page.minimap_evidence().work_pending
    });
    drain_frames(&page, 2);
    let constructed = page.minimap_source_map_widget().expect("constructed map");
    let describe = |map: &sourceview5::Map| {
        (
            map.wrap_mode(),
            map.margin_start(),
            map.margin_end(),
            map.is_editable(),
            map.is_cursor_visible(),
            map.css_classes(),
            map.view().is_some(),
            map.hexpands(),
            map.vexpands(),
        )
    };
    let constructed_shape = describe(&constructed);
    drop(constructed);

    let outcomes = Rc::new(RefCell::new(Vec::new()));
    page.replace_buffer_for_test(
        "rebuilt\n".to_string(),
        1,
        Rc::new(Cell::new(true)),
        Rc::clone(&outcomes),
    );
    wait_until(Duration::from_secs(5), || !outcomes.borrow().is_empty());
    wait_until(Duration::from_secs(5), || {
        page.minimap_evidence().projection_attached
    });
    // Geometry that reads the laid-out map settles after its first layout.
    drain_frames(&page, 3);
    wait_until(Duration::from_secs(5), || {
        !page.minimap_evidence().work_pending
    });
    drain_frames(&page, 2);
    let rebuilt = page.minimap_source_map_widget().expect("rebuilt map");
    assert_eq!(describe(&rebuilt), constructed_shape);
    // The reflow freeze is re-pointed at the new instance.
    let render_hold = page.imp().minimap.render_hold.borrow();
    let live_child = render_hold
        .as_ref()
        .expect("render hold")
        .live_child()
        .clone();
    assert_eq!(live_child, rebuilt.upcast::<gtk4::Widget>());
}

// ─── 9. The reduced-features notice ─────────────────────────────────────

fn info_bar_title(page: &LushtextEditorPage) -> String {
    page.info_bar().imp().alert_title.label().to_string()
}

fn info_bar_body(page: &LushtextEditorPage) -> String {
    page.info_bar().imp().alert_body.label().to_string()
}

/// Take the shown alert off a windowless page's own bar (a user dismissal).
fn dismiss_alert(page: &LushtextEditorPage) {
    page.info_bar().render_notification(None);
}

fn notice_visible(page: &LushtextEditorPage) -> bool {
    page.info_bar().is_visible() && page.document_scale_evidence().notice_showing
}

#[test]
fn test_notice_appears_on_open_of_a_long_line_document() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("notice tempdir");
    let path = dir.path().join("long.txt");
    fixture::write_text(&path, &format!("{}\n", "l".repeat(n(L_MINIMAP) + 1)));
    let page = LushtextEditorPage::new();
    load_and_wait(&page, &path);
    wait_until(Duration::from_secs(5), || notice_visible(&page));
    let label = info_bar_title(&page);
    assert_eq!(label, REDUCED_FEATURES_NOTICE_TITLE);
    let body = info_bar_body(&page).to_lowercase();
    assert!(body.contains("syntax highlighting"), "{body}");
    assert!(body.contains("the minimap"), "{body}");
    assert!(body.contains("because of very long lines"), "{body}");
}

#[test]
fn test_recovery_alert_takes_precedence_over_the_notice() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("precedence tempdir");
    let path = dir.path().join("restored.txt");
    fixture::write_text(&path, &format!("{}\n", "r".repeat(n(L_SYNTAX) + 1)));
    let page = LushtextEditorPage::new();
    let restored_alert = InlineActionNotification {
        style: InlineNotificationStyle::Warning,
        title: "Unsaved Changes Restored".to_string(),
        body: "A recovery draft was applied.".to_string(),
        primary_button: Some("_Discard…".to_string()),
        secondary_button: Some("_Save…".to_string()),
    };
    let page_weak = page.downgrade();
    let alert = restored_alert.clone();
    page.connect_file_loaded(move || {
        if let Some(page) = page_weak.upgrade() {
            page.emit_inline_notification(alert.clone(), InlineAlertLifetime::SupersededByLoad);
        }
    });
    load_and_wait(&page, &path);
    flush_events();
    assert_eq!(info_bar_title(&page), restored_alert.title);
    assert!(page.document_scale_evidence().notice_pending);
    dismiss_alert(&page);
    wait_until(Duration::from_secs(5), || notice_visible(&page));
}

#[test]
fn test_notice_reappears_for_an_additional_downgrade_after_dismissal() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    page.buffer()
        .set_text(&format!("{}\n", "s".repeat(n(L_SYNTAX) + 1)));
    wait_until(Duration::from_secs(5), || notice_visible(&page));
    dismiss_alert(&page);
    flush_events();
    assert!(!page.info_bar().is_visible());
    // The same reductions do not raise it again.
    insert_at_end(&page, "more");
    flush_events();
    assert!(!page.info_bar().is_visible());
    // A newly reduced feature does.
    insert_at_end(&page, &"i".repeat(n(L_INVISIBLES) + 1));
    wait_until(Duration::from_secs(5), || notice_visible(&page));
    assert!(info_bar_body(&page).contains("invisible characters"));
}

#[test]
fn test_notice_announcement_is_bounded_and_content_free() {
    ensure_gtk_init();
    let page = LushtextEditorPage::new();
    let secret = "SECRET-CONTENT-";
    let body = format!("{}{}\n", secret, "c".repeat(n(S_MINIMAP) / 1_000));
    page.buffer().set_text(&body.repeat(2));
    insert_at_end(&page, &"x".repeat(n(L_INVISIBLES) + 1));
    wait_until(Duration::from_secs(5), || notice_visible(&page));
    let shown = info_bar_body(&page);
    assert!(!shown.contains(secret));
    assert!(shown.chars().count() <= 320);
    let description = page.document_scale().reduced.describe().unwrap_or_default();
    assert!(!description.contains(secret));
    assert_ne!(page.document_scale().reduced, ReducedFeatures::NONE);
}
