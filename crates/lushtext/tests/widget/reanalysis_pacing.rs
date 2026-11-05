// SPDX-License-Identifier: GPL-3.0-or-later

//! The paced re-analysis of edits (`bound-context-flip-keystrokes`, C6c):
//! an edit's re-analysis runs in bounded steps before the context engine's
//! first update, and an edit that needs more than the engine's first-update
//! allowance removes syntax highlighting for the session before any
//! `G_PRIORITY_LOW` worker slice runs (ledger A85).
//!
//! Every fixture is synthetic. Assertions read the pacing evidence
//! (`reanalysis_pacing_evidence`), the document scale and the priority of
//! the source each `highlight-updated` was emitted from.

use crate::common::{
    active_editor, ensure_gtk_init, flush_after_delay, present_window, test_window, wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::model::document_scale::{ReductionReason, ScaleFeature};
use lushtext_core::model::reanalysis_pacing::override_reanalysis_allowance_for_test;
use lushtext_core::ui::editor_page::load::policy::SettleStage;
use lushtext_core::ui::editor_page::{EditorLoadState, LushtextEditorPage};
use lushtext_core::ui::window::LushtextWindow;
use sourceview5::prelude::BufferExt as _;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// Async budget: a load, the first frame and the low-priority attach.
const SETTLE_BUDGET: Duration = Duration::from_secs(10);

fn write(dir: &tempfile::TempDir, name: &str, text: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    lushtext_core::services::filesystem::fixture::write_text(&path, text);
    path
}

/// Open `path` and wait until its language is attached and settled.
fn opened_highlighted(window: &LushtextWindow, path: &std::path::Path) -> LushtextEditorPage {
    window.open_document(path);
    let editor = active_editor(window);
    wait_until(SETTLE_BUDGET, || {
        let evidence = editor.load_evidence();
        evidence.load_state == EditorLoadState::Loaded
            && evidence.settle_stage == SettleStage::Settled
            && editor.buffer().language().is_some()
    });
    flush_after_delay(Duration::from_millis(400));
    assert!(
        editor.buffer().language().is_some(),
        "the language attached"
    );
    editor
}

/// Blank-line-free TypeScript of about `bytes` bytes: its grammar re-analyses
/// most of what follows an edit at about 2.5 µs per character (ledger A85).
fn typescript(bytes: usize) -> String {
    let mut text = String::with_capacity(bytes + 128);
    let mut n = 0;
    while text.len() < bytes {
        n += 1;
        text.push_str(&match n % 3 {
            0 => format!(
                "export function value_{n}(value: number): number {{ return value + {n}; }}\n"
            ),
            1 => format!("const label_{n}: string = \"label {n}\";\n"),
            _ => format!("interface Item{n} {{ id: number; name: string }} // {n}\n"),
        });
    }
    text
}

/// A short Rust document.
fn rust(lines: usize) -> String {
    (0..lines)
        .map(|n| format!("fn value_{n}(input: u32) -> u32 {{ input + {n} }}\n\n"))
        .collect()
}

/// The priority of the source every `highlight-updated` is emitted from.
fn watch_priorities(editor: &LushtextEditorPage) -> Rc<RefCell<Vec<i32>>> {
    let priorities: Rc<RefCell<Vec<i32>>> = Rc::default();
    let sink = Rc::clone(&priorities);
    editor.buffer().connect_highlight_updated(move |_, _, _| {
        let priority = glib::main_current_source().map_or(i32::MIN, |source| source.priority());
        sink.borrow_mut().push(priority);
    });
    priorities
}

fn type_at(editor: &LushtextEditorPage, offset: i32, text: &str) {
    let buffer = editor.buffer();
    buffer.begin_user_action();
    let mut at = buffer.iter_at_offset(offset);
    buffer.insert(&mut at, text);
    buffer.end_user_action();
}

fn status_text(window: &LushtextWindow) -> String {
    window
        .imp()
        .status_bar
        .imp()
        .message_label
        .label()
        .to_string()
}

#[test]
fn test_an_edit_needing_more_than_the_first_update_allowance_removes_highlighting_before_any_worker_slice()
 {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    // Below TypeScript's total-size gate, so the document opens highlighted;
    // a key at its start re-analyses about 10 KB (about 25 ms).
    let text = typescript(10 * 1024);
    let path = write(&dir, "large.ts", &text);
    let editor = opened_highlighted(&window, &path);
    assert!(editor.document_scale().syntax_enabled);
    // 10 KB of TypeScript re-analyses in about 25 ms here; a 2 ms allowance
    // keeps the test independent of the machine's speed.
    let _allowance = override_reanalysis_allowance_for_test(2_000);
    let priorities = watch_priorities(&editor);
    type_at(&editor, 0, "x");
    wait_until(SETTLE_BUDGET, || {
        editor.reanalysis_pacing_evidence().downgrades == 1
    });
    flush_after_delay(Duration::from_millis(300));
    let evidence = editor.reanalysis_pacing_evidence();
    assert_eq!(evidence.downgrades, 1, "{evidence:?}");
    assert!(evidence.steps >= 1, "{evidence:?}");
    assert!(!evidence.active);
    assert!(
        !priorities.borrow().contains(&glib::ffi::G_PRIORITY_LOW),
        "no worker slice ran: {:?}",
        priorities.borrow()
    );
    let scale = editor.document_scale();
    assert!(!scale.syntax_enabled);
    assert!(
        scale
            .reduced
            .has_reason(ScaleFeature::Syntax, ReductionReason::EditReanalysis)
    );
    assert!(scale.bracket_matching_enabled, "bracket matching stays");
    assert!(editor.buffer().language().is_none());
    // Text unchanged apart from the key.
    let buffer = editor.buffer();
    let now = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
    assert_eq!(now.as_str(), format!("x{text}"));
    // The reduction is announced like every other.
    wait_until(SETTLE_BUDGET, || {
        status_text(&window).contains("re-highlighting after an edit was too slow")
    });
    // Later keys run no pacing: the session keeps highlighting off.
    let sessions = editor.reanalysis_pacing_evidence().sessions;
    type_at(&editor, 1, "y");
    flush_after_delay(Duration::from_millis(200));
    assert_eq!(editor.reanalysis_pacing_evidence().sessions, sessions);
    assert!(editor.buffer().language().is_none());
}

#[test]
fn test_an_ordinary_key_in_a_small_document_is_paced_and_keeps_highlighting() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "small.rs", &rust(40));
    let editor = opened_highlighted(&window, &path);
    let priorities = watch_priorities(&editor);
    let middle = editor.buffer().char_count() / 2;
    for key in 0..5 {
        type_at(&editor, middle + key, "x");
        flush_after_delay(Duration::from_millis(30));
    }
    wait_until(SETTLE_BUDGET, || {
        !editor.reanalysis_pacing_evidence().active
    });
    let evidence = editor.reanalysis_pacing_evidence();
    assert!(evidence.sessions >= 1, "{evidence:?}");
    assert_eq!(evidence.merged, evidence.sessions, "{evidence:?}");
    assert_eq!(evidence.downgrades, 0, "{evidence:?}");
    assert!(editor.document_scale().syntax_enabled);
    assert_eq!(
        editor
            .buffer()
            .language()
            .map(|language| language.id().to_string())
            .as_deref(),
        Some("rust")
    );
    // The steps did the re-analysis: the first update found nothing to
    // report, and no worker ran.
    let priorities = priorities.borrow();
    assert!(
        !priorities.contains(&glib::ffi::G_PRIORITY_LOW),
        "{priorities:?}"
    );
    assert!(
        !priorities.contains(&glib::ffi::G_PRIORITY_HIGH_IDLE),
        "{priorities:?}"
    );
}

#[test]
fn test_a_plain_text_edit_starts_no_pacing() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write(&dir, "notes.txt", "one\ntwo\nthree\n");
    window.open_document(&path);
    let editor = active_editor(&window);
    wait_until(SETTLE_BUDGET, || {
        editor.load_evidence().load_state == EditorLoadState::Loaded
    });
    flush_after_delay(Duration::from_millis(200));
    type_at(&editor, 2, "x");
    flush_after_delay(Duration::from_millis(100));
    assert_eq!(editor.reanalysis_pacing_evidence().sessions, 0);
}

#[test]
fn test_a_reload_after_the_reduction_highlights_again() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    let text = typescript(10 * 1024);
    let path = write(&dir, "again.ts", &text);
    let editor = opened_highlighted(&window, &path);
    let allowance = override_reanalysis_allowance_for_test(2_000);
    type_at(&editor, 0, "x");
    wait_until(SETTLE_BUDGET, || {
        editor.reanalysis_pacing_evidence().downgrades == 1
    });
    assert!(!editor.document_scale().syntax_enabled);
    drop(allowance);
    // An external reload re-profiles the document: a new session.
    editor.buffer().set_modified(false);
    editor.load_file_async(&path);
    wait_until(SETTLE_BUDGET, || {
        editor.load_evidence().settle_stage == SettleStage::Settled
            && editor.buffer().language().is_some()
    });
    assert!(editor.document_scale().syntax_enabled);
}

/// C with `//` comments only: no `*/` anywhere after the cursor.
fn c_without_closers(bytes: usize) -> String {
    let mut text = String::with_capacity(bytes + 128);
    let mut n = 0;
    while text.len() < bytes {
        n += 1;
        text.push_str(&format!(
            "static int value_{n}(int x) {{ return x + {n}; }} // note {n}\n"
        ));
    }
    text
}

#[test]
fn test_a_comment_opened_at_the_start_of_a_large_c_document_downgrades_before_any_worker_slice() {
    ensure_gtk_init();
    let window = test_window();
    present_window(&window);
    let dir = tempfile::tempdir().expect("tempdir");
    // Below C's gate; `/*` at the start turns the whole document into a
    // comment, which the engine's first update cannot finish (ledger A85).
    let text = c_without_closers(80 * 1024);
    let path = write(&dir, "flip.c", &text);
    let editor = opened_highlighted(&window, &path);
    let _allowance = override_reanalysis_allowance_for_test(1_000);
    let priorities = watch_priorities(&editor);
    type_at(&editor, 0, "/*");
    wait_until(SETTLE_BUDGET, || {
        editor.reanalysis_pacing_evidence().downgrades == 1
    });
    flush_after_delay(Duration::from_millis(300));
    assert!(
        !priorities.borrow().contains(&glib::ffi::G_PRIORITY_LOW),
        "no worker slice ran: {:?}",
        priorities.borrow()
    );
    assert!(editor.buffer().language().is_none());
    assert!(
        editor
            .document_scale()
            .reduced
            .has_reason(ScaleFeature::Syntax, ReductionReason::EditReanalysis)
    );
}
