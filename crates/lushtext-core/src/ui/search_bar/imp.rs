// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the in-editor search bar widget.
//!
//! This GTK adapter owns template children, search option actions, replace-row
//! projection, and attach/detach state for one active `GtkSourceView`.

use crate::ui::accessibility;
use gtk_lush_signals::SignalBag;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};

/// Private GTK implementation for `LushtextSearchBar`.
///
/// Holds the composite-template children and per-attachment search state used
/// by the public wrapper's editor search workflow.
#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/search-bar.ui")]
pub struct LushtextSearchBar {
    // --- Template children ---
    /// Search entry that owns the query typed by the user.
    #[template_child]
    pub search_entry: TemplateChild<gtk4::SearchEntry>,
    /// Replacement text entry shown only while replace mode is active.
    #[template_child]
    pub replace_entry: TemplateChild<gtk4::Entry>,
    /// Label showing the current match index and total count.
    #[template_child]
    pub match_label: TemplateChild<gtk4::Label>,
    /// Button that moves to the previous search match.
    #[template_child]
    pub prev_button: TemplateChild<gtk4::Button>,
    /// Button that moves to the next search match.
    #[template_child]
    pub next_button: TemplateChild<gtk4::Button>,
    /// Button that dismisses the search bar and returns focus to the editor.
    #[template_child]
    pub close_button: TemplateChild<gtk4::Button>,
    /// Button that replaces the current search match.
    #[template_child]
    pub replace_button: TemplateChild<gtk4::Button>,
    /// Button that replaces all matches through the editor search workflow.
    #[template_child]
    pub replace_all_button: TemplateChild<gtk4::Button>,
    /// Toggle that reveals the replace controls while find-only search stays compact.
    #[template_child]
    pub replace_mode_button: TemplateChild<gtk4::ToggleButton>,
    /// Menu button that exposes regex, case, and whole-word search options.
    #[template_child]
    pub options_button: TemplateChild<gtk4::MenuButton>,
    /// Revealer for the replacement entry, bound to `replace_mode_button.active`.
    #[template_child]
    pub replace_entry_revealer: TemplateChild<gtk4::Revealer>,
    /// Revealer for the Replace button, bound with the entry so the row moves as one unit.
    #[template_child]
    pub replace_button_revealer: TemplateChild<gtk4::Revealer>,
    /// Revealer for the Replace All button, bound with the rest of replace mode.
    #[template_child]
    pub replace_all_revealer: TemplateChild<gtk4::Revealer>,

    // --- Search state (populated during attach, cleared on detach) ---
    /// GtkSourceView SearchContext — owns the search state and highlighting.
    /// Created in attach() with the active buffer + settings.
    pub search_context: RefCell<Option<sourceview5::SearchContext>>,
    /// GtkSourceView SearchSettings — shared by the SearchContext.
    /// Controls search text, regex, case sensitivity, word boundaries.
    pub search_settings: RefCell<Option<sourceview5::SearchSettings>>,
    /// Weak reference to the source view for scroll_mark_onscreen after match navigation.
    pub view_ref: RefCell<Option<glib::WeakRef<sourceview5::View>>>,
    /// SearchContext signal lifetimes for the current attachment.
    pub occurrences_signals: SignalBag,
    /// Debounces screen-reader announcements for changing result counts.
    pub match_announcement_throttler: accessibility::AnnouncementThrottler,

    // --- Navigation state ---
    /// Whether the user navigated to a match (next/prev). When true, Escape
    /// keeps the cursor at the navigated position instead of restoring.
    pub navigated: Cell<bool>,

    // --- Options action group ---
    /// The "search-options" action group for regex/case/word toggles.
    /// Stored here for direct access during attach() without widget lookup.
    pub options_group: RefCell<Option<gtk4::gio::SimpleActionGroup>>,

    // --- Close callback ---
    /// Closure called when the search bar should be hidden. Set by the
    /// EditorPage when wiring the bar; fires on close button and Escape.
    pub close_callback: RefCell<Option<Box<dyn Fn()>>>,
    /// Callback fired when any search state affecting live highlights changes.
    ///
    /// The minimap uses this to refresh its search markers when the query,
    /// toggles, or attach/detach lifecycle changes.
    pub search_state_changed_callback: RefCell<Option<Box<dyn Fn()>>>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextSearchBar {
    const NAME: &'static str = "LushtextSearchBar";
    type Type = super::LushtextSearchBar;
    type ParentType = gtk4::Grid;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextSearchBar {
    fn constructed(&self) {
        self.parent_constructed();

        // Use revealers instead of visibility so the hidden replace row still
        // contributes natural width and the entry column does not shift.
        // `sync_create()` applies the initial collapsed state immediately.
        for revealer in [
            &*self.replace_entry_revealer,
            &*self.replace_button_revealer,
            &*self.replace_all_revealer,
        ] {
            self.replace_mode_button
                .bind_property("active", revealer, "reveal-child")
                .sync_create()
                .build();
        }

        {
            let button = self.replace_mode_button.clone();
            self.replace_mode_button
                .connect_active_notify(move |button| {
                    accessibility::set_pressed(button, button.is_active());
                });
            accessibility::set_pressed(&button, button.is_active());
        }

        // Build the options popover menu with checkbox items for search
        // settings (regex, case-sensitive, whole word). The actions are
        // stateful booleans in a "search-options" group on this widget.
        self.setup_options_menu();
        self.apply_accessibility_metadata();

        // Wire all button and keyboard signals once here, not in attach().
        // The handlers delegate to methods on the wrapper type which check
        // search_context.is_some() and no-op when no session is active.
        // This prevents signal handler accumulation on attach/detach cycles.
        self.wire_ui_signals();
    }
}

impl WidgetImpl for LushtextSearchBar {}
impl GridImpl for LushtextSearchBar {}

impl LushtextSearchBar {
    /// Build the gear-icon popover menu with three toggle options.
    /// Actions are registered in a "search-options" action group so they
    /// persist across attach/detach cycles.
    fn setup_options_menu(&self) {
        let menu = gtk4::gio::Menu::new();
        menu.append(Some("Regular Expressions"), Some("search-options.regex"));
        menu.append(
            Some("Case Sensitive"),
            Some("search-options.case-sensitive"),
        );
        menu.append(
            Some("Match Whole Word Only"),
            Some("search-options.whole-word"),
        );

        let popover = gtk4::PopoverMenu::from_model(Some(&menu));
        self.options_button.set_popover(Some(&popover));

        // Create stateful boolean actions — state is synced to SearchSettings
        // in attach(). Default states: all off.
        let group = gtk4::gio::SimpleActionGroup::new();
        for name in ["regex", "case-sensitive", "whole-word"] {
            let action = gtk4::gio::SimpleAction::new_stateful(name, None, &false.to_variant());
            let option_name = name.to_string();
            let bar_weak = self.obj().downgrade();
            action.connect_activate(move |action, _| {
                let current: bool = action.state().and_then(|v| v.get()).unwrap_or(false);
                let next = !current;
                action.set_state(&next.to_variant());
                if let Some(bar) = bar_weak.upgrade() {
                    bar.apply_option_state(&option_name, next);
                    bar.emit_search_state_changed();
                }
            });
            group.add_action(&action);
        }
        self.obj()
            .insert_action_group("search-options", Some(&group));
        self.options_group.replace(Some(group));
    }

    /// Assign stable names to icon-only search controls for screen readers and
    /// the AT-SPI smoke lane. Tooltips are visual hints; accessible labels are
    /// the semantic contract assistive technologies query.
    fn apply_accessibility_metadata(&self) {
        accessibility::set_labelled_description(
            &*self.search_entry,
            "Find text",
            "Search within the active document",
        );
        accessibility::set_labelled_description(
            &*self.replace_entry,
            "Replacement text",
            "Replacement text for find and replace",
        );
        accessibility::set_role(&*self.match_label, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &*self.match_label,
            "Search match count",
            "Current match position and total matches",
        );
        accessibility::set_label(&*self.prev_button, "Previous search match");
        accessibility::set_key_shortcuts(&*self.prev_button, "<Shift>Return");
        accessibility::set_label(&*self.next_button, "Next search match");
        accessibility::set_key_shortcuts(&*self.next_button, "Return");
        accessibility::set_label(&*self.replace_mode_button, "Toggle replace controls");
        accessibility::set_label(&*self.options_button, "Search options");
        accessibility::set_has_popup(&*self.options_button, true);
        accessibility::set_label(&*self.close_button, "Close search");
        accessibility::set_key_shortcuts(&*self.close_button, "Escape");
        accessibility::set_label(&*self.replace_button, "Replace current match");
        accessibility::set_label(&*self.replace_all_button, "Replace all matches");
    }

    /// Wire all button clicks and keyboard handlers once during construction.
    /// Handlers delegate to wrapper-type methods that check for an active
    /// SearchContext, so they safely no-op when no search session is attached.
    fn wire_ui_signals(&self) {
        let bar_weak = self.obj().downgrade();
        self.prev_button.connect_clicked(move |_| {
            if let Some(bar) = bar_weak.upgrade() {
                bar.move_prev();
            }
        });
        let bar_weak = self.obj().downgrade();
        self.next_button.connect_clicked(move |_| {
            if let Some(bar) = bar_weak.upgrade() {
                bar.move_next();
            }
        });

        let bar_weak = self.obj().downgrade();
        self.replace_button.connect_clicked(move |_| {
            if let Some(bar) = bar_weak.upgrade() {
                bar.replace_current();
            }
        });
        let bar_weak = self.obj().downgrade();
        self.replace_all_button.connect_clicked(move |_| {
            if let Some(bar) = bar_weak.upgrade() {
                bar.replace_all();
            }
        });

        // Search-as-you-type: pipe entry text → active SearchSettings.
        let bar_weak = self.obj().downgrade();
        self.search_entry.connect_search_changed(move |entry| {
            let Some(bar) = bar_weak.upgrade() else {
                return;
            };
            let imp = bar.imp();
            if let Some(ref settings) = *imp.search_settings.borrow() {
                let text = entry.text();
                if text.is_empty() {
                    settings.set_search_text(None);
                } else {
                    settings.set_search_text(Some(text.as_str()));
                }
            }
            bar.update_match_info();
            bar.emit_search_state_changed();
        });

        // Enter/Shift+Enter on the search entry for match navigation.
        let bar_weak = self.obj().downgrade();
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, keyval, _keycode, state| {
            let shift = state.contains(gtk4::gdk::ModifierType::SHIFT_MASK);
            match keyval {
                gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter => {
                    if let Some(bar) = bar_weak.upgrade() {
                        if shift {
                            bar.move_prev();
                        } else {
                            bar.move_next();
                        }
                    }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.search_entry.add_controller(key_controller);

        // Escape on the replace entry also closes the search bar.
        let bar_weak = self.obj().downgrade();
        let replace_key_controller = gtk4::EventControllerKey::new();
        replace_key_controller.connect_key_pressed(move |_, keyval, _keycode, _state| {
            if keyval == gtk4::gdk::Key::Escape
                && let Some(bar) = bar_weak.upgrade()
                && let Some(ref close_cb) = *bar.imp().close_callback.borrow()
            {
                close_cb();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.replace_entry.add_controller(replace_key_controller);
    }
}
