// SPDX-License-Identifier: GPL-3.0-or-later

//! Draft-service conformance (`verify-shell-conformance-against-proven-cores`
//! D5, D7): drive the real `draft_service` in lockstep with the Kani-checked
//! journal model (`draft_service::journal_model`), using the model's single
//! window as the window-half owner and comparing the abstracted real disk and
//! authority against the model's disk and authority after every step.
//!
//! Compiled only under `cfg(test)` and `test-utils`. Names neither `proptest`
//! nor `tempfile`: callers supply the script and a directory they own.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, UNIX_EPOCH};

use crate::app_data_layout::AppDataLocation;
use crate::draft_service::journal_core::{
    BodyWriteDecision, DeletionStep, RestoreDisposition, RestoreEnding, body_write_decision,
    ownership, registration_required, unapplied_restore_disposition,
};
use crate::draft_service::journal_model::{
    Action, CONTENTS, Choices, ContentId, Fault, Journal, LaneHolder, LaneUse, MAX_EDITS,
    SingleWindowScope, bit,
};
use crate::draft_service::{
    self, DraftOrphanCleanupFailure, DraftOrphanCleanupManifestError, DraftOrphanCleanupPlan,
    DraftRegistration, RegisteredDraft, drafts_dir, set_aside,
};
use crate::filesystem::backend_tap::{
    self as tap, Address, Call, Injected, ProtocolKind, TapPlan, TapTrace,
};
use crate::filesystem::{
    WriteLabel, fixture, leftovers, metadata as fs_metadata, write as fs_write,
};
use crate::recovery_metadata::{RecoveryMetadataClass, RecoveryProblem};
use crate::{editor_io, session_service};
use lushtext_model::draft::{
    DraftEntry, DraftManifest, DraftManifestAuthority, FileDraftRestoreResolution,
    FileDraftRestoreSkip, PreloadedDraftRestore, PreloadedDraftSkip, StaleDraftPreservation,
    UNPROVEN_BACKING_MTIME_SECS,
};
use lushtext_model::session::{SessionData, SessionTab};

/// Draft ids modeled by `SingleWindowScope`: two file-backed (`0`, `1`) and one untitled (`2`).
pub const DRAFT_IDS: usize = 3;

/// Number of boolean choices consumed by `Journal::previous_session` for 3 ids (2 file-backed, 1 untitled).
pub const INITIAL_LAYOUT_BITS: usize = 8;

/// Base epoch seconds for deterministic whole-second backing mtimes.
const BASE_MTIME_SECS: u64 = 1_700_000_000;

/// Fault kind an op may request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftFault {
    None,
    BeforeEffect,
    AfterEffect,
    At(usize),
}

/// Controls which fault classes a conformance run activates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultDomain {
    /// Task 4.3: only `BeforeEffect` (scope-wide call 0), plus `Crash` and `Startup`.
    BeforeOnly,
    /// Task 5.2: also `AfterEffect` and `At(k)`.
    Full,
}

/// One enabled-indexed operation in a draft conformance script.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DraftOp {
    /// Index into the model's currently enabled weighted actions.
    pub action_index: usize,
    /// Index into the draft ids for which that action is currently enabled.
    pub id_index: usize,
    /// Fault injected for this step (subject to the active `FaultDomain` and action).
    pub fault: DraftFault,
    /// Window-side `RestoreEnding` preference (`Applied`, `EditedOver`, `InstallCancelled`)
    /// used only when `resolve_draft_restore` yields a restorable body.
    pub restore_choice: u8,
}

/// Step and coverage counters observed during one run or summed across a campaign.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReachCounts {
    pub total_steps: usize,
    pub write_body: usize,
    pub commit: usize,
    pub deletion_step: usize,
    pub restore_apply: usize,
    pub exec_cleanup: usize,
    pub startup: usize,
    pub crash: usize,
    pub leftovers_observed: usize,
    pub extra_preserved_steps: usize,
}

impl ReachCounts {
    /// Accumulate counts from another run.
    pub fn add_assign(&mut self, other: &Self) {
        self.total_steps += other.total_steps;
        self.write_body += other.write_body;
        self.commit += other.commit;
        self.deletion_step += other.deletion_step;
        self.restore_apply += other.restore_apply;
        self.exec_cleanup += other.exec_cleanup;
        self.startup += other.startup;
        self.crash += other.crash;
        self.leftovers_observed += other.leftovers_observed;
        self.extra_preserved_steps += other.extra_preserved_steps;
    }

    fn record_action(&mut self, action: Action) {
        self.total_steps += 1;
        match action {
            Action::WriteBody => self.write_body += 1,
            Action::Commit => self.commit += 1,
            Action::DeletionStep => self.deletion_step += 1,
            Action::RestoreApply => self.restore_apply += 1,
            Action::ExecCleanup => self.exec_cleanup += 1,
            Action::Startup => self.startup += 1,
            Action::Crash => self.crash += 1,
            _ => {}
        }
    }
}

/// Projected entry metadata matching `DiskEntry::present` and `DiskEntry::backing`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectedEntry {
    pub present: bool,
    pub backing: u8,
}

/// Real service state projected onto the journal model's disk and authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbstractState {
    pub entry: [ProjectedEntry; DRAFT_IDS],
    pub body: [Option<ContentId>; DRAFT_IDS],
    pub backing: [u8; DRAFT_IDS],
    pub preserved: u16,
    pub trusted: bool,
    pub leftovers: usize,
}

/// Deterministic injective encoding of model content ids `1..CONTENTS`,
/// with multibyte UTF-8 text on even ids so byte-vs-char handling is exercised.
#[must_use]
pub fn content_text(content: ContentId) -> String {
    debug_assert!(usize::from(content) > 0 && usize::from(content) < CONTENTS);
    match content {
        2 | 4 | 6 => format!("c{content} — λ\n"),
        _ => format!("c{content}\n"),
    }
}

/// Inverse of [`content_text`].
#[must_use]
pub fn parse_content_text(text: &str) -> Option<ContentId> {
    (1..CONTENTS)
        .filter_map(|id| ContentId::try_from(id).ok())
        .find(|&id| content_text(id) == text)
}

fn slot_u8(id: usize) -> u8 {
    u8::try_from(id).expect("draft slot index fits in u8")
}

struct ScriptChoices {
    initial_bits: [bool; INITIAL_LAYOUT_BITS],
    bit_cursor: usize,
    queued_flags: VecDeque<bool>,
    next_restore_ending: Option<RestoreEnding>,
}

impl ScriptChoices {
    fn new(initial_bits: [bool; INITIAL_LAYOUT_BITS]) -> Self {
        Self {
            initial_bits,
            bit_cursor: 0,
            queued_flags: VecDeque::new(),
            next_restore_ending: None,
        }
    }
}

impl Choices for ScriptChoices {
    fn flag(&mut self) -> bool {
        if let Some(flag) = self.queued_flags.pop_front() {
            return flag;
        }
        let bit = self
            .initial_bits
            .get(self.bit_cursor)
            .copied()
            .unwrap_or(true);
        self.bit_cursor += 1;
        bit
    }

    fn restore_ending(&mut self, admits: impl Fn(RestoreEnding) -> bool) -> RestoreEnding {
        let ending = self
            .next_restore_ending
            .take()
            .expect("scripted restore ending must be set before RestoreApply");
        assert!(
            admits(ending),
            "service returned restore ending {ending:?} that journal model does not admit"
        );
        ending
    }
}

/// Choices for pure model-only reach evaluation (Task 4.5).
struct ModelOnlyChoices {
    initial_bits: [bool; INITIAL_LAYOUT_BITS],
    bit_cursor: usize,
    restore_choice: u8,
}

impl ModelOnlyChoices {
    fn new(initial_bits: [bool; INITIAL_LAYOUT_BITS]) -> Self {
        Self {
            initial_bits,
            bit_cursor: 0,
            restore_choice: 0,
        }
    }
}

impl Choices for ModelOnlyChoices {
    fn flag(&mut self) -> bool {
        let bit = self
            .initial_bits
            .get(self.bit_cursor)
            .copied()
            .unwrap_or(false);
        self.bit_cursor += 1;
        bit
    }

    fn restore_ending(&mut self, admits: impl Fn(RestoreEnding) -> bool) -> RestoreEnding {
        const CANDIDATES: [RestoreEnding; 4] = [
            RestoreEnding::Applied,
            RestoreEnding::EditedOver,
            RestoreEnding::InstallCancelled,
            RestoreEnding::ReadFailed,
        ];
        let mut allowed = CANDIDATES
            .into_iter()
            .filter(|&e| admits(e))
            .collect::<Vec<_>>();
        if allowed.is_empty() {
            allowed.push(RestoreEnding::Applied);
        }
        allowed[usize::from(self.restore_choice) % allowed.len()]
    }
}

/// Return whether `action` can make progress on `id` in `model`.
fn action_enabled_for(
    model: &Journal<SingleWindowScope, DRAFT_IDS, 1, 1>,
    action: Action,
    id: usize,
) -> bool {
    if !model.processes[0].running() {
        return action == Action::Startup && id == 0;
    }
    let me = model.me();
    let ed = me.editors[id];
    match action {
        Action::Startup | Action::OpenWindow => false,
        Action::Crash => id == 0,
        Action::CloseWindow => {
            if id != 0 {
                return false;
            }
            let busy = (0..DRAFT_IDS).any(|slot| {
                let s = me.editors[slot];
                s.dirty
                    || s.token
                    || s.written.is_some()
                    || s.restore_pending
                    || s.deletion.is_some()
            });
            let holds_lane = model
                .lane_holder()
                .is_some_and(|holder| holder.window == me.id);
            !busy && !holds_lane
        }
        Action::Inspect => id == 0 && me.trusted && model.lane_admits(LaneUse::Cleanup),
        Action::ExecCleanup => {
            id == 0
                && model.lane_holder()
                    == Some(LaneHolder {
                        window: me.id,
                        lane_use: LaneUse::Cleanup,
                    })
        }
        Action::Open => {
            !ed.open
                && (!model.untitled(id) || (!model.entry[id].present && model.body[id].is_none()))
        }
        Action::Edit => ed.open && model.edits < MAX_EDITS && !ed.token && ed.written.is_none(),
        Action::Save | Action::Discard => ed.open && ed.deletion.is_none(),
        Action::Register => {
            if !ed.open
                || !ed.dirty
                || ed.token
                || ed.deletion.is_some()
                || !model.lane_admits(LaneUse::Pass(slot_u8(id)))
            {
                return false;
            }
            let needed =
                registration_required(!model.untitled(id), me.known[id].is_some(), me.trusted);
            body_write_decision(ownership(model.window_body_facts(id)), needed)
                != BodyWriteDecision::Hold
        }
        Action::WriteBody => {
            if !ed.open || !ed.dirty || !ed.token {
                return false;
            }
            let needed =
                registration_required(!model.untitled(id), me.known[id].is_some(), me.trusted);
            body_write_decision(ownership(model.window_body_facts(id)), needed)
                != BodyWriteDecision::Hold
        }
        Action::Commit => ed.written.is_some(),
        Action::DeletionStep => {
            ed.deletion.is_some() && model.lane_admits(LaneUse::Deletion(slot_u8(id)))
        }
        Action::RestoreApply => ed.restore_pending,
        Action::ExternalMtime => !model.untitled(id),
    }
}

/// Choose the `(Action, id)` pair for `op` among the currently enabled steps,
/// weighting in-flight journal progress stages (`Register`, `WriteBody`,
/// `Commit`, `DeletionStep`, `ExecCleanup`, `RestoreApply`) so generated scripts
/// reliably traverse multi-step pipelines instead of repeatedly resetting state.
fn pick_enabled_step(
    model: &Journal<SingleWindowScope, DRAFT_IDS, 1, 1>,
    op: DraftOp,
) -> (Action, usize) {
    let mut actions: Vec<Action> = Vec::new();
    for action in [
        Action::Startup,
        Action::Open,
        Action::Edit,
        Action::Register,
        Action::WriteBody,
        Action::Commit,
        Action::Save,
        Action::Discard,
        Action::DeletionStep,
        Action::Inspect,
        Action::ExecCleanup,
        Action::RestoreApply,
        Action::ExternalMtime,
        Action::CloseWindow,
        Action::Crash,
    ] {
        if !(0..DRAFT_IDS).any(|id| action_enabled_for(model, action, id)) {
            continue;
        }
        let weight = match action {
            Action::WriteBody | Action::Commit => 12,
            Action::Register => 8,
            Action::DeletionStep | Action::ExecCleanup => 6,
            Action::RestoreApply | Action::Edit => 5,
            Action::Open | Action::Inspect => 2,
            _ => 1,
        };
        for _ in 0..weight {
            actions.push(action);
        }
    }
    let action = actions[op.action_index % actions.len()];
    let ids: Vec<usize> = (0..DRAFT_IDS)
        .filter(|&id| action_enabled_for(model, action, id))
        .collect();
    let id = ids[op.id_index % ids.len()];
    (action, id)
}

/// Run a script over the journal model alone and return reach counts (Task 4.5).
#[must_use]
pub fn evaluate_model_reach(
    initial_bits: [bool; INITIAL_LAYOUT_BITS],
    ops: &[DraftOp],
) -> ReachCounts {
    let mut choices = ModelOnlyChoices::new(initial_bits);
    let mut model = Journal::<SingleWindowScope, DRAFT_IDS, 1, 1>::previous_session(&mut choices);
    model.assert_invariants();
    let mut counts = ReachCounts::default();

    for &op in ops {
        let (action, id) = pick_enabled_step(&model, op);
        choices.restore_choice = op.restore_choice;
        let fault = match op.fault {
            DraftFault::None | DraftFault::At(_) => Fault::None,
            DraftFault::BeforeEffect => Fault::BeforeEffect,
            DraftFault::AfterEffect => Fault::AfterEffect,
        };
        counts.record_action(action);
        model.step(&mut choices, 0, 0, action, id, fault);
        model.assert_invariants();
    }
    counts
}

struct DriverEnv {
    data_dir: PathBuf,
    backing_paths: [Option<PathBuf>; DRAFT_IDS],
    draft_ids: [String; DRAFT_IDS],
    mtime_versions: [HashMap<u64, u8>; DRAFT_IDS],
    next_mtime_offset: [u64; DRAFT_IDS],
    saved_at_seq: u64,
    authority: DraftManifestAuthority,
    tokens: [Option<RegisteredDraft>; DRAFT_IDS],
    cleanup_plan: Option<DraftOrphanCleanupPlan>,
    persisted_entries: [Option<DraftEntry>; DRAFT_IDS],
}

impl DriverEnv {
    fn new(root: &Path) -> Self {
        let data_dir = root.join("data");
        let files_dir = root.join("files");
        fixture::create_dir_all(&data_dir);
        fixture::create_dir_all(&files_dir);
        let path0 = files_dir.join("file0.txt");
        let path1 = files_dir.join("file1.txt");
        let draft_ids = [
            draft_service::draft_id_for_path(&path0),
            draft_service::draft_id_for_path(&path1),
            draft_service::draft_id_for_untitled(2),
        ];
        Self {
            data_dir,
            backing_paths: [Some(path0), Some(path1), None],
            draft_ids,
            mtime_versions: [HashMap::new(), HashMap::new(), HashMap::new()],
            next_mtime_offset: [0; DRAFT_IDS],
            saved_at_seq: BASE_MTIME_SECS,
            authority: DraftManifestAuthority::default(),
            tokens: [None, None, None],
            cleanup_plan: None,
            persisted_entries: [None, None, None],
        }
    }

    fn next_saved_at_secs(&mut self) -> u64 {
        self.saved_at_seq += 1;
        self.saved_at_seq
    }

    fn seed_backing_version(&mut self, id: usize, version: u8, text: &str) {
        let Some(path) = self.backing_paths[id].as_deref() else {
            return;
        };
        let mtime = BASE_MTIME_SECS + u64::from(version);
        self.next_mtime_offset[id] = u64::from(version);
        self.mtime_versions[id].insert(mtime, version);
        self.mtime_versions[id].insert(UNPROVEN_BACKING_MTIME_SECS, u8::MAX);
        fixture::write_text(path, text);
        fixture::set_modified(path, UNIX_EPOCH + Duration::from_secs(mtime));
    }

    fn advance_backing_mtime(&mut self, id: usize, new_version: u8, rewrite_bytes: bool) {
        let Some(path) = self.backing_paths[id].as_deref() else {
            return;
        };
        self.next_mtime_offset[id] += 1;
        let mtime = BASE_MTIME_SECS + self.next_mtime_offset[id];
        self.mtime_versions[id].insert(mtime, new_version);
        if rewrite_bytes {
            let body = format!("saved-v{new_version}-offset-{mtime}\n");
            fs_write::atomic_replace(path, WriteLabel::SAVE, body.as_bytes())
                .expect("conformance Save durable write");
        }
        fixture::set_modified(path, UNIX_EPOCH + Duration::from_secs(mtime));
    }

    fn seed_previous_session(
        &mut self,
        model: &Journal<SingleWindowScope, DRAFT_IDS, 1, 1>,
    ) -> Result<(), String> {
        let mut entries = Vec::new();
        let mut tabs = Vec::new();
        for id in 0..DRAFT_IDS {
            if self.backing_paths[id].is_some() {
                let mtime0 = BASE_MTIME_SECS;
                self.mtime_versions[id].insert(mtime0, 0);
                self.mtime_versions[id].insert(UNPROVEN_BACKING_MTIME_SECS, u8::MAX);
                self.seed_backing_version(
                    id,
                    model.backing[id],
                    &format!("initial-{id}-v{}\n", model.backing[id]),
                );
            }
            if model.entry[id].present {
                let entry = DraftEntry {
                    draft_id: self.draft_ids[id].clone(),
                    original_path: self.backing_paths[id].clone(),
                    original_mtime_secs: self.backing_paths[id]
                        .as_ref()
                        .map(|_| BASE_MTIME_SECS + u64::from(model.entry[id].backing)),
                    saved_at_secs: self.next_saved_at_secs(),
                };
                entries.push(entry);
                tabs.push(SessionTab {
                    path: self.backing_paths[id].clone(),
                    draft_id: self.backing_paths[id]
                        .is_none()
                        .then(|| self.draft_ids[id].clone()),
                    cursor_line: 0,
                    cursor_col: 0,
                    scroll_line: 0,
                    pinned: false,
                });
            }
            if let Some(content) = model.body[id] {
                draft_service::fixture::write_body(
                    &self.data_dir,
                    &self.draft_ids[id],
                    &content_text(content),
                )
                .map_err(|e| format!("seed body {id}: {e}"))?;
            }
        }
        if !entries.is_empty() {
            let manifest = DraftManifest {
                drafts: entries,
                cleanup_continuation: None,
            };
            draft_service::save_manifest(&self.data_dir, &manifest)
                .map_err(|e| format!("seed manifest: {e}"))?;
        }
        if !tabs.is_empty() {
            let session = SessionData {
                tabs,
                active_tab_index: Some(0),
            };
            session_service::save(&self.data_dir, &session)
                .map_err(|e| format!("seed session: {e}"))?;
        }
        Ok(())
    }

    fn session_for_open_tabs(
        &self,
        model: &Journal<SingleWindowScope, DRAFT_IDS, 1, 1>,
    ) -> SessionData {
        let mut tabs = Vec::new();
        let me = model.me();
        if me.running {
            for id in 0..DRAFT_IDS {
                if me.editors[id].open {
                    tabs.push(SessionTab {
                        path: self.backing_paths[id].clone(),
                        draft_id: self.backing_paths[id]
                            .is_none()
                            .then(|| self.draft_ids[id].clone()),
                        cursor_line: 0,
                        cursor_col: 0,
                        scroll_line: 0,
                        pinned: false,
                    });
                }
            }
        }
        SessionData {
            active_tab_index: (!tabs.is_empty()).then_some(0),
            tabs,
        }
    }

    fn sync_session_on_disk(&self, model: &Journal<SingleWindowScope, DRAFT_IDS, 1, 1>) {
        let session = self.session_for_open_tabs(model);
        session_service::save(&self.data_dir, &session).expect("conformance session sync");
    }

    fn make_entry(&mut self, id: usize) -> DraftEntry {
        let original_mtime_secs = self.backing_paths[id]
            .as_deref()
            .map(|path| editor_io::mtime_secs(path).expect("backing file mtime"));
        DraftEntry {
            draft_id: self.draft_ids[id].clone(),
            original_path: self.backing_paths[id].clone(),
            original_mtime_secs,
            saved_at_secs: self.next_saved_at_secs(),
        }
    }

    fn resolve_entry_for(&mut self, id: usize) -> DraftEntry {
        if let Some(entry) = self.persisted_entries[id].clone() {
            return entry;
        }
        let manifest =
            draft_service::load_manifest(&self.data_dir).expect("load manifest for entry lookup");
        if let Some(entry) = manifest.find_by_id(&self.draft_ids[id]).cloned() {
            self.persisted_entries[id] = Some(entry.clone());
            return entry;
        }
        self.make_entry(id)
    }

    fn body_absent_on_disk(&self, id: usize) -> bool {
        let body_path = drafts_dir(&self.data_dir).join(format!("{}.draft", self.draft_ids[id]));
        !matches!(
            fs_metadata::path_status(&body_path),
            Ok(status) if status.is_present()
        )
    }

    fn set_aside_has_content(&self, content: ContentId) -> bool {
        let Ok(listing) = set_aside::list(&self.data_dir) else {
            return false;
        };
        let expected = content_text(content);
        listing.rows.iter().any(|item| {
            set_aside::read(&self.data_dir, &item.path).is_ok_and(|text| text == expected)
        })
    }

    fn queue_reconcile_flags(
        &self,
        choices: &mut ScriptChoices,
        model: &Journal<SingleWindowScope, DRAFT_IDS, 1, 1>,
        excluding: Option<usize>,
        allow_session_attribution: bool,
    ) {
        for slot in 0..DRAFT_IDS {
            if Some(slot) == excluding {
                continue;
            }
            if !model.untitled(slot)
                && let Some(content) = model.body[slot]
                && !model.entry[slot].present
            {
                let flag = if allow_session_attribution && model.me().editors[slot].open {
                    self.set_aside_has_content(content)
                } else {
                    self.body_absent_on_disk(slot)
                };
                choices.queued_flags.push_back(flag);
            }
        }
    }

    fn drop_held_values(&mut self) {
        self.authority = DraftManifestAuthority::default();
        self.tokens = [None, None, None];
        self.cleanup_plan = None;
        self.persisted_entries = [None, None, None];
    }

    fn abstract_state(&self) -> Result<AbstractState, String> {
        let manifest = draft_service::load_manifest(&self.data_dir)
            .map_err(|e| format!("abstract load_manifest: {e}"))?;

        let mut entry = [ProjectedEntry {
            present: false,
            backing: 0,
        }; DRAFT_IDS];
        for (id, entry_slot) in entry.iter_mut().enumerate() {
            if let Some(found) = manifest.find_by_id(&self.draft_ids[id]) {
                let backing = if self.backing_paths[id].is_some() {
                    let mtime = found.original_mtime_secs.ok_or_else(|| {
                        format!(
                            "file-backed manifest entry {} missing original_mtime_secs",
                            self.draft_ids[id]
                        )
                    })?;
                    *self.mtime_versions[id].get(&mtime).ok_or_else(|| {
                        format!(
                            "unmapped entry mtime {mtime} for id {id} ({})",
                            self.draft_ids[id]
                        )
                    })?
                } else {
                    0
                };
                *entry_slot = ProjectedEntry {
                    present: true,
                    backing,
                };
            }
        }

        let dir = drafts_dir(&self.data_dir);
        let mut body = [None; DRAFT_IDS];
        let mut leftovers_count = 0usize;
        if fixture::exists(&dir) {
            let manifest_name = AppDataLocation::DraftManifest.file_name();
            let set_aside_name = set_aside::dir(&self.data_dir)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("set-aside")
                .to_string();
            for name in fixture::entry_names(&dir) {
                if name == manifest_name || name == set_aside_name {
                    continue;
                }
                if leftovers::parse_durable_temp_name(&name).is_some() {
                    leftovers_count += 1;
                    continue;
                }
                let matched_id =
                    (0..DRAFT_IDS).find(|&id| format!("{}.draft", self.draft_ids[id]) == name);
                let Some(id) = matched_id else {
                    return Err(format!("unknown file in drafts directory: {name}"));
                };
                let text = draft_service::read_draft(&self.data_dir, &self.draft_ids[id])
                    .map_err(|e| format!("abstract read_draft {id}: {e}"))?
                    .ok_or_else(|| format!("listed draft body disappeared: {name}"))?;
                let content = parse_content_text(&text)
                    .ok_or_else(|| format!("draft body {name} contains unmapped text: {text:?}"))?;
                body[id] = Some(content);
            }
        }

        let mut backing = [0u8; DRAFT_IDS];
        for (id, backing_slot) in backing.iter_mut().enumerate() {
            if let Some(path) = self.backing_paths[id].as_deref() {
                let mtime = editor_io::mtime_secs(path)
                    .ok_or_else(|| format!("missing backing file {}", path.display()))?;
                *backing_slot = *self.mtime_versions[id]
                    .get(&mtime)
                    .ok_or_else(|| format!("unmapped backing mtime {mtime} for id {id}"))?;
            }
        }

        let listing = set_aside::list(&self.data_dir)
            .map_err(|e| format!("abstract set_aside::list: {e}"))?;
        if !listing.totals.complete {
            return Err("set-aside listing exceeded scan bound".to_string());
        }
        let mut preserved = 0u16;
        for item in &listing.rows {
            let text = set_aside::read(&self.data_dir, &item.path)
                .map_err(|e| format!("abstract set_aside::read {}: {e}", item.path.display()))?;
            let content = parse_content_text(&text).ok_or_else(|| {
                format!(
                    "set-aside body {} contains unmapped text: {text:?}",
                    item.path.display()
                )
            })?;
            preserved |= bit(content);
        }

        Ok(AbstractState {
            entry,
            body,
            backing,
            preserved,
            trusted: self.authority.is_trusted(),
            leftovers: leftovers_count,
        })
    }
}

fn compare_states(
    step_idx: usize,
    action: Action,
    id: usize,
    real: &AbstractState,
    model: &Journal<SingleWindowScope, DRAFT_IDS, 1, 1>,
    counts: &mut ReachCounts,
) -> Result<(), String> {
    counts.leftovers_observed += real.leftovers;
    for i in 0..DRAFT_IDS {
        let m_entry = model.entry[i];
        let r_entry = real.entry[i];
        if r_entry.present != m_entry.present
            || (m_entry.present && r_entry.backing != m_entry.backing)
        {
            return Err(format!(
                "step {step_idx} ({action:?} id={id}): manifest entry[{i}] mismatch: real={r_entry:?}, model=(present={}, backing={})",
                m_entry.present, m_entry.backing
            ));
        }
        if real.body[i] != model.body[i] {
            return Err(format!(
                "step {step_idx} ({action:?} id={id}): body[{i}] mismatch: real={:?}, model={:?}",
                real.body[i], model.body[i]
            ));
        }
        if real.backing[i] != model.backing[i] {
            return Err(format!(
                "step {step_idx} ({action:?} id={id}): backing[{i}] mismatch: real={}, model={}",
                real.backing[i], model.backing[i]
            ));
        }
    }
    if (model.preserved & !real.preserved) != 0 {
        return Err(format!(
            "step {step_idx} ({action:?} id={id}): preserved mismatch: real={:#b} does not cover model={:#b}",
            real.preserved, model.preserved
        ));
    }
    if real.preserved != model.preserved {
        counts.extra_preserved_steps += 1;
    }
    if real.trusted != model.me().trusted {
        return Err(format!(
            "step {step_idx} ({action:?} id={id}): authority trusted mismatch: real={}, model={}",
            real.trusted,
            model.me().trusted
        ));
    }
    Ok(())
}

enum AfterTarget {
    None,
    Destination(PathBuf),
    NextSetAsideSyncDir,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TraceFaultKind {
    None,
    BeforeEffect,
    AfterEffect,
    Ignored,
}

fn manifest_file_path(data_dir: &Path) -> PathBuf {
    crate::app_data_layout::AppDataLayout::new(data_dir).path(AppDataLocation::DraftManifest)
}

fn body_write_destination(data_dir: &Path, draft_id: &str) -> PathBuf {
    let path = drafts_dir(data_dir).join(format!("{draft_id}.draft"));
    fs_write::resolve_target_identity(&path)
        .map_or(path, |identity| identity.as_path().to_path_buf())
}

fn tap_plan_for(fault: DraftFault, domain: FaultDomain, after: AfterTarget) -> (TapPlan, bool) {
    let effective = match (domain, fault) {
        (FaultDomain::BeforeOnly, DraftFault::BeforeEffect) => DraftFault::BeforeEffect,
        (FaultDomain::BeforeOnly, _) => DraftFault::None,
        (FaultDomain::Full, other) => other,
    };
    match effective {
        DraftFault::BeforeEffect => (
            TapPlan::record().inject(Address::ScopeWide { call: 0 }, Injected::FAIL),
            true,
        ),
        DraftFault::At(call) => (
            TapPlan::record().inject(Address::ScopeWide { call }, Injected::FAIL),
            true,
        ),
        DraftFault::AfterEffect => match after {
            AfterTarget::Destination(destination) => (
                TapPlan::record().inject(
                    Address::Destination {
                        kind: ProtocolKind::Write,
                        destination,
                        action: "SyncDir",
                        call: 0,
                    },
                    Injected::FAIL,
                ),
                true,
            ),
            AfterTarget::NextSetAsideSyncDir => (
                TapPlan::record().inject(
                    Address::NextInAction {
                        kind: ProtocolKind::Write,
                        action: "SyncDir",
                        call: Call::SyncDir,
                    },
                    Injected::FAIL,
                ),
                true,
            ),
            AfterTarget::None => (TapPlan::record(), false),
        },
        DraftFault::None => (TapPlan::record(), false),
    }
}

fn classify_trace_fault(trace: &TapTrace, expected_sync_dir: Option<&Path>) -> TraceFaultKind {
    for step in &trace.steps {
        if step.calls.iter().any(|call| call.injected) {
            return match step.action_name.as_str() {
                "SyncDir"
                    if expected_sync_dir.is_none_or(|expected| {
                        step.calls
                            .iter()
                            .any(|call| call.injected && call.path == expected)
                    }) =>
                {
                    TraceFaultKind::AfterEffect
                }
                "RemoveTemp" => TraceFaultKind::Ignored,
                _ => TraceFaultKind::BeforeEffect,
            };
        }
    }
    if trace.loose_calls.iter().any(|call| call.injected) {
        return TraceFaultKind::BeforeEffect;
    }
    TraceFaultKind::None
}

/// Execute one `Action::ExecCleanup` step against both the real `draft_service`
/// and the proven journal model, projecting partial `DraftFault::At(k)` outcomes
/// faithfully when one phase (orphan unlink vs. missing-entry manifest commit)
/// fails while the other succeeds.
fn run_exec_cleanup_step(
    env: &mut DriverEnv,
    model: &mut Journal<SingleWindowScope, DRAFT_IDS, 1, 1>,
    choices: &mut impl Choices,
    id: usize,
    fault: DraftFault,
    domain: FaultDomain,
    step_idx: usize,
) -> Result<(), String> {
    let mut stepped = false;
    let mut model_fault = Fault::None;
    if model.lane_holder()
        == Some(LaneHolder {
            window: model.me().id,
            lane_use: LaneUse::Cleanup,
        })
        && let Some(plan) = env.cleanup_plan.take()
    {
        let (tap_plan, injected) = tap_plan_for(
            fault,
            domain,
            AfterTarget::Destination(manifest_file_path(&env.data_dir)),
        );
        let guard = tap::install(tap_plan);
        let outcome = draft_service::execute_orphan_cleanup(&env.data_dir, plan);
        let trace = guard.finish();
        let drafts_parent = drafts_dir(&env.data_dir);
        let fired = classify_trace_fault(&trace, Some(&drafts_parent));
        if !outcome.failures.is_empty() && (!injected || fired == TraceFaultKind::None) {
            return Err(format!(
                "step {step_idx} ExecCleanup unexpected failures: {:?}",
                outcome.failures
            ));
        }
        let load_failed = outcome.failures.iter().any(|failure| {
            matches!(
                failure,
                DraftOrphanCleanupFailure::Manifest(DraftOrphanCleanupManifestError::Load { .. })
            )
        });
        if load_failed {
            model_fault = Fault::BeforeEffect;
        } else if outcome.failures.is_empty() && fired == TraceFaultKind::None {
            model_fault = Fault::None;
        } else {
            // `execute_orphan_cleanup` unlinks orphan bodies first and commits
            // missing-body manifest removals second. Under `DraftFault::At(k)`,
            // either phase can fail while the other commits; project each
            // observed phase onto the model using its existing primitives.
            for slot in 0..DRAFT_IDS {
                if model.me().orphan_candidate[slot].is_some() {
                    let body_path =
                        drafts_dir(&env.data_dir).join(format!("{}.draft", env.draft_ids[slot]));
                    let unlinked = outcome.deleted_files.iter().any(|p| p == &body_path)
                        || outcome.already_absent_files.iter().any(|p| p == &body_path);
                    model.exec_cleanup_one(
                        slot,
                        if unlinked {
                            Fault::None
                        } else {
                            Fault::BeforeEffect
                        },
                    );
                }
            }
            let manifest_write_failed = outcome.failures.iter().any(|failure| {
                matches!(
                    failure,
                    DraftOrphanCleanupFailure::Manifest(
                        DraftOrphanCleanupManifestError::Write { .. }
                    )
                )
            });
            if manifest_write_failed {
                env.authority = DraftManifestAuthority::default();
                if fired == TraceFaultKind::AfterEffect {
                    model.step(choices, 0, 0, Action::ExecCleanup, id, Fault::AfterEffect);
                } else {
                    model.me_mut().missing_candidate = [None; DRAFT_IDS];
                    model.step(choices, 0, 0, Action::ExecCleanup, id, Fault::None);
                }
                model.revoke_trust();
            } else {
                for slot in 0..DRAFT_IDS {
                    if model.me().missing_candidate[slot].is_some()
                        && !outcome
                            .committed_manifest_removals
                            .iter()
                            .any(|fp| fp.draft_id == env.draft_ids[slot])
                    {
                        model.me_mut().missing_candidate[slot] = None;
                    }
                }
                model.step(choices, 0, 0, Action::ExecCleanup, id, Fault::None);
            }
            stepped = true;
        }
    }
    if !stepped {
        model.step(choices, 0, 0, Action::ExecCleanup, id, model_fault);
    }
    Ok(())
}

/// Run a script against both the real `draft_service` and the proven journal model.
///
/// # Panics
///
/// Panics if the model invariants (`S1`, `S3`, `S4`, or single-owner) break or
/// if an unscripted `RegisteredDraft` / `RestoreEnding` mismatch occurs.
///
/// # Errors
///
/// Returns an explanatory divergence message if the projected real state or restore
/// endings ever diverge from the model.
pub fn check_draft_script(
    root: &Path,
    initial_bits: [bool; INITIAL_LAYOUT_BITS],
    ops: &[DraftOp],
    domain: FaultDomain,
) -> Result<ReachCounts, String> {
    let mut choices = ScriptChoices::new(initial_bits);
    let mut model = Journal::<SingleWindowScope, DRAFT_IDS, 1, 1>::previous_session(&mut choices);
    model.assert_invariants();

    let mut env = DriverEnv::new(root);
    env.seed_previous_session(&model)?;

    let mut counts = ReachCounts::default();
    let mut history = Vec::with_capacity(ops.len());
    let pre_state = env.abstract_state()?;
    compare_states(0, Action::Startup, 0, &pre_state, &model, &mut counts)?;

    for (step_idx, &op) in ops.iter().enumerate() {
        let (action, id) = pick_enabled_step(&model, op);
        counts.record_action(action);
        history.push(format!(
            "step {}: {:?} id={} fault={:?}",
            step_idx + 1,
            action,
            id,
            op.fault
        ));

        let mut model_fault = Fault::None;
        match action {
            Action::Startup => {
                if !model.processes[0].running() {
                    let (plan, _) = match (domain, op.fault) {
                        (FaultDomain::BeforeOnly | FaultDomain::Full, DraftFault::BeforeEffect) => {
                            let session_path =
                                crate::app_data_layout::AppDataLayout::new(&env.data_dir)
                                    .path(AppDataLocation::Session);
                            let call = if fixture::exists(&session_path) { 3 } else { 1 };
                            (
                                TapPlan::record()
                                    .inject(Address::ScopeWide { call }, Injected::FAIL),
                                true,
                            )
                        }
                        _ => tap_plan_for(
                            op.fault,
                            domain,
                            AfterTarget::Destination(manifest_file_path(&env.data_dir)),
                        ),
                    };
                    let cancel = AtomicBool::new(false);
                    let guard = tap::install(plan);
                    let restored =
                        draft_service::load_restore_state_cancellable(&env.data_dir, &cancel);
                    let trace = guard.finish();
                    let drafts_parent = drafts_dir(&env.data_dir);
                    let fired = classify_trace_fault(&trace, Some(&drafts_parent));

                    env.authority = restored.manifest_authority;
                    for slot in 0..DRAFT_IDS {
                        env.persisted_entries[slot] =
                            restored.manifest.find_by_id(&env.draft_ids[slot]).cloned();
                    }
                    let phase1_failed = restored.diagnostics.iter().any(|diag| {
                        diag.class == RecoveryMetadataClass::DraftManifest
                            && !matches!(diag.problem, RecoveryProblem::Repaired { .. })
                    });
                    if fired == TraceFaultKind::None && !env.authority.is_trusted() {
                        return Err(format!(
                            "step {step_idx} Startup unexpectedly untrusted: {:?}",
                            restored.diagnostics
                        ));
                    }
                    model_fault = if !phase1_failed {
                        Fault::None
                    } else if fired == TraceFaultKind::AfterEffect {
                        Fault::AfterEffect
                    } else {
                        Fault::BeforeEffect
                    };
                    if model_fault == Fault::BeforeEffect {
                        env.queue_reconcile_flags(&mut choices, &model, None, false);
                        for slot in 0..DRAFT_IDS {
                            if model.entry[slot].present
                                && !model.untitled(slot)
                                && model.body[slot].is_some()
                                && !model.stale(slot)
                            {
                                let unproven = restored
                                    .manifest
                                    .find_by_id(&env.draft_ids[slot])
                                    .is_some_and(|e| {
                                        e.original_mtime_secs == Some(UNPROVEN_BACKING_MTIME_SECS)
                                    });
                                choices.queued_flags.push_back(unproven);
                            }
                        }
                    }
                    model.step(&mut choices, 0, 0, Action::Startup, 0, model_fault);

                    // `load_restore_state_cancellable` preserves & retires stale entries
                    // immediately when Phase 1 is trusted, whereas `journal_model::startup`
                    // queues `DeletionStep::Preserve`. Replay any retired/attempted steps
                    // against what happened on disk and in `restored`.
                    if !phase1_failed && model.me().trusted {
                        let disk_manifest = draft_service::load_manifest(&env.data_dir)
                            .map_err(|e| format!("Startup post-check load_manifest: {e}"))?;
                        for slot in 0..DRAFT_IDS {
                            if model.me().editors[slot].deletion.is_none() {
                                continue;
                            }
                            let draft_id = &env.draft_ids[slot];
                            let in_session =
                                restored.session.tabs.iter().any(|tab| match &tab.path {
                                    Some(path) => {
                                        &draft_service::draft_id_for_path(path) == draft_id
                                    }
                                    None => tab.draft_id.as_deref() == Some(draft_id.as_str()),
                                });
                            if !in_session {
                                continue;
                            }
                            // If `resolve_file_draft_restore` returned `Unavailable`/`Err` (e.g. a
                            // faulted backing `mtime_secs` check), `cleanup_stale_restore_entries`
                            // never ran for this slot: `preloaded_drafts` has no entry, the manifest
                            // still holds `draft_id`, and authority stays trusted. Leave the queued
                            // `DeletionStep::Preserve` untouched for later steps.
                            if !restored.preloaded_drafts.contains_key(draft_id)
                                && restored.manifest.find_by_id(draft_id).is_some()
                                && env.authority.is_trusted()
                            {
                                continue;
                            }
                            let preserve_failed = matches!(
                                restored.preloaded_drafts.get(draft_id),
                                Some(PreloadedDraftRestore::Skip(PreloadedDraftSkip::StaleFile(
                                    StaleDraftPreservation::Kept
                                )))
                            );
                            if preserve_failed {
                                let f = if fired == TraceFaultKind::AfterEffect {
                                    Fault::AfterEffect
                                } else {
                                    Fault::BeforeEffect
                                };
                                model.step(&mut choices, 0, 0, Action::DeletionStep, slot, f);
                                continue;
                            }
                            // Preserve succeeded (or body was already absent).
                            model.step(&mut choices, 0, 0, Action::DeletionStep, slot, Fault::None);
                            let body_path =
                                drafts_dir(&env.data_dir).join(format!("{draft_id}.draft"));
                            if matches!(
                                fs_metadata::path_status(&body_path),
                                Ok(status) if status.is_present()
                            ) {
                                model.step(
                                    &mut choices,
                                    0,
                                    0,
                                    Action::DeletionStep,
                                    slot,
                                    Fault::BeforeEffect,
                                );
                                continue;
                            }
                            model.step(&mut choices, 0, 0, Action::DeletionStep, slot, Fault::None);
                            let remove_fault = if restored.manifest.find_by_id(draft_id).is_none() {
                                Fault::None
                            } else if disk_manifest.find_by_id(draft_id).is_none() {
                                Fault::AfterEffect
                            } else {
                                Fault::BeforeEffect
                            };
                            model.step(
                                &mut choices,
                                0,
                                0,
                                Action::DeletionStep,
                                slot,
                                remove_fault,
                            );
                        }
                    }
                    env.sync_session_on_disk(&model);
                }
            }
            Action::Crash => {
                env.drop_held_values();
                model.step(&mut choices, 0, 0, Action::Crash, 0, Fault::None);
            }
            Action::CloseWindow => {
                env.drop_held_values();
                model.step(&mut choices, 0, 0, Action::CloseWindow, 0, Fault::None);
                env.sync_session_on_disk(&model);
            }
            Action::Open => {
                model.step(&mut choices, 0, 0, Action::Open, id, Fault::None);
                env.sync_session_on_disk(&model);
            }
            Action::Edit => {
                model.step(&mut choices, 0, 0, Action::Edit, id, Fault::None);
            }
            Action::ExternalMtime => {
                let prev_backing = model.backing[id];
                model.step(&mut choices, 0, 0, Action::ExternalMtime, id, Fault::None);
                if model.backing[id] != prev_backing {
                    env.advance_backing_mtime(id, model.backing[id], false);
                }
            }
            Action::Save => {
                let ed = model.me().editors[id];
                let will_save = ed.open && ed.deletion.is_none();
                model.step(&mut choices, 0, 0, Action::Save, id, Fault::None);
                if will_save && !model.untitled(id) {
                    env.advance_backing_mtime(id, model.backing[id], true);
                }
                env.tokens[id] = None;
            }
            Action::Discard => {
                model.step(&mut choices, 0, 0, Action::Discard, id, Fault::None);
                env.tokens[id] = None;
            }
            Action::Register => {
                let ed = model.me().editors[id];
                let needed = registration_required(
                    !model.untitled(id),
                    model.me().known[id].is_some(),
                    model.me().trusted,
                );
                let decision = body_write_decision(ownership(model.window_body_facts(id)), needed);
                if ed.open
                    && ed.dirty
                    && !ed.token
                    && ed.deletion.is_none()
                    && model.lane_admits(LaneUse::Pass(slot_u8(id)))
                {
                    match decision {
                        BodyWriteDecision::Hold => {}
                        BodyWriteDecision::Write | BodyWriteDecision::PreserveThenWrite => {
                            env.tokens[id] = RegisteredDraft::without_registration(
                                &env.draft_ids[id],
                                !model.untitled(id),
                                model.me().known[id].is_some(),
                                model.me().trusted,
                            );
                        }
                        BodyWriteDecision::RegisterFirst => {
                            let session = env.session_for_open_tabs(&model);
                            let entry = env.make_entry(id);
                            let (plan, injected) = tap_plan_for(
                                op.fault,
                                domain,
                                AfterTarget::Destination(manifest_file_path(&env.data_dir)),
                            );
                            let guard = tap::install(plan);
                            let reg_result = draft_service::register_draft_entries(
                                &env.data_dir,
                                &session,
                                env.authority,
                                std::slice::from_ref(&entry),
                            );
                            let trace = guard.finish();
                            let drafts_parent = drafts_dir(&env.data_dir);
                            let fired = classify_trace_fault(&trace, Some(&drafts_parent));
                            match reg_result {
                                Ok(DraftRegistration::Committed {
                                    commit,
                                    mut registered,
                                }) => {
                                    env.authority = commit.authority;
                                    for slot in 0..DRAFT_IDS {
                                        env.persisted_entries[slot] = commit
                                            .manifest
                                            .find_by_id(&env.draft_ids[slot])
                                            .cloned();
                                    }
                                    env.tokens[id] = registered.pop();
                                    model_fault = Fault::None;
                                }
                                Ok(DraftRegistration::Additive {
                                    authority,
                                    mut registered,
                                }) => {
                                    env.authority = authority;
                                    env.tokens[id] = registered.pop();
                                    if fired == TraceFaultKind::None {
                                        return Err(format!(
                                            "step {step_idx} Register unexpectedly fell back to Additive without fault"
                                        ));
                                    }
                                    model_fault = Fault::BeforeEffect;
                                    let id_moved = if !model.untitled(id)
                                        && !model.entry[id].present
                                        && model.body[id].is_some()
                                    {
                                        let moved = env.body_absent_on_disk(id);
                                        choices.queued_flags.push_back(moved);
                                        moved
                                    } else {
                                        true
                                    };
                                    if id_moved {
                                        env.queue_reconcile_flags(
                                            &mut choices,
                                            &model,
                                            Some(id),
                                            true,
                                        );
                                        choices.queued_flags.push_back(true);
                                    }
                                }
                                Err(error) => {
                                    env.authority = error.authority();
                                    if !injected || fired == TraceFaultKind::None {
                                        return Err(format!(
                                            "step {step_idx} Register unexpected error: {error}"
                                        ));
                                    }
                                    model_fault = if fired == TraceFaultKind::AfterEffect {
                                        Fault::AfterEffect
                                    } else {
                                        Fault::BeforeEffect
                                    };
                                    if model_fault == Fault::BeforeEffect {
                                        let id_moved = if !model.untitled(id)
                                            && !model.entry[id].present
                                            && model.body[id].is_some()
                                        {
                                            let moved = env.body_absent_on_disk(id);
                                            choices.queued_flags.push_back(moved);
                                            moved
                                        } else {
                                            true
                                        };
                                        if id_moved {
                                            env.queue_reconcile_flags(
                                                &mut choices,
                                                &model,
                                                Some(id),
                                                true,
                                            );
                                            choices.queued_flags.push_back(false);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                model.step(&mut choices, 0, 0, Action::Register, id, model_fault);
            }
            Action::WriteBody => {
                let ed = model.me().editors[id];
                if ed.open && ed.dirty && ed.token {
                    let needed = registration_required(
                        !model.untitled(id),
                        model.me().known[id].is_some(),
                        model.me().trusted,
                    );
                    if body_write_decision(ownership(model.window_body_facts(id)), needed)
                        != BodyWriteDecision::Hold
                    {
                        let token = env.tokens[id]
                            .take()
                            .expect("driver token must exist whenever model token is set");
                        let text = content_text(ed.content);
                        let (plan, injected) = tap_plan_for(
                            op.fault,
                            domain,
                            AfterTarget::Destination(body_write_destination(
                                &env.data_dir,
                                &env.draft_ids[id],
                            )),
                        );
                        let guard = tap::install(plan);
                        let write_res = draft_service::write_draft(&env.data_dir, &token, &text);
                        let trace = guard.finish();
                        let drafts_parent = drafts_dir(&env.data_dir);
                        let fired = classify_trace_fault(&trace, Some(&drafts_parent));
                        match write_res {
                            Ok(()) => model_fault = Fault::None,
                            Err(error) => {
                                if !injected || fired == TraceFaultKind::None {
                                    return Err(format!(
                                        "step {step_idx} WriteBody unexpected error: {error}"
                                    ));
                                }
                                model_fault = if fired == TraceFaultKind::AfterEffect {
                                    Fault::AfterEffect
                                } else {
                                    Fault::BeforeEffect
                                };
                            }
                        }
                    }
                }
                model.step(&mut choices, 0, 0, Action::WriteBody, id, model_fault);
            }
            Action::Commit => {
                let ed = model.me().editors[id];
                if ed.written.is_some() {
                    let session = env.session_for_open_tabs(&model);
                    let entry = env.make_entry(id);
                    let (plan, injected) = tap_plan_for(
                        op.fault,
                        domain,
                        AfterTarget::Destination(manifest_file_path(&env.data_dir)),
                    );
                    let guard = tap::install(plan);
                    let commit_res = draft_service::update_manifest(
                        &env.data_dir,
                        &session,
                        env.authority,
                        |manifest| {
                            manifest.upsert(entry);
                        },
                    );
                    let trace = guard.finish();
                    let drafts_parent = drafts_dir(&env.data_dir);
                    let fired = classify_trace_fault(&trace, Some(&drafts_parent));
                    match commit_res {
                        Ok(commit) => {
                            env.authority = commit.authority;
                            for slot in 0..DRAFT_IDS {
                                env.persisted_entries[slot] =
                                    commit.manifest.find_by_id(&env.draft_ids[slot]).cloned();
                            }
                            model_fault = Fault::None;
                        }
                        Err(error) => {
                            env.authority = error.authority();
                            if !injected || fired == TraceFaultKind::None {
                                return Err(format!(
                                    "step {step_idx} Commit unexpected error: {error}"
                                ));
                            }
                            model_fault = if fired == TraceFaultKind::AfterEffect {
                                Fault::AfterEffect
                            } else {
                                Fault::BeforeEffect
                            };
                            if model_fault == Fault::BeforeEffect {
                                env.queue_reconcile_flags(&mut choices, &model, None, true);
                            }
                        }
                    }
                }
                model.step(&mut choices, 0, 0, Action::Commit, id, model_fault);
            }
            Action::DeletionStep => {
                if let Some(step) = model.me().editors[id].deletion
                    && model.lane_admits(LaneUse::Deletion(slot_u8(id)))
                {
                    match step {
                        DeletionStep::Preserve => {
                            let entry = env.resolve_entry_for(id);
                            let (plan, injected) =
                                tap_plan_for(op.fault, domain, AfterTarget::NextSetAsideSyncDir);
                            let guard = tap::install(plan);
                            let res =
                                draft_service::preserve_stale_draft_body(&env.data_dir, &entry);
                            let trace = guard.finish();
                            let set_aside_parent = set_aside::dir(&env.data_dir);
                            let fired = classify_trace_fault(&trace, Some(&set_aside_parent));
                            match res {
                                Ok(_) => model_fault = Fault::None,
                                Err(error) => {
                                    if !injected || fired == TraceFaultKind::None {
                                        return Err(format!(
                                            "step {step_idx} DeletionStep(Preserve) unexpected error: {error}"
                                        ));
                                    }
                                    model_fault = if fired == TraceFaultKind::AfterEffect {
                                        Fault::AfterEffect
                                    } else {
                                        Fault::BeforeEffect
                                    };
                                }
                            }
                        }
                        DeletionStep::DeleteBody => {
                            let (plan, injected) =
                                tap_plan_for(op.fault, domain, AfterTarget::None);
                            let guard = tap::install(plan);
                            let res =
                                draft_service::delete_draft_file(&env.data_dir, &env.draft_ids[id]);
                            let trace = guard.finish();
                            let fired = classify_trace_fault(&trace, None);
                            match res {
                                Ok(()) => model_fault = Fault::None,
                                Err(error) => {
                                    if !injected || fired == TraceFaultKind::None {
                                        return Err(format!(
                                            "step {step_idx} DeletionStep(DeleteBody) unexpected error: {error}"
                                        ));
                                    }
                                    model_fault = Fault::BeforeEffect;
                                }
                            }
                        }
                        DeletionStep::RemoveEntry => {
                            let session = env.session_for_open_tabs(&model);
                            let (plan, injected) = tap_plan_for(
                                op.fault,
                                domain,
                                AfterTarget::Destination(manifest_file_path(&env.data_dir)),
                            );
                            let guard = tap::install(plan);
                            let res = draft_service::remove_manifest_entry(
                                &env.data_dir,
                                &session,
                                env.authority,
                                &env.draft_ids[id],
                            );
                            let trace = guard.finish();
                            let drafts_parent = drafts_dir(&env.data_dir);
                            let fired = classify_trace_fault(&trace, Some(&drafts_parent));
                            match res {
                                Ok(commit) => {
                                    env.authority = commit.authority;
                                    for slot in 0..DRAFT_IDS {
                                        env.persisted_entries[slot] = commit
                                            .manifest
                                            .find_by_id(&env.draft_ids[slot])
                                            .cloned();
                                    }
                                    model_fault = Fault::None;
                                }
                                Err(error) => {
                                    env.authority = error.authority();
                                    if !injected || fired == TraceFaultKind::None {
                                        return Err(format!(
                                            "step {step_idx} DeletionStep(RemoveEntry) unexpected error: {error}"
                                        ));
                                    }
                                    model_fault = if fired == TraceFaultKind::AfterEffect {
                                        Fault::AfterEffect
                                    } else {
                                        Fault::BeforeEffect
                                    };
                                    if model_fault == Fault::BeforeEffect {
                                        env.queue_reconcile_flags(
                                            &mut choices,
                                            &model,
                                            Some(id),
                                            true,
                                        );
                                    }
                                }
                            }
                        }
                        DeletionStep::Done | DeletionStep::Stopped => {}
                    }
                }
                model.step(&mut choices, 0, 0, Action::DeletionStep, id, model_fault);
            }
            Action::Inspect => {
                if model.me().trusted && model.lane_admits(LaneUse::Cleanup) {
                    let guard = tap::install(TapPlan::record());
                    let plan =
                        draft_service::inspect_orphan_cleanup_against_persisted(&env.data_dir, 0)
                            .map_err(|e| format!("step {step_idx} Inspect failed: {e}"))?;
                    let _trace = guard.finish();
                    env.cleanup_plan = Some(plan);
                }
                model.step(&mut choices, 0, 0, Action::Inspect, id, Fault::None);
            }
            Action::ExecCleanup => {
                run_exec_cleanup_step(
                    &mut env,
                    &mut model,
                    &mut choices,
                    id,
                    op.fault,
                    domain,
                    step_idx,
                )?;
            }
            Action::RestoreApply => {
                if model.me().editors[id].restore_pending {
                    let entry = env.resolve_entry_for(id);
                    let (plan, injected) =
                        tap_plan_for(op.fault, domain, AfterTarget::NextSetAsideSyncDir);
                    let guard = tap::install(plan);
                    let resolved = draft_service::resolve_draft_restore(&env.data_dir, &entry);
                    let ending = match resolved {
                        Ok(FileDraftRestoreResolution::Restore { .. }) => {
                            const WINDOW_ENDINGS: [RestoreEnding; 3] = [
                                RestoreEnding::Applied,
                                RestoreEnding::EditedOver,
                                RestoreEnding::InstallCancelled,
                            ];
                            WINDOW_ENDINGS[usize::from(op.restore_choice) % WINDOW_ENDINGS.len()]
                        }
                        Ok(FileDraftRestoreResolution::Skip(FileDraftRestoreSkip::Stale)) => {
                            RestoreEnding::Stale
                        }
                        Ok(FileDraftRestoreResolution::Skip(FileDraftRestoreSkip::Unavailable)) => {
                            RestoreEnding::Unavailable
                        }
                        Ok(FileDraftRestoreResolution::Skip(
                            FileDraftRestoreSkip::MissingDraft,
                        )) => RestoreEnding::MissingBody,
                        Ok(FileDraftRestoreResolution::Skip(FileDraftRestoreSkip::Oversized)) => {
                            return Err(format!(
                                "step {step_idx} RestoreApply observed Oversized with tiny payloads"
                            ));
                        }
                        Err(_) => {
                            if !injected {
                                return Err(format!(
                                    "step {step_idx} RestoreApply unexpected read error"
                                ));
                            }
                            RestoreEnding::ReadFailed
                        }
                    };
                    if unapplied_restore_disposition(ending) == RestoreDisposition::PreserveCopy {
                        match draft_service::preserve_stale_draft_body(&env.data_dir, &entry) {
                            Ok(_) => model_fault = Fault::None,
                            Err(error) => {
                                if !injected {
                                    return Err(format!(
                                        "step {step_idx} RestoreApply preserve_stale_draft_body failed: {error}"
                                    ));
                                }
                                model_fault = Fault::BeforeEffect;
                            }
                        }
                    }
                    let trace = guard.finish();
                    let set_aside_parent = set_aside::dir(&env.data_dir);
                    if matches!(model_fault, Fault::BeforeEffect)
                        && classify_trace_fault(&trace, Some(&set_aside_parent))
                            == TraceFaultKind::AfterEffect
                    {
                        model_fault = Fault::AfterEffect;
                    }
                    if ending == RestoreEnding::Stale
                        && !model.stale(id)
                        && !model.me().editors[id].unverified_mtime
                    {
                        return Err(format!(
                            "step {step_idx} RestoreApply returned Stale while model.stale({id})=false (entry=(present={}, backing={}), backing={}, persisted={:?}, disk_mtime={:?})\nhistory: {}",
                            model.entry[id].present,
                            model.entry[id].backing,
                            model.backing[id],
                            entry,
                            env.backing_paths[id]
                                .as_deref()
                                .and_then(editor_io::mtime_secs),
                            history.join(" -> "),
                        ));
                    }
                    choices.next_restore_ending = Some(ending);
                }
                model.step(&mut choices, 0, 0, Action::RestoreApply, id, model_fault);
            }
            Action::OpenWindow => {}
        }

        model.assert_invariants();
        let real = env.abstract_state()?;
        if let Err(err) = compare_states(step_idx + 1, action, id, &real, &model, &mut counts) {
            return Err(format!("{err}\nhistory: {}", history.join(" -> ")));
        }
    }

    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn op(
        action: Action,
        model: &Journal<SingleWindowScope, DRAFT_IDS, 1, 1>,
        id: usize,
    ) -> DraftOp {
        let mut action_index = None;
        for candidate_action_idx in 0..48 {
            let mut found_id_idx = None;
            for candidate_id_idx in 0..DRAFT_IDS {
                let probe = DraftOp {
                    action_index: candidate_action_idx,
                    id_index: candidate_id_idx,
                    fault: DraftFault::None,
                    restore_choice: 0,
                };
                if pick_enabled_step(model, probe) == (action, id) {
                    found_id_idx = Some(candidate_id_idx);
                    break;
                }
            }
            if let Some(id_idx) = found_id_idx {
                action_index = Some((candidate_action_idx, id_idx));
                break;
            }
        }
        let (action_index, id_index) =
            action_index.unwrap_or_else(|| panic!("action {action:?} id={id} not enabled"));
        DraftOp {
            action_index,
            id_index,
            fault: DraftFault::None,
            restore_choice: 0,
        }
    }

    #[test]
    fn edit_register_write_commit_discard_conforms_to_journal_model() {
        let dir = TempDir::new().expect("tempdir");
        let mut model = Journal::<SingleWindowScope, DRAFT_IDS, 1, 1>::previous_session(
            &mut ScriptChoices::new([false; INITIAL_LAYOUT_BITS]),
        );
        let mut choices = ScriptChoices::new([false; INITIAL_LAYOUT_BITS]);
        let mut script = Vec::new();
        for &(action, id) in &[
            (Action::Startup, 0),
            (Action::Open, 0),
            (Action::Edit, 0),
            (Action::Register, 0),
            (Action::WriteBody, 0),
            (Action::Commit, 0),
            (Action::Discard, 0),
            (Action::DeletionStep, 0),
            (Action::DeletionStep, 0),
        ] {
            let next_op = op(action, &model, id);
            script.push(next_op);
            model.step(&mut choices, 0, 0, action, id, Fault::None);
        }
        let counts = check_draft_script(
            dir.path(),
            [false; INITIAL_LAYOUT_BITS],
            &script,
            FaultDomain::BeforeOnly,
        )
        .expect("happy path conforms");
        assert!(counts.write_body >= 1 && counts.commit >= 1 && counts.deletion_step >= 2);
    }

    #[test]
    fn startup_mtime_fault_preserves_authority_and_defers_stale_cleanup() {
        let initial_bits = [false, false, false, true, false, true, false, false];
        let dir = tempfile::tempdir().expect("tempdir");
        let ops = [DraftOp {
            action_index: 0,
            id_index: 0,
            fault: DraftFault::At(23),
            restore_choice: 0,
        }];
        check_draft_script(dir.path(), initial_bits, &ops, FaultDomain::Full)
            .expect("startup mtime fault conforms");

        for call in 0..32 {
            let sweep_dir = tempfile::tempdir().expect("tempdir");
            let sweep_ops = [DraftOp {
                action_index: 0,
                id_index: 0,
                fault: DraftFault::At(call),
                restore_choice: 0,
            }];
            check_draft_script(
                sweep_dir.path(),
                initial_bits,
                &sweep_ops,
                FaultDomain::Full,
            )
            .unwrap_or_else(|err| panic!("fault at call {call} diverged: {err}"));
        }
    }

    #[test]
    fn exploratory_mixed_scripts_with_before_effect_faults() {
        let mut rng_state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next_u64 = || {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            rng_state
        };
        let mut failures = Vec::new();
        for case_idx in 0..64 {
            let mask = (next_u64() & 0xff) as u8;
            let mut bits = [false; INITIAL_LAYOUT_BITS];
            for (i, slot) in bits.iter_mut().enumerate() {
                *slot = (mask >> i) & 1 == 1;
            }
            let len = 8 + usize::try_from(next_u64() % 9).expect("length mod 9 fits in usize");
            let mut ops = Vec::with_capacity(len);
            for _ in 0..len {
                let fault = if next_u64() % 4 == 0 {
                    DraftFault::BeforeEffect
                } else {
                    DraftFault::None
                };
                ops.push(DraftOp {
                    action_index: usize::try_from(next_u64() & u64::from(u32::MAX))
                        .expect("masked u32 fits in usize"),
                    id_index: usize::try_from(next_u64() & u64::from(u32::MAX))
                        .expect("masked u32 fits in usize"),
                    fault,
                    restore_choice: u8::try_from(next_u64() & 0xff)
                        .expect("masked byte fits in u8"),
                });
            }
            let dir = TempDir::new().expect("tempdir");
            if let Err(err) = check_draft_script(dir.path(), bits, &ops, FaultDomain::BeforeOnly) {
                failures.push(format!("case {case_idx} (mask={mask:#010b}): {err}"));
            }
        }
        assert!(
            failures.is_empty(),
            "divergences ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn previous_session_variants_and_startup_faults_conform() {
        for mask in 0u8..=255 {
            let mut bits = [false; INITIAL_LAYOUT_BITS];
            for (i, bit_slot) in bits.iter_mut().enumerate() {
                *bit_slot = (mask >> i) & 1 == 1;
            }
            for fault in [DraftFault::None, DraftFault::BeforeEffect] {
                let dir = TempDir::new().expect("tempdir");
                let ops = [DraftOp {
                    action_index: 0,
                    id_index: 0,
                    fault,
                    restore_choice: 0,
                }];
                check_draft_script(dir.path(), bits, &ops, FaultDomain::BeforeOnly)
                    .unwrap_or_else(|err| panic!("mask={mask:#010b} fault={fault:?}: {err}"));
            }
        }
    }

    #[test]
    fn exec_cleanup_after_effect_revokes_trust_and_forces_reregistration() {
        let dir = TempDir::new().expect("tempdir");
        let bits = [true, false, false, true, false, false, false, true];
        let mut choices = ScriptChoices::new(bits);
        let mut model =
            Journal::<SingleWindowScope, DRAFT_IDS, 1, 1>::previous_session(&mut choices);
        let mut script = Vec::new();
        for &(action, id, fault) in &[
            (Action::Startup, 0, DraftFault::None),
            (Action::Inspect, 0, DraftFault::None),
            (Action::ExecCleanup, 0, DraftFault::AfterEffect),
            (Action::Edit, 0, DraftFault::None),
            (Action::Register, 0, DraftFault::None),
            (Action::WriteBody, 0, DraftFault::None),
        ] {
            let mut next_op = op(action, &model, id);
            next_op.fault = fault;
            script.push(next_op);
            let model_fault = match fault {
                DraftFault::None => Fault::None,
                DraftFault::BeforeEffect | DraftFault::At(_) => Fault::BeforeEffect,
                DraftFault::AfterEffect => Fault::AfterEffect,
            };
            model.step(&mut choices, 0, 0, action, id, model_fault);
        }
        check_draft_script(dir.path(), bits, &script, FaultDomain::Full)
            .expect("ExecCleanup AfterEffect must revoke trust before next body write");
    }

    #[test]
    fn exec_cleanup_at_k_missing_entry_script_conforms() {
        // Slot 1: missing-body entry (entry=true, body=false, backing=false).
        let bits = [false, false, false, true, false, false, false, false];
        for call in 0..=32 {
            let dir = TempDir::new().expect("tempdir");
            let mut choices = ScriptChoices::new(bits);
            let mut model =
                Journal::<SingleWindowScope, DRAFT_IDS, 1, 1>::previous_session(&mut choices);
            let mut script = Vec::new();
            for &(action, fault) in &[
                (Action::Startup, DraftFault::None),
                (Action::Inspect, DraftFault::None),
                (Action::ExecCleanup, DraftFault::At(call)),
            ] {
                let mut next_op = op(action, &model, 0);
                next_op.fault = fault;
                script.push(next_op);
                let model_fault = match fault {
                    DraftFault::None | DraftFault::At(_) => Fault::None,
                    DraftFault::BeforeEffect => Fault::BeforeEffect,
                    DraftFault::AfterEffect => Fault::AfterEffect,
                };
                model.step(&mut choices, 0, 0, action, 0, model_fault);
            }
            check_draft_script(dir.path(), bits, &script, FaultDomain::Full)
                .unwrap_or_else(|err| panic!("At({call}) diverged: {err}"));
        }
    }

    #[test]
    fn exec_cleanup_at_k_with_both_orphan_and_missing_entry_conforms() {
        // Empty previous session except slot 1 missing-body entry (`entry=true, body=false`);
        // after Startup, place an unaccepted untitled orphan body in slot 2 so both
        // `orphan_candidate[2]` and `missing_candidate[1]` coexist in one Inspect -> ExecCleanup
        // pass while keeping S1/S3/S4 invariants satisfied.
        let bits = [false, false, false, true, false, false, false, false];
        let mut saw_orphan_fail_only = false;
        let mut saw_before_rename_manifest_fail = false;
        let mut saw_after_effect_manifest_fail = false;

        for call in 0..=32 {
            let dir = TempDir::new().expect("tempdir");
            let mut choices = ScriptChoices::new(bits);
            let mut model =
                Journal::<SingleWindowScope, DRAFT_IDS, 1, 1>::previous_session(&mut choices);
            let mut env = DriverEnv::new(dir.path());
            env.seed_previous_session(&model)
                .expect("seed previous session");
            env.authority = draft_service::load_restore_state_cancellable(
                &env.data_dir,
                &AtomicBool::new(false),
            )
            .manifest_authority;
            assert!(env.authority.is_trusted());
            model.step(&mut choices, 0, 0, Action::Startup, 0, Fault::None);

            let content = 1;
            model.body[2] = Some(content);
            model.confirmed_body[2] = Some(content);
            draft_service::fixture::write_body(
                &env.data_dir,
                &env.draft_ids[2],
                &content_text(content),
            )
            .expect("seed untitled orphan body");
            model.assert_invariants();

            model.step(&mut choices, 0, 0, Action::Inspect, 0, Fault::None);
            assert!(model.me().orphan_candidate[2].is_some());
            assert!(model.me().missing_candidate[1].is_some());
            env.cleanup_plan = Some(
                draft_service::inspect_orphan_cleanup_against_persisted(&env.data_dir, 0)
                    .expect("inspect cleanup"),
            );

            run_exec_cleanup_step(
                &mut env,
                &mut model,
                &mut choices,
                0,
                DraftFault::At(call),
                FaultDomain::Full,
                2,
            )
            .unwrap_or_else(|err| panic!("At({call}) ExecCleanup failed: {err}"));
            model.assert_invariants();
            let real = env
                .abstract_state()
                .expect("abstract state after ExecCleanup");
            let mut counts = ReachCounts::default();
            compare_states(3, Action::ExecCleanup, 0, &real, &model, &mut counts)
                .unwrap_or_else(|err| panic!("At({call}) diverged: {err}"));

            let orphan_exists = fixture::exists(
                &drafts_dir(&env.data_dir).join(format!("{}.draft", env.draft_ids[2])),
            );
            let missing_entry_exists = draft_service::load_manifest(&env.data_dir)
                .expect("load manifest after At(k)")
                .find_by_id(&env.draft_ids[1])
                .is_some();
            if orphan_exists && !missing_entry_exists {
                saw_orphan_fail_only = true;
            }
            if !orphan_exists && missing_entry_exists {
                saw_before_rename_manifest_fail = true;
            }
            if !orphan_exists && !missing_entry_exists && !model.me().trusted {
                saw_after_effect_manifest_fail = true;
            }
        }
        assert!(
            saw_orphan_fail_only,
            "At(k) sweep must hit orphan unlink failure while missing-entry commit succeeds"
        );
        assert!(
            saw_before_rename_manifest_fail,
            "At(k) sweep must hit BeforeRename manifest failure after orphan unlink succeeds"
        );
        assert!(
            saw_after_effect_manifest_fail,
            "At(k) sweep must hit AfterEffect manifest failure revoking trust"
        );
    }
}
