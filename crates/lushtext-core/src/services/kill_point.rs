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
//! label is the durable write's `WriteLabel`).

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
}

impl KillWindow {
    /// The name `LUSHTEXT_KILL_AT` uses for this window.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DraftBodyBeforeCommit => "draft-body-before-commit",
            Self::DurableRenamedBeforeDirSync => "durable-renamed-before-dirsync",
            Self::StalePreservedBeforeRetire => "stale-preserved-before-retire",
        }
    }
}

/// Mark that the process is inside `window`, for an operation labelled `label`.
#[cfg(feature = "crash-kill-points")]
pub fn reach(window: KillWindow, label: &str) {
    use std::sync::OnceLock;

    static TARGET: OnceLock<Option<(String, Option<String>)>> = OnceLock::new();
    let target = TARGET.get_or_init(|| {
        let value = std::env::var("LUSHTEXT_KILL_AT").ok()?;
        Some(match value.split_once('@') {
            Some((name, label)) => (name.to_string(), Some(label.to_string())),
            None => (value, None),
        })
    });
    let Some((name, wanted_label)) = target else {
        return;
    };
    if name == window.name() && wanted_label.as_deref().is_none_or(|wanted| wanted == label) {
        eprintln!("LUSHTEXT_KILL_AT: aborting inside {name} ({label})");
        std::process::abort();
    }
}

/// Mark that the process is inside `window`; compiled out without the
/// `crash-kill-points` feature.
#[cfg(not(feature = "crash-kill-points"))]
#[inline(always)]
pub fn reach(_window: KillWindow, _label: &str) {}
