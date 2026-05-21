// SPDX-License-Identifier: GPL-3.0-or-later

//! Built-in command registry and unified command-palette search entry points.
//!
//! This slice owns static command definitions and the merge logic that combines
//! command results with file-index matches.

use crate::model::palette::{
    CommandCategory, CommandDef, PaletteFileEntry, ScoredResult, SearchMode, SearchResultItem,
};

use super::fuzzy::search_items;
use super::index::FileIndex;

/// All built-in commands available in the palette.
#[must_use]
pub fn all_commands() -> &'static [CommandDef] {
    static COMMANDS: &[CommandDef] = &[
        CommandDef {
            id: "win.new-tab",
            label: "New File",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+T"),
        },
        CommandDef {
            id: "win.open-file",
            label: "Open File",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+O"),
        },
        CommandDef {
            id: "win.open-folder",
            label: "Open Folder",
            category: CommandCategory::File,
            shortcut: None,
        },
        CommandDef {
            id: "win.save",
            label: "Save",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+S"),
        },
        CommandDef {
            id: "win.save-as",
            label: "Save As",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+Shift+S"),
        },
        CommandDef {
            id: "win.show-local-history",
            label: "Local History",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+Alt+L"),
        },
        CommandDef {
            id: "win.print",
            label: "Print",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+P"),
        },
        CommandDef {
            id: "win.begin-search",
            label: "Find and Replace",
            category: CommandCategory::Edit,
            shortcut: Some("Ctrl+F"),
        },
        CommandDef {
            id: "win.toggle-bookmark",
            label: "Toggle Bookmark",
            category: CommandCategory::Edit,
            shortcut: Some("Ctrl+F2"),
        },
        CommandDef {
            id: "win.edit-bookmark-label",
            label: "Edit Bookmark Label",
            category: CommandCategory::Edit,
            shortcut: Some("Ctrl+Shift+F2"),
        },
        CommandDef {
            id: "win.next-bookmark",
            label: "Next Bookmark",
            category: CommandCategory::Edit,
            shortcut: Some("F2"),
        },
        CommandDef {
            id: "win.prev-bookmark",
            label: "Previous Bookmark",
            category: CommandCategory::Edit,
            shortcut: Some("Shift+F2"),
        },
        CommandDef {
            id: "win.add-annotation",
            label: "Add Range Note",
            category: CommandCategory::Edit,
            shortcut: Some("Ctrl+Alt+N"),
        },
        CommandDef {
            id: "win.edit-annotation",
            label: "Edit Range Note",
            category: CommandCategory::Edit,
            shortcut: Some("Ctrl+Alt+M"),
        },
        CommandDef {
            id: "win.open-document-note",
            label: "Open Document Note",
            category: CommandCategory::View,
            shortcut: None,
        },
        CommandDef {
            id: "win.open-workspace-note",
            label: "Open Workspace Note",
            category: CommandCategory::View,
            shortcut: None,
        },
        CommandDef {
            id: "win.close-tab",
            label: "Close Tab",
            category: CommandCategory::Edit,
            shortcut: Some("Ctrl+W"),
        },
        CommandDef {
            id: "win.show-bookmarks",
            label: "Browse Bookmarks",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+Alt+B"),
        },
        CommandDef {
            id: "win.show-annotations",
            label: "Browse Notes",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+Alt+A"),
        },
        CommandDef {
            id: "win.export-annotations",
            label: "Export Range Notes",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+Alt+Shift+A"),
        },
        CommandDef {
            id: "win.toggle-sidebar",
            label: "Toggle Sidebar",
            category: CommandCategory::View,
            shortcut: None,
        },
        CommandDef {
            id: "win.toggle-properties",
            label: "Document Properties",
            category: CommandCategory::View,
            shortcut: Some("F9"),
        },
        CommandDef {
            id: "win.toggle-fullscreen",
            label: "Fullscreen",
            category: CommandCategory::View,
            shortcut: Some("F11"),
        },
        CommandDef {
            id: "win.toggle-focus-mode",
            label: "Focus Mode",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+Shift+F11"),
        },
        CommandDef {
            id: "win.zoom-in",
            label: "Zoom In",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+="),
        },
        CommandDef {
            id: "win.zoom-out",
            label: "Zoom Out",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+-"),
        },
        CommandDef {
            id: "win.zoom-reset",
            label: "Reset Zoom",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+0"),
        },
        CommandDef {
            id: "win.show-help-overlay",
            label: "Keyboard Shortcuts",
            category: CommandCategory::View,
            shortcut: None,
        },
        CommandDef {
            id: "app.preferences",
            label: "Preferences",
            category: CommandCategory::App,
            shortcut: None,
        },
        CommandDef {
            id: "app.about",
            label: "About LushText",
            category: CommandCategory::App,
            shortcut: None,
        },
        CommandDef {
            id: "app.quit",
            label: "Quit",
            category: CommandCategory::App,
            shortcut: Some("Ctrl+Q"),
        },
    ];
    COMMANDS
}

/// Search the command registry with a fuzzy query.
pub fn search_commands(query: &str, max: usize) -> Vec<ScoredResult<'static>> {
    search_items(
        all_commands().iter(),
        |command| command.label,
        SearchResultItem::Command,
        query,
        max,
    )
}

/// Search open file-backed tab entries with the same fuzzy matcher as indexed files.
pub fn search_open_files<'a>(
    files: &'a [PaletteFileEntry],
    query: &str,
    max: usize,
) -> Vec<ScoredResult<'a>> {
    search_items(
        files.iter(),
        |file| file.display_name.as_str(),
        SearchResultItem::OpenFile,
        query,
        max,
    )
}

/// Search both files and commands according to the given mode.
#[must_use]
pub fn search_all<'a>(
    index: &'a FileIndex,
    query: &str,
    mode: SearchMode,
    max: usize,
) -> Vec<ScoredResult<'a>> {
    match mode {
        SearchMode::Files => index.search(query, max),
        SearchMode::Commands => search_commands(query, max),
        SearchMode::All => {
            let files = index.search(query, max);
            let commands = search_commands(query, max);
            merge_sorted(files, commands, max)
        }
    }
}

fn merge_sorted<'a>(
    a: Vec<ScoredResult<'a>>,
    b: Vec<ScoredResult<'a>>,
    max: usize,
) -> Vec<ScoredResult<'a>> {
    let mut result = Vec::with_capacity(max.min(a.len() + b.len()));
    let mut a = a.into_iter().peekable();
    let mut b = b.into_iter().peekable();
    while result.len() < max {
        match (a.peek(), b.peek()) {
            (Some(x), Some(y)) => {
                if x.score >= y.score {
                    result.push(
                        a.next()
                            .expect("peeked iterator entry should still be available"),
                    );
                } else {
                    result.push(
                        b.next()
                            .expect("peeked iterator entry should still be available"),
                    );
                }
            }
            (Some(_), None) => result.push(
                a.next()
                    .expect("peeked iterator entry should still be available"),
            ),
            (None, Some(_)) => result.push(
                b.next()
                    .expect("peeked iterator entry should still be available"),
            ),
            (None, None) => break,
        }
    }
    result
}
