// SPDX-License-Identifier: GPL-3.0-or-later

//! Main application window.
//!
//! The window is the top-level driving adapter for the application shell. Its
//! public API stays here, while document lifecycle, action wiring, notifications,
//! focus/indexing, and transient-surface dismissal live in dedicated modules to
//! keep the adapter readable.

mod actions;
mod adaptive_shell;
mod dialogs;
mod documents;
mod drafts;
mod encoding;
mod focus_indexing;
mod focus_mode;
// gtk-rs keeps the private GObject subclass implementation in `imp.rs`; this
// public module exposes the safe wrapper and workflow methods callers use.
mod imp;
pub(crate) mod local_history;
mod notes;
mod notifications;
mod preview;
mod print;
mod recent_open;
mod search;
mod session_restore;
mod startup_data;
mod tabs;
mod transient_surfaces;
mod workspace_scope;
mod zoom;

use crate::config::keys;
use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;

pub use drafts::DraftFlushError;

#[cfg(feature = "test-utils")]
pub use dialogs::set_close_safety_completion_delay_for_test;
#[cfg(feature = "test-utils")]
pub use documents::set_canonical_refresh_delay_for_test;
#[cfg(feature = "test-utils")]
pub use drafts::{
    fail_next_draft_mutations_for_test, set_automatic_draft_limit_for_test,
    set_draft_manifest_completion_delay_for_test, set_draft_mutation_delays_for_test,
    set_draft_restore_delay_for_test, set_first_dirty_autosave_delay_for_test,
    set_lazy_draft_read_delay_for_test, set_next_draft_body_disposal_probe_for_test,
    set_orphan_cleanup_delays_for_test,
};
#[cfg(feature = "test-utils")]
pub use encoding::set_lossy_encoding_analysis_delay_for_test;
pub use local_history::{
    LocalHistoryPreviewInstallEvidence, local_history_preview_install_evidence,
};
// The four evidence surfaces are re-exported together, behind one gate, so the
// role has one visibility rule instead of three. Widget tests are their only
// out-of-crate readers and they build with `test-utils`; field access through the
// accessor needs no import, so nothing on the production path depends on these
// names being reachable from outside the crate.
#[cfg(feature = "test-utils")]
pub use drafts::evidence::DraftEvidence;
#[cfg(feature = "test-utils")]
pub use local_history::LocalHistoryEvidence;
#[cfg(feature = "test-utils")]
pub use local_history::{
    set_local_history_baseline_delay_for_test, set_local_history_baseline_failures_for_test,
    set_local_history_preview_install_delay_for_test,
    set_local_history_preview_read_delay_for_test,
};
#[cfg(feature = "test-utils")]
pub use notes::{
    NotesBrowserRuntimeSnapshot, set_bookmark_excerpt_preview_delay_for_test,
    set_note_source_delay_for_test, set_notes_browser_query_delay_for_test,
    set_notes_browser_source_entry_limit_for_test,
};
#[cfg(feature = "test-utils")]
pub use print::{PrintDocumentSnapshot, PrintOutcome, with_print_runner_for_test};
#[cfg(feature = "test-utils")]
pub use search::set_replace_reload_facts_delay_for_test;
#[cfg(feature = "test-utils")]
pub use session_restore::evidence::SessionRestoreEvidence;

/// Map a GSettings `color-scheme` string to its `libadwaita::ColorScheme` variant.
/// Unknown values fall back to `Default` (follow system).
#[must_use]
pub fn parse_color_scheme(value: &str) -> libadwaita::ColorScheme {
    match value {
        "force-light" => libadwaita::ColorScheme::ForceLight,
        "force-dark" => libadwaita::ColorScheme::ForceDark,
        _ => libadwaita::ColorScheme::Default,
    }
}

// glib::wrapper! generates the public wrapper type for this widget.
// @extends declares the GTK class hierarchy; @implements lists interfaces.
glib::wrapper! {
    /// Top-level Libadwaita application window for one LushText shell.
    ///
    /// This GObject wrapper owns the GTK widget hierarchy and delegates document,
    /// workspace, command-palette, transient-surface, and persistence workflows
    /// to sibling window modules. Like all GTK widgets, it is main-thread only.
    pub struct LushtextWindow(ObjectSubclass<imp::LushtextWindow>)
        @extends libadwaita::ApplicationWindow, gtk4::ApplicationWindow, gtk4::Window, gtk4::Widget,
        @implements gio::ActionMap, gio::ActionGroup, gtk4::Accessible, gtk4::Buildable,
                    gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl LushtextWindow {
    /// Construct a fully wired application window for the given application.
    ///
    /// This runs on the GTK main thread, builds the composite template, installs
    /// window actions and surface dismissal, and starts notification rendering.
    /// Callers receive a ready-to-present shell; this constructor performs no
    /// disk writes.
    #[must_use]
    pub fn new(app: &libadwaita::Application) -> Self {
        let window: Self = Object::builder().property("application", app).build();
        window.setup_actions();
        window.setup_tab_management();
        window.setup_fullscreen();
        window.setup_focus_mode();
        window.setup_transient_surface_dismissal();
        window.setup_theme_selector();
        preview::setup_preview_actions(&window);
        print::setup_print_action(&window);
        zoom::setup_zoom_actions(&window);
        zoom::setup_zoom_controls(&window);
        search::setup_search_panel(&window);
        window.start_notification_sweep_timer();
        window.setup_shortcuts();
        window.load_recent_documents_async();
        window.update_content_stack();
        window.refresh_status_bar();
        window.render_notifications();
        window.begin_startup_data_flow();
        window
    }

    /// Return the compact secondary-surface owner as a stable automation label.
    #[must_use]
    pub(crate) fn compact_surface_label(&self) -> Option<&'static str> {
        match self.imp().secondary_surfaces.compact_surface.get() {
            Some(imp::SecondarySurface::Workspace) => Some("workspace"),
            Some(imp::SecondarySurface::DocumentProperties) => Some("document-properties"),
            None => None,
        }
    }

    /// Create the theme selector widget (follow-system/light/dark circles)
    /// matching GNOME Text Editor's visual pattern, and insert it into
    /// the hamburger menu's popover as a custom child.
    fn setup_theme_selector(&self) {
        let settings = &self.imp().settings;
        let style_manager = libadwaita::StyleManager::default();

        // Container with the CSS class that targets the custom styling.
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        container.add_css_class("theme-selector");
        container.set_hexpand(true);
        container.set_halign(gtk4::Align::Center);

        // GtkCheckButton radio group — CSS hides the radio indicator
        // and styles each button as a 44px colored circle.
        let follow_btn = gtk4::CheckButton::builder()
            .tooltip_text("Follow System Style")
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .focus_on_click(false)
            .build();
        follow_btn.add_css_class("follow");

        let light_btn = gtk4::CheckButton::builder()
            .tooltip_text("Light Style")
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .focus_on_click(false)
            .group(&follow_btn)
            .build();
        light_btn.add_css_class("light");

        let dark_btn = gtk4::CheckButton::builder()
            .tooltip_text("Dark Style")
            .halign(gtk4::Align::Center)
            .hexpand(true)
            .focus_on_click(false)
            .group(&follow_btn)
            .build();
        dark_btn.add_css_class("dark");

        container.append(&follow_btn);
        container.append(&light_btn);
        container.append(&dark_btn);

        let scheme = settings.string(keys::COLOR_SCHEME);
        match parse_color_scheme(scheme.as_str()) {
            libadwaita::ColorScheme::ForceLight => light_btn.set_active(true),
            libadwaita::ColorScheme::ForceDark => dark_btn.set_active(true),
            _ => follow_btn.set_active(true),
        }

        {
            let sm = style_manager.clone();
            let s = settings.clone();
            // GTK signals are observer callbacks; every radio toggle emits, so
            // persist only when this button becomes active.
            light_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    sm.set_color_scheme(libadwaita::ColorScheme::ForceLight);
                    let _ = s.set_string(keys::COLOR_SCHEME, "force-light");
                }
            });
        }
        {
            let sm = style_manager.clone();
            let s = settings.clone();
            follow_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    sm.set_color_scheme(libadwaita::ColorScheme::Default);
                    let _ = s.set_string(keys::COLOR_SCHEME, "default");
                }
            });
        }
        {
            let sm = style_manager;
            let s = settings.clone();
            dark_btn.connect_toggled(move |btn| {
                if btn.is_active() {
                    sm.set_color_scheme(libadwaita::ColorScheme::ForceDark);
                    let _ = s.set_string(keys::COLOR_SCHEME, "force-dark");
                }
            });
        }

        let menu_button = &self.imp().primary_menu_button;
        let Some(popover) = menu_button.popover() else {
            tracing::error!("setup_theme_selector: primary_menu_button has no popover");
            return;
        };
        // MenuButton exposes the generic popover base. The runtime-checked
        // GObject cast unlocks PopoverMenu APIs only for the expected template.
        let Ok(popover_menu) = popover.downcast::<gtk4::PopoverMenu>() else {
            tracing::error!("setup_theme_selector: popover is not a PopoverMenu");
            return;
        };
        if !popover_menu.add_child(&container, "theme") {
            tracing::error!(
                "setup_theme_selector: failed to add theme widget \
                 (missing 'theme' custom slot in menu XML?)"
            );
        }
    }
}
