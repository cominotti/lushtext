# Initial Concept
A fast, minimalist text editor for GNOME built with Rust, GTK4, and Libadwaita. Similar in spirit to GNOME Text Editor, but with a persistent workspace sidebar, an optional properties sidebar, and workspace support.

## Target Audience
- GNOME Desktop users seeking a lightweight, native, and fast text editor.
- Developers and writers who need a persistent workspace tree and basic syntax highlighting without the overhead of a full IDE.

## Core Features
- **Workspaces:** Named collections of root directories, persisted across sessions.
- **Dual Sidebars:** Persistent left workspace tree plus an optional right properties panel.
- **Robust Persistence:** Session persistence (tabs, cursor positions, scroll offsets) and draft recovery for crash resilience.
- **Advanced Editing:** Syntax highlighting via GtkSourceView, EditorConfig support, multi-file replace all, and find/replace functionalities.
- **Performance & Polish:** Fast command palette (SIMD-accelerated), graceful handling of large files, background buffer eviction to save memory, and dark mode support.