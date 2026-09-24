// SPDX-License-Identifier: GPL-3.0-or-later

//! E1 finding, reduced to the public `draft_service` API (no Quint): the
//! divergence `tests/quint_connect_service.rs` reports on every seed.
//!
//! Quint trace (`k3_service`, `stepServiceWeighted`, e.g. `QUINT_SEED=12`):
//! previous session id 1 = entry (saved_at 2) + body `c2`; `Startup`;
//! `RestoreApply(EditedOver)` (copy `c2` aside); `Edit` (content 4);
//! `Register`; `WriteBody` (`c4`, no `Commit`); `Crash`; `Startup`;
//! `RestoreApply(EditedOver)`. The model then has `preserved = {2, 4}`; the
//! service has only `{2}`.
//!
//! Cause: `preserve_stale_draft_body` copies aside under
//! `set_aside::keep_copy(id, entry.saved_at_secs)`, and `keep_copy` treats an
//! existing `{id}.{stamp}.draft` as "this body is already kept". After a
//! crash between the body write and its manifest commit, the body on disk is
//! newer than its entry's `saved_at_secs`, so the name no longer identifies
//! the content: the newer body is not copied, the call still reports
//! `SetAside`, and production then releases the restore hold and lets
//! autosave replace the only copy. S1 does not flag it, because the body was
//! never committed (never "accepted"), and Kani does not model set-aside
//! naming.
//!
//! Fixed in `bound-draft-set-aside-retention`: an existing set-aside name
//! counts as kept only when it holds a byte-identical copy
//! (`journal_core::set_aside_name_step`), so both bodies are now kept. This
//! test asserts the fixed behaviour; the promoted production test is
//! `draft_service::tests::unapplied_restore_keeps_an_uncommitted_newer_body_under_the_same_stamp`.

use lushtext_core::model::draft::{DraftEntry, DraftManifest, StaleDraftPreservation};
use lushtext_core::services::draft_service::{self, RegisteredDraft, set_aside};

#[test]
fn unrestored_copy_is_kept_when_an_uncommitted_body_shares_the_entry_stamp() {
    let data = tempfile::tempdir().expect("tempdir");
    let path = std::path::PathBuf::from("/e1-quint-connect/file1.txt");
    let id = draft_service::draft_id_for_path(&path);
    let entry = DraftEntry {
        draft_id: id.clone(),
        original_path: Some(path),
        original_mtime_secs: None,
        saved_at_secs: 2,
    };
    let mut manifest = DraftManifest::default();
    manifest.upsert(entry.clone());
    draft_service::save_manifest(data.path(), &manifest).expect("manifest");
    let token = || {
        RegisteredDraft::without_registration(&id, true, true, true).expect("registered id")
    };
    // The previous session's committed body.
    draft_service::write_draft(data.path(), &token(), "c2").expect("body c2");

    // First unapplied restore: `c2` is copied aside as `{id}.2.draft`.
    assert_eq!(
        draft_service::preserve_stale_draft_body(data.path(), &entry).expect("preserve c2"),
        Some(StaleDraftPreservation::SetAside)
    );

    // Autosave writes the next body; the process dies before the manifest
    // commit, so the entry still says `saved_at_secs = 2`.
    draft_service::write_draft(data.path(), &token(), "c4").expect("body c4");

    // Second unapplied restore, after restart: reported as kept aside,
    assert_eq!(
        draft_service::preserve_stale_draft_body(data.path(), &entry).expect("preserve c4"),
        Some(StaleDraftPreservation::SetAside)
    );
    // and the set-aside area now holds both bodies.
    let mut kept: Vec<String> = set_aside::list(data.path())
        .expect("list")
        .into_iter()
        .map(|body| std::fs::read_to_string(body.path).expect("read"))
        .collect();
    kept.sort();
    assert_eq!(kept, vec!["c2".to_string(), "c4".to_string()], "c4 was not preserved");
}
