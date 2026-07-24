// SPDX-License-Identifier: GPL-3.0-or-later

//! Small cross-workflow synchronization helpers.

use std::sync::{Mutex, MutexGuard};

/// Lock a [`Mutex`], recovering the guard even if a prior panic poisoned it.
///
/// Poisoning means some thread panicked while holding the lock, so the
/// protected value *might* be left inconsistent. Every caller of this helper
/// guards state that is either rebuildable or whose worst-case post-panic
/// inconsistency is tolerable — bounded disposal-admission accounting and the
/// ordered-save generation map — so recovering the inner guard and continuing
/// is strictly better than a second panic. In particular, a poisoned
/// session-save ordering lock must not be allowed to turn a close-time session
/// save into a lost snapshot.
///
/// Do not use this to guard an invariant that a mid-panic writer could have
/// corrupted into an unsafe or silently wrong state; there, the poison is the
/// correct signal and the lock should stay poisoned.
pub(crate) fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
