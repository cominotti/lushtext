// SPDX-License-Identifier: GPL-3.0-or-later

//! The post-open settle of an opened document (`bound-post-open-settle`):
//! the deferred language attach stage (design D2, review R1, R2, R6, R8), the
//! paced install of dense documents (D6.1, R13, R14), EditorConfig applied
//! before the first insert (D6.2, R17), and the settle readiness the
//! `document-settled` predicate reads (D1).
//!
//! Assertions read the load workflow's evidence surface (`LoadEvidence`) and
//! the buffer's language, through end-state waits.

use crate::common::{
    active_editor, ensure_gtk_init, flush_after_delay, present_window, test_window, wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::buffer_replacement::V_SLICE;
use lushtext_core::ui::editor_page::load::policy::{ATTACH_DEADLINE_US, SettleStage};
use lushtext_core::ui::editor_page::{EditorLoadState, LushtextEditorPage};
use lushtext_core::ui::window::LushtextWindow;
use sourceview5::prelude::{BufferExt as _, ViewExt as _};
use std::time::Duration;

/// Async budget: a load plus the first frame plus the low-priority attach.
const SETTLE_BUDGET: Duration = Duration::from_secs(10);

fn write(dir: &tempfile::TempDir, name: &str, text: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    lushtext_core::services::filesystem::fixture::write_text(&path, text);
    path
}

fn language_id(editor: &LushtextEditorPage) -> Option<String> {
    editor
        .buffer()
        .language()
        .map(|language| language.id().to_string())
}

fn opened(window: &LushtextWindow, path: &std::path::Path) -> LushtextEditorPage {
    window.open_document(path);
    let editor = active_editor(window);
    wait_until(SETTLE_BUDGET, || {
        editor.load_state() == EditorLoadState::Loaded
    });
    editor
}

const HTML: &str = "<!doctype html>\n<html><body><p class=\"a\">hello</p></body></html>\n";

#[test]
fn test_the_activation_dispatch_attaches_no_language() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "page.html", HTML);
    window.open_document(&path);
    let editor = active_editor(&window);
    // `open_document` returned: the activation turn is over, and no language
    // was attached inside it.
    assert_eq!(
        language_id(&editor),
        None,
        "the activation turn attached a language"
    );
    wait_until(SETTLE_BUDGET, || {
        language_id(&editor).as_deref() == Some("html")
    });
    let attach = editor.load_evidence().language_attach;
    assert!(
        attach.first_frame_seen,
        "the attach ran before the editor's first frame"
    );
    assert_eq!(
        attach.stages_run, 1,
        "the language came from the attach stage"
    );
    assert!(!attach.pending);
}

#[test]
fn test_neither_publish_nor_the_replacement_terminal_attaches() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "page.html", HTML);
    let editor = opened(&window, &path);
    wait_until(SETTLE_BUDGET, || {
        language_id(&editor).as_deref() == Some("html")
    });
    // Every language the editor holds came through one stage (review R1).
    assert_eq!(editor.load_evidence().language_attach.stages_run, 1);
    // A reload of the same document requests the same language: no stage.
    editor.load_file_async(&path);
    wait_until(SETTLE_BUDGET, || {
        editor.load_state() == EditorLoadState::Loaded
    });
    flush_after_delay(Duration::from_millis(200));
    assert_eq!(language_id(&editor).as_deref(), Some("html"));
    assert_eq!(editor.load_evidence().language_attach.stages_run, 1);
}

#[test]
fn test_a_markdown_document_is_markdown_while_its_attach_is_pending() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "notes.md", "# Heading\n\nBody text.\n");
    window.open_document(&path);
    let editor = active_editor(&window);
    let mut saw_pending = false;
    wait_until(SETTLE_BUDGET, || {
        if editor.load_evidence().language_attach.pending {
            saw_pending = true;
            assert_eq!(
                editor.document_language_id().as_deref(),
                Some("markdown"),
                "a pending attach must report the requested language (review R2)"
            );
        }
        language_id(&editor).as_deref() == Some("markdown")
    });
    assert!(saw_pending, "the attach stage was never observed pending");
}

#[test]
fn test_a_tab_that_is_not_selected_attaches_only_once_shown() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let first = write(&dir, "first.html", HTML);
    let second = write(&dir, "second.txt", "plain\n");
    let background = opened(&window, &first);
    let _foreground = opened(&window, &second);
    // Reload the background tab while it is not mapped: its attach waits.
    background.set_file_path(&dir.path().join("renamed.rs"));
    flush_after_delay(Duration::from_millis(500));
    assert!(!background.source_view().is_mapped());
    assert!(background.load_evidence().language_attach.pending);
    assert_eq!(background.document_language_id().as_deref(), Some("rust"));
    let tab_view = &window.imp().tab_view;
    tab_view.set_selected_page(&tab_view.page(&background));
    wait_until(SETTLE_BUDGET, || {
        language_id(&background).as_deref() == Some("rust")
    });
}

#[test]
fn test_the_attach_waits_for_input_quiet_until_its_deadline() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "page.html", HTML);
    window.open_document(&path);
    let editor = active_editor(&window);
    // Keys keep arriving: the attach must not run before the deadline.
    let started = std::time::Instant::now();
    let deadline = Duration::from_micros(u64::try_from(ATTACH_DEADLINE_US).expect("positive"));
    while started.elapsed() < deadline / 2 {
        lushtext_core::ui::input_quiet::simulate_user_input();
        flush_after_delay(Duration::from_millis(5));
    }
    assert_eq!(
        language_id(&editor),
        None,
        "the attach ran while keys arrived"
    );
    // Under continuous input, the deadline lets it run anyway.
    // Input keeps arriving every few milliseconds, far inside the quiet
    // interval, so only the deadline can let the attach run.
    while started.elapsed() < SETTLE_BUDGET && language_id(&editor).as_deref() != Some("html") {
        lushtext_core::ui::input_quiet::simulate_user_input();
        flush_after_delay(Duration::from_millis(2));
    }
    assert_eq!(language_id(&editor).as_deref(), Some("html"));
    assert!(
        started.elapsed() >= deadline,
        "the attach ran before its deadline"
    );
    assert_eq!(editor.load_evidence().language_attach.deadline_runs, 1);
}

#[test]
fn test_an_opened_document_reaches_settled() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "page.html", HTML);
    let editor = opened(&window, &path);
    wait_until(SETTLE_BUDGET, || {
        editor.load_evidence().settle_stage == SettleStage::Settled
    });
    assert_eq!(language_id(&editor).as_deref(), Some("html"));
}

/// `lines` lines of `width` visible characters.
fn dense(width: usize, lines: usize) -> String {
    format!("{}\n", "d".repeat(width)).repeat(lines)
}

#[test]
fn test_a_dense_document_installs_in_paced_slices() {
    for wrap in [false, true] {
        ensure_gtk_init();
        let window = test_window();
        present_window(&window);
        let dir = tempfile::tempdir().expect("tempdir");
        let text = dense(710, 160);
        let path = write(&dir, "dense.txt", &text);
        window.open_document(&path);
        let editor = active_editor(&window);
        editor.source_view().set_wrap_mode(if wrap {
            gtk4::WrapMode::WordChar
        } else {
            gtk4::WrapMode::None
        });
        wait_until(SETTLE_BUDGET, || {
            editor.load_state() == EditorLoadState::Loaded
        });
        let evidence = editor.load_evidence();
        assert!(
            evidence.installation_chunked,
            "a dense document must be sliced"
        );
        let visible = u64::try_from(text.len() - 160).expect("fits");
        assert!(
            evidence.installation_slice_count >= visible.div_ceil(V_SLICE),
            "{} slices for {visible} visible characters",
            evidence.installation_slice_count
        );
        assert_eq!(crate::common::editor_text(&editor), text);
    }
}

#[test]
fn test_editorconfig_tab_width_does_not_change_after_a_dense_install_starts() {
    // GtkTextView invalidates every line on a default-style property change
    // and queues no resize (A37), so on a dense document such a change after
    // the first slice re-lays out everything installed so far in one dispatch.
    // EditorConfig's tab width (R17) is therefore applied before the first
    // slice; this test fails when it is applied after it.
    //
    // `bottom-margin` is deliberately not observed. The end-of-document
    // overscroll follows the viewport height, which the editor's map handler
    // and its viewport-bounds handler already apply before a load's planning
    // worker returns, so no fixture of an opened, mapped editor changes it
    // after the first slice, with or without the before-install refresh (that
    // refresh only covers an install starting before the map idle has run).
    // Its later changes come from a resize or the first map of a background
    // tab, which the A58 exception runs at `G_PRIORITY_LOW` instead.
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    write(&dir, ".editorconfig", "root = true\n\n[*]\ntab_width = 7\n");
    let path = write(&dir, "dense.txt", &dense(710, 160));
    window.open_document(&path);
    let editor = active_editor(&window);
    let late_changes = std::rc::Rc::new(std::cell::Cell::new(0u32));
    {
        let late_changes = late_changes.clone();
        let buffer = editor.buffer();
        editor
            .source_view()
            .connect_notify_local(Some("tab-width"), move |_, _| {
                // The tab opened empty, so text in the buffer means the first
                // slice is in.
                if buffer.char_count() > 0 {
                    late_changes.set(late_changes.get() + 1);
                }
            });
    }
    wait_until(SETTLE_BUDGET, || {
        editor.load_state() == EditorLoadState::Loaded
    });
    flush_after_delay(Duration::from_millis(300));
    assert!(editor.load_evidence().installation_chunked);
    assert_eq!(editor.source_view().tab_width(), 7);
    assert_eq!(
        late_changes.get(),
        0,
        "tab-width changed after the first slice"
    );
}

#[test]
fn test_a_short_line_document_stays_direct() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "short.txt", &dense(40, 2_000));
    let editor = opened(&window, &path);
    let evidence = editor.load_evidence();
    assert!(!evidence.installation_chunked);
    assert_eq!(evidence.installation_slice_count, 0);
}

#[test]
fn test_editorconfig_is_applied_before_the_first_insert() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    write(&dir, ".editorconfig", "root = true\n\n[*]\ntab_width = 7\n");
    let path = write(&dir, "tabs.txt", "\tindented\n");
    let editor = opened(&window, &path);
    let evidence = editor.load_evidence();
    assert!(evidence.formatting_applied_before_insert);
    assert_eq!(editor.formatting_overrides().tab_width, Some(7));
    assert_eq!(editor.source_view().tab_width(), 7);
}

#[test]
fn test_the_glob_table_guesses_exactly_like_gtksourceview() {
    ensure_gtk_init();
    lushtext_core::ui::language_catalog::start();
    wait_until(SETTLE_BUDGET, lushtext_core::ui::language_catalog::is_ready);
    let table = lushtext_core::ui::language_catalog::glob_table().expect("catalog ready");
    let manager = lushtext_core::ui::language_catalog::manager();
    // A name for every glob of every shipped language, as a full path the
    // way LushText passes it (review R4), plus names that overlap globs.
    let mut names: Vec<String> = table
        .languages()
        .flat_map(|language| language.globs.iter())
        .filter(|glob| !glob.is_empty())
        .flat_map(|glob| {
            let bare = glob.replace('*', "x").replace('?', "y");
            [format!("/home/u/project/{bare}"), bare]
        })
        .collect();
    names.extend(
        [
            "/p/CMakeLists.txt",
            "CMakeLists.txt",
            "/p/x.h",
            "/p/x.in",
            "/p/Makefile",
            "/p/makefile",
            "/p/x.HTML",
            "/p/x.tar.gz",
            "/p/.bashrc",
            "/p/x.md",
            "/p/x.rs",
            "/p/x.json",
            "/p/no-extension",
            "/p/dir.with.dots/file",
        ]
        .map(str::to_owned),
    );
    assert!(names.len() > 100, "{} names", names.len());
    for name in &names {
        let expected = manager
            .guess_language(Some(name.as_str()), None::<&str>)
            .map(|language| language.id().to_string());
        assert_eq!(table.guess(name).map(str::to_owned), expected, "{name}");
    }
}

/// A presented window with a dense 710 document open and installed.
fn dense_window(name: &str) -> (LushtextWindow, LushtextEditorPage, tempfile::TempDir) {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, name, &dense(710, 400));
    let editor = opened(&window, &path);
    (window, editor, dir)
}

#[test]
fn test_a_wrap_toggle_on_a_dense_document_is_paced() {
    let (_window, editor, _dir) = dense_window("dense-wrap.txt");
    let before = editor.load_evidence().restyle;
    let wrap = editor.source_view().wrap_mode() != gtk4::WrapMode::None;
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", !wrap);
    let after_interaction = editor.load_evidence().restyle;
    assert_eq!(
        after_interaction.restyles,
        before.restyles + 1,
        "the toggle was not paced"
    );
    assert!(after_interaction.pending);
    wait_until(SETTLE_BUDGET, || !editor.load_evidence().restyle.pending);
    let done = editor.load_evidence().restyle;
    assert!(done.reveals > 1, "the rest was revealed in one dispatch");
    assert!(
        done.largest_reveal_chars <= V_SLICE,
        "a reveal dispatch made {} characters visible",
        done.largest_reveal_chars
    );
    assert_eq!(crate::common::editor_text(&editor), dense(710, 400));
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", wrap);
}

#[test]
fn test_a_document_that_is_not_dense_restyles_directly() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let editor = opened(&window, &write(&dir, "short.txt", &dense(40, 400)));
    let wrap = editor.source_view().wrap_mode() != gtk4::WrapMode::None;
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", !wrap);
    assert_eq!(editor.load_evidence().restyle.restyles, 0);
    assert_eq!(
        editor.source_view().wrap_mode() != gtk4::WrapMode::None,
        !wrap
    );
    lushtext_core::ui::settings_writes::set_boolean("word-wrap", wrap);
}

#[test]
fn test_a_restyle_paces_the_selected_dense_editor_first_and_the_other_when_shown() {
    let (window, background, dir) = dense_window("first.txt");
    let selected = opened(&window, &write(&dir, "second.txt", &dense(710, 400)));
    assert!(!background.source_view().is_mapped());
    let paced = lushtext_core::ui::editor_page::restyle::prepare_paced_restyle_everywhere();
    assert_eq!(paced.len(), 2);
    lushtext_core::ui::editor_page::restyle::start_paced_reveal_everywhere(&paced);
    wait_until(SETTLE_BUDGET, || !selected.load_evidence().restyle.pending);
    flush_after_delay(Duration::from_millis(300));
    let waiting = background.load_evidence().restyle;
    assert!(
        waiting.pending && waiting.reveals == 0,
        "a hidden editor revealed while unmapped"
    );
    let tab_view = &window.imp().tab_view;
    tab_view.set_selected_page(&tab_view.page(&background));
    wait_until(SETTLE_BUDGET, || {
        !background.load_evidence().restyle.pending
    });
    assert!(background.load_evidence().restyle.largest_reveal_chars <= V_SLICE);
}
