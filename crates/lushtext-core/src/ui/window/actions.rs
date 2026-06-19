// SPDX-License-Identifier: GPL-3.0-or-later

//! Window action and shortcut wiring.

use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::value::ToValue;
use gtk4::prelude::*;
use gtk4::{gio, glib};

use crate::config::{self, keys};
use crate::ui::accessibility::{self, AnnouncementLane};
use crate::ui::editor_page::BookmarkNavigationDirection;

use super::{LushtextWindow, imp};

impl LushtextWindow {
    pub(super) fn setup_actions(&self) {
        self.add_action_entries([
            gio::ActionEntry::builder("new-tab")
                .activate(|window: &Self, _, _| {
                    window.new_tab();
                    window.focus_selected_editor_after_action();
                })
                .build(),
            gio::ActionEntry::builder("open-file")
                .activate(|window: &Self, _, _| window.show_open_file_dialog())
                .build(),
            gio::ActionEntry::builder("open-recent")
                .activate(|window: &Self, _, _| window.open_recent_popover())
                .build(),
            gio::ActionEntry::builder("set-open-popover-query")
                .parameter_type(Some(glib::VariantTy::STRING))
                .activate(|window: &Self, _, parameter| {
                    let Some(query) = parameter.and_then(glib::Variant::get::<String>) else {
                        tracing::error!("set-open-popover-query: expected string parameter");
                        return;
                    };
                    window.set_open_popover_query(&query);
                })
                .build(),
            gio::ActionEntry::builder("open-folder")
                .activate(|window: &Self, _, _| {
                    window.imp().sidebar.create_new_workspace();
                })
                .build(),
            gio::ActionEntry::builder("show-help-overlay")
                .activate(|window: &Self, _, _| window.show_help_overlay())
                .build(),
            gio::ActionEntry::builder("save")
                .activate(|window: &Self, _, _| window.save_current())
                .build(),
            gio::ActionEntry::builder("save-as")
                .activate(|window: &Self, _, _| window.show_save_as_dialog())
                .build(),
            gio::ActionEntry::builder("show-local-history")
                .activate(|window: &Self, _, _| window.show_local_history_dialog())
                .build(),
            gio::ActionEntry::builder("show-encoding-controls")
                .activate(|window: &Self, _, _| window.show_encoding_controls_dialog())
                .build(),
            gio::ActionEntry::builder("show-line-ending-controls")
                .activate(|window: &Self, _, _| window.show_line_ending_controls_dialog())
                .build(),
            gio::ActionEntry::builder("show-file-health")
                .activate(|window: &Self, _, _| window.show_file_health_dialog())
                .build(),
            gio::ActionEntry::builder("cycle-invisible-characters")
                .activate(|window: &Self, _, _| window.cycle_invisible_characters())
                .build(),
            gio::ActionEntry::builder("begin-search")
                .activate(|window: &Self, _, _| window.open_editor_search(false))
                .build(),
            gio::ActionEntry::builder("set-search-query")
                .parameter_type(Some(glib::VariantTy::STRING))
                .activate(|window: &Self, _, parameter| {
                    let Some(query) = parameter.and_then(glib::Variant::get::<String>) else {
                        tracing::error!("set-search-query: expected string parameter");
                        return;
                    };
                    window.set_editor_search_query(&query);
                })
                .build(),
            gio::ActionEntry::builder("begin-replace")
                .activate(|window: &Self, _, _| window.open_editor_search(true))
                .build(),
            gio::ActionEntry::builder("next-match")
                .activate(|window: &Self, _, _| {
                    if let Some(editor) = window.active_editor()
                        && editor.is_search_visible()
                    {
                        editor.search_bar().move_next();
                    }
                })
                .build(),
            gio::ActionEntry::builder("prev-match")
                .activate(|window: &Self, _, _| {
                    if let Some(editor) = window.active_editor()
                        && editor.is_search_visible()
                    {
                        editor.search_bar().move_prev();
                    }
                })
                .build(),
            gio::ActionEntry::builder("close-tab")
                .activate(|window: &Self, _, _| {
                    let tab_view = &window.imp().tab_view;
                    if let Some(page) = tab_view.selected_page() {
                        tab_view.close_page(&page);
                    }
                })
                .build(),
            gio::ActionEntry::builder("select-tab")
                .parameter_type(Some(glib::VariantTy::UINT32))
                .activate(|window: &Self, _, parameter| {
                    let Some(index) = parameter.and_then(glib::Variant::get::<u32>) else {
                        tracing::error!("select-tab: expected uint32 parameter");
                        return;
                    };
                    window.select_tab_by_index(index);
                })
                .build(),
            gio::ActionEntry::builder("toggle-command-palette")
                .activate(|window: &Self, _, _| window.toggle_command_palette())
                .build(),
            gio::ActionEntry::builder("set-command-palette-query")
                .parameter_type(Some(glib::VariantTy::STRING))
                .activate(|window: &Self, _, parameter| {
                    let Some(query) = parameter.and_then(glib::Variant::get::<String>) else {
                        tracing::error!("set-command-palette-query: expected string parameter");
                        return;
                    };
                    window.set_command_palette_query(&query);
                })
                .build(),
            gio::ActionEntry::builder("set-command-palette-mode")
                .parameter_type(Some(glib::VariantTy::STRING))
                .activate(|window: &Self, _, parameter| {
                    let Some(mode) = parameter.and_then(glib::Variant::get::<String>) else {
                        tracing::error!("set-command-palette-mode: expected string parameter");
                        return;
                    };
                    window.set_command_palette_mode(&mode);
                })
                .build(),
            gio::ActionEntry::builder("toggle-search-panel")
                .activate(|window: &Self, _, _| window.toggle_search_panel())
                .build(),
            // Target-state actions give automation and smoke tests idempotent
            // commands while routing through the same visible workflows as
            // menus, shortcuts, and toggle buttons.
            gio::ActionEntry::builder("set-sidebar-visible")
                .parameter_type(Some(glib::VariantTy::BOOLEAN))
                .activate(|window: &Self, _, parameter| {
                    if let Some(visible) =
                        boolean_action_parameter("set-sidebar-visible", parameter)
                    {
                        window.change_boolean_action_state("toggle-sidebar", visible);
                    }
                })
                .build(),
            gio::ActionEntry::builder("focus-workspace-tree")
                .activate(|window: &Self, _, _| window.focus_workspace_tree())
                .build(),
            gio::ActionEntry::builder("focus-workspace-header")
                .activate(|window: &Self, _, _| window.focus_workspace_header())
                .build(),
            gio::ActionEntry::builder("show-workspace-tree-context-menu")
                .activate(|window: &Self, _, _| window.show_workspace_tree_context_menu())
                .build(),
            gio::ActionEntry::builder("show-workspace-header-context-menu")
                .activate(|window: &Self, _, _| window.show_workspace_header_context_menu())
                .build(),
            gio::ActionEntry::builder("set-properties-visible")
                .parameter_type(Some(glib::VariantTy::BOOLEAN))
                .activate(|window: &Self, _, parameter| {
                    if let Some(visible) =
                        boolean_action_parameter("set-properties-visible", parameter)
                    {
                        window.change_boolean_action_state("toggle-properties", visible);
                    }
                })
                .build(),
            gio::ActionEntry::builder("set-minimap-visible")
                .parameter_type(Some(glib::VariantTy::BOOLEAN))
                .activate(|window: &Self, _, parameter| {
                    if let Some(visible) =
                        boolean_action_parameter("set-minimap-visible", parameter)
                    {
                        window.change_boolean_action_state("toggle-minimap", visible);
                    }
                })
                .build(),
            gio::ActionEntry::builder("set-search-panel-visible")
                .parameter_type(Some(glib::VariantTy::BOOLEAN))
                .activate(|window: &Self, _, parameter| {
                    if let Some(visible) =
                        boolean_action_parameter("set-search-panel-visible", parameter)
                    {
                        window.set_search_panel_visible(visible);
                    }
                })
                .build(),
            gio::ActionEntry::builder("set-search-panel-query")
                .parameter_type(Some(glib::VariantTy::STRING))
                .activate(|window: &Self, _, parameter| {
                    let Some(query) = parameter.and_then(glib::Variant::get::<String>) else {
                        tracing::error!("set-search-panel-query: expected string parameter");
                        return;
                    };
                    window.set_search_panel_query(&query);
                })
                .build(),
            gio::ActionEntry::builder("set-search-panel-replace-query")
                .parameter_type(Some(glib::VariantTy::STRING))
                .activate(|window: &Self, _, parameter| {
                    let Some(text) = parameter.and_then(glib::Variant::get::<String>) else {
                        tracing::error!(
                            "set-search-panel-replace-query: expected string parameter"
                        );
                        return;
                    };
                    window.set_search_panel_replace_query(&text);
                })
                .build(),
            gio::ActionEntry::builder("preview-search-panel-replacements")
                .activate(|window: &Self, _, _| window.preview_search_panel_replacements())
                .build(),
            gio::ActionEntry::builder("confirm-search-panel-replacements")
                .activate(|window: &Self, _, _| window.confirm_search_panel_replacements())
                .build(),
            gio::ActionEntry::builder("undo-search-panel-replacements")
                .activate(|window: &Self, _, _| window.undo_search_panel_replacements())
                .build(),
            gio::ActionEntry::builder("search-next-match")
                .activate(|window: &Self, _, _| {
                    window.imp().search_panel.navigate_next_match();
                })
                .build(),
            gio::ActionEntry::builder("search-prev-match")
                .activate(|window: &Self, _, _| {
                    window.imp().search_panel.navigate_prev_match();
                })
                .build(),
            gio::ActionEntry::builder("toggle-bookmark")
                .activate(|window: &Self, _, _| window.toggle_bookmark())
                .build(),
            // Menu-scoped note actions stay separate from the existing
            // shortcuts/command-palette actions so the Notes menu can become
            // insensitive without disabling those other invocation surfaces.
            gio::ActionEntry::builder("notes-toggle-bookmark")
                .activate(|window: &Self, _, _| window.toggle_bookmark())
                .build(),
            gio::ActionEntry::builder("edit-bookmark-label")
                .activate(|window: &Self, _, _| window.edit_bookmark())
                .build(),
            gio::ActionEntry::builder("next-bookmark")
                .activate(|window: &Self, _, _| {
                    window.navigate_bookmark_action(BookmarkNavigationDirection::Next);
                })
                .build(),
            gio::ActionEntry::builder("prev-bookmark")
                .activate(|window: &Self, _, _| {
                    window.navigate_bookmark_action(BookmarkNavigationDirection::Previous);
                })
                .build(),
            gio::ActionEntry::builder("show-bookmarks")
                .activate(|window: &Self, _, _| window.show_bookmarks_dialog())
                .build(),
            gio::ActionEntry::builder("open-document-note")
                .activate(|window: &Self, _, _| window.open_document_note())
                .build(),
            gio::ActionEntry::builder("notes-open-document-note")
                .activate(|window: &Self, _, _| window.open_document_note())
                .build(),
            gio::ActionEntry::builder("open-folder-note")
                .activate(|window: &Self, _, _| window.open_folder_note())
                .build(),
            gio::ActionEntry::builder("notes-open-folder-note")
                .activate(|window: &Self, _, _| window.open_folder_note())
                .build(),
            gio::ActionEntry::builder("show-notes")
                .activate(|window: &Self, _, _| window.show_notes_dialog())
                .build(),
            gio::ActionEntry::builder("notes-show-notes")
                .activate(|window: &Self, _, _| window.show_notes_dialog())
                .build(),
            gio::ActionEntry::builder("set-notes-browser-query")
                .parameter_type(Some(glib::VariantTy::STRING))
                .activate(|window: &Self, _, parameter| {
                    let Some(query) = parameter.and_then(glib::Variant::get::<String>) else {
                        tracing::error!("set-notes-browser-query: expected string parameter");
                        return;
                    };
                    window.set_notes_browser_query(&query);
                })
                .build(),
            gio::ActionEntry::builder("select-notes-browser-row")
                .parameter_type(Some(glib::VariantTy::UINT32))
                .activate(|window: &Self, _, parameter| {
                    let Some(index) = parameter.and_then(glib::Variant::get::<u32>) else {
                        tracing::error!("select-notes-browser-row: expected uint32 parameter");
                        return;
                    };
                    window.select_notes_browser_row(index);
                })
                .build(),
            gio::ActionEntry::builder("open-notes-browser-selection")
                .activate(|window: &Self, _, _| window.open_notes_browser_selection())
                .build(),
        ]);
        self.set_open_popover_actions_enabled(false);
        self.set_notes_browser_actions_enabled(false);
        self.set_command_palette_actions_enabled(false);
        self.set_search_panel_actions_enabled(false);

        let discard_action = gio::SimpleAction::new("discard-changes", None);
        discard_action.set_enabled(false);
        {
            let window_weak = self.downgrade();
            discard_action.connect_activate(move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    window.discard_changes();
                }
            });
        }
        self.add_action(&discard_action);

        self.register_secondary_surface_toggle_action(
            "toggle-sidebar",
            self.rendered_workspace_sidebar_visible(),
            Self::set_workspace_sidebar_requested_visible,
        );
        self.register_secondary_surface_toggle_action(
            "toggle-properties",
            self.rendered_document_properties_visible(),
            Self::set_document_properties_requested_visible,
        );
        self.register_boolean_setting_toggle_action("toggle-minimap", keys::SHOW_MINIMAP);
    }

    fn change_boolean_action_state(&self, action_name: &str, desired: bool) {
        let Some(action) = self.lookup_action(action_name) else {
            return;
        };
        if action
            .state()
            .and_then(|state| state.get::<bool>())
            .is_some_and(|current| current == desired)
        {
            return;
        }
        action.change_state(&desired.to_variant());
    }

    fn set_search_panel_visible(&self, visible: bool) {
        let currently_visible = self.imp().search_panel_revealer.reveals_child();
        if visible == currently_visible {
            return;
        }
        if visible {
            self.toggle_search_panel();
        } else {
            self.close_search_panel();
        }
    }

    /// Present the shipped keyboard-shortcuts window through the normal action path.
    ///
    /// GTK keeps `GtkShortcutsWindow` as a top-level transient window rather
    /// than a child widget. Looking up an existing transient first makes
    /// repeated menu, palette, or D-Bus activation focus the same surface
    /// instead of leaking duplicate help windows.
    fn show_help_overlay(&self) {
        if let Some(shortcuts) = self.existing_shortcuts_window() {
            shortcuts.present();
            return;
        }

        let builder = gtk4::Builder::from_resource(&format!(
            "{}/ui/shortcuts.ui",
            config::RESOURCE_BASE_PATH
        ));
        let shortcuts = builder
            .object::<gtk4::Window>("help_overlay")
            .expect("shortcuts.ui should define GtkShortcutsWindow#help_overlay");
        shortcuts.set_transient_for(Some(self));
        shortcuts.set_destroy_with_parent(true);
        if let Some(application) = self.application() {
            shortcuts.set_application(Some(&application));
        }
        shortcuts.present();
    }

    fn existing_shortcuts_window(&self) -> Option<gtk4::Window> {
        let this_window: gtk4::Window = self.clone().upcast();
        self.application()?
            .windows()
            .into_iter()
            .filter(|window| window.type_().name() == "GtkShortcutsWindow")
            .find(|shortcuts| {
                shortcuts
                    .transient_for()
                    .is_some_and(|parent| parent == this_window)
            })
    }

    /// Select an existing tab by its visible zero-based index for automation
    /// and smoke helpers that should not depend on tab-strip coordinates.
    fn select_tab_by_index(&self, index: u32) {
        let Ok(index) = i32::try_from(index) else {
            tracing::warn!("select-tab: index is too large for GTK tab positions");
            return;
        };
        let tab_view = &self.imp().tab_view;
        if index >= tab_view.n_pages() {
            tracing::warn!("select-tab: index {index} is outside the open tab range");
            return;
        }

        let page = tab_view.nth_page(index);
        tab_view.set_selected_page(&page);
        self.focus_selected_editor_after_action();
        self.refresh_status_bar();
        self.save_session_debounced();
    }

    fn register_secondary_surface_toggle_action(
        &self,
        action_name: &'static str,
        initial_state: bool,
        apply: fn(&Self, bool),
    ) {
        let action =
            gio::SimpleAction::new_stateful(action_name, None, &initial_state.to_variant());
        {
            action.connect_activate(move |action, _| {
                let current = action
                    .state()
                    .and_then(|state| state.get::<bool>())
                    .unwrap_or(false);
                action.change_state(&(!current).to_variant());
            });
        }
        {
            let window_weak = self.downgrade();
            action.connect_change_state(move |_action, state| {
                let Some(state) = state else { return };
                let Some(new_visible) = state.get::<bool>() else {
                    tracing::error!("{action_name}: expected bool state");
                    return;
                };
                if let Some(window) = window_weak.upgrade() {
                    apply(&window, new_visible);
                }
            });
        }
        self.add_action(&action);
    }

    /// Persist the user's explicit workspace-sidebar preference, then let the
    /// adaptive shell decide how that preference is rendered right now.
    fn set_workspace_sidebar_requested_visible(&self, visible: bool) {
        if visible != self.rendered_workspace_sidebar_visible() {
            self.protect_active_minimap_for_shell_width_transition();
        }
        let state = &self.imp().secondary_surfaces;
        state.workspace_requested_visible.set(visible);
        if self.document_properties_uses_bottom_sheet() {
            if visible {
                state
                    .compact_surface
                    .set(Some(imp::SecondarySurface::Workspace));
            } else if state.compact_surface.get() == Some(imp::SecondarySurface::Workspace) {
                state.compact_surface.set(None);
            }
        }
        let _ = self
            .imp()
            .settings
            .set_boolean(keys::WORKSPACE_SIDEBAR_VISIBLE, visible);
        self.set_toggle_action_state("toggle-sidebar", visible);
        self.imp()
            .status_bar
            .set_workspace_sidebar_toggle_pressed(visible);
        self.announce_workflow_update(
            AnnouncementLane::StatusUpdate,
            if visible {
                "workspace-sidebar-shown"
            } else {
                "workspace-sidebar-hidden"
            },
            if visible {
                "Workspace sidebar shown"
            } else {
                "Workspace sidebar hidden"
            },
        );
        self.start_workspace_sidebar_transition();
    }

    /// Move keyboard focus into the first visible workspace file tree.
    ///
    /// This keeps sidebar context-menu shortcuts reachable for keyboard and
    /// assistive-technology users even when AT-SPI cannot focus recycled list
    /// rows directly.
    pub(crate) fn focus_workspace_tree(&self) {
        self.change_boolean_action_state("toggle-sidebar", true);
        if !self.imp().sidebar.focus_first_visible_file_tree() {
            tracing::warn!("focus-workspace-tree: no visible workspace tree accepted focus");
        }
    }

    /// Move keyboard focus to the first visible workspace header control.
    ///
    /// Header menu shortcuts are handled by the header container, but focus
    /// lives on its collapse button so key events can bubble through GTK.
    pub(crate) fn focus_workspace_header(&self) {
        self.change_boolean_action_state("toggle-sidebar", true);
        if !self.imp().sidebar.focus_first_visible_header_controls() {
            tracing::warn!("focus-workspace-header: no visible workspace header accepted focus");
        }
    }

    /// Open the selected workspace file-tree row's context menu.
    ///
    /// This is a normal GTK action path for automation and smoke proof; it uses
    /// the same section menu state as pointer and keyboard activation.
    pub(crate) fn show_workspace_tree_context_menu(&self) {
        self.change_boolean_action_state("toggle-sidebar", true);
        if !self
            .imp()
            .sidebar
            .show_first_visible_file_tree_context_menu()
        {
            tracing::warn!(
                "show-workspace-tree-context-menu: no selected visible workspace row had a menu"
            );
        }
    }

    /// Open the first visible workspace header context menu.
    pub(crate) fn show_workspace_header_context_menu(&self) {
        self.change_boolean_action_state("toggle-sidebar", true);
        if !self.imp().sidebar.show_first_visible_header_context_menu() {
            tracing::warn!(
                "show-workspace-header-context-menu: no visible workspace header had a menu"
            );
        }
    }

    /// Persist the user's explicit document-properties preference, then let the
    /// adaptive shell render it as a side pane or bottom sheet as needed.
    fn set_document_properties_requested_visible(&self, visible: bool) {
        if visible != self.rendered_document_properties_visible() {
            self.protect_active_minimap_for_shell_width_transition();
        }
        let state = &self.imp().secondary_surfaces;
        state.properties_requested_visible.set(visible);
        if self.document_properties_uses_bottom_sheet() {
            if visible {
                state
                    .compact_surface
                    .set(Some(imp::SecondarySurface::DocumentProperties));
            } else if state.compact_surface.get() == Some(imp::SecondarySurface::DocumentProperties)
            {
                state.compact_surface.set(None);
            }
        }
        let _ = self
            .imp()
            .settings
            .set_boolean(keys::PROPERTIES_SIDEBAR_VISIBLE, visible);
        self.set_toggle_action_state("toggle-properties", visible);
        if self.imp().document_properties_toggle_button.is_active() != visible {
            self.imp()
                .document_properties_toggle_button
                .set_active(visible);
        }
        accessibility::set_pressed(&*self.imp().document_properties_toggle_button, visible);
        self.announce_workflow_update(
            AnnouncementLane::StatusUpdate,
            if visible {
                "document-properties-shown"
            } else {
                "document-properties-hidden"
            },
            if visible {
                "Document properties shown"
            } else {
                "Document properties hidden"
            },
        );
        self.sync_secondary_surface_layout();
    }

    /// Prime the active minimap before a shell transition starts consuming width.
    ///
    /// Adjustment page-size changes are the passive reflow signal once the
    /// animation is already moving, but the first adjustment callback can only
    /// snapshot the source map after GTK has advanced its width. Shell actions
    /// know the transition is about to start, so they capture the settled
    /// native map pixels one callback earlier and let the adjustment observers
    /// extend the same settle burst through the animation.
    fn protect_active_minimap_for_shell_width_transition(&self) {
        if let Some(editor) = self.active_editor()
            && editor.is_minimap_visible()
        {
            editor.schedule_minimap_reflow_settle_with_freeze();
        }
    }

    /// Update the rendered on/off state that powers both toggle buttons and
    /// any other surfaces bound to the same stateful window actions.
    pub(super) fn sync_secondary_surface_action_states(&self) {
        let sidebar_visible = if self.is_focus_mode_active() {
            self.workspace_sidebar_requested_visible()
        } else {
            self.rendered_workspace_sidebar_visible()
        };
        self.set_toggle_action_state("toggle-sidebar", sidebar_visible);
        self.imp()
            .status_bar
            .set_workspace_sidebar_toggle_pressed(sidebar_visible);

        let properties_visible = if self.is_focus_mode_active() {
            self.document_properties_requested_visible()
        } else {
            self.rendered_document_properties_visible()
        };
        self.set_toggle_action_state("toggle-properties", properties_visible);
        if self.imp().document_properties_toggle_button.is_active() != properties_visible {
            self.imp()
                .document_properties_toggle_button
                .set_active(properties_visible);
        }
        accessibility::set_pressed(
            &*self.imp().document_properties_toggle_button,
            properties_visible,
        );
    }

    fn set_toggle_action_state(&self, action_name: &str, visible: bool) {
        let Some(action) = self.lookup_action(action_name) else {
            return;
        };
        let Some(action) = action.downcast_ref::<gio::SimpleAction>() else {
            return;
        };
        let current = action
            .state()
            .and_then(|state| state.get::<bool>())
            .unwrap_or(!visible);
        if current != visible {
            action.set_state(&visible.to_variant());
        }
    }

    fn register_boolean_setting_toggle_action(
        &self,
        action_name: &'static str,
        settings_key: &'static str,
    ) {
        let initial = self.imp().settings.boolean(settings_key);
        let action = gio::SimpleAction::new_stateful(action_name, None, &initial.to_variant());

        {
            let settings = self.imp().settings.clone();
            action.connect_activate(move |action, _| {
                let current = action
                    .state()
                    .and_then(|state| state.get::<bool>())
                    .unwrap_or(false);
                action.change_state(&(!current).to_variant());
            });
            action.connect_change_state(move |action, state| {
                let Some(state) = state else { return };
                let Some(enabled) = state.get::<bool>() else {
                    tracing::error!("{action_name}: expected bool state");
                    return;
                };
                action.set_state(&enabled.to_variant());
                let _ = settings.set_boolean(settings_key, enabled);
            });
        }

        let action_weak = action.downgrade();
        let window_weak = self.downgrade();
        self.imp()
            .settings
            .connect_changed(Some(settings_key), move |s, _| {
                let enabled = s.boolean(settings_key);
                if let Some(action) = action_weak.upgrade() {
                    action.set_state(&enabled.to_variant());
                }
                if settings_key == keys::SHOW_MINIMAP
                    && let Some(window) = window_weak.upgrade()
                {
                    window.announce_workflow_update(
                        AnnouncementLane::StatusUpdate,
                        if enabled {
                            "minimap-shown"
                        } else {
                            "minimap-hidden"
                        },
                        if enabled {
                            "Minimap shown"
                        } else {
                            "Minimap hidden"
                        },
                    );
                }
            });

        self.add_action(&action);
    }

    pub(super) fn setup_shortcuts(&self) {
        let controller = gtk4::ShortcutController::new();
        controller.set_scope(gtk4::ShortcutScope::Managed);

        let shortcuts = [
            ("win.new-tab", "<Control>n"),
            ("win.open-file", "<Control>o"),
            ("win.open-recent", "<Control>k"),
            ("win.save", "<Control>s"),
            ("win.save-as", "<Control><Shift>s"),
            ("win.show-local-history", "<Control><Alt>l"),
            ("win.cycle-invisible-characters", "<Control><Shift>i"),
            ("win.begin-search", "<Control>f"),
            ("win.begin-replace", "<Control>h"),
            ("win.next-match", "<Control>g"),
            ("win.prev-match", "<Control><Shift>g"),
            ("win.close-tab", "<Control>w"),
            ("win.print", "<Control>p"),
            ("win.toggle-command-palette", "<Control><Shift>p"),
            ("win.toggle-minimap", "<Control><Shift>m"),
            ("win.toggle-search-panel", "<Control><Shift>f"),
            ("win.search-next-match", "F4"),
            ("win.search-prev-match", "<Shift>F4"),
            ("win.toggle-bookmark", "<Control>F2"),
            ("win.edit-bookmark-label", "<Control><Shift>F2"),
            ("win.next-bookmark", "F2"),
            ("win.prev-bookmark", "<Shift>F2"),
            ("win.show-bookmarks", "<Control><Alt>b"),
            ("win.show-notes", "<Control><Alt>a"),
            ("win.toggle-properties", "F9"),
            ("win.toggle-focus-mode", "<Control><Shift>F11"),
            ("win.toggle-preview-mode", "<Alt>p"),
            ("win.toggle-fullscreen", "F11"),
            (
                "win.zoom-in",
                "<Control>equal|<Control>plus|<Control>KP_Add",
            ),
            ("win.zoom-out", "<Control>minus|<Control>KP_Subtract"),
            ("win.zoom-reset", "<Control>0|<Control>KP_0"),
        ];

        for (action, accel) in shortcuts {
            controller.add_shortcut(gtk4::Shortcut::new(
                gtk4::ShortcutTrigger::parse_string(accel),
                Some(gtk4::NamedAction::new(action)),
            ));
        }

        self.add_controller(controller);
    }

    /// Register fullscreen/unfullscreen/toggle-fullscreen actions and wire
    /// the `fullscreened` property to toggle which menu item is visible.
    pub(super) fn setup_fullscreen(&self) {
        let fullscreen_action = gio::SimpleAction::new("fullscreen", None);
        let unfullscreen_action = gio::SimpleAction::new("unfullscreen", None);
        unfullscreen_action.set_enabled(false);

        {
            let window_weak = self.downgrade();
            fullscreen_action.connect_activate(move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    window.fullscreen();
                }
            });
        }
        {
            let window_weak = self.downgrade();
            unfullscreen_action.connect_activate(move |_, _| {
                if let Some(window) = window_weak.upgrade() {
                    window.unfullscreen();
                }
            });
        }

        self.add_action(&fullscreen_action);
        self.add_action(&unfullscreen_action);

        self.add_action_entries([gio::ActionEntry::builder("toggle-fullscreen")
            .activate(|window: &Self, _, _| {
                if window.is_fullscreen() {
                    window.unfullscreen();
                } else {
                    window.fullscreen();
                }
            })
            .build()]);

        // Action enabled state follows the window's fullscreen property:
        // Fullscreen uses the inverse transform, Unfullscreen mirrors it
        // directly, and `sync_create()` seeds the initial action state. GLib
        // releases each binding when either bound object is finalized.
        self.bind_property("fullscreened", &fullscreen_action, "enabled")
            .transform_to(|_: &glib::Binding, fullscreened: &glib::Value| {
                let fullscreened = fullscreened.get::<bool>().ok()?;
                Some((!fullscreened).to_value())
            })
            .sync_create()
            .build();
        self.bind_property("fullscreened", &unfullscreen_action, "enabled")
            .sync_create()
            .build();
    }

    /// Open the in-editor find or replace bar, closing the workspace panel first if needed.
    fn open_editor_search(&self, show_replace: bool) {
        if self.imp().search_panel_revealer.reveals_child() {
            self.close_search_panel();
            self.after_search_panel_transition(move |window| {
                if let Some(editor) = window.active_editor() {
                    if show_replace {
                        editor.show_replace();
                    } else {
                        editor.show_search();
                    }
                }
            });
        } else if let Some(editor) = self.active_editor() {
            if show_replace {
                editor.show_replace();
            } else {
                editor.show_search();
            }
        }
    }

    /// Open or update the active editor search bar through the visible workflow.
    ///
    /// Automation uses this instead of mutating `SearchSettings` directly so
    /// focus, minimap markers, match counts, and close behavior stay identical
    /// to typing in the search entry after pressing Ctrl+F.
    fn set_editor_search_query(&self, query: &str) {
        if self.imp().search_panel_revealer.reveals_child() {
            let query = query.to_owned();
            self.close_search_panel();
            self.after_search_panel_transition(move |window| {
                if let Some(editor) = window.active_editor() {
                    editor.show_search();
                    editor.search_bar().search_entry().set_text(&query);
                }
            });
        } else if let Some(editor) = self.active_editor() {
            editor.show_search();
            editor.search_bar().search_entry().set_text(query);
        }
    }
}

fn boolean_action_parameter(action_name: &str, parameter: Option<&glib::Variant>) -> Option<bool> {
    let value = parameter.and_then(glib::Variant::get::<bool>);
    if value.is_none() {
        tracing::error!("{action_name}: expected bool parameter");
    }
    value
}
