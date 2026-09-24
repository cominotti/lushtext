// SPDX-License-Identifier: GPL-3.0-or-later

//! E1 part B: Quint Connect replay of `../quint/journal.qnt`'s `k3_service`
//! instance against the REAL `lushtext_core::services::draft_service` over a
//! fresh tempdir per trace.
//!
//! Scope: one process, fault-free, and only `Edit`, `Register`, `WriteBody`,
//! `Commit`, `Discard`, `DeletionStepAction`, `RestoreApply`, `Crash`, `Startup`, with no
//! backing-file movement. The disk is real; the window is not. The service
//! has no editor state, so this driver plays the window (the role
//! `ui/window/drafts/journal.rs` has in production): it keeps the per-id
//! editor bits the Kani harness keeps, takes every decision from
//! `journal_core` as the harness does, and turns each disk effect into the
//! public service call production makes:
//!
//! | model effect                  | service call                                  |
//! |-------------------------------|-----------------------------------------------|
//! | `Register` (RegisterFirst)    | `register_draft_entries` -> `RegisteredDraft` |
//! | `Register` (Write)            | `RegisteredDraft::without_registration`       |
//! | `WriteBody`                   | `write_draft(token)`                          |
//! | `Commit`                      | `update_manifest(upsert)`                     |
//! | deletion `Preserve`           | `preserve_stale_draft_body(entry)`            |
//! | `RestoreApply` PreserveCopy   | `preserve_stale_draft_body(entry)`            |
//! | deletion `DeleteBody`         | `delete_draft_file`                           |
//! | deletion `RemoveEntry`        | `remove_manifest_entry`                       |
//! | `Startup`                     | `load_restore_state`                          |
//! | `Crash`                       | drop every in-memory value (tokens included)  |
//!
//! Compared after every step: which ids have a manifest entry, which have a
//! body file and with which content, which contents are in the set-aside
//! area, and the window's `running`/`trusted` flags (the latter is the
//! service's returned authority). Content identity: a body's bytes are
//! `c{content id}`, the model's numbering; `saved_at_secs` is the committed
//! content id, standing in for production's commit time (distinct per commit).
//!
//! Finding: every seed of `k3_service_weighted` diverges on `preserved`; see
//! `tests/e1_findings.rs`. `E1_CONTENT_STAMP=1` explores past it.

#![expect(
    non_snake_case,
    reason = "`switch!` binds nondet picks by their Quint names (entryPresent)"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lushtext_core::model::draft::{DraftEntry, DraftManifest, DraftManifestAuthority};
use lushtext_core::model::session::SessionData;
use lushtext_core::services::draft_service::{
    self, DraftRegistration, RegisteredDraft, journal_core, set_aside,
};
use lushtext_core::services::draft_service::journal_core::{
    BodyFacts, BodyWriteDecision, DeletionStep, RestoreDisposition, RestoreEnding,
};
use lushtext_journal_stateright::env::{CLOSED, Editor, IDS, MAX_EDITS};
use quint_connect::{Driver, Result, State, Step, quint_run, switch};
use serde::Deserialize;

mod common;
use common::{QAction, QEnding, QJournal};

// --- What is compared ------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
struct DiskView {
    entry: BTreeMap<i64, bool>,
    /// -1 = no body file.
    body: BTreeMap<i64, i64>,
    preserved: BTreeSet<i64>,
    running: bool,
    trusted: bool,
}

impl<'de> Deserialize<'de> for DiskView {
    fn deserialize<D: serde::Deserializer<'de>>(_: D) -> core::result::Result<Self, D::Error> {
        Err(serde::de::Error::custom("unused: DiskView::from_spec is overridden"))
    }
}

impl From<&QJournal> for DiskView {
    fn from(j: &QJournal) -> Self {
        Self {
            entry: j.entry.iter().map(|(id, e)| (*id, e.present)).collect(),
            body: j.body.clone(),
            preserved: j.preserved.clone(),
            running: j.running,
            trusted: j.trusted,
        }
    }
}

// --- The driver ----------------------------------------------------------------

fn body_text(content: i64) -> String {
    format!("c{content}")
}

fn parse_body(text: &str) -> i64 {
    text.strip_prefix('c')
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("unexpected body bytes {text:?}"))
}

/// The window half of the harness: what production keeps in memory.
struct Window {
    editors: [Editor; IDS],
    /// Body-write tokens; `RegisteredDraft` is neither `Clone` nor
    /// constructible, so it lives here between `Register` and `WriteBody`.
    tokens: [Option<RegisteredDraft>; IDS],
    running: bool,
    authority: DraftManifestAuthority,
}

impl Window {
    fn closed() -> Self {
        Self {
            editors: [CLOSED; IDS],
            tokens: [None, None, None],
            running: false,
            authority: DraftManifestAuthority::default(),
        }
    }
}

struct ServiceDriver {
    dir: Option<tempfile::TempDir>,
    window: Window,
    session: SessionData,
    next_content: i64,
    edits: usize,
    traces: usize,
    steps: usize,
    effective: usize,
    service_calls: usize,
    /// Calls per service entry point, for the report.
    calls: BTreeMap<&'static str, usize>,
    started: std::time::Instant,
}

impl Default for ServiceDriver {
    fn default() -> Self {
        Self {
            dir: None,
            window: Window::closed(),
            session: SessionData::default(),
            next_content: 0,
            edits: 0,
            traces: 0,
            steps: 0,
            effective: 0,
            service_calls: 0,
            calls: BTreeMap::new(),
            started: std::time::Instant::now(),
        }
    }
}

impl Drop for ServiceDriver {
    fn drop(&mut self) {
        eprintln!(
            "E1-REPORT service: {} traces, {} steps (init included; {} state-changing), {} service calls, in {:?} (trace generation included)",
            self.traces,
            self.steps,
            self.effective,
            self.service_calls,
            self.started.elapsed()
        );
        eprintln!("E1-REPORT service calls by kind: {:?}", self.calls);
    }
}

impl ServiceDriver {
    fn count(&mut self, call: &'static str) {
        self.service_calls += 1;
        *self.calls.entry(call).or_default() += 1;
    }

    fn data(&self) -> &Path {
        self.dir.as_ref().expect("init runs first").path()
    }

    fn draft_id(id: usize) -> String {
        draft_service::draft_id_for_path(&Self::original_path(id))
    }

    fn original_path(id: usize) -> PathBuf {
        PathBuf::from(format!("/e1-quint-connect/file{id}.txt"))
    }

    fn entry(id: usize, saved_at: i64) -> DraftEntry {
        DraftEntry {
            draft_id: Self::draft_id(id),
            original_path: Some(Self::original_path(id)),
            // No `Save`/`ExternalMtime` in scope: the backing never moves.
            original_mtime_secs: Some(0),
            saved_at_secs: u64::try_from(saved_at).expect("content ids are non-negative"),
        }
    }

    fn manifest(&self) -> DraftManifest {
        draft_service::load_manifest(self.data()).expect("manifest loads")
    }

    fn body(&self, id: usize) -> Option<i64> {
        draft_service::read_draft(self.data(), &Self::draft_id(id))
            .expect("body reads")
            .map(|text| parse_body(&text))
    }

    fn trusted(&self) -> bool {
        self.window.authority.is_trusted()
    }

    fn view(&self) -> DiskView {
        let manifest = self.manifest();
        let preserved = set_aside::list(self.data())
            .expect("set-aside lists")
            .into_iter()
            .map(|b| parse_body(&std::fs::read_to_string(&b.path).expect("set-aside reads")))
            .collect();
        DiskView {
            entry: (0..IDS)
                .map(|id| (id as i64, manifest.find_by_id(&Self::draft_id(id)).is_some()))
                .collect(),
            body: (0..IDS)
                .map(|id| (id as i64, self.body(id).unwrap_or(-1)))
                .collect(),
            preserved,
            running: self.window.running,
            trusted: self.window.running && self.trusted(),
        }
    }

    /// The previous session's disk (written raw: an unregistered body is
    /// exactly what a crash leaves), then `Startup`.
    fn init(&mut self, entry_present: &BTreeMap<i64, bool>, body_present: &BTreeMap<i64, bool>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let drafts = draft_service::drafts_dir(dir.path());
        std::fs::create_dir_all(&drafts).expect("drafts dir");
        let mut manifest = DraftManifest::default();
        for id in 0..IDS {
            let content = 1 + id as i64;
            let body = body_present[&(id as i64)];
            if body {
                std::fs::write(
                    drafts.join(format!("{}.draft", Self::draft_id(id))),
                    body_text(content),
                )
                .expect("seed body");
            }
            if entry_present[&(id as i64)] {
                manifest.upsert(Self::entry(id, if body { content } else { 0 }));
            }
        }
        draft_service::save_manifest(dir.path(), &manifest).expect("seed manifest");
        self.dir = Some(dir);
        self.window = Window::closed();
        self.next_content = 1 + IDS as i64;
        self.edits = 0;
        self.traces += 1;
        self.steps += 1;
        self.startup();
    }

    fn startup(&mut self) {
        if self.window.running {
            return;
        }
        let restore = draft_service::load_restore_state(self.data());
        self.count("load_restore_state");
        self.window.running = true;
        self.window.authority = restore.manifest_authority;
        for id in 0..IDS {
            if restore.manifest.find_by_id(&Self::draft_id(id)).is_none() {
                continue;
            }
            let mut editor = CLOSED;
            editor.open = true;
            editor.known_entry = true;
            // Stale retirement is out of scope (the backing never moves).
            if self.body(id).is_some() {
                editor.restore_pending = true;
            }
            self.window.editors[id] = editor;
        }
    }

    fn window_body_facts(&self, id: usize) -> BodyFacts {
        let e = self.window.editors[id];
        BodyFacts::from_window_view(e.known_entry, e.restore_pending)
    }

    fn step(&mut self, a: QAction, id: i64) {
        self.steps += 1;
        let before = self.view();
        self.act(a, usize::try_from(id).expect("ids are non-negative"));
        if self.view() != before {
            self.effective += 1;
        }
    }

    fn act(&mut self, a: QAction, id: usize) {
        if !self.window.running && a != QAction::Startup {
            return;
        }
        let editor = self.window.editors[id];
        match a {
            QAction::Edit => {
                if editor.open && self.edits < MAX_EDITS {
                    let c = self.next_content;
                    self.next_content += 1;
                    self.edits += 1;
                    let e = &mut self.window.editors[id];
                    e.content = u8::try_from(c).expect("small content ids");
                    e.dirty = true;
                    e.token = false;
                    self.window.tokens[id] = None;
                }
            }
            QAction::Register => {
                if !editor.open || !editor.dirty || editor.token || editor.deletion.is_some() {
                    return;
                }
                let needed =
                    journal_core::registration_required(true, editor.known_entry, self.trusted());
                match journal_core::body_write_decision(
                    journal_core::ownership(self.window_body_facts(id)),
                    needed,
                ) {
                    BodyWriteDecision::Hold => {}
                    BodyWriteDecision::Write | BodyWriteDecision::PreserveThenWrite => {
                        let token = RegisteredDraft::without_registration(
                            &Self::draft_id(id),
                            true,
                            editor.known_entry,
                            self.trusted(),
                        )
                        .expect("the core said no registration is needed");
                        self.window.tokens[id] = Some(token);
                        self.window.editors[id].token = true;
                    }
                    BodyWriteDecision::RegisterFirst => {
                        self.count("register_draft_entries");
                        match draft_service::register_draft_entries(
                            self.data(),
                            &self.session,
                            self.window.authority,
                            &[Self::entry(id, 0)],
                        ) {
                            Ok(mut registration) => {
                                let mut tokens = registration.take_registered();
                                self.window.authority = match registration {
                                    DraftRegistration::Committed { commit, .. } => commit.authority,
                                    DraftRegistration::Additive { authority, .. } => authority,
                                };
                                self.window.tokens[id] = tokens.pop();
                                let e = &mut self.window.editors[id];
                                e.known_entry = true;
                                e.token = journal_core::may_write_after_registration(true, true);
                            }
                            Err(error) => {
                                // Out of scope (fault-free); a divergence will show.
                                eprintln!("register_draft_entries failed: {error}");
                                self.window.authority = error.authority();
                            }
                        }
                    }
                }
            }
            QAction::WriteBody => {
                if !editor.open || !editor.dirty || !editor.token {
                    return;
                }
                let needed =
                    journal_core::registration_required(true, editor.known_entry, self.trusted());
                if journal_core::body_write_decision(
                    journal_core::ownership(self.window_body_facts(id)),
                    needed,
                ) == BodyWriteDecision::Hold
                {
                    return;
                }
                self.window.editors[id].token = false;
                let token = self.window.tokens[id]
                    .take()
                    .expect("a token accompanies editor.token");
                self.count("write_draft");
                draft_service::write_draft(
                    self.data(),
                    &token,
                    &body_text(i64::from(editor.content)),
                )
                .expect("write_draft");
                self.window.editors[id].written = Some(editor.content);
            }
            QAction::Commit => {
                let Some(written) = editor.written else {
                    return;
                };
                self.window.editors[id].written = None;
                self.count("update_manifest");
                match draft_service::update_manifest(
                    self.data(),
                    &self.session,
                    self.window.authority,
                    |manifest| manifest.upsert(Self::entry(id, i64::from(written))),
                ) {
                    Ok(commit) => {
                        self.window.authority = commit.authority;
                        if self.body(id) == Some(i64::from(written)) && editor.content == written
                        {
                            self.window.editors[id].dirty = false;
                        }
                    }
                    Err(error) => self.window.authority = error.authority(),
                }
            }
            QAction::Discard => {
                if !editor.open || editor.deletion.is_some() {
                    return;
                }
                let start =
                    journal_core::deletion_start(journal_core::ownership(self.window_body_facts(id)));
                let e = &mut self.window.editors[id];
                e.dirty = false;
                e.token = false;
                e.written = None;
                e.content = 0;
                e.known_entry = false;
                e.preserve_queued = start == DeletionStep::Preserve;
                e.deletion = Some(start);
                self.window.tokens[id] = None;
            }
            QAction::RestoreApply(chosen) => self.restore_apply(id, chosen),
            QAction::DeletionStepAction => self.deletion_step(id),
            QAction::Crash => {
                self.window = Window::closed();
            }
            QAction::Startup => self.startup(),
            other => panic!("{other:?} is outside the k3_service scope"),
        }
    }

    /// `RestoreApply`: `Stale` is unreachable (the backing never moves), and
    /// the window decides the ending from the body on disk.
    fn restore_apply(&mut self, id: usize, chosen: QEnding) {
        let editor = self.window.editors[id];
        if !editor.restore_pending {
            return;
        }
        let body = self.body(id);
        let ending = if body.is_none() {
            RestoreEnding::MissingBody
        } else {
            chosen.into()
        };
        self.window.editors[id].restore_pending = false;
        match journal_core::unapplied_restore_disposition(ending) {
            RestoreDisposition::Nothing => {
                if ending == RestoreEnding::Applied
                    && let Some(content) = body
                {
                    let e = &mut self.window.editors[id];
                    e.content = u8::try_from(content).expect("small content ids");
                    e.dirty = false;
                }
            }
            RestoreDisposition::PreserveCopy => {
                // Production retries a failed copy with the hold kept; this
                // scope is fault-free, so a failure is a divergence to show.
                assert!(self.preserve(id), "preserve_stale_draft_body failed");
            }
            RestoreDisposition::PreserveThenRetire => {
                panic!("stale restores are outside the k3_service scope")
            }
        }
    }

    /// Both production preserve paths (an unapplied restore, and a
    /// deletion's `Preserve` step) call `preserve_stale_draft_body` with the
    /// manifest's entry for the id. `E1_CONTENT_STAMP=1` instead names the
    /// copy by the body's content (a hypothetical content-keyed set-aside
    /// name), to explore past the finding in `tests/e1_findings.rs`.
    fn preserve(&mut self, id: usize) -> bool {
        let Some(entry) = self.manifest().find_by_id(&Self::draft_id(id)).cloned() else {
            // No entry: nothing names the body, so there is nothing to keep.
            return self.body(id).is_none();
        };
        if std::env::var_os("E1_CONTENT_STAMP").is_some() {
            let Some(content) = self.body(id) else {
                return true;
            };
            self.count("keep_copy(content stamp)");
            let stamp = u64::try_from(content).expect("content ids are non-negative");
            return set_aside::keep_copy(self.data(), &Self::draft_id(id), stamp).is_ok();
        }
        self.count("preserve_stale_draft_body");
        draft_service::preserve_stale_draft_body(self.data(), &entry).is_ok()
    }

    fn deletion_step(&mut self, id: usize) {
        let Some(step) = self.window.editors[id].deletion else {
            return;
        };
        let succeeded = match step {
            DeletionStep::Preserve => {
                // Production preserves under the entry's `saved_at_secs`.
                let ok = self.preserve(id);
                if ok {
                    self.window.editors[id].preserve_queued = false;
                }
                ok
            }
            DeletionStep::DeleteBody => {
                self.count("delete_draft_file");
                draft_service::delete_draft_file(self.data(), &Self::draft_id(id)).is_ok()
            }
            DeletionStep::RemoveEntry => {
                self.count("remove_manifest_entry");
                match draft_service::remove_manifest_entry(
                    self.data(),
                    &self.session,
                    self.window.authority,
                    &Self::draft_id(id),
                ) {
                    Ok(commit) => {
                        self.window.authority = commit.authority;
                    }
                    Err(error) => self.window.authority = error.authority(),
                }
                // The model's step succeeds either way (fault = false).
                true
            }
            DeletionStep::Done | DeletionStep::Stopped => true,
        };
        let next = journal_core::next_deletion_step(step, succeeded);
        self.window.editors[id].deletion = match next {
            DeletionStep::Done => None,
            DeletionStep::Stopped => Some(if self.window.editors[id].preserve_queued {
                DeletionStep::Preserve
            } else {
                DeletionStep::DeleteBody
            }),
            other => Some(other),
        };
    }
}

impl Driver for ServiceDriver {
    type State = DiskView;

    fn step(&mut self, step: &Step) -> Result {
        switch!(step {
            initService(entryPresent: BTreeMap<i64, bool>, bodyPresent: BTreeMap<i64, bool>) =>
                self.init(&entryPresent, &bodyPresent),
            stepService(a: QAction, id: i64) => ServiceDriver::step(self, a, id),
            workStep(a: QAction, id: i64) => ServiceDriver::step(self, a, id),
            lifecycleStep(a: QAction, id: i64) => ServiceDriver::step(self, a, id),
        })
    }
}

impl State<ServiceDriver> for DiskView {
    fn from_driver(driver: &ServiceDriver) -> Result<Self> {
        Ok(driver.view())
    }

    fn from_spec(value: itf::Value) -> Result<Self> {
        let itf::Value::Record(record) = value else {
            anyhow::bail!("expected the state to be a record");
        };
        let (_, j) = record
            .iter()
            .find(|(k, _)| k.ends_with("journal::j"))
            .ok_or_else(|| anyhow::anyhow!("no `*journal::j` variable in the trace"))?;
        let mut view = DiskView::from(&QJournal::deserialize(j.clone())?);
        // Quint keeps `trusted` after a crash only as a stale window bit; the
        // harness clears it on `Crash`, so this is the same value either way.
        view.trusted = view.running && view.trusted;
        Ok(view)
    }
}

/// The uniform walk: kept to show how little of the protocol it reaches.
#[quint_run(
    spec = "../quint/journal.qnt",
    main = "k3_service",
    init = "initService",
    step = "stepService",
    max_samples = 500,
    max_steps = 16
)]
fn k3_service_uniform() -> impl Driver {
    ServiceDriver::default()
}

/// Effective, work-weighted steps (see `stepServiceWeighted`).
///
/// Expected to diverge: this pins the E1 finding (`tests/e1_findings.rs`),
/// which trace 22 of seed 12 reaches. Run with `E1_CONTENT_STAMP=1` (and
/// without this attribute's `should_panic`) to explore past it; the other
/// seeds tried (11 to 18) all reach the same divergence within 25 traces.
#[should_panic(expected = "State invariant failed")]
#[quint_run(
    spec = "../quint/journal.qnt",
    main = "k3_service",
    init = "initService",
    step = "stepServiceWeighted",
    max_samples = 1000,
    max_steps = 16,
    seed = "12"
)]
fn k3_service_weighted() -> impl Driver {
    ServiceDriver::default()
}
