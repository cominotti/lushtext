// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — the Focus Mode workflow's single observable state.
//!
//! One accessor reads the whole surface. Three constraints follow:
//!
//! * **Reading must not mutate.** Every field here is a `Cell` read, a widget
//!   property, or a pure derivation. In particular the surface must never call
//!   the workflow's own `apply_*` helpers to find out what they would do: those
//!   set widget state, and an observer that changes what it observes is not an
//!   observation.
//! * **No field may be read from inside a mutable borrow** of the state it
//!   reads. This surface takes no `RefCell` borrow at all — `FocusModeState` is
//!   entirely `Cell<bool>` plus a `SupersedingTimer` — so the constraint holds by
//!   construction rather than by discipline.
//! * **A disposed widget is a stage.** Chrome visibility, the affordance
//!   revealer, and the editor count are all reached through `try_get()` in the
//!   row's called presentation surface, which this module calls rather than
//!   repeating.
//!
//! Reading materializes nothing. The editor count walks `AdwTabView`, which
//! holds its pages eagerly and creates nothing on demand; no accessor here
//! reaches a lazily-populated GTK collection such as `GtkTreeListModel`.
//!
//! **The affordance timer is deliberately absent.** `SupersedingTimer` exposes no
//! armed query, and this workflow has no independent record of whether one is
//! armed. Reporting a guess would be worse than reporting nothing, and adding a
//! parallel `Cell<bool>` to track it — the shape the draft row needed — is not
//! justified here because no proof requires it. The observable consequence of the
//! timer, `affordance_revealed`, is reported instead.

use glib::subclass::prelude::ObjectSubclassIsExt;

use super::super::LushtextWindow;
use super::policy::{FocusModeEntry, FocusModeRestore, exit_restoration};
use super::{chrome, preview_readable_column_applies};

/// Everything a test or probe may observe about Focus Mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusModeEvidence {
    /// Whether Focus Mode is currently active for this window.
    pub active: bool,
    /// The shell state Focus Mode borrowed on entry.
    ///
    /// Meaningful only while `active`; it retains the last session's capture
    /// afterwards, which is what makes an exit's restoration decision auditable
    /// after the fact.
    pub entry: FocusModeEntry,
    /// Whether the user made an explicit preview choice during this session.
    pub preview_changed_while_focused: bool,
    /// What an exit right now would restore.
    ///
    /// A pure derivation over the two fields above, reported so a test asserts
    /// the decision rather than re-implementing it.
    pub exit_restores: FocusModeRestore,
    /// Whether the overlaid affordance currently reveals its child.
    pub affordance_revealed: bool,
    /// Whether keyboard focus is inside the affordance.
    pub affordance_contains_focus: bool,
    /// Whether the header bar is visible.
    pub header_bar_visible: bool,
    /// Whether the status bar is visible.
    pub status_bar_visible: bool,
    /// Whether the Markdown preview currently takes the readable column.
    pub preview_takes_readable_column: bool,
    /// How many open editor pages carry Focus Mode presentation.
    ///
    /// Bounded by the tab count, `0` when the tab view is gone, and a disposed
    /// page is skipped rather than panicked on.
    pub editors_with_focus_mode: usize,
}

/// Read the whole Focus Mode surface.
#[must_use]
pub fn focus_mode_evidence(window: &LushtextWindow) -> FocusModeEvidence {
    // Every scalar is computed before the struct literal, so no read outlives a
    // borrow it produced.
    let state = &window.imp().focus_mode;
    let entry = FocusModeEntry {
        was_fullscreen: state.was_fullscreen_on_entry.get(),
        had_side_by_side_preview: state.restore_side_by_side_preview.get(),
    };
    let preview_changed_while_focused = state.preview_changed_while_focused.get();
    let (header_bar_visible, status_bar_visible) = chrome::chrome_visibility(window);

    FocusModeEvidence {
        active: state.active.get(),
        entry,
        preview_changed_while_focused,
        exit_restores: exit_restoration(entry, preview_changed_while_focused),
        affordance_revealed: chrome::affordance_revealed(window),
        affordance_contains_focus: chrome::affordance_contains_focus(window),
        header_bar_visible,
        status_bar_visible,
        preview_takes_readable_column: preview_readable_column_applies(window),
        editors_with_focus_mode: chrome::editors_with_focus_mode_applied(window),
    }
}
