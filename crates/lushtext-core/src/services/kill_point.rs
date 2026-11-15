// SPDX-License-Identifier: GPL-3.0-or-later

//! Deterministic crash injection for the crash-recovery smoke.
//!
//! The real-process smoke (`make crash-recovery-smoke`) needs to kill LushText
//! *inside* a protocol window — after a draft body is written but before its
//! manifest commit, after a durable rename but before its directory sync, after
//! a stale body is preserved but before it is retired — which a signal from
//! outside cannot hit reliably. Each such window calls [`reach`]. With the
//! `crash-kill-points` Cargo feature, `reach` reads `LUSHTEXT_KILL_AT` once and
//! calls `std::process::abort()` (no destructors run, which is as close to
//! `SIGKILL` as a process can do to itself) when the window matches. Without the
//! feature — every release, Meson, Flatpak, and Snap build — `reach` is an empty
//! inline function and no kill-point code or environment read is compiled.
//!
//! `LUSHTEXT_KILL_AT` names one window (`draft-body-before-commit`), optionally
//! narrowed to one label (`durable-renamed-before-dirsync@draft`, where the
//! label is the durable write's `WriteLabel`) and to its n-th reach
//! (`replace-entry-before-target-rename@replace#3`, one-based, counted over the
//! reaches that match the name and label), so the smoke can kill on the third
//! file of five after the first two were replaced.

/// A named protocol window the smoke can kill the process inside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillWindow {
    /// A draft body is durably written; its manifest commit has not run.
    DraftBodyBeforeCommit,
    /// A durable write's rename landed; its parent-directory sync has not run.
    DurableRenamedBeforeDirSync,
    /// A stale draft body is preserved; its journal body and entry are not
    /// retired yet.
    StalePreservedBeforeRetire,
    /// A Replace All undo body is renamed into `bodies/`; its entry is not
    /// written yet.
    ReplaceBodyBeforeEntry,
    /// Inside pass B's producer at its end: the target temp holds every byte,
    /// the entry is armed, and the rename has not happened.
    ReplaceEntryBeforeTargetRename,
    /// The target's durable write returned; the next file has not started.
    ReplaceTargetRenamedBeforeFinish,
    /// Inside an Undo's restore producer at its end: the temp holds the
    /// original bytes and the rename has not happened.
    ReplaceUndoBeforeTargetRename,
}

impl KillWindow {
    /// The name `LUSHTEXT_KILL_AT` uses for this window.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DraftBodyBeforeCommit => "draft-body-before-commit",
            Self::DurableRenamedBeforeDirSync => "durable-renamed-before-dirsync",
            Self::StalePreservedBeforeRetire => "stale-preserved-before-retire",
            Self::ReplaceBodyBeforeEntry => "replace-body-before-entry",
            Self::ReplaceEntryBeforeTargetRename => "replace-entry-before-target-rename",
            Self::ReplaceTargetRenamedBeforeFinish => "replace-target-renamed-before-finish",
            Self::ReplaceUndoBeforeTargetRename => "replace-undo-before-target-rename",
        }
    }
}

/// A parsed `LUSHTEXT_KILL_AT` value: `name[@label][#n]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KillTarget {
    /// The window name.
    pub name: String,
    /// The label the window must be reached with, when narrowed.
    pub label: Option<String>,
    /// The one-based reach that kills; the first when absent.
    pub occurrence: u32,
}

impl KillTarget {
    /// Parse `name[@label][#n]`; `None` for an empty name or a bad ordinal.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let (rest, occurrence) = match value.rsplit_once('#') {
            Some((rest, digits)) => (rest, digits.parse::<u32>().ok().filter(|n| *n > 0)?),
            None => (value, 1),
        };
        let (name, label) = match rest.split_once('@') {
            Some((name, label)) => (name, Some(label.to_string())),
            None => (rest, None),
        };
        (!name.is_empty()).then(|| Self {
            name: name.to_string(),
            label,
            occurrence,
        })
    }

    /// Whether a reach of `window` with `label` matches the name and label.
    #[must_use]
    pub fn matches(&self, window: KillWindow, label: &str) -> bool {
        self.name == window.name() && self.label.as_deref().is_none_or(|wanted| wanted == label)
    }
}

/// Mark that the process is inside `window`, for an operation labelled `label`.
#[cfg(feature = "crash-kill-points")]
pub fn reach(window: KillWindow, label: &str) {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TARGET: OnceLock<Option<KillTarget>> = OnceLock::new();
    static REACHED: AtomicU32 = AtomicU32::new(0);
    let target = TARGET.get_or_init(|| KillTarget::parse(&std::env::var("LUSHTEXT_KILL_AT").ok()?));
    let Some(target) = target else {
        return;
    };
    if target.matches(window, label)
        && REACHED.fetch_add(1, Ordering::SeqCst).saturating_add(1) == target.occurrence
    {
        eprintln!(
            "LUSHTEXT_KILL_AT: aborting inside {} ({label}) at reach {}",
            target.name, target.occurrence
        );
        std::process::abort();
    }
}

/// Mark that the process is inside `window`; compiled out without the
/// `crash-kill-points` feature.
#[cfg(not(feature = "crash-kill-points"))]
#[inline(always)]
pub fn reach(_window: KillWindow, _label: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_name_kills_at_the_first_reach_of_any_label() {
        let target = KillTarget::parse("replace-body-before-entry").expect("parses");
        assert_eq!(target.occurrence, 1);
        assert!(target.matches(KillWindow::ReplaceBodyBeforeEntry, "replace-undo-body"));
        assert!(!target.matches(KillWindow::ReplaceEntryBeforeTargetRename, "replace"));
    }

    #[test]
    fn a_label_and_an_ordinal_narrow_the_reach() {
        let target =
            KillTarget::parse("replace-entry-before-target-rename@replace#3").expect("parses");
        assert_eq!(target.name, "replace-entry-before-target-rename");
        assert_eq!(target.label.as_deref(), Some("replace"));
        assert_eq!(target.occurrence, 3);
        assert!(target.matches(KillWindow::ReplaceEntryBeforeTargetRename, "replace"));
        assert!(!target.matches(KillWindow::ReplaceEntryBeforeTargetRename, "draft"));
        let unlabelled = KillTarget::parse("durable-renamed-before-dirsync#2").expect("parses");
        assert_eq!((unlabelled.label, unlabelled.occurrence), (None, 2));
    }

    #[test]
    fn a_zero_or_malformed_ordinal_and_an_empty_name_are_refused() {
        assert_eq!(KillTarget::parse("replace-body-before-entry#0"), None);
        assert_eq!(KillTarget::parse("replace-body-before-entry#x"), None);
        assert_eq!(KillTarget::parse("@replace"), None);
    }

    #[test]
    fn every_window_has_a_distinct_name() {
        let windows = [
            KillWindow::DraftBodyBeforeCommit,
            KillWindow::DurableRenamedBeforeDirSync,
            KillWindow::StalePreservedBeforeRetire,
            KillWindow::ReplaceBodyBeforeEntry,
            KillWindow::ReplaceEntryBeforeTargetRename,
            KillWindow::ReplaceTargetRenamedBeforeFinish,
            KillWindow::ReplaceUndoBeforeTargetRename,
        ];
        let names: std::collections::HashSet<_> = windows.iter().map(|w| w.name()).collect();
        assert_eq!(names.len(), windows.len());
    }
}
