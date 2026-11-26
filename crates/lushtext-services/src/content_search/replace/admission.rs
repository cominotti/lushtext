// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace All admission: the no-loss ledger and disk admission (design D4).
//!
//! Pure: free space is passed in by the shell, which probes it through the
//! filesystem boundary.
//!
//! **Disk admission.** A body needs its source length plus
//! [`REPLACE_DISK_RESERVE_BYTES`] free on the journal's filesystem, and an
//! output its length plus the reserve on the target's. Undo and rollback need
//! only their writes' own size, never the reserve.
//!
//! **No-loss waiver.** [`MainAdmissionLedger`] replays `main`'s admission rule
//! (`main` at `2c91ccf2`): two passes in path order at
//! [`MAIN_REPLACE_FILE_BYTES`] then [`MAIN_MAX_REPLACE_FILE_BYTES`] for source
//! and output; each admitted file charges source plus output against
//! `MAIN_REPLACE_UNDO_BYTES` and its entry weight against
//! [`MAX_REPLACE_UNDO_RETAINED_BYTES`]; a pre-rename failure reclaims both.
//! A file the ledger admits is waived from the reserve, so Replace All never
//! skips for disk space a file `main` replaced. The decision is made twice:
//! provisionally at P1, from the source alone, and finally at P3, once the
//! output length is known.

use std::path::Path;

use super::{
    MAIN_MAX_REPLACE_FILE_BYTES, MAIN_REPLACE_FILE_BYTES, MAX_REPLACE_UNDO_RETAINED_BYTES,
    REPLACE_DISK_RESERVE_BYTES, main_retained_entry_weight,
};
use lushtext_model::content_search::UndoPayloadLedger;

/// One of Replace All's two passes in path order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplacePass {
    /// Files whose source and output fit `main`'s first-pass limit.
    First,
    /// Every file the first pass deferred, up to the per-file limit.
    Second,
}

impl ReplacePass {
    /// `main`'s per-file limit for this pass, which the ledger replays.
    #[must_use]
    pub(super) const fn main_limit(self) -> u64 {
        match self {
            Self::First => MAIN_REPLACE_FILE_BYTES,
            Self::Second => MAIN_MAX_REPLACE_FILE_BYTES,
        }
    }
}

/// The charges the ledger took for one admitted file, given back by
/// [`MainAdmissionLedger::reclaim`] when that file fails before its rename.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MainCharge {
    payload: u64,
    retained: u64,
}

/// Replays `main`'s undo admission, so Replace All waives the disk reserve
/// for exactly the files `main` would have admitted (design D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MainAdmissionLedger {
    payload: UndoPayloadLedger,
    retained: UndoPayloadLedger,
}

impl MainAdmissionLedger {
    /// A ledger with `main`'s aggregate undo payload `payload_limit`.
    #[must_use]
    pub(super) const fn new(payload_limit: u64) -> Self {
        Self {
            payload: UndoPayloadLedger::new(payload_limit),
            retained: UndoPayloadLedger::new(MAX_REPLACE_UNDO_RETAINED_BYTES),
        }
    }

    /// P1: whether the file can still be one `main` admitted, from its source
    /// length alone. Every file `main` admitted passes, because `main` charged
    /// source plus output, which is at least the source.
    #[must_use]
    pub(super) fn provisionally_waived(&self, pass: ReplacePass, source_len: u64) -> bool {
        source_len <= pass.main_limit() && source_len <= self.payload.remaining_bytes()
    }

    /// P3: charge the file exactly as `main` did. `Some` means `main` admitted
    /// it and the reserve is waived; `None` means it needs the reserve.
    pub(super) fn charge(
        &mut self,
        pass: ReplacePass,
        path: &Path,
        source_len: u64,
        output_len: u64,
    ) -> Option<MainCharge> {
        let limit = pass.main_limit();
        if source_len > limit || output_len > limit {
            return None;
        }
        let payload = source_len.saturating_add(output_len);
        if !self.payload.try_charge(payload) {
            return None;
        }
        let retained = main_retained_entry_weight(path, source_len, output_len);
        if !self.retained.try_charge(retained) {
            self.payload.reclaim(payload);
            return None;
        }
        Some(MainCharge { payload, retained })
    }

    /// Give back a charge for a file that failed before its rename, as `main` did.
    pub(super) fn reclaim(&mut self, charge: MainCharge) {
        let payload = self.payload.reclaim(charge.payload);
        debug_assert_eq!(payload, charge.payload);
        let retained = self.retained.reclaim(charge.retained);
        debug_assert_eq!(retained, charge.retained);
    }
}

/// Whether a write of `need` bytes is admitted with `free` bytes available.
///
/// `free == None` is an unknown probe: the write is admitted, and an `ENOSPC`
/// becomes a failure before its rename, which is safe because nothing is
/// renamed. With `reserve`, [`REPLACE_DISK_RESERVE_BYTES`] must stay free too.
#[must_use]
pub(super) fn disk_admits(free: Option<u64>, need: u64, reserve: bool) -> bool {
    let Some(free) = free else {
        return true;
    };
    let reserve = if reserve {
        REPLACE_DISK_RESERVE_BYTES
    } else {
        0
    };
    need.checked_add(reserve).is_some_and(|total| free >= total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_search::replace::MAIN_REPLACE_UNDO_BYTES;
    use std::path::PathBuf;

    /// One file of a transcribed run: its source and output lengths, and
    /// whether its write fails before rename.
    #[derive(Clone, Copy, Debug)]
    struct File {
        source: u64,
        output: u64,
        fails: bool,
    }

    const MIB: u64 = 1024 * 1024;

    fn path(index: usize) -> PathBuf {
        PathBuf::from(format!("/w/{index:04}.txt"))
    }

    /// `main`'s `apply_replacements_inner` admission, transcribed: which files
    /// it replaced.
    fn main_replaced(files: &[File], undo_limit: u64) -> Vec<bool> {
        let mut replaced = vec![false; files.len()];
        let mut undo = UndoPayloadLedger::new(undo_limit);
        let mut retained = UndoPayloadLedger::new(MAX_REPLACE_UNDO_RETAINED_BYTES);
        let mut pending: Vec<usize> = (0..files.len()).collect();
        let mut deferred = Vec::new();
        for pass_bytes in [MAIN_REPLACE_FILE_BYTES, MAIN_MAX_REPLACE_FILE_BYTES] {
            let first = pass_bytes < MAIN_MAX_REPLACE_FILE_BYTES;
            for index in std::mem::take(&mut pending) {
                let file = files[index];
                if file.source > MAIN_MAX_REPLACE_FILE_BYTES {
                    continue;
                }
                if first && file.source > pass_bytes {
                    deferred.push(index);
                    continue;
                }
                let remaining = undo.remaining_bytes().saturating_sub(file.source);
                let output_limit = remaining.min(pass_bytes);
                let limited_by_file = pass_bytes <= remaining;
                if file.output > output_limit {
                    if first && limited_by_file {
                        deferred.push(index);
                    }
                    continue;
                }
                let payload = file.source + file.output;
                if !undo.try_charge(payload) {
                    continue;
                }
                let weight = main_retained_entry_weight(&path(index), file.source, file.output);
                if !retained.try_charge(weight) {
                    undo.reclaim(payload);
                    continue;
                }
                if file.fails {
                    undo.reclaim(payload);
                    retained.reclaim(weight);
                    continue;
                }
                replaced[index] = true;
            }
            pending = std::mem::take(&mut deferred);
        }
        replaced
    }

    /// The branch's ledger over the same run: which files it waives.
    fn ledger_waived(files: &[File], undo_limit: u64) -> Vec<bool> {
        let mut waived = vec![false; files.len()];
        let mut ledger = MainAdmissionLedger::new(undo_limit);
        let mut pending: Vec<usize> = (0..files.len()).collect();
        let mut deferred = Vec::new();
        for pass in [ReplacePass::First, ReplacePass::Second] {
            for index in std::mem::take(&mut pending) {
                let file = files[index];
                if pass == ReplacePass::First
                    && (file.source > MAIN_REPLACE_FILE_BYTES
                        || file.output > MAIN_REPLACE_FILE_BYTES)
                {
                    deferred.push(index);
                    continue;
                }
                let provisional = ledger.provisionally_waived(pass, file.source);
                let charge = ledger.charge(pass, &path(index), file.source, file.output);
                if let Some(charge) = charge {
                    assert!(
                        provisional,
                        "a finally waived file was provisionally waived"
                    );
                    if file.fails {
                        ledger.reclaim(charge);
                        continue;
                    }
                    waived[index] = true;
                }
            }
            pending = std::mem::take(&mut deferred);
        }
        waived
    }

    fn assert_no_loss(files: &[File], undo_limit: u64) {
        let main = main_replaced(files, undo_limit);
        let waived = ledger_waived(files, undo_limit);
        for (index, (main, waived)) in main.iter().zip(&waived).enumerate() {
            assert!(
                !main || *waived,
                "file {index} {:?}: main replaced it, the ledger did not waive it",
                files[index]
            );
        }
        // Without pre-rename failures the two agree exactly.
        if files.iter().all(|file| !file.fails) {
            assert_eq!(main, waived);
        }
    }

    fn file(source: u64, output: u64) -> File {
        File {
            source,
            output,
            fails: false,
        }
    }

    #[test]
    fn ledger_matches_main_on_six_files_at_the_old_caps() {
        assert_no_loss(&[file(10 * MIB, 10 * MIB); 6], MAIN_REPLACE_UNDO_BYTES);
        assert_no_loss(&[file(16 * MIB, 16 * MIB); 6], MAIN_REPLACE_UNDO_BYTES);
    }

    #[test]
    fn ledger_defers_a_small_file_whose_output_crosses_ten_mib_as_main_did() {
        // A 9 MiB file whose output is 11 MiB sorts first; main defers it and
        // replaces the 10 MiB files within its 64 MiB payload.
        let mut files = vec![file(9 * MIB, 11 * MIB)];
        files.extend([file(10 * MIB, 10 * MIB); 3]);
        assert_no_loss(&files, MAIN_REPLACE_UNDO_BYTES);
        assert!(
            main_replaced(&files, MAIN_REPLACE_UNDO_BYTES)[1..]
                .iter()
                .all(|r| *r)
        );
    }

    #[test]
    fn ledger_follows_main_when_the_retained_weight_stops_a_file() {
        // A tiny payload limit is never reached; the retained weight is, once
        // the payloads alone fill MAX_REPLACE_UNDO_RETAINED_BYTES.
        // Six 10 MiB payloads fill 60 MiB of it; a seventh does not fit, and a
        // small file after it still does.
        let mut files = vec![file(5 * MIB, 5 * MIB); 7];
        files.push(file(4, 4));
        assert_no_loss(&files, u64::MAX);
        let main = main_replaced(&files, u64::MAX);
        let mut expected = vec![true; 6];
        expected.extend([false, true]);
        assert_eq!(main, expected);
    }

    #[test]
    fn a_pre_rename_failure_reclaims_its_charge_for_a_later_file() {
        let files = [
            File {
                source: 30 * MIB,
                output: 30 * MIB,
                fails: true,
            },
            file(15 * MIB, 15 * MIB),
        ];
        // 30 MiB is past every main limit: it never charges, and the second
        // file fits either way.
        assert_no_loss(&files, MAIN_REPLACE_UNDO_BYTES);
        // Two 32 MiB payloads plus their entry weight cross the 64 MiB
        // retained ceiling, so the second file fits only because the failed
        // first one gave its charge back.
        let files = [
            File {
                source: 16 * MIB,
                output: 16 * MIB,
                fails: true,
            },
            file(16 * MIB, 16 * MIB),
        ];
        assert_no_loss(&files, MAIN_REPLACE_UNDO_BYTES);
        assert_eq!(
            main_replaced(&files, MAIN_REPLACE_UNDO_BYTES),
            vec![false, true]
        );
        assert_eq!(
            ledger_waived(&files, MAIN_REPLACE_UNDO_BYTES),
            vec![false, true]
        );
    }

    #[test]
    fn a_large_file_never_charges_the_ledger() {
        let files = [file(300 * MIB, 300 * MIB), file(10 * MIB, 10 * MIB)];
        assert_no_loss(&files, MAIN_REPLACE_UNDO_BYTES);
        assert_eq!(
            ledger_waived(&files, MAIN_REPLACE_UNDO_BYTES),
            vec![false, true]
        );
    }

    #[test]
    fn ledger_matches_main_over_generated_runs() {
        // A deterministic linear-congruential walk over sizes around the
        // limits, with and without pre-rename failures.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = |modulus: u64| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            (state >> 33) % modulus
        };
        let sizes = [
            0,
            1,
            9 * MIB,
            10 * MIB,
            10 * MIB + 1,
            11 * MIB,
            16 * MIB,
            16 * MIB + 1,
            40 * MIB,
        ];
        for _ in 0..2_000 {
            let count = 1 + usize::try_from(next(7)).expect("small");
            let files: Vec<File> = (0..count)
                .map(|_| File {
                    source: sizes[usize::try_from(next(9)).expect("small")],
                    output: sizes[usize::try_from(next(9)).expect("small")],
                    fails: next(5) == 0,
                })
                .collect();
            let limit = [MAIN_REPLACE_UNDO_BYTES, 32 * MIB, 20 * MIB]
                [usize::try_from(next(3)).expect("small")];
            assert_no_loss(&files, limit);
        }
    }

    #[test]
    fn disk_admission_requires_the_reserve_only_when_asked() {
        assert!(disk_admits(None, u64::MAX, true));
        assert!(disk_admits(Some(10), 10, false));
        assert!(!disk_admits(Some(9), 10, false));
        assert!(!disk_admits(Some(REPLACE_DISK_RESERVE_BYTES + 9), 10, true));
        assert!(disk_admits(Some(REPLACE_DISK_RESERVE_BYTES + 10), 10, true));
        assert!(!disk_admits(Some(u64::MAX), u64::MAX, true));
    }
}
