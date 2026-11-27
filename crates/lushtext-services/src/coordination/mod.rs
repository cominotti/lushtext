// SPDX-License-Identifier: GPL-3.0-or-later

//! Concurrency coordination, protocol execution, locking, and window notifications.

pub mod notifications;
pub(crate) mod protocol_drive;
pub mod single_flight;
pub mod sync;
