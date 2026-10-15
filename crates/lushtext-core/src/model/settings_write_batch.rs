// SPDX-License-Identifier: GPL-3.0-or-later

//! The pending-set algebra of LushText's coalescing settings writer
//! (`ui::settings_writes`, `bound-window-construction-dispatch`).
//!
//! A write is recorded in memory at once and persisted later, in one flush per
//! persistence window. Between the two, this batch knows, per key, the last
//! value known to be persisted and the newest value not yet persisted:
//!
//! - **last writer wins**: recording a key again replaces its pending value;
//! - **change-only**: a value equal to the last persisted one is not pending
//!   (and cancels a pending different value, since the newest write is what
//!   must end up persisted);
//! - **snapshot and acknowledge**: a flush takes a snapshot of the pending
//!   values, and acknowledging that snapshot makes each value the persisted
//!   one and drops it from the pending set unless a newer value replaced it
//!   meanwhile.
//!
//! Generic over keys and values and free of GTK, so the unit and property
//! tests and the Kani harness (`settings_write_batch/kani_proofs.rs`) check the
//! code that ships. The writer instantiates it with schema key names and
//! `glib::Variant` values.

#[cfg(kani)]
mod kani_proofs;

/// The batch's per-key states, found by linear lookup: a writer holds a few
/// dozen schema keys at most.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SmallMap<K, V>(Vec<(K, V)>);

impl<K: Eq, V> SmallMap<K, V> {
    fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    fn position(&self, key: &K) -> Option<usize> {
        self.0.iter().position(|(probe, _)| probe == key)
    }

    fn get(&self, key: &K) -> Option<&V> {
        self.position(key).map(|index| &self.0[index].1)
    }

    fn get_or_insert_with(&mut self, key: K, value: impl FnOnce() -> V) -> &mut V {
        let index = if let Some(index) = self.position(&key) {
            index
        } else {
            self.0.push((key, value()));
            self.0.len() - 1
        };
        &mut self.0[index].1
    }

    fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.0.iter()
    }

    fn iter_mut(&mut self) -> impl Iterator<Item = &mut (K, V)> {
        self.0.iter_mut()
    }
}
/// What recording one value did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordOutcome {
    /// The value differs from the persisted one and is now pending.
    Pending,
    /// The value equals the persisted one: nothing is pending for the key.
    DroppedEqual,
}

/// Counters the writer's evidence reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SettingsWriteCounters {
    /// Values that became pending.
    pub recorded: u64,
    /// Values dropped because they equalled the persisted value.
    pub dropped_equal: u64,
    /// Flushes acknowledged.
    pub flushes: u64,
    /// Values those flushes persisted.
    pub flushed_values: u64,
}

/// What the writer knows about one key: the last value known to be persisted
/// (once seeded), the value a flush is writing now, and the newest value not
/// yet persisted.
///
/// The batch algebra is this per-key state machine; keys never interact, so
/// the batch only finds the key's state. Kani proves the machine
/// (`settings_write_batch/kani_proofs.rs`). Its first harness found that
/// comparing a new value only against the persisted one loses a write made
/// while a flush is in flight (write `a`, flush `a`, write back the old value
/// `p` before the acknowledgement: `p` equalled the stale persisted value and
/// was dropped, and the backend kept `a`); a value is therefore compared with
/// the in-flight value first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyWriteState<V> {
    persisted: Option<V>,
    in_flight: Option<V>,
    pending: Option<V>,
}

impl<V> Default for KeyWriteState<V> {
    fn default() -> Self {
        Self {
            persisted: None,
            in_flight: None,
            pending: None,
        }
    }
}

impl<V: Eq + Clone> KeyWriteState<V> {
    /// The value the backend will hold once nothing more is written: the
    /// in-flight value, else the persisted one.
    fn baseline(&self) -> Option<&V> {
        self.in_flight.as_ref().or(self.persisted.as_ref())
    }

    /// Record `value` as the key's newest value; `persisted_now` seeds the
    /// persisted value the first time.
    pub fn record(&mut self, value: V, persisted_now: impl FnOnce() -> V) -> RecordOutcome {
        if self.persisted.is_none() {
            self.persisted = Some(persisted_now());
        }
        if self.baseline() == Some(&value) {
            self.pending = None;
            RecordOutcome::DroppedEqual
        } else {
            self.pending = Some(value);
            RecordOutcome::Pending
        }
    }

    /// The backend now holds `value` (not through this writer's flush).
    pub fn observe_persisted(&mut self, value: V) {
        if self.pending.as_ref() == Some(&value) {
            self.pending = None;
        }
        self.persisted = Some(value);
    }

    /// Start a flush: the pending value, if any, becomes the in-flight one.
    pub fn begin_flush(&mut self) -> Option<V> {
        let value = self.pending.take()?;
        self.in_flight = Some(value.clone());
        Some(value)
    }

    /// The flush that wrote `value` finished: it is the persisted value.
    pub fn acknowledge(&mut self, value: &V) {
        if self.in_flight.as_ref() == Some(value) {
            self.in_flight = None;
        }
        if self.pending.as_ref() == Some(value) {
            self.pending = None;
        }
        self.persisted = Some(value.clone());
    }

    /// The newest value not yet written, if any.
    #[must_use]
    pub fn pending(&self) -> Option<&V> {
        self.pending.as_ref()
    }

    /// The value a flush is writing now, if any.
    #[must_use]
    pub fn in_flight(&self) -> Option<&V> {
        self.in_flight.as_ref()
    }

    /// The last value known to be persisted, once seeded.
    #[must_use]
    pub fn persisted(&self) -> Option<&V> {
        self.persisted.as_ref()
    }
}

/// The pending and persisted values of a coalescing settings writer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsWriteBatch<K: Eq + Clone, V: Eq + Clone> {
    keys: SmallMap<K, KeyWriteState<V>>,
    counters: SettingsWriteCounters,
}

impl<K: Eq + Clone, V: Eq + Clone> Default for SettingsWriteBatch<K, V> {
    fn default() -> Self {
        Self::with_capacity(0)
    }
}

impl<K: Eq + Clone, V: Eq + Clone> SettingsWriteBatch<K, V> {
    /// A batch sized for `keys` distinct keys (it grows past that if needed).
    #[must_use]
    pub fn with_capacity(keys: usize) -> Self {
        Self {
            keys: SmallMap::with_capacity(keys),
            counters: SettingsWriteCounters::default(),
        }
    }

    fn state(&mut self, key: K) -> &mut KeyWriteState<V> {
        self.keys.get_or_insert_with(key, KeyWriteState::default)
    }

    /// Record `value` as the newest value of `key`.
    ///
    /// `persisted_now` reads the backend's value; it is called only the first
    /// time a key is seen, to seed what is known to be persisted.
    pub fn record(&mut self, key: K, value: V, persisted_now: impl FnOnce() -> V) -> RecordOutcome {
        let outcome = self.state(key).record(value, persisted_now);
        match outcome {
            RecordOutcome::Pending => {
                self.counters.recorded = self.counters.recorded.saturating_add(1);
            }
            RecordOutcome::DroppedEqual => {
                self.counters.dropped_equal = self.counters.dropped_equal.saturating_add(1);
            }
        }
        outcome
    }

    /// Learn that the backend now holds `value` for `key` (an external change
    /// that was not this writer's flush): it becomes the persisted value, and a
    /// pending value equal to it is no longer pending.
    pub fn observe_persisted(&mut self, key: K, value: V) {
        self.state(key).observe_persisted(value);
    }

    /// Start a flush: every pending value becomes in flight, and is returned
    /// for the flush to write (in no particular order).
    pub fn begin_flush(&mut self) -> Vec<(K, V)> {
        let mut flushed = Vec::new();
        for (key, state) in self.keys.iter_mut() {
            if let Some(value) = state.begin_flush() {
                flushed.push((key.clone(), value));
            }
        }
        flushed
    }

    /// The values a flush would write now (in no particular order).
    #[must_use]
    pub fn snapshot(&self) -> Vec<(K, V)> {
        self.keys
            .iter()
            .filter_map(|(key, state)| state.pending().map(|value| (key.clone(), value.clone())))
            .collect()
    }

    /// A flush persisted `flushed`: each value becomes the persisted one, and
    /// leaves the pending set unless a newer value replaced it since the
    /// snapshot.
    pub fn acknowledge(&mut self, flushed: &[(K, V)]) {
        for (key, value) in flushed {
            self.state(key.clone()).acknowledge(value);
        }
        if !flushed.is_empty() {
            self.counters.flushes = self.counters.flushes.saturating_add(1);
            self.counters.flushed_values = self
                .counters
                .flushed_values
                .saturating_add(u64::try_from(flushed.len()).unwrap_or(u64::MAX));
        }
    }

    /// Whether nothing is pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending_len() == 0
    }

    /// How many keys are pending.
    #[must_use]
    pub fn pending_len(&self) -> usize {
        self.keys
            .iter()
            .filter(|(_, state)| state.pending().is_some())
            .count()
    }

    /// The pending value of `key`, if any.
    #[must_use]
    pub fn pending(&self, key: &K) -> Option<&V> {
        self.keys.get(key).and_then(KeyWriteState::pending)
    }

    /// The last value of `key` known to be persisted, once seeded.
    #[must_use]
    pub fn persisted(&self, key: &K) -> Option<&V> {
        self.keys.get(key).and_then(KeyWriteState::persisted)
    }

    /// The writer's counters.
    #[must_use]
    pub fn counters(&self) -> SettingsWriteCounters {
        self.counters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch() -> SettingsWriteBatch<&'static str, i32> {
        SettingsWriteBatch::default()
    }

    #[test]
    fn the_last_write_wins_and_one_snapshot_holds_it() {
        let mut batch = batch();
        assert_eq!(batch.record("width", 900, || 800), RecordOutcome::Pending);
        assert_eq!(
            batch.record("width", 950, || unreachable!()),
            RecordOutcome::Pending
        );
        assert_eq!(batch.snapshot(), vec![("width", 950)]);
        assert_eq!(batch.counters().recorded, 2);
    }

    #[test]
    fn a_value_equal_to_the_persisted_one_is_dropped_and_cancels_the_pending_one() {
        let mut batch = batch();
        assert_eq!(
            batch.record("visible", 1, || 1),
            RecordOutcome::DroppedEqual
        );
        assert!(batch.is_empty());
        assert_eq!(batch.record("visible", 0, || 1), RecordOutcome::Pending);
        assert_eq!(
            batch.record("visible", 1, || 1),
            RecordOutcome::DroppedEqual
        );
        assert!(batch.is_empty());
        assert_eq!(batch.counters().dropped_equal, 2);
    }

    #[test]
    fn acknowledging_keeps_a_value_written_after_the_snapshot() {
        let mut batch = batch();
        batch.record("width", 900, || 800);
        let flushed = batch.begin_flush();
        batch.record("width", 1000, || unreachable!());
        batch.acknowledge(&flushed);
        assert_eq!(batch.persisted(&"width"), Some(&900));
        assert_eq!(batch.snapshot(), vec![("width", 1000)]);
        let second = batch.begin_flush();
        batch.acknowledge(&second);
        assert!(batch.is_empty());
        assert_eq!(batch.counters().flushes, 2);
        assert_eq!(batch.counters().flushed_values, 2);
    }

    #[test]
    fn an_acknowledged_value_is_not_flushed_again() {
        let mut batch = batch();
        batch.record("width", 900, || 800);
        let flushed = batch.begin_flush();
        batch.acknowledge(&flushed);
        assert_eq!(
            batch.record("width", 900, || unreachable!()),
            RecordOutcome::DroppedEqual
        );
        assert!(batch.snapshot().is_empty());
    }

    #[test]
    fn writing_back_the_old_value_during_a_flush_is_kept() {
        let mut batch = batch();
        batch.record("width", 900, || 800);
        let flushed = batch.begin_flush();
        // The user returns to the old value before the flush is acknowledged.
        assert_eq!(
            batch.record("width", 800, || unreachable!()),
            RecordOutcome::Pending
        );
        batch.acknowledge(&flushed);
        assert_eq!(batch.snapshot(), vec![("width", 800)]);
    }

    #[test]
    fn an_empty_batch_flushes_nothing() {
        let mut batch = batch();
        assert!(batch.snapshot().is_empty());
        batch.acknowledge(&[]);
        assert_eq!(batch.counters(), SettingsWriteCounters::default());
    }

    #[test]
    fn an_external_change_becomes_the_persisted_value() {
        let mut batch = batch();
        batch.record("scheme", 2, || 0);
        batch.observe_persisted("scheme", 2);
        assert!(batch.is_empty());
        batch.observe_persisted("other", 5);
        assert_eq!(
            batch.record("other", 5, || unreachable!()),
            RecordOutcome::DroppedEqual
        );
    }
}
