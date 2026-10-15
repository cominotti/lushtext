// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for the settings writer's batch algebra
//! (`model::settings_write_batch`).
//!
//! The Kani harness proves the per-key machine; these properties replay long
//! generated sequences over several keys through the whole batch (the lookup,
//! the multi-key flush, and the counters the harness leaves to tests) against
//! a model of the backend, and assert that after every operation each key's
//! newest value is either pending or what the backend holds, and that a flush
//! never carries a value the backend already holds.

use lushtext_core::model::settings_write_batch::SettingsWriteBatch;
use proptest::prelude::*;

/// Keys per sequence.
const KEYS: u8 = 4;
/// Values per key.
const VALUES: u8 = 4;

#[derive(Clone, Copy, Debug)]
enum Op {
    /// LushText writes `value` to `key`.
    Write { key: u8, value: u8 },
    /// Another process writes `value` to `key`.
    External { key: u8, value: u8 },
    /// A flush, optionally with a write landing before its acknowledgement.
    Flush { late_write: Option<(u8, u8)> },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..KEYS, 0..VALUES).prop_map(|(key, value)| Op::Write { key, value }),
        (0..KEYS, 0..VALUES).prop_map(|(key, value)| Op::External { key, value }),
        proptest::option::of((0..KEYS, 0..VALUES)).prop_map(|late_write| Op::Flush { late_write }),
    ]
}

proptest! {
    #[test]
    fn the_newest_value_is_pending_or_persisted_and_flushes_change_the_backend(
        initial in proptest::collection::vec(0..VALUES, KEYS as usize),
        ops in proptest::collection::vec(op(), 1..48),
    ) {
        let mut backend = initial;
        let mut latest: Vec<Option<u8>> = vec![None; KEYS as usize];
        let mut batch: SettingsWriteBatch<u8, u8> = SettingsWriteBatch::default();
        let write = |batch: &mut SettingsWriteBatch<u8, u8>,
                         backend: &[u8],
                         latest: &mut Vec<Option<u8>>,
                         key: u8,
                         value: u8| {
            let seed = backend[usize::from(key)];
            let _ = batch.record(key, value, || seed);
            latest[usize::from(key)] = Some(value);
        };
        for op in ops {
            match op {
                Op::Write { key, value } => write(&mut batch, &backend, &mut latest, key, value),
                Op::External { key, value } => {
                    backend[usize::from(key)] = value;
                    batch.observe_persisted(key, value);
                    if batch.pending(&key).is_none() {
                        latest[usize::from(key)] = Some(value);
                    }
                }
                Op::Flush { late_write } => {
                    let flushed = batch.begin_flush();
                    for (key, value) in &flushed {
                        prop_assert_ne!(backend[usize::from(*key)], *value);
                        backend[usize::from(*key)] = *value;
                    }
                    if let Some((key, value)) = late_write {
                        write(&mut batch, &backend, &mut latest, key, value);
                    }
                    batch.acknowledge(&flushed);
                }
            }
            for key in 0..KEYS {
                if let Some(newest) = latest[usize::from(key)] {
                    let pending = batch.pending(&key).copied();
                    prop_assert!(
                        pending == Some(newest)
                            || (pending.is_none() && backend[usize::from(key)] == newest),
                        "key {key}: newest {newest}, pending {pending:?}, backend {}",
                        backend[usize::from(key)]
                    );
                }
            }
        }
    }
}
