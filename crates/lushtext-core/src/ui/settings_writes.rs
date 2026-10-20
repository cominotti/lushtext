// SPDX-License-Identifier: GPL-3.0-or-later

//! The application's one coalescing GSettings writer.
//!
//! **Cross-cutting coordination with no workflow role and no `WFR-*` row**: it
//! serves every workflow that persists a preference or shell layout, and owns
//! when and on which dispatch a value reaches the GSettings backend
//! (`bound-window-construction-dispatch`, C4 of the long-line programme).
//!
//! # Why
//!
//! Under the keyfile backend every `set_*` replaces the settings file and
//! `fsync`s it on the calling thread, once per call, even for an unchanged
//! value (ledger A38); the shipped Flatpak runs on that backend. C4's lane
//! measured 10–13 ms per write on the reference machine's home filesystem, so
//! eight writes made window construction take 135–151 ms, and every toggle or
//! resize notification stalled its dispatch the same way.
//!
//! # How
//!
//! - **One shared delayed instance.** Every LushText reader, binding and
//!   `changed` subscriber of the application schema uses [`shared`], one
//!   `gio::Settings` per process in `delay()` mode. A write is kept in memory
//!   and read back at once by every LushText reader, and `changed` reaches every
//!   LushText subscriber inside the write, so effects on other windows and
//!   editor pages stay immediate; another instance would see it only at the
//!   flush (ledger A39, A41). External changes (another process,
//!   `gsettings set`) still arrive through it.
//! - **Change-only writes.** The `set_*` functions here drop a value equal to
//!   the current one before it reaches GSettings, and the pending set
//!   (`model::settings_write_batch`) drops a value equal to the last one known
//!   to be persisted, so a steady-state launch writes nothing.
//! - **One flush per persistence window, never before the first frame.** A
//!   flush is one `apply()`, one backend write for every pending key (A39). It
//!   runs no earlier than the first presented frame of the first window, at
//!   most once per [`PERSISTENCE_WINDOW`] (a trailing debounce), on a
//!   low-priority idle. It runs on the GTK thread: the keyfile backend takes no
//!   lock, so a worker flush through the shared backend would race it, and one
//!   through a private backend object would leave every other reader stale
//!   until a file-monitor round trip (A40).
//! - **Exact state at exit.** The last window's confirmed close and application
//!   shutdown flush synchronously ([`flush_now`]); closing a window while
//!   others remain flushes nothing synchronously.
//!
//! A crash before a flush loses at most one persistence window of preference
//! and layout values (never user content); the split-view migration re-runs on
//! the next launch.
//!
//! Only the GTK thread may call into this module: the shared instance and its
//! state are thread-local.

use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk_lush_settle::SupersedingTimer;
use gtk4::gio;
use gtk4::prelude::*;

use crate::config;
use crate::model::settings_write_batch::SettingsWriteBatch;
use crate::ui::construction_spans;

/// Room for every key of the application schema without regrowing.
const SCHEMA_KEYS_HINT: usize = 64;

/// The quiet time after the last write before pending values are flushed.
pub const PERSISTENCE_WINDOW: Duration = Duration::from_millis(500);

/// What the settings writer reports to the shell-geometry evidence and the
/// automation snapshot: counts only, never a key or a value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsWriterEvidence {
    /// Keys whose newest value is not yet persisted.
    pub pending: u64,
    /// Values flushed to the backend so far.
    pub flushed: u64,
    /// Flushes (backend writes) so far.
    pub flushes: u64,
    /// Writes dropped because the value was already the current or persisted one.
    pub dropped_equal: u64,
    /// Backend writes made before the first window's first presented frame.
    pub writes_before_first_frame: u64,
}

struct SettingsWriter {
    /// The shared delayed instance every LushText reader uses.
    settings: gio::Settings,
    /// A plain instance on the same backend: what is persisted now.
    persisted: gio::Settings,
    batch: RefCell<SettingsWriteBatch<String, glib::Variant>>,
    dropped_before_set: Cell<u64>,
    writes_before_first_frame: Cell<u64>,
    first_frame_presented: Cell<bool>,
    flush_timer: SupersedingTimer,
    flush_idle: RefCell<Option<glib::SourceId>>,
}

thread_local! {
    static WRITER: OnceCell<Rc<SettingsWriter>> = const { OnceCell::new() };
}

fn instance() -> Rc<SettingsWriter> {
    WRITER.with(|cell| Rc::clone(cell.get_or_init(SettingsWriter::new)))
}

impl SettingsWriter {
    fn new() -> Rc<Self> {
        let settings = gio::Settings::new(config::APP_ID);
        settings.delay();
        let writer = Rc::new(Self {
            settings,
            persisted: gio::Settings::new(config::APP_ID),
            batch: RefCell::new(SettingsWriteBatch::with_capacity(SCHEMA_KEYS_HINT)),
            dropped_before_set: Cell::new(0),
            writes_before_first_frame: Cell::new(0),
            first_frame_presented: Cell::new(false),
            flush_timer: SupersedingTimer::new(),
            flush_idle: RefCell::new(None),
        });
        let weak = Rc::downgrade(&writer);
        writer.settings.connect_changed(None, move |_, key| {
            if let Some(writer) = weak.upgrade() {
                writer.observe(key);
            }
        });
        writer
    }

    /// Classify a `changed` on the shared instance: a value that differs from
    /// the backend's is pending; one equal to it (an external change, or a
    /// write undone before its flush) is persisted.
    fn observe(&self, key: &str) {
        let value = self.settings.value(key);
        let persisted = self.persisted.value(key);
        let pending = {
            let mut batch = self.batch.borrow_mut();
            if value == persisted {
                batch.observe_persisted(key.to_owned(), value);
            } else {
                let _ = batch.record(key.to_owned(), value, || persisted);
            }
            !batch.is_empty()
        };
        if pending {
            self.schedule_flush();
        }
    }

    fn write(&self, key: &str, value: &glib::Variant) {
        if self.settings.value(key) == *value {
            self.count_dropped();
            return;
        }
        if let Err(error) = self.settings.set_value(key, value) {
            tracing::error!("settings writer: cannot set {key}: {error}");
        }
    }

    fn count_dropped(&self) {
        self.dropped_before_set
            .set(self.dropped_before_set.get().saturating_add(1));
    }

    fn schedule_flush(&self) {
        if !self.first_frame_presented.get() {
            return;
        }
        // Re-arming removes the superseded timeout, so a resize drag that
        // writes every frame keeps one live source, not one per write.
        let _ = self
            .flush_timer
            .arm(&self.settings, PERSISTENCE_WINDOW, |_settings, _token| {
                let writer = instance();
                if writer.flush_idle.borrow().is_some() {
                    return;
                }
                let id = glib::idle_add_local_full(glib::Priority::LOW, || {
                    let writer = instance();
                    writer.flush_idle.replace(None);
                    writer.flush();
                    glib::ControlFlow::Break
                });
                writer.flush_idle.replace(Some(id));
            });
    }

    /// Persist every pending value in one backend write.
    fn flush(&self) {
        let snapshot = self.batch.borrow_mut().begin_flush();
        if snapshot.is_empty() {
            // Only writes undone before their flush are left: drop them
            // without touching the backend.
            if self.settings.has_unapplied() {
                self.settings.revert();
            }
            return;
        }
        let start_us = construction_spans::now_us();
        self.settings.apply();
        construction_spans::emit("settings_flush", start_us, construction_spans::now_us());
        self.batch.borrow_mut().acknowledge(&snapshot);
        if !self.first_frame_presented.get() {
            self.writes_before_first_frame
                .set(self.writes_before_first_frame.get().saturating_add(1));
        }
    }

    fn flush_now(&self) {
        let _ = self.flush_timer.invalidate();
        if let Some(id) = self.flush_idle.borrow_mut().take() {
            id.remove();
        }
        self.flush();
    }

    fn evidence(&self) -> SettingsWriterEvidence {
        let batch = self.batch.borrow();
        let counters = batch.counters();
        SettingsWriterEvidence {
            pending: u64::try_from(batch.pending_len()).unwrap_or(u64::MAX),
            flushed: counters.flushed_values,
            flushes: counters.flushes,
            dropped_equal: counters
                .dropped_equal
                .saturating_add(self.dropped_before_set.get()),
            writes_before_first_frame: self.writes_before_first_frame.get(),
        }
    }
}

/// The application's shared settings instance (delayed; see the module docs).
/// Read, bind and subscribe through it; write through the `set_*` functions.
#[must_use]
pub fn shared() -> gio::Settings {
    instance().settings.clone()
}

/// Run `on_changed` whenever `key` changes on the shared instance, for as long
/// as `owner` lives.
///
/// The shared instance outlives every window and widget, so a handler
/// connected to it directly would stay registered (and keep everything its
/// closure captured) after its owner is gone. This one holds `owner` weakly
/// and is disconnected when `owner` is finalized.
pub fn connect_changed_for<O, F>(owner: &O, key: &'static str, on_changed: F)
where
    O: IsA<glib::Object>,
    F: Fn(&O, &gio::Settings) + 'static,
{
    let settings = shared();
    let owner_weak = owner.downgrade();
    let handler = settings.connect_changed(Some(key), move |settings, _| {
        if let Some(owner) = owner_weak.upgrade() {
            on_changed(&owner, settings);
        }
    });
    let _ = owner.add_weak_ref_notify_local(move || settings.disconnect(handler));
}

/// Write `value` to `key` through the writer (change-only, flushed later).
pub fn set_value(key: &str, value: &glib::Variant) {
    instance().write(key, value);
}

/// Write a boolean key through the writer.
pub fn set_boolean(key: &str, value: bool) {
    set_value(key, &value.to_variant());
}

/// Write an `i` key through the writer.
pub fn set_int(key: &str, value: i32) {
    set_value(key, &value.to_variant());
}

/// Write a `u` key through the writer.
pub fn set_uint(key: &str, value: u32) {
    set_value(key, &value.to_variant());
}

/// Write a `d` key through the writer.
pub fn set_double(key: &str, value: f64) {
    set_value(key, &value.to_variant());
}

/// Write an `s` key through the writer.
pub fn set_string(key: &str, value: &str) {
    set_value(key, &value.to_variant());
}

/// Write an `as` key through the writer.
pub fn set_strv(key: &str, value: &[&str]) {
    set_value(key, &value.to_variant());
}

/// Reset `key` to its schema default through the writer.
pub fn reset(key: &str) {
    let writer = instance();
    if writer.settings.user_value(key).is_none() {
        writer.count_dropped();
        return;
    }
    writer.settings.reset(key);
}

/// The first window presented its first frame: flushing may start.
pub(crate) fn first_frame_presented() {
    let writer = instance();
    if writer.first_frame_presented.replace(true) {
        return;
    }
    if !writer.batch.borrow().is_empty() {
        writer.schedule_flush();
    }
}

/// Persist every pending value now, synchronously (the last window's close
/// and application shutdown).
pub fn flush_now() {
    instance().flush_now();
}

/// The writer's counters.
#[must_use]
pub fn evidence() -> SettingsWriterEvidence {
    instance().evidence()
}
