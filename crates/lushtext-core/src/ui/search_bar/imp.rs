// SPDX-License-Identifier: GPL-3.0-or-later

use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use sourceview5::prelude::*;
use std::cell::{Cell, RefCell};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/search-bar.ui")]
pub struct LushtextSearchBar {
    // --- Template children ---
    #[template_child]
    pub search_entry: TemplateChild<gtk4::SearchEntry>,
    #[template_child]
    pub replace_entry: TemplateChild<gtk4::Entry>,
    #[template_child]
    pub match_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub prev_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub next_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub close_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub replace_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub replace_all_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub replace_mode_button: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub options_button: TemplateChild<gtk4::MenuButton>,

    // --- Search state (populated during attach, cleared on detach) ---
    /// GtkSourceView SearchContext — owns the search state and highlighting.
    /// Created in attach() with the active buffer + settings.
    pub search_context: RefCell<Option<sourceview5::SearchContext>>,
    /// GtkSourceView SearchSettings — shared by the SearchContext.
    /// Controls search text, regex, case sensitivity, word boundaries.
    pub search_settings: RefCell<Option<sourceview5::SearchSettings>>,
    /// Weak reference to the source view for scroll_mark_onscreen after match navigation.
    pub view_ref: RefCell<Option<glib::WeakRef<sourceview5::View>>>,
    /// Signal handler for SearchContext::connect_occurrences_count_notify.
    /// Disconnected during detach() to break the reference cycle.
    pub occurrences_handler_id: RefCell<Option<glib::SignalHandlerId>>,

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

        // Replace mode toggle: show/hide all row-1 widgets. The GtkGrid
        // collapses the row when all children are invisible.
        let replace_entry = self.replace_entry.clone();
        let replace_btn = self.replace_button.clone();
        let replace_all_btn = self.replace_all_button.clone();
        self.replace_mode_button.connect_toggled(move |button| {
            let visible = button.is_active();
            replace_entry.set_visible(visible);
            replace_btn.set_visible(visible);
            replace_all_btn.set_visible(visible);
        });

        // Build the options popover menu with checkbox items for search
        // settings (regex, case-sensitive, whole word). The actions are
        // stateful booleans in a "search-options" group on this widget.
        self.setup_options_menu();

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
            let action_clone = action.clone();
            action.connect_activate(move |_, _| {
                let current: bool = action_clone.state().and_then(|v| v.get()).unwrap_or(false);
                action_clone.set_state(&(!current).to_variant());
            });
            group.add_action(&action);
        }
        self.obj()
            .insert_action_group("search-options", Some(&group));
        self.options_group.replace(Some(group));
    }

    /// Wire all button clicks and keyboard handlers once during construction.
    /// Handlers delegate to wrapper-type methods that check for an active
    /// SearchContext, so they safely no-op when no search session is attached.
    fn wire_ui_signals(&self) {
        let obj = self.obj().clone();

        // Nav buttons
        let bar = obj.clone();
        self.prev_button.connect_clicked(move |_| bar.move_prev());
        let bar = obj.clone();
        self.next_button.connect_clicked(move |_| bar.move_next());

        // Replace buttons
        let bar = obj.clone();
        self.replace_button
            .connect_clicked(move |_| bar.replace_current());
        let bar = obj.clone();
        self.replace_all_button
            .connect_clicked(move |_| bar.replace_all());

        // Search-as-you-type: pipe entry text → active SearchSettings.
        let bar = obj.clone();
        self.search_entry.connect_search_changed(move |entry| {
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
        });

        // Enter/Shift+Enter on the search entry for match navigation.
        let bar = obj.clone();
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, keyval, _keycode, state| {
            let shift = state.contains(gtk4::gdk::ModifierType::SHIFT_MASK);
            match keyval {
                gtk4::gdk::Key::Return | gtk4::gdk::Key::KP_Enter => {
                    if shift {
                        bar.move_prev();
                    } else {
                        bar.move_next();
                    }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.search_entry.add_controller(key_controller);

        // Escape on the replace entry also closes the search bar.
        let bar_weak = obj.downgrade();
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
