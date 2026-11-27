// SPDX-License-Identifier: GPL-3.0-or-later

//! Persistence driven adapters: durable writes, JSON store, drafts, sessions, history, and upgrades.

pub mod app_data_layout;
pub mod app_data_leftovers;
pub mod draft_service;
pub(crate) mod durable_write;
pub mod format_upgrade;
pub mod json_format;
pub mod json_store;
pub mod local_history_service;
pub mod migration_ledger;
pub mod recent_documents;
pub mod recovery_metadata;
pub mod session_service;
