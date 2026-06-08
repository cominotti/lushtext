// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette service — fuzzy matching, file indexing, and command registry.
//!
//! This module stays GTK-free and fully unit-testable. The implementation is
//! split by workflow so file indexing, command registry maintenance, and fuzzy
//! scoring can evolve independently without one giant service file.

mod commands;
mod fuzzy;
mod index;

#[cfg(feature = "property-tests")]
pub use commands::merge_sorted_for_property_test;
pub use commands::{
    NoteCommandSection, all_commands, is_note_command, note_command_section, search_all,
    search_commands, search_non_note_commands, search_note_commands,
    search_note_commands_for_section, search_open_files,
};
pub use fuzzy::fuzzy_score;
pub use index::FileIndex;

#[cfg(test)]
mod tests;
