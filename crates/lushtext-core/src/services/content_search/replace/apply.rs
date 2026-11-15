// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace All apply orchestration: the shell of [`ReplaceFileProtocol`].
//!
//! Every file runs the protocol's actions in order, one call each (design D3):
//! guard and facts (P0), body admission (P1), pass A into the undo body (P2),
//! output admission (P3), the entry and the armed manifest (P4), pass B into
//! the target (P5), then release (P6). Pass A streams the target into its body
//! while dry-running the edits into a digest; pass B streams the **body**
//! through the edits into the target, so the rewritten bytes are exactly the
//! bytes the undo body holds. Files are processed in `main`'s two passes in
//! path order (design D4).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::admission::{MainAdmissionLedger, MainCharge, ReplacePass, disk_admits};
use super::protocol::{FileAction, FileEnd, FileOutcome, ReplaceFileProtocol, RunJournalState};
use super::stream::{
    REPLACE_STREAM_CHUNK_BYTES, ReplaceStream, ReplacementTextOutcome, Sha256Sink, StreamEdit,
    StreamStop, plan_edits,
};
use super::undo::rollback_applied_files;
use super::{
    ApplyReplacementsOutcome, ContentDigest, MAIN_REPLACE_FILE_BYTES, MAX_REPLACE_FILE_BYTES,
    ReplaceConstructionMetrics, ReplaceJournalFreshness, ReplaceProgress, ReplaceUndoBackup,
    ReplaceUndoEntry, UndoOriginal, effective_main_replace_undo_bytes,
    effective_max_replace_file_bytes, replace_undo_retained_byte_weight,
    run_after_pass_a_hook_for_test, run_before_target_write_hook_for_test,
    take_replace_before_rename_failure_for_test,
};
use crate::model::content_search::{
    BoundedDiagnosticSample, MAX_REPLACE_PREVIEW_ROWS, ReplacePreviewIneligibleFile, ReplaceResult,
    ReplaceSkipReason, ReplaceSkipReport, Replacement,
};
use crate::model::size_units::iec_size;
use crate::services::filesystem::leftovers::LaunchId;
use crate::services::filesystem::read::{BoundedStreamError, stream_bounded};
use crate::services::filesystem::types::FileFacts;
use crate::services::filesystem::{WriteLabel, metadata as fs_metadata, write as fs_write};
use crate::services::kill_point::{self, KillWindow};
use crate::services::search_backup::{self, StreamEntryRecord, TargetIdentity};

/// Everything one Replace All run needs, borrowed from its caller.
#[derive(Clone, Copy)]
pub struct ReplaceRunRequest<'a> {
    /// Checked preview rows of the run.
    pub replacements: &'a [Replacement],
    /// Files open with unsaved changes or an active save: never written.
    pub skip_paths: &'a HashSet<PathBuf>,
    /// Canonical identities of open tabs, for the affected-open-paths result.
    pub open_canonical_identities: &'a HashSet<PathBuf>,
    /// Files whose matches were not eligible for the preview, by file.
    pub ineligible: &'a [ReplacePreviewIneligibleFile],
    /// Set to cancel the run; observed at the next chunk boundary.
    pub cancel: &'a AtomicBool,
    /// Progress the GTK side polls, when it does.
    pub progress: Option<&'a ReplaceProgress>,
    /// The app data directory holding the undo journal.
    pub journal_data_dir: &'a Path,
}

/// Apply replacements to files on disk with an undo journal under
/// `journal_data_dir`.
///
/// Groups replacements by file, streams each file twice (into its undo body,
/// then from that body into the target), and returns the counts, the skip
/// report and the undo descriptors (no file bytes).
///
/// # Errors
///
/// Returns an error if every candidate file fails, or if the run is cancelled
/// and its rollback restored every file (then nothing remains to undo).
pub fn apply_replacements(
    replacements: &[Replacement],
    skip_paths: &HashSet<PathBuf>,
    cancel: &AtomicBool,
    journal_data_dir: &Path,
) -> anyhow::Result<ApplyReplacementsOutcome> {
    apply_replacements_with(ReplaceRunRequest {
        replacements,
        skip_paths,
        open_canonical_identities: &HashSet::new(),
        ineligible: &[],
        cancel,
        progress: None,
        journal_data_dir,
    })
}

/// [`apply_replacements`] with every input of a run.
///
/// # Errors
///
/// As [`apply_replacements`].
pub fn apply_replacements_with(
    request: ReplaceRunRequest<'_>,
) -> anyhow::Result<ApplyReplacementsOutcome> {
    apply_replacements_inner(request, None).and_then(|outcome| {
        outcome.ok_or_else(|| anyhow::anyhow!("unguarded Replace All became stale"))
    })
}

/// Apply only if the UI reservation is still current after acquiring the journal lock.
#[cfg(test)]
pub(crate) fn apply_replacements_if_current(
    replacements: &[Replacement],
    skip_paths: &HashSet<PathBuf>,
    open_canonical_identities: &HashSet<PathBuf>,
    cancel: &AtomicBool,
    journal_data_dir: &Path,
    freshness: &ReplaceJournalFreshness,
) -> anyhow::Result<Option<ApplyReplacementsOutcome>> {
    apply_request_if_current(
        ReplaceRunRequest {
            replacements,
            skip_paths,
            open_canonical_identities,
            ineligible: &[],
            cancel,
            progress: None,
            journal_data_dir,
        },
        freshness,
    )
}

/// Apply a run only if the UI reservation is still current after acquiring
/// the journal lock.
pub(crate) fn apply_request_if_current(
    request: ReplaceRunRequest<'_>,
    freshness: &ReplaceJournalFreshness,
) -> anyhow::Result<Option<ApplyReplacementsOutcome>> {
    apply_replacements_inner(request, Some(freshness))
}

/// How one file ended, with the reason the report carries when it was not
/// replaced.
#[expect(
    clippy::large_enum_variant,
    reason = "one value per file, returned once from its per-file step and consumed at once by the run loop; boxing would add an allocation per file and retain nothing less"
)]
enum FileResult {
    Replaced {
        count: usize,
        durable: bool,
        descriptor: ReplaceUndoEntry,
        facts: FileFacts,
        body_len: u64,
    },
    Skipped {
        reason: ReplaceSkipReason,
        line: Option<u64>,
    },
    Deferred,
    Unchanged,
    Cancelled,
}

/// Per-run state shared by every file.
struct Run<'a> {
    request: ReplaceRunRequest<'a>,
    file_limit: u64,
    ledger: MainAdmissionLedger,
    journal: RunJournalState,
    launch: LaunchId,
    metrics: ReplaceConstructionMetrics,
    errors: BoundedDiagnosticSample,
    live_body_bytes: u64,
}

#[expect(
    clippy::too_many_lines,
    reason = "the run loop narrates the two passes, the report folding and the terminal in one place"
)]
pub(super) fn apply_replacements_inner(
    request: ReplaceRunRequest<'_>,
    freshness: Option<&ReplaceJournalFreshness>,
) -> anyhow::Result<Option<ApplyReplacementsOutcome>> {
    if request.replacements.len() > MAX_REPLACE_PREVIEW_ROWS {
        anyhow::bail!(
            "Replace All selection exceeds the {MAX_REPLACE_PREVIEW_ROWS}-replacement limit"
        );
    }
    let Some(_journal_guard) = search_backup::acquire_journal_guard_cancellable(request.cancel)?
    else {
        // Cancelled while startup recovery or a journal disk job held the
        // guard: nothing was touched (design D3a).
        anyhow::bail!("Replace cancelled");
    };
    // The journal lock serializes this decision with startup recovery and all
    // journal commits. A stale UI generation must exit before preparing a new
    // journal or mutating any target file.
    if freshness.is_some_and(|freshness| !freshness.is_current()) {
        return Ok(None);
    }

    let mut by_file: BTreeMap<PathBuf, Vec<&Replacement>> = BTreeMap::new();
    for replacement in request.replacements {
        by_file
            .entry(replacement.path.clone())
            .or_default()
            .push(replacement);
    }
    let ineligible: HashMap<&Path, &ReplacePreviewIneligibleFile> = request
        .ineligible
        .iter()
        .map(|file| (file.path.as_path(), file))
        .collect();
    if let Some(progress) = request.progress {
        progress.begin(&by_file);
    }
    let candidate_paths: HashSet<PathBuf> = by_file.keys().cloned().collect();

    let mut run = Run {
        request,
        file_limit: effective_max_replace_file_bytes(),
        ledger: MainAdmissionLedger::new(effective_main_replace_undo_bytes()),
        journal: RunJournalState {
            prepared: false,
            armed: false,
        },
        launch: LaunchId::current(),
        metrics: ReplaceConstructionMetrics::default(),
        errors: BoundedDiagnosticSample::default(),
        live_body_bytes: 0,
    };
    let mut backup = ReplaceUndoBackup::new();
    let mut report = ReplaceSkipReport::default();
    let mut skipped_sample = BoundedDiagnosticSample::default();
    let mut replaced_count = 0usize;
    let mut files_affected = 0usize;
    let mut affected_open_paths = Vec::new();
    let mut cancelled = false;

    // Two passes in path order: the files `main` replaced first (source and
    // output within its 10 MiB), then every deferred file up to the open tier.
    let mut pending: Vec<_> = by_file.into_iter().collect();
    let mut deferred = Vec::new();
    'passes: for pass in [ReplacePass::First, ReplacePass::Second] {
        for (path, mut file_replacements) in std::mem::take(&mut pending) {
            if run.request.cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break 'passes;
            }
            let first_line = file_replacements.iter().map(|r| r.line_number).min();
            if run.request.skip_paths.contains(&path) {
                skipped_sample.record_path(&path);
                report.record_skipped(&path, ReplaceSkipReason::UnsavedChanges, first_line);
                finish_progress(&run, &path);
                continue;
            }
            match process_file(&mut run, pass, &path, &mut file_replacements) {
                FileResult::Deferred => deferred.push((path, file_replacements)),
                FileResult::Cancelled => {
                    cancelled = true;
                    break 'passes;
                }
                FileResult::Unchanged => finish_progress(&run, &path),
                FileResult::Skipped { reason, line } => {
                    skipped_sample.record_path(&path);
                    report.record_skipped(&path, reason, line.or(first_line));
                    finish_progress(&run, &path);
                }
                FileResult::Replaced {
                    count,
                    durable,
                    descriptor,
                    facts,
                    body_len,
                } => {
                    if !durable {
                        run.errors.push(format!(
                            "Replaced {}, but durability sync failed",
                            path.display()
                        ));
                    }
                    let identity = facts.canonical_path.as_deref().unwrap_or(&path);
                    if run.request.open_canonical_identities.contains(identity) {
                        affected_open_paths.push(path.clone());
                    }
                    replaced_count += count;
                    files_affected += 1;
                    run.live_body_bytes = run.live_body_bytes.saturating_add(body_len);
                    run.metrics.undo_bytes = run.metrics.undo_bytes.max(run.live_body_bytes);
                    if let Some(file) = ineligible.get(path.as_path()) {
                        report.record_partly_replaced(
                            &path,
                            file.reason,
                            file.first_line,
                            file.matches,
                        );
                    }
                    backup.insert(path.clone(), descriptor);
                    finish_progress(&run, &path);
                    kill_point::reach(
                        KillWindow::ReplaceTargetRenamedBeforeFinish,
                        WriteLabel::REPLACE.as_str(),
                    );
                }
            }
        }
        pending = std::mem::take(&mut deferred);
    }

    if cancelled {
        if let Some(progress) = run.request.progress {
            progress.set_phase(super::ReplacePhase::RollingBack);
        }
        let rollback = rollback_applied_files(&backup);
        if rollback.remaining.is_empty() {
            search_backup::delete(run.request.journal_data_dir).map_err(|e| {
                anyhow::anyhow!("Replace cancelled; undo backup cleanup failed: {e}")
            })?;
            anyhow::bail!("Replace cancelled");
        }
        // Rollback could not restore every file: the journal keeps exactly
        // those, stays active, and offers Undo for them (design D8).
        let result = ReplaceResult {
            replaced_count: 0,
            files_affected: rollback.remaining.len(),
            skipped_count: skipped_sample.total_count(),
            error_count: rollback.errors.total_count(),
            skipped_sample,
            error_sample: rollback.errors,
            affected_open_paths: Vec::new(),
        };
        run.metrics.undo_descriptor_bytes = replace_undo_retained_byte_weight(&rollback.remaining);
        return Ok(Some(ApplyReplacementsOutcome {
            result,
            undo_backup: rollback.remaining,
            metrics: run.metrics,
            skip_report: ReplaceSkipReport::default(),
            cancelled: true,
        }));
    }

    // Files whose matches were all ineligible never reached the worker.
    for file in run.request.ineligible {
        if candidate_paths.contains(&file.path) {
            continue;
        }
        let reason = if run.request.skip_paths.contains(&file.path) {
            ReplaceSkipReason::UnsavedChanges
        } else {
            file.reason
        };
        report.record_skipped(&file.path, reason, Some(file.first_line));
    }
    report.finish();

    // A run that replaced nothing still retires the superseded journal: the
    // UI already retired its Undo when this run started (design D3a).
    if backup.is_empty()
        && let Err(e) = search_backup::delete(run.request.journal_data_dir)
    {
        run.errors
            .push(format!("Failed to clean empty undo journal: {e}"));
    }

    if files_affected == 0
        && skipped_sample.total_count() == 0
        && report.is_empty()
        && run.errors.total_count() > 0
    {
        return Err(anyhow::anyhow!(run.errors.summary("Replace All failed")));
    }

    run.metrics.undo_live_bytes = run.live_body_bytes;
    run.metrics.undo_descriptor_bytes = replace_undo_retained_byte_weight(&backup);
    let result = ReplaceResult {
        replaced_count,
        files_affected,
        skipped_count: skipped_sample.total_count(),
        error_count: run.errors.total_count(),
        skipped_sample,
        error_sample: run.errors,
        affected_open_paths,
    };
    Ok(Some(ApplyReplacementsOutcome {
        result,
        undo_backup: backup,
        metrics: run.metrics,
        skip_report: report,
        cancelled: false,
    }))
}

fn finish_progress(run: &Run<'_>, path: &Path) {
    if let Some(progress) = run.request.progress {
        progress.finish_file(path);
    }
}

/// The facts pass A and pass B must find unchanged at their end (design D5).
fn facts_unchanged(before: &FileFacts, path: &Path) -> bool {
    fs_metadata::file_stat(path).is_ok_and(|now| {
        now.byte_size == before.byte_size
            && now.modified_at_nanos == before.modified_at_nanos
            && now.identity == before.identity
    })
}

fn target_identity(facts: &FileFacts) -> TargetIdentity {
    TargetIdentity {
        dev: facts.identity.map_or(0, |identity| identity.device),
        ino: facts.identity.map_or(0, |identity| identity.inode),
        size: facts.byte_size,
        mtime_ns: facts
            .modified_at_nanos
            .map_or(0, |nanos| u64::try_from(nanos).unwrap_or(u64::MAX)),
    }
}

/// What a producer saw that made it fail; read after the durable write returns.
#[derive(Default)]
struct ProducerVerdict {
    stop: Option<ReplaceSkipReason>,
    line: Option<u64>,
    defer: bool,
    cancelled: bool,
    unchanged: bool,
}

impl ProducerVerdict {
    fn fail(&mut self, reason: ReplaceSkipReason) -> std::io::Error {
        self.stop = Some(reason);
        std::io::Error::other(format!("{reason:?}"))
    }
}

/// Execute one file's protocol.
#[expect(
    clippy::too_many_lines,
    reason = "one arm per protocol action keeps the shell a direct reading of the protocol"
)]
fn process_file(
    run: &mut Run<'_>,
    pass: ReplacePass,
    path: &Path,
    file_replacements: &mut [&Replacement],
) -> FileResult {
    let cancel = run.request.cancel;
    let edits: Vec<StreamEdit<'_>> =
        match plan_edits(file_replacements, || cancel.load(Ordering::Relaxed)) {
            Ok(edits) => edits,
            Err(ReplacementTextOutcome::Unchanged) => return FileResult::Unchanged,
            Err(ReplacementTextOutcome::Cancelled) => return FileResult::Cancelled,
            Err(_) => {
                run.errors.push(format!(
                    "Skipped {}: invalid replacement preview",
                    path.display()
                ));
                return FileResult::Skipped {
                    reason: ReplaceSkipReason::InvalidPreview,
                    line: None,
                };
            }
        };
    run.metrics.retained_edit_records = run.metrics.retained_edit_records.max(edits.len());
    run.metrics.retained_edit_bytes = run.metrics.retained_edit_bytes.max(
        edits
            .capacity()
            .saturating_mul(std::mem::size_of::<StreamEdit<'static>>()),
    );

    let data_dir = run.request.journal_data_dir;
    let body_path = search_backup::body_path(data_dir, path);
    let (mut protocol, mut action) = ReplaceFileProtocol::start(run.journal);
    let mut guard = None;
    let mut facts: Option<FileFacts> = None;
    let mut source_len = 0u64;
    let mut original: Option<ContentDigest> = None;
    let mut replaced: Option<ContentDigest> = None;
    let mut charge: Option<MainCharge> = None;
    let mut provisional = false;
    let mut reason = ReplaceSkipReason::WriteFailed;
    let mut line = None;
    let mut count = 0usize;

    let end = loop {
        let outcome = match action {
            FileAction::Finish(end) => break end,
            FileAction::AcquireTarget => match fs_write::TargetWriteGuard::acquire(path) {
                Err(e) => {
                    run.errors
                        .push(format!("Failed to lock {}: {e}", path.display()));
                    reason = ReplaceSkipReason::Unreadable;
                    FileOutcome::Failed
                }
                Ok(acquired) => {
                    guard = Some(acquired);
                    if cancel.load(Ordering::Relaxed) {
                        FileOutcome::Cancelled
                    } else {
                        match fs_metadata::file_facts(path) {
                            Err(e) => {
                                run.errors
                                    .push(format!("Failed to stat {}: {e}", path.display()));
                                reason = ReplaceSkipReason::Unreadable;
                                FileOutcome::Failed
                            }
                            Ok(found) if found.byte_size > run.file_limit => {
                                run.errors.push(format!(
                                    "Skipped {}: file is larger than the {} Replace All limit",
                                    path.display(),
                                    iec_size(run.file_limit),
                                ));
                                reason = ReplaceSkipReason::LargerThanOpenTier;
                                FileOutcome::Failed
                            }
                            Ok(found)
                                if pass == ReplacePass::First
                                    && found.byte_size > MAIN_REPLACE_FILE_BYTES =>
                            {
                                FileOutcome::Defer
                            }
                            Ok(found) => {
                                source_len = found.byte_size;
                                facts = Some(found);
                                FileOutcome::Done
                            }
                        }
                    }
                }
            },
            FileAction::AdmitBody => {
                provisional = run.ledger.provisionally_waived(pass, source_len);
                let free = fs_metadata::free_space(data_dir).ok();
                if disk_admits(free, source_len, !provisional) {
                    FileOutcome::Done
                } else {
                    reason = ReplaceSkipReason::NotEnoughDiskSpace;
                    FileOutcome::Failed
                }
            }
            FileAction::PrepareJournal => {
                match search_backup::begin_incremental_journal(data_dir) {
                    Ok(()) => FileOutcome::Done,
                    Err(e) => {
                        run.errors.push(format!(
                            "Failed to prepare undo journal before replacing {}: {e}",
                            path.display()
                        ));
                        reason = ReplaceSkipReason::UndoJournalFailed;
                        FileOutcome::Failed
                    }
                }
            }
            FileAction::WriteBody => {
                // The first pass replays `main`'s per-file bound, which never
                // exceeds the per-file limit in force.
                let output_limit = match pass {
                    ReplacePass::First => MAIN_REPLACE_FILE_BYTES.min(run.file_limit),
                    ReplacePass::Second => run.file_limit,
                };
                let file_limit = run.file_limit;
                let mut verdict = ProducerVerdict::default();
                let mut summary = None;
                let pass_a = fs_write::atomic_replace_stream(
                    &body_path,
                    WriteLabel::REPLACE_UNDO_BODY,
                    |body| {
                        let mut stream = ReplaceStream::new(
                            &edits,
                            output_limit,
                            std::io::sink(),
                            Sha256Sink::default(),
                            Sha256Sink::default(),
                        );
                        let streamed = stream_bounded(
                            path,
                            file_limit,
                            REPLACE_STREAM_CHUNK_BYTES,
                            || cancel.load(Ordering::Relaxed),
                            |chunk| {
                                body.write_all(chunk).map_err(StreamStop::Io)?;
                                stream.feed(chunk)
                            },
                        );
                        let stopped = match streamed {
                            Ok(_) => None,
                            Err(BoundedStreamError::Cancelled) => {
                                verdict.cancelled = true;
                                return Err(std::io::Error::other("cancelled"));
                            }
                            Err(BoundedStreamError::LimitExceeded { .. }) => {
                                return Err(verdict.fail(ReplaceSkipReason::LargerThanOpenTier));
                            }
                            Err(BoundedStreamError::Io(e)) => {
                                verdict.stop = Some(ReplaceSkipReason::Unreadable);
                                return Err(e);
                            }
                            Err(BoundedStreamError::Consumer(StreamStop::Io(e))) => return Err(e),
                            Err(BoundedStreamError::Consumer(StreamStop::Outcome(o))) => Some(o),
                        };
                        let (done, _) = stream.finish().map_err(|stop| match stop {
                            StreamStop::Io(e) => e,
                            StreamStop::Outcome(_) => std::io::Error::other("stream stopped"),
                        })?;
                        match stopped.unwrap_or(done.outcome) {
                            ReplacementTextOutcome::Replaced { .. } => {}
                            ReplacementTextOutcome::OutputLimitExceeded
                                if pass == ReplacePass::First =>
                            {
                                verdict.defer = true;
                                return Err(std::io::Error::other("deferred"));
                            }
                            ReplacementTextOutcome::OutputLimitExceeded => {
                                return Err(verdict.fail(ReplaceSkipReason::LargerThanOpenTier));
                            }
                            ReplacementTextOutcome::NotUtf8 => {
                                return Err(verdict.fail(ReplaceSkipReason::NotUtf8));
                            }
                            ReplacementTextOutcome::StaleLine { line_number } => {
                                verdict.line = Some(line_number);
                                return Err(verdict.fail(ReplaceSkipReason::ChangedSinceSearch));
                            }
                            ReplacementTextOutcome::Unchanged => {
                                verdict.unchanged = true;
                                return Err(std::io::Error::other("unchanged"));
                            }
                            ReplacementTextOutcome::InvalidPreview { .. } => {
                                return Err(verdict.fail(ReplaceSkipReason::InvalidPreview));
                            }
                            ReplacementTextOutcome::Cancelled => {
                                verdict.cancelled = true;
                                return Err(std::io::Error::other("cancelled"));
                            }
                        }
                        let facts = facts.as_ref().expect("facts precede pass A");
                        if done.source_len != facts.byte_size || !facts_unchanged(facts, path) {
                            return Err(verdict.fail(ReplaceSkipReason::ChangedDuringReplace));
                        }
                        summary = Some(done);
                        Ok(())
                    },
                );
                match pass_a {
                    Ok(()) => {
                        let done = summary.expect("a successful pass A has a summary");
                        run.metrics.absorb_construction(done.metrics);
                        if let ReplacementTextOutcome::Replaced { replacement_count } = done.outcome
                        {
                            count = replacement_count;
                        }
                        source_len = done.source_len;
                        original = Some(ContentDigest {
                            len: done.source_len,
                            sha256: done.source_digest.finish(),
                        });
                        replaced = Some(ContentDigest {
                            len: done.output_len,
                            sha256: done.output_digest.finish(),
                        });
                        kill_point::reach(
                            KillWindow::ReplaceBodyBeforeEntry,
                            WriteLabel::REPLACE_UNDO_BODY.as_str(),
                        );
                        run_after_pass_a_hook_for_test(path);
                        FileOutcome::Done
                    }
                    Err(fs_write::DurableWriteError::AfterRename(e)) => {
                        run.errors.push(format!(
                            "Undo body for {} may not be durable: {e}",
                            path.display()
                        ));
                        reason = ReplaceSkipReason::UndoJournalFailed;
                        FileOutcome::BodyUnconfirmed
                    }
                    Err(fs_write::DurableWriteError::BeforeRename(e)) => {
                        if verdict.cancelled
                            || (cancel.load(Ordering::Relaxed)
                                && verdict.stop.is_none()
                                && !verdict.defer)
                        {
                            FileOutcome::Cancelled
                        } else if verdict.unchanged {
                            FileOutcome::NoChange
                        } else if verdict.defer {
                            FileOutcome::Defer
                        } else {
                            reason = verdict.stop.unwrap_or(ReplaceSkipReason::UndoJournalFailed);
                            line = verdict.line;
                            run.errors.push(format!(
                                "Skipped {}: {}",
                                path.display(),
                                skip_detail(reason, &e)
                            ));
                            FileOutcome::Failed
                        }
                    }
                }
            }
            FileAction::AdmitOutput => {
                let output_len = replaced.map_or(0, |digest| digest.len);
                charge = if provisional {
                    run.ledger.charge(pass, path, source_len, output_len)
                } else {
                    None
                };
                let target_dir = fs_write::parent_or_current(path);
                let target_free = fs_metadata::free_space(target_dir).ok();
                let admitted = if charge.is_some() {
                    disk_admits(target_free, output_len, false)
                } else {
                    // Not waived after all: both filesystems keep the reserve.
                    let journal_free = fs_metadata::free_space(data_dir).ok();
                    disk_admits(target_free, output_len, true) && disk_admits(journal_free, 0, true)
                };
                if admitted {
                    FileOutcome::Done
                } else {
                    if let Some(taken) = charge.take() {
                        run.ledger.reclaim(taken);
                    }
                    reason = ReplaceSkipReason::NotEnoughDiskSpace;
                    FileOutcome::Failed
                }
            }
            FileAction::WriteEntry => {
                let record = StreamEntryRecord {
                    original: original.expect("pass A recorded the original"),
                    replaced: replaced.expect("pass A recorded the output"),
                    identity: target_identity(facts.as_ref().expect("facts precede the entry")),
                };
                match search_backup::save_stream_entry(data_dir, path, &record) {
                    Ok(()) => FileOutcome::Done,
                    Err(e) => {
                        run.errors.push(format!(
                            "Failed to persist undo journal before replacing {}: {e}",
                            path.display()
                        ));
                        reason = ReplaceSkipReason::UndoJournalFailed;
                        FileOutcome::Failed
                    }
                }
            }
            FileAction::ArmJournal => {
                match search_backup::mark_incremental_journal_active(data_dir, run.launch) {
                    Ok(()) => FileOutcome::Done,
                    Err(e) => {
                        run.errors.push(format!(
                            "Failed to activate undo journal before replacing {}: {e}",
                            path.display()
                        ));
                        reason = ReplaceSkipReason::UndoJournalFailed;
                        FileOutcome::Failed
                    }
                }
            }
            FileAction::WriteTarget => {
                run_before_target_write_hook_for_test(path);
                let digest_in = original.expect("pass A recorded the original");
                let digest_out = replaced.expect("pass A recorded the output");
                let facts_before = facts.as_ref().expect("facts precede pass B");
                let mut verdict = ProducerVerdict::default();
                let result = write_target_from_body(
                    path,
                    &body_path,
                    &edits,
                    digest_in,
                    digest_out,
                    facts_before,
                    cancel,
                    run.request.progress,
                    &mut verdict,
                );
                match result {
                    Ok(()) => FileOutcome::Done,
                    Err(fs_write::DurableWriteError::AfterRename(e)) => {
                        run.errors.push(format!(
                            "Failed to sync parent directory for {}: {e}",
                            path.display()
                        ));
                        FileOutcome::RenamedUnconfirmed
                    }
                    Err(fs_write::DurableWriteError::BeforeRename(e)) => {
                        if verdict.cancelled {
                            FileOutcome::Cancelled
                        } else {
                            reason = verdict.stop.unwrap_or(ReplaceSkipReason::WriteFailed);
                            run.errors
                                .push(format!("Failed to write {}: {e}", path.display()));
                            FileOutcome::Failed
                        }
                    }
                }
            }
            FileAction::RemoveEntry => match search_backup::delete_entry(data_dir, path) {
                Ok(()) => FileOutcome::Done,
                Err(e) => {
                    run.errors.push(format!(
                        "Failed to remove undo journal entry for {}: {e}",
                        path.display()
                    ));
                    FileOutcome::Failed
                }
            },
            FileAction::RemoveBody => match search_backup::delete_body(data_dir, path) {
                Ok(()) => FileOutcome::Done,
                Err(e) => {
                    run.errors.push(format!(
                        "Failed to remove undo body for {}: {e}",
                        path.display()
                    ));
                    FileOutcome::Failed
                }
            },
        };
        action = protocol.step(outcome);
    };
    run.journal = protocol.journal();
    drop(guard);

    match end {
        FileEnd::Replaced { durable } => {
            let facts = facts.expect("a replaced file has facts");
            FileResult::Replaced {
                count,
                durable,
                descriptor: ReplaceUndoEntry {
                    entry_file: search_backup::entry_path(data_dir, path),
                    original_source: UndoOriginal::Body(body_path),
                    original: original.expect("a replaced file has its original digest"),
                    replaced: replaced.expect("a replaced file has its replaced digest"),
                    launch: run.launch,
                },
                facts,
                body_len: source_len,
            }
        }
        FileEnd::NotReplaced { .. } => {
            if let Some(taken) = charge {
                // A pre-rename failure reclaims `main`'s charge, as `main` did.
                run.ledger.reclaim(taken);
            }
            FileResult::Skipped { reason, line }
        }
        FileEnd::Deferred => FileResult::Deferred,
        FileEnd::Unchanged => FileResult::Unchanged,
        FileEnd::Cancelled => {
            if let Some(taken) = charge {
                run.ledger.reclaim(taken);
            }
            FileResult::Cancelled
        }
    }
}

fn skip_detail(reason: ReplaceSkipReason, error: &impl std::fmt::Display) -> String {
    match reason {
        ReplaceSkipReason::ChangedSinceSearch => "a matched line changed since search".to_string(),
        ReplaceSkipReason::ChangedDuringReplace => {
            "it changed while Replace All was writing it".to_string()
        }
        ReplaceSkipReason::NotUtf8 => "it is not valid UTF-8".to_string(),
        ReplaceSkipReason::LargerThanOpenTier => format!(
            "it or its replacement would be larger than the {} Replace All limit",
            iec_size(MAX_REPLACE_FILE_BYTES)
        ),
        _ => error.to_string(),
    }
}

/// Pass B (P5): stream the body through the edits into the target's durable
/// write. The rename happens only if the body's digest equals the original's,
/// the output's equals pass A's, and the target's facts did not change.
#[expect(
    clippy::too_many_arguments,
    reason = "pass B's inputs are exactly the facts pass A established; bundling them would only rename them"
)]
fn write_target_from_body(
    path: &Path,
    body_path: &Path,
    edits: &[StreamEdit<'_>],
    original: ContentDigest,
    replaced: ContentDigest,
    facts: &FileFacts,
    cancel: &AtomicBool,
    progress: Option<&ReplaceProgress>,
    verdict: &mut ProducerVerdict,
) -> Result<(), fs_write::DurableWriteError> {
    if take_replace_before_rename_failure_for_test(path) {
        return Err(fs_write::DurableWriteError::BeforeRename(
            std::io::Error::other(format!(
                "Injected pre-rename Replace All failure for {}",
                path.display()
            )),
        ));
    }
    let write_path = fs_write::resolve_target_identity(path)
        .map_err(fs_write::DurableWriteError::BeforeRename)?
        .into_path_buf();
    fs_write::atomic_replace_workspace_file_stream(&write_path, WriteLabel::REPLACE, |target| {
        let mut stream = ReplaceStream::new(
            edits,
            replaced.len,
            target,
            Sha256Sink::default(),
            Sha256Sink::default(),
        );
        let streamed = stream_bounded(
            body_path,
            original.len,
            REPLACE_STREAM_CHUNK_BYTES,
            || cancel.load(Ordering::Relaxed),
            |chunk| {
                if let Some(progress) = progress {
                    progress.add_bytes(chunk.len());
                }
                stream.feed(chunk)
            },
        );
        match streamed {
            Ok(_) => {}
            Err(BoundedStreamError::Cancelled) => {
                verdict.cancelled = true;
                return Err(std::io::Error::other("cancelled"));
            }
            Err(BoundedStreamError::Consumer(StreamStop::Io(e)) | BoundedStreamError::Io(e)) => {
                return Err(e);
            }
            Err(_) => return Err(verdict.fail(ReplaceSkipReason::UndoJournalFailed)),
        }
        let (done, _) = stream.finish().map_err(|stop| match stop {
            StreamStop::Io(e) => e,
            StreamStop::Outcome(_) => std::io::Error::other("stream stopped"),
        })?;
        let body_digest = ContentDigest {
            len: done.source_len,
            sha256: done.source_digest.finish(),
        };
        let output_digest = ContentDigest {
            len: done.output_len,
            sha256: done.output_digest.finish(),
        };
        if !matches!(done.outcome, ReplacementTextOutcome::Replaced { .. })
            || body_digest != original
            || output_digest != replaced
        {
            // The body no longer holds what pass A wrote: never rename.
            return Err(verdict.fail(ReplaceSkipReason::UndoJournalFailed));
        }
        if !facts_unchanged(facts, path) {
            return Err(verdict.fail(ReplaceSkipReason::ChangedDuringReplace));
        }
        kill_point::reach(
            KillWindow::ReplaceEntryBeforeTargetRename,
            WriteLabel::REPLACE.as_str(),
        );
        Ok(())
    })
}

/// Hash a whole file without keeping it, under `limit`.
pub(super) fn digest_file(
    path: &Path,
    limit: u64,
) -> Result<ContentDigest, BoundedStreamError<std::convert::Infallible>> {
    let mut sink = Sha256Sink::default();
    let len = stream_bounded(
        path,
        limit,
        REPLACE_STREAM_CHUNK_BYTES,
        || false,
        |chunk| {
            super::stream::DigestSink::update(&mut sink, chunk);
            Ok(())
        },
    )?;
    Ok(ContentDigest {
        len,
        sha256: sink.finish(),
    })
}
