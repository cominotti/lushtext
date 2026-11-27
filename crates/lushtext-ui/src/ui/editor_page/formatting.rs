// SPDX-License-Identifier: GPL-3.0-or-later

//! Cross-cutting preference bindings, formatting application, and initial editor signal setup.

use glib::value::ToValue;
use gtk_lush_signals::SignalBag;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::ObjectSubclassIsExt;
use lushtext_model::document::formatting_overrides::FormattingOverrides;
use sourceview5::prelude::*;

use crate::config::keys;

use super::LushtextEditorPage;
use super::style_scheme::apply_color_scheme_to_editor;

/// Application-global settings and theme signal lifetimes disconnected at `dispose`.
#[derive(Default)]
pub struct PreferenceBindingState {
    pub signals: SignalBag,
}

impl LushtextEditorPage {
    /// Wire GSettings bindings, font/theme observers, and buffer/view signals at construction.
    pub(super) fn setup_editor_preferences_and_signals(&self, buffer: &sourceview5::Buffer) {
        let imp = self.imp();
        let settings = &imp.settings;
        let invisible_mode = lushtext_model::document::encoding::InvisibleCharactersMode::from_id(
            settings.string(keys::INVISIBLE_CHARACTERS_MODE).as_str(),
        )
        .unwrap_or_default();
        imp.document_metadata.invisible_mode.set(invisible_mode);

        settings
            .bind(
                keys::SHOW_LINE_NUMBERS,
                &*imp.source_view,
                "show-line-numbers",
            )
            .flags(gio::SettingsBindFlags::GET)
            .build();
        settings
            .bind(
                keys::BOOKMARK_GUTTER_VISIBLE,
                &*imp.source_view,
                "show-line-marks",
            )
            .flags(gio::SettingsBindFlags::GET)
            .build();
        settings
            .bind(
                keys::HIGHLIGHT_CURRENT_LINE,
                &*imp.source_view,
                "highlight-current-line",
            )
            .flags(gio::SettingsBindFlags::GET)
            .build();

        apply_formatting_settings(&imp.source_view, settings, FormattingOverrides::default());
        for key in [keys::TAB_WIDTH, keys::INSERT_SPACES] {
            let editor_weak = self.downgrade();
            let id = settings.connect_changed(Some(key), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    let overrides = editor.imp().formatting_overrides.get();
                    editor.restyle_paced(|editor| {
                        apply_formatting_settings(
                            &editor.imp().source_view,
                            &editor.imp().settings,
                            overrides,
                        );
                    });
                }
            });
            imp.preference_bindings.signals.track(settings, id);
        }

        imp.source_view
            .set_wrap_mode(word_wrap_mode(settings.boolean(keys::WORD_WRAP)));
        let editor_weak = self.downgrade();
        let id = settings.connect_changed(Some(keys::WORD_WRAP), move |settings, _| {
            if let Some(editor) = editor_weak.upgrade() {
                let mode = word_wrap_mode(settings.boolean(keys::WORD_WRAP));
                editor.restyle_paced(|editor| editor.source_view().set_wrap_mode(mode));
                editor.invalidate_minimap_analysis_request(false);
                editor.sync_minimap_wrap_mode();
                editor.schedule_minimap_refresh();
            }
        });
        imp.preference_bindings.signals.track(settings, id);

        apply_color_scheme_to_editor(self);
        imp.document_surface_opacity
            .set(settings.double(keys::TAB_CONTENT_OPACITY).clamp(0.0, 1.0));
        {
            let editor_weak = self.downgrade();
            let id = settings.connect_changed(Some(keys::STYLE_SCHEME), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    apply_color_scheme_to_editor(&editor);
                }
            });
            imp.preference_bindings.signals.track(settings, id);
        }
        {
            let editor_weak = self.downgrade();
            let id = settings.connect_changed(Some(keys::TAB_CONTENT_OPACITY), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    apply_color_scheme_to_editor(&editor);
                }
            });
            imp.preference_bindings.signals.track(settings, id);
        }
        {
            let editor_weak = self.downgrade();
            let style_manager = libadwaita::StyleManager::default();
            let handler_id = style_manager.connect_dark_notify(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    apply_color_scheme_to_editor(&editor);
                }
            });
            imp.preference_bindings
                .signals
                .track(&style_manager, handler_id);
        }

        {
            let editor_weak = self.downgrade();
            let id = settings.connect_changed(Some(keys::SHOW_MINIMAP), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.invalidate_minimap_analysis_request(false);
                    editor.schedule_minimap_refresh();
                }
            });
            imp.preference_bindings.signals.track(settings, id);
        }
        {
            let editor_weak = self.downgrade();
            let id = settings.connect_changed(
                Some(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE),
                move |_, _| {
                    if let Some(editor) = editor_weak.upgrade() {
                        editor.invalidate_minimap_analysis_request(true);
                        editor.schedule_minimap_refresh();
                    }
                },
            );
            imp.preference_bindings.signals.track(settings, id);
        }
        {
            settings
                .bind(keys::MINIMAP_WIDTH, &*imp.minimap_overlay, "width-request")
                .get_only()
                .mapping(|variant, _| {
                    let width = variant.get::<i32>()?.clamp(64, 160);
                    Some(width.to_value())
                })
                .build();
            let editor_weak = self.downgrade();
            let id = settings.connect_changed(Some(keys::MINIMAP_WIDTH), move |_, _| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.schedule_minimap_refresh();
                }
            });
            imp.preference_bindings.signals.track(settings, id);
        }
        {
            let editor_weak = self.downgrade();
            let id =
                settings.connect_changed(Some(keys::FOCUS_MODE_TARGET_COLUMNS), move |s, _| {
                    if let Some(editor) = editor_weak.upgrade() {
                        editor
                            .set_focus_mode_target_columns(s.uint(keys::FOCUS_MODE_TARGET_COLUMNS));
                    }
                });
            imp.preference_bindings.signals.track(settings, id);
        }
        {
            let editor_weak = self.downgrade();
            let id = settings.connect_changed(
                Some(keys::FOCUS_MODE_TYPEWRITER_SCROLLING),
                move |s, _| {
                    if let Some(editor) = editor_weak.upgrade() {
                        editor.set_focus_mode_typewriter_scrolling(
                            s.boolean(keys::FOCUS_MODE_TYPEWRITER_SCROLLING),
                        );
                    }
                },
            );
            imp.preference_bindings.signals.track(settings, id);
        }

        self.setup_bookmark_projection();
        self.setup_local_history_context_menu();
        self.setup_local_history_tracking();

        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_end_user_action(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    if editor.reconcile_bookmarks_after_edit() {
                        editor.emit_bookmarks_changed();
                    }
                    editor.schedule_minimap_refresh();
                }
            });
            imp.editing_buffer_signals.track(buffer, handler_id);
        }

        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_changed(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    if editor.load_projection_suspended() {
                        return;
                    }
                    editor.notify_memory_policy_changed();
                }
            });
            imp.editing_buffer_signals.track(buffer, handler_id);
        }

        {
            let editor_weak = self.downgrade();
            let handler_id = buffer.connect_modified_changed(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    if editor.load_projection_suspended() {
                        return;
                    }
                    editor.notify_memory_policy_changed();
                }
            });
            imp.editing_buffer_signals.track(buffer, handler_id);
        }

        let editor_weak = self.downgrade();
        imp.search_bar.connect_close(move || {
            if let Some(editor) = editor_weak.upgrade() {
                editor.hide_search();
            }
        });

        {
            let editor_weak = self.downgrade();
            imp.source_view.connect_map(move |_| {
                if let Some(editor) = editor_weak.upgrade() {
                    editor.schedule_dynamic_overscroll_update();
                }
            });
        }
    }
}

/// Apply formatting settings to the source view, resolving EditorConfig
/// overrides against GSettings fallbacks. Called on initial construction,
/// GSettings change, and EditorConfig resolution.
pub(super) fn apply_formatting_settings(
    view: &sourceview5::View,
    settings: &gio::Settings,
    overrides: FormattingOverrides,
) {
    let tab_width = overrides
        .tab_width
        .unwrap_or_else(|| settings.uint(keys::TAB_WIDTH));
    view.set_tab_width(tab_width);

    let insert_spaces = overrides
        .insert_spaces
        .unwrap_or_else(|| settings.boolean(keys::INSERT_SPACES));
    view.set_insert_spaces_instead_of_tabs(insert_spaces);

    let indent_width = overrides.indent_width.unwrap_or(-1);
    view.set_indent_width(indent_width);
}

fn word_wrap_mode(enabled: bool) -> gtk4::WrapMode {
    if enabled {
        gtk4::WrapMode::Word
    } else {
        gtk4::WrapMode::None
    }
}
