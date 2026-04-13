# Tech Stack

## Language
- **Rust (Edition 2024, MSRV 1.94.1):** The core programming language for the application, ensuring performance and safety.

## GUI Framework
- **GTK4 (0.11):** The foundational toolkit for building the modern, touch-friendly graphical user interface.
- **Libadwaita (0.9):** Provides a set of Adwaita widgets and styles to deliver a first-class GNOME experience.
- **GtkSourceView 5 (0.11):** Used for advanced text editing features like syntax highlighting and line numbers.

## Configuration
- **GSettings (via gio):** Standard GNOME system for persisting user preferences and application state.

## Build and Packaging
- **Cargo Workspace:** Primary tool for managing Rust dependencies and development builds.
- **Meson:** Orchestrates the build process for installation and packaging.
- **Flatpak:** Target packaging format, ensuring consistent distribution on GNOME.