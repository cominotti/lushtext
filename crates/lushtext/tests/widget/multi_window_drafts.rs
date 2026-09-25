// SPDX-License-Identifier: GPL-3.0-or-later

//! The draft journal with two windows of one application process.
//!
//! Every test drives two `LushtextWindow`s of one `GApplication` over one
//! isolated data directory, through production actions and the draft
//! workflow's own test delays, and reproduces a counterexample the Kani
//! two-window journal harness
//! (`services/draft_service/kani_proofs.rs::journal_invariants_hold_across_two_windows`)
//! found against the per-window journal coordinator. The programme record
//! (`docs/next/formal-verification.md`, phase 4) keeps each decoded trace.

use crate::common::{
    DraftPipelinePolicyReset, FirstDirtyAutosaveDelayReset, IsolatedDataDir,
    OrphanCleanupPolicyReset, active_editor, editor_text, ensure_gtk_init, fixture,
    flush_after_delay, isolated_data_dir, present_window, test_application, wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use lushtext_core::services::{draft_service, json_store};
use lushtext_core::ui::window::{
    LushtextWindow, set_draft_mutation_delays_for_test, set_first_dirty_autosave_delay_for_test,
    set_orphan_cleanup_delays_for_test,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long an asynchronous journal completion may take under load.
const JOURNAL_BUDGET: Duration = Duration::from_secs(10);

/// Windows of one application over one isolated data directory.
struct OneProcess {
    app: libadwaita::Application,
    first: LushtextWindow,
    // Declared last so the windows drop before the data directory is restored.
    documents: tempfile::TempDir,
    _data: IsolatedDataDir,
}

impl OneProcess {
    fn documents(&self) -> &Path {
        self.documents.path()
    }

    /// Another window of the same application, as a "New Window" would
    /// create it, once its startup data flow has settled.
    fn open_window(&self) -> LushtextWindow {
        let window = LushtextWindow::new(&self.app);
        present_window(&window);
        wait_until(JOURNAL_BUDGET, || startup_settled(&window));
        window
    }
}

/// Whether a window's startup data flow has restored (or skipped) its session.
fn startup_settled(window: &LushtextWindow) -> bool {
    let evidence = window.session_restore_evidence();
    !evidence.startup_descriptors_pending
        && !evidence.startup_load_cancellable
        && !evidence.restoring
        && !window.draft_evidence().blocks_readiness
}

/// Build the first window of one application and let its startup restore
/// settle. `seed` runs against the empty data directory before it exists.
fn one_process(seed: impl FnOnce(&Path, &Path)) -> OneProcess {
    ensure_gtk_init();
    let data = isolated_data_dir();
    let documents = tempfile::tempdir().expect("documents tempdir");
    seed(&json_store::data_dir(), documents.path());
    let app = test_application();
    let first = LushtextWindow::new(&app);
    present_window(&first);
    wait_until(JOURNAL_BUDGET, || startup_settled(&first));
    OneProcess {
        app,
        first,
        documents,
        _data: data,
    }
}

/// Open `path` in `window` and wait until its load has published.
fn open_loaded(window: &LushtextWindow, path: &Path) {
    window.open_document(path);
    wait_until(JOURNAL_BUDGET, || {
        let editor = active_editor(window);
        editor.file_path().as_deref() == Some(path) && editor.file_size().is_some()
    });
}

fn write_document(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    fixture::write_text(&path, text);
    path
}

fn manifest_lists(draft_id: &str) -> bool {
    draft_service::load_manifest(&json_store::data_dir())
        .expect("load manifest")
        .find_by_id(draft_id)
        .is_some()
}

fn body_of(draft_id: &str) -> Option<String> {
    draft_service::read_draft(&json_store::data_dir(), draft_id).expect("read draft body")
}

fn journal_idle(window: &LushtextWindow) -> bool {
    let evidence = window.draft_evidence();
    !evidence.autosave_inflight
        && !evidence.mutation_inflight
        && !evidence.cleanup_worker_active
        && !evidence.cleanup_timer_pending
}

/// Mark `window`'s active editor dirty with `text`.
fn edit_active(window: &LushtextWindow, text: &str) {
    let editor = active_editor(window);
    editor.buffer().set_text(text);
    editor.buffer().set_modified(true);
    assert!(editor.draft_dirty(), "the edit must make the draft dirty");
}

/// A second window's orphan cleanup, running while the first window's pass
/// sits between its write-ahead registration and its body write, must not
/// retire the entry the first window registered: otherwise the first window
/// writes a body no entry describes.
#[test]
fn test_one_windows_cleanup_keeps_another_windows_registered_entry() {
    let _policy = DraftPipelinePolicyReset;
    let _cleanup = OrphanCleanupPolicyReset;
    let _first_dirty = FirstDirtyAutosaveDelayReset;
    set_first_dirty_autosave_delay_for_test(60_000);
    let process = one_process(|_, _| {});
    let path = write_document(process.documents(), "a.txt", "on disk\n");
    let draft_id = draft_service::draft_id_for_path(&path);
    open_loaded(&process.first, &path);
    edit_active(&process.first, "window A's unsaved edits\n");

    // A registers the id, then waits in its body write.
    set_draft_mutation_delays_for_test(2_000, 0, 0);
    process.first.autosave_tick_for_test();
    wait_until(JOURNAL_BUDGET, || manifest_lists(&draft_id));
    assert_eq!(
        body_of(&draft_id),
        None,
        "A is between registration and body write"
    );

    // B's startup reads the persisted manifest and runs its orphan cleanup
    // while A's body write is still pending.
    set_orphan_cleanup_delays_for_test(0, 1, 0);
    let second = process.open_window();
    wait_until(JOURNAL_BUDGET, || {
        assert!(
            manifest_lists(&draft_id),
            "window B's cleanup retired the entry window A registered before its body write"
        );
        !(process.first.draft_evidence().autosave_inflight && body_of(&draft_id).is_none())
    });
    wait_until(JOURNAL_BUDGET, || {
        journal_idle(&process.first) && journal_idle(&second)
    });
    assert!(manifest_lists(&draft_id));
    assert_eq!(
        body_of(&draft_id).as_deref(),
        Some("window A's unsaved edits\n")
    );
}

/// One window's orphan cleanup must not delete an untitled body another
/// window wrote and has not committed yet: that window's commit would then
/// accept a draft whose body is gone.
#[test]
fn test_one_windows_cleanup_keeps_another_windows_uncommitted_untitled_body() {
    let _policy = DraftPipelinePolicyReset;
    let _cleanup = OrphanCleanupPolicyReset;
    let _first_dirty = FirstDirtyAutosaveDelayReset;
    set_first_dirty_autosave_delay_for_test(60_000);
    let process = one_process(|_, _| {});
    let second = process.open_window();
    second.new_tab();
    edit_active(&second, "window B's untitled edits\n");
    let editor = active_editor(&second);
    let draft_id = editor.draft_id().expect("untitled draft id");

    // B writes the body, then waits before its manifest commit.
    set_draft_mutation_delays_for_test(0, 1_500, 0);
    second.autosave_tick_for_test();
    wait_until(JOURNAL_BUDGET, || body_of(&draft_id).is_some());
    assert!(!manifest_lists(&draft_id), "B's commit is still pending");

    // A, the window that restored the session, runs its orphan cleanup.
    set_orphan_cleanup_delays_for_test(0, 1, 0);
    process.first.schedule_orphan_cleanup_for_test(true);
    wait_until(JOURNAL_BUDGET, || {
        journal_idle(&process.first) && journal_idle(&second)
    });
    assert!(!editor.draft_dirty(), "B's pass accepted the draft");
    assert!(
        process.first.draft_evidence().cleanup_workers_started > 0,
        "A's cleanup ran once the lane was free"
    );
    assert_eq!(
        body_of(&draft_id).as_deref(),
        Some("window B's untitled edits\n"),
        "window A's cleanup deleted the body window B was committing"
    );
    assert!(manifest_lists(&draft_id));
}

/// A second window of a process that already restored its session starts
/// empty: restoring again would open the same draft ids in two windows.
#[test]
fn test_a_second_window_does_not_restore_the_session_again() {
    let process = one_process(|data_dir, documents| {
        let path = write_document(documents, "restored.txt", "on disk\n");
        let draft_id = draft_service::draft_id_for_path(&path);
        draft_service::fixture::write_body(data_dir, &draft_id, "recovered edits\n")
            .expect("seed body");
        seed_manifest_and_session(data_dir, &path, &draft_id);
    });
    wait_until(JOURNAL_BUDGET, || {
        process.first.imp().tab_view.n_pages() == 1
            && editor_text(&active_editor(&process.first)) == "recovered edits\n"
    });
    let second = process.open_window();
    flush_after_delay(Duration::from_millis(200));
    assert_eq!(
        second.imp().tab_view.n_pages(),
        0,
        "the second window restored the session's draft-backed tab again"
    );
}

/// Opening a file already open in another window presents that window's tab
/// instead of creating a second editor for the same draft id.
#[test]
fn test_the_same_file_opened_in_two_windows_keeps_one_editor() {
    let process = one_process(|_, _| {});
    let second = process.open_window();
    let path = write_document(process.documents(), "shared.txt", "on disk\n");
    open_loaded(&process.first, &path);
    edit_active(&process.first, "edits in window A\n");
    second.open_document(&path);
    flush_after_delay(Duration::from_millis(200));
    assert_eq!(
        second.imp().tab_view.n_pages(),
        0,
        "window B opened a second editor for a file window A owns"
    );
    assert_eq!(process.first.imp().tab_view.n_pages(), 1);
    assert_eq!(
        editor_text(&active_editor(&process.first)),
        "edits in window A\n"
    );
}

fn seed_manifest_and_session(data_dir: &Path, path: &Path, draft_id: &str) {
    use lushtext_core::model::draft::{DraftEntry, DraftManifest};
    use lushtext_core::model::session::{SessionData, SessionTab};
    use lushtext_core::services::{editor_io, session_service};
    draft_service::save_manifest(
        data_dir,
        &DraftManifest {
            drafts: vec![DraftEntry {
                draft_id: draft_id.to_string(),
                original_path: Some(path.to_path_buf()),
                original_mtime_secs: editor_io::mtime_secs(path),
                saved_at_secs: 1,
            }],
            cleanup_continuation: None,
        },
    )
    .expect("seed manifest");
    session_service::save(
        data_dir,
        &SessionData {
            tabs: vec![SessionTab {
                path: Some(path.to_path_buf()),
                draft_id: None,
                cursor_line: 0,
                cursor_col: 0,
                scroll_line: 0,
                pinned: false,
            }],
            active_tab_index: Some(0),
        },
    )
    .expect("seed session");
}

/// Whichever window saves the session, the file lists every window's tabs:
/// otherwise the next startup never offers the other window's untitled draft.
#[test]
fn test_a_session_save_from_one_window_keeps_the_other_windows_tabs() {
    use lushtext_core::services::session_service;
    let process = one_process(|_, _| {});
    process.first.new_tab();
    edit_active(&process.first, "untitled work in window A\n");
    let untitled_id = active_editor(&process.first)
        .draft_id()
        .expect("untitled draft id");
    process.first.save_session_sync();
    let second = process.open_window();
    let path = write_document(process.documents(), "b.txt", "on disk\n");
    open_loaded(&second, &path);
    second.save_session_sync();
    let session = session_service::load(&json_store::data_dir()).expect("load session");
    assert!(
        session
            .tabs
            .iter()
            .any(|tab| tab.draft_id.as_deref() == Some(untitled_id.as_str())),
        "window B's session save dropped window A's untitled tab: {session:?}"
    );
    assert!(
        session
            .tabs
            .iter()
            .any(|tab| tab.path.as_deref() == Some(path.as_path())),
        "window B's session save did not record its own tab: {session:?}"
    );
}

/// A window's close flush waits for another window's autosave pass to leave
/// the journal lane, then protects its own unsaved work: neither window's
/// draft is lost to the other.
#[test]
fn test_a_window_close_flush_waits_for_another_windows_pass() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let _policy = DraftPipelinePolicyReset;
    let _first_dirty = FirstDirtyAutosaveDelayReset;
    set_first_dirty_autosave_delay_for_test(60_000);
    let process = one_process(|_, _| {});
    let second = process.open_window();
    let path = write_document(process.documents(), "a.txt", "on disk\n");
    let file_id = draft_service::draft_id_for_path(&path);
    open_loaded(&process.first, &path);
    edit_active(&process.first, "window A's edits\n");
    second.new_tab();
    edit_active(&second, "window B's edits\n");
    let untitled_id = active_editor(&second).draft_id().expect("untitled id");

    set_draft_mutation_delays_for_test(1_000, 0, 0);
    process.first.autosave_tick_for_test();
    wait_until(JOURNAL_BUDGET, || {
        process.first.draft_evidence().autosave_inflight
    });
    let result: Rc<RefCell<Option<Result<(), String>>>> = Rc::default();
    let sink = Rc::clone(&result);
    second.flush_dirty_drafts_async(move |outcome| {
        *sink.borrow_mut() = Some(outcome.map_err(|error| error.to_string()));
    });
    assert!(
        !second.draft_evidence().mutation_inflight,
        "B's close flush must not start while A's pass holds the journal lane"
    );
    wait_until(JOURNAL_BUDGET, || result.borrow().is_some());
    assert_eq!(result.borrow().clone(), Some(Ok(())));
    wait_until(JOURNAL_BUDGET, || journal_idle(&process.first));
    assert_eq!(body_of(&file_id).as_deref(), Some("window A's edits\n"));
    assert_eq!(body_of(&untitled_id).as_deref(), Some("window B's edits\n"));
    assert!(manifest_lists(&file_id) && manifest_lists(&untitled_id));
}

/// Destroying a window that holds nothing leaves the journal: the other
/// window's autosave runs at once, and restoring and claiming work as before.
#[test]
fn test_a_destroyed_window_leaves_the_journal() {
    let _policy = DraftPipelinePolicyReset;
    let _first_dirty = FirstDirtyAutosaveDelayReset;
    set_first_dirty_autosave_delay_for_test(60_000);
    let process = one_process(|_, _| {});
    let second = process.open_window();
    let path = write_document(process.documents(), "shared.txt", "on disk\n");
    open_loaded(&second, &path);
    second.destroy();
    flush_after_delay(Duration::from_millis(50));
    // The destroyed window no longer owns the path: A opens its own editor.
    open_loaded(&process.first, &path);
    edit_active(&process.first, "window A owns it now\n");
    process.first.autosave_tick_for_test();
    let draft_id = draft_service::draft_id_for_path(&path);
    wait_until(JOURNAL_BUDGET, || {
        journal_idle(&process.first) && body_of(&draft_id).is_some()
    });
    assert_eq!(
        body_of(&draft_id).as_deref(),
        Some("window A owns it now\n")
    );
}

/// A window destroyed while its autosave pass is in flight keeps the journal
/// lane until that pass finishes: another window's journal work waits for it
/// instead of racing the destroyed window's body write and commit.
#[test]
fn test_a_window_destroyed_mid_pass_holds_the_lane_until_the_pass_ends() {
    let _policy = DraftPipelinePolicyReset;
    let _first_dirty = FirstDirtyAutosaveDelayReset;
    set_first_dirty_autosave_delay_for_test(60_000);
    let process = one_process(|_, _| {});
    let second = process.open_window();
    second.new_tab();
    edit_active(&second, "window B's untitled edits\n");
    let untitled_id = active_editor(&second).draft_id().expect("untitled id");
    process.first.new_tab();
    edit_active(&process.first, "window A's untitled edits\n");

    set_draft_mutation_delays_for_test(0, 1_500, 0);
    second.autosave_tick_for_test();
    wait_until(JOURNAL_BUDGET, || body_of(&untitled_id).is_some());
    second.destroy();
    process.first.autosave_tick_for_test();
    assert!(
        !process.first.draft_evidence().autosave_inflight,
        "window A's pass must wait for the destroyed window's pass"
    );
    assert!(process.first.draft_evidence().autosave_pending);
    wait_until(JOURNAL_BUDGET, || manifest_lists(&untitled_id));
    wait_until(JOURNAL_BUDGET, || {
        let evidence = process.first.draft_evidence();
        !evidence.autosave_pending && !evidence.autosave_inflight
    });
    assert!(!active_editor(&process.first).draft_dirty());
}

/// The Kani trace: window A restores a tab whose entry has no body, and its
/// manifest copy still lists that entry; window B's cleanup retires the
/// missing-body entry; A's next autosave must not write a body no entry
/// describes on the strength of its stale copy.
#[test]
fn test_a_stale_manifest_copy_never_skips_registration_after_another_windows_cleanup() {
    let _policy = DraftPipelinePolicyReset;
    let _cleanup = OrphanCleanupPolicyReset;
    let _first_dirty = FirstDirtyAutosaveDelayReset;
    set_first_dirty_autosave_delay_for_test(60_000);
    // Keep A's own startup cleanup from running first.
    set_orphan_cleanup_delays_for_test(600_000, 600_000, 0);
    let mut seeded = None;
    let process = one_process(|data_dir, documents| {
        let path = write_document(documents, "entry-only.txt", "on disk\n");
        let draft_id = draft_service::draft_id_for_path(&path);
        seed_manifest_and_session(data_dir, &path, &draft_id);
        seeded = Some((path, draft_id));
    });
    let (path, draft_id) = seeded.expect("seeded");
    wait_until(JOURNAL_BUDGET, || {
        process.first.imp().tab_view.n_pages() == 1
            && active_editor(&process.first).file_size().is_some()
    });
    assert!(manifest_lists(&draft_id) && body_of(&draft_id).is_none());
    assert_eq!(
        active_editor(&process.first).file_path().as_deref(),
        Some(path.as_path())
    );
    edit_active(&process.first, "window A's edits\n");

    // Release A's startup preloads (a second window's startup read waits for
    // that disposal capacity) without letting A's own cleanup run.
    set_orphan_cleanup_delays_for_test(0, 1, 0);
    process.first.schedule_orphan_cleanup_for_test(false);
    wait_until(JOURNAL_BUDGET, || {
        !process.first.draft_evidence().cleanup_timer_pending
    });
    let second = process.open_window();
    second.schedule_orphan_cleanup_for_test(true);
    wait_until(JOURNAL_BUDGET, || {
        journal_idle(&second) && journal_idle(&process.first)
    });
    // Hold A's pass between its body write and its manifest commit: a crash
    // there must still find an entry describing the body.
    set_draft_mutation_delays_for_test(0, 1_500, 0);
    process.first.autosave_tick_for_test();
    wait_until(JOURNAL_BUDGET, || body_of(&draft_id).is_some());
    assert!(
        manifest_lists(&draft_id),
        "window A wrote a body no manifest entry describes"
    );
    wait_until(JOURNAL_BUDGET, || {
        journal_idle(&process.first) && !active_editor(&process.first).draft_dirty()
    });
    assert_eq!(body_of(&draft_id).as_deref(), Some("window A's edits\n"));
    assert!(
        manifest_lists(&draft_id),
        "window A wrote a body no manifest entry describes"
    );
}
