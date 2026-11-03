// SPDX-License-Identifier: GPL-3.0-or-later

//! The application's GtkSourceView language catalog (`bound-post-open-settle`,
//! design D2).
//!
//! **Cross-cutting, no role and no `WFR-*` row of its own.** Its owning rows
//! are `WFR-DOCUMENT-LOAD` (the planning worker guesses a document's language
//! id from the catalog's glob table, and the attach stage takes the language
//! from it), `WFR-DOCUMENT-SCALE` (the per-language syntax threshold reads
//! that id) and `WFR-MARKDOWN-PREVIEW` (code blocks resolve their fence
//! language through it).
//!
//! The first use of a `GtkSourceLanguageManager` reads the metadata of every
//! language file, about 4 ms that used to land in the activation dispatch.
//! The catalog pays it in a dispatch of its own at application startup,
//! while the window is being constructed (review R3): an idle that loads the
//! default manager's language list and copies every language's globs into an
//! immutable [`LanguageGlobTable`], which load-planning workers read.
//!
//! "Of its own" needs a priority no other source shares: GLib dispatches every
//! ready source of the top priority in one iteration (ledger A81). At
//! `G_PRIORITY_DEFAULT_IDLE` the build shared stage W's dispatch on `main`
//! (11.3 ms together). Once worker completions arrived ahead of default-idle
//! work (`spawn-completion-priority`, ledger A80), it shared the first
//! session-restore turn's dispatch instead (14–19 ms). It now runs at
//! `CATALOG_BUILD_PRIORITY`, two steps above `G_PRIORITY_DEFAULT_IDLE`: above
//! the composition stages and alone in its iteration, and still below the
//! frame clock and input.
//!
//! It cannot run on a worker (design D2 planned that): GtkSourceView keeps
//! part of its language specifications in its own GResource bundle, and a
//! manager whose list is loaded off the GTK thread fails to open every one of
//! them (`Unable to open 'resource:///org/gnome/gtksourceview/language-specs/…'`,
//! ledger A67), so its catalog would lack YAML and others. One ~4 ms dispatch
//! fits `B_SETTLE_US` and is off the activation turn.
//!
//! Every LushText use of a language manager goes through [`manager`]; the
//! attach stage waits for readiness ([`when_ready`]), so it never pays the
//! list load in its own dispatch.

use std::cell::RefCell;
use std::sync::Arc;

use gtk4::glib;

use crate::model::language_guess::{LanguageGlobTable, LanguageGlobs};

enum CatalogState {
    NotStarted,
    Building(Vec<Box<dyn FnOnce()>>),
    Ready {
        manager: sourceview5::LanguageManager,
        table: Arc<LanguageGlobTable>,
    },
}

thread_local! {
    static CATALOG: RefCell<CatalogState> = const { RefCell::new(CatalogState::NotStarted) };
}

/// `G_PRIORITY_DEFAULT_IDLE - 2`, a priority only this build uses.
const CATALOG_BUILD_PRIORITY: i32 = 198;

fn catalog_build_priority() -> glib::Priority {
    glib::Priority::from(CATALOG_BUILD_PRIORITY)
}

/// Start building the catalog in an idle of its own, once per process
/// (application `startup`; any later caller that needs it starts it too).
pub fn start() {
    let started = CATALOG.with(|state| {
        let mut state = state.borrow_mut();
        if matches!(*state, CatalogState::NotStarted) {
            *state = CatalogState::Building(Vec::new());
            true
        } else {
            false
        }
    });
    if started {
        glib::idle_add_local_full(catalog_build_priority(), || {
            build();
            glib::ControlFlow::Break
        });
    }
}

/// Load the language list and copy the globs, in one dispatch.
fn build() {
    let _span = crate::ui::responsiveness_probe::PostOpenSpan::begin("post_open_language_catalog");
    let manager = sourceview5::LanguageManager::default();
    let languages = manager
        .language_ids()
        .iter()
        .map(|id| LanguageGlobs {
            id: id.to_string(),
            globs: manager
                .language(id)
                .map(|language| language.globs().iter().map(ToString::to_string).collect())
                .unwrap_or_default(),
        })
        .collect();
    let waiting = CATALOG.with(|state| {
        let previous = state.replace(CatalogState::Ready {
            manager,
            table: Arc::new(LanguageGlobTable::new(languages)),
        });
        match previous {
            CatalogState::Building(waiting) => waiting,
            _ => Vec::new(),
        }
    });
    for callback in waiting {
        callback();
    }
}

/// Whether the catalog is ready.
#[must_use]
pub fn is_ready() -> bool {
    CATALOG.with(|state| matches!(*state.borrow(), CatalogState::Ready { .. }))
}

/// Run `callback` once the catalog is ready: now when it is, otherwise when
/// its idle has run (starting it if nothing has).
pub fn when_ready(callback: impl FnOnce() + 'static) {
    start();
    let callback: Box<dyn FnOnce()> = Box::new(callback);
    let ready_now = CATALOG.with(|state| match &mut *state.borrow_mut() {
        CatalogState::Building(waiting) => {
            waiting.push(callback);
            None
        }
        CatalogState::Ready { .. } => Some(callback),
        CatalogState::NotStarted => unreachable!("start() left the catalog unstarted"),
    });
    if let Some(callback) = ready_now {
        callback();
    }
}

/// The language manager. Until the catalog is ready, using it may pay the
/// list load in the caller's dispatch (the one fallback).
#[must_use]
pub fn manager() -> sourceview5::LanguageManager {
    CATALOG
        .with(|state| match &*state.borrow() {
            CatalogState::Ready { manager, .. } => Some(manager.clone()),
            _ => None,
        })
        .unwrap_or_else(sourceview5::LanguageManager::default)
}

/// The glob table workers guess language ids from, once the catalog is
/// ready.
#[must_use]
pub fn glob_table() -> Option<Arc<LanguageGlobTable>> {
    CATALOG.with(|state| match &*state.borrow() {
        CatalogState::Ready { table, .. } => Some(Arc::clone(table)),
        _ => None,
    })
}
