// SPDX-License-Identifier: GPL-3.0-or-later

//! Application services: business logic and I/O operations.
//!
//! This layer sits between the domain model (`model/`) and the UI (`ui/`).
//! All services are GTK-free and fully unit-testable. Includes workspace
//! management, session persistence, file tree scanning, editor file I/O,
//! file size policy, fuzzy search, and the background task concurrency guard.

pub mod async_task;
pub mod content_search;
pub mod draft_service;
pub mod editor_io;
pub mod editorconfig;
pub mod file_limits;
pub mod file_tree;
pub mod json_store;
pub mod notifications;
pub mod palette;
pub mod saved_searches;
pub mod search_backup;
pub mod search_history;
pub mod session_service;
pub mod workspace_manager;
