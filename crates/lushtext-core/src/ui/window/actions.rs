// SPDX-License-Identifier: GPL-3.0-or-later

//! Window action and shortcut wiring.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;

use crate::config::keys;
use crate::ui::editor_page::BookmarkNavigationDirection;

use super::{LushtextWindow, imp};

impl LushtextWindow {
    pub(super) fn setup_actions(&self) {
        self.add_action_entries([
            gio::ActionEntry::builder("new-tab")
                .activate(|window: &Self, _, _| window.new_tab())
                .build(),
            gio::ActionEntry::builder("open-file")
                .activate(|window: &Self, _, _| window.show_open_file_dialog())
                .build(),
            gio::ActionEntry::builder("open-folder")
                .activate(|window: &Self, _, _| {
                    window.imp().sidebar.create_new_workspace();
                })
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
            gio::ActionEntry::builder("show-document-format-controls")
                .activate(|window: &Self, _, _| window.show_document_format_controls_dialog())
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
            gio::ActionEntry::builder("toggle-command-palette")
                .activate(|window: &Self, _, _| window.toggle_command_palette())
                .build(),
            gio::ActionEntry::builder("toggle-search-panel")
                .activate(|window: &Self, _, _| window.toggle_search_panel())
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
            gio::ActionEntry::builder("edit-bookmark-label")
                .activate(|window: &Self, _, _| window.edit_bookmark_label())
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
            gio::ActionEntry::builder("add-annotation")
                .activate(|window: &Self, _, _| window.add_annotation())
                .build(),
            gio::ActionEntry::builder("edit-annotation")
                .activate(|window: &Self, _, _| window.edit_annotation())
                .build(),
            gio::ActionEntry::builder("show-annotations")
                .activate(|window: &Self, _, _| window.show_annotations_dialog())
                .build(),
            gio::ActionEntry::builder("export-annotations")
                .activate(|window: &Self, _, _| window.export_annotations())
                .build(),
        ]);

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

        self.register_split_view_toggle_action(
            "toggle-sidebar",
            &self.imp().workspace_split_view,
            keys::WORKSPACE_SIDEBAR_VISIBLE,
            |imp, visible| {
                imp.sidebar_visible.set(visible);
            },
        );
        self.register_split_view_toggle_action(
            "toggle-properties",
            &self.imp().properties_split_view,
            keys::PROPERTIES_SIDEBAR_VISIBLE,
            |imp, visible| {
                imp.properties_sidebar_visible.set(visible);
            },
        );
        self.register_boolean_setting_toggle_action("toggle-minimap", keys::SHOW_MINIMAP);
    }

    fn register_split_view_toggle_action(
        &self,
        action_name: &'static str,
        split_view: &libadwaita::OverlaySplitView,
        settings_key: &'static str,
        cache_visible: fn(&imp::LushtextWindow, bool),
    ) {
        let action = gio::SimpleAction::new_stateful(
            action_name,
            None,
            &split_view.shows_sidebar().to_variant(),
        );
        {
            let split_view = split_view.clone();
            action.connect_change_state(move |_action, state| {
                let Some(state) = state else { return };
                let Some(new_visible) = state.get::<bool>() else {
                    tracing::error!("{action_name}: expected bool state");
                    return;
                };
                split_view.set_show_sidebar(new_visible);
            });
        }
        self.add_action(&action);

        let window_weak = self.downgrade();
        split_view.connect_show_sidebar_notify(move |split| {
            let visible = split.shows_sidebar();
            action.set_state(&visible.to_variant());
            if let Some(window) = window_weak.upgrade() {
                cache_visible(window.imp(), visible);
                let _ = window.imp().settings.set_boolean(settings_key, visible);
                if !visible {
                    window.restore_focus_after_secondary_pane_close();
                }
            }
        });
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

        let action_clone = action.clone();
        self.imp()
            .settings
            .connect_changed(Some(settings_key), move |s, _| {
                action_clone.set_state(&s.boolean(settings_key).to_variant());
            });

        self.add_action(&action);
    }

    pub(super) fn setup_shortcuts(&self) {
        let controller = gtk4::ShortcutController::new();
        controller.set_scope(gtk4::ShortcutScope::Managed);

        let shortcuts = [
            ("win.new-tab", "<Control>t"),
            ("win.open-file", "<Control>o"),
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
            ("win.add-annotation", "<Control><Alt>n"),
            ("win.edit-annotation", "<Control><Alt>m"),
            ("win.show-annotations", "<Control><Alt>a"),
            ("win.export-annotations", "<Control><Alt><Shift>a"),
            ("win.toggle-sidebar", "F9"),
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

        let fs_action = fullscreen_action;
        let unfs_action = unfullscreen_action;
        self.connect_notify_local(Some("fullscreened"), move |window, _| {
            let is_fs = window.is_fullscreen();
            fs_action.set_enabled(!is_fs);
            unfs_action.set_enabled(is_fs);
        });
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
}
