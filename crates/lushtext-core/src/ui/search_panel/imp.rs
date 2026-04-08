// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the workspace search panel.
//!
//! Handles search execution with channel-based streaming, result grouping
//! into a `GtkTreeListModel`, and debounced query processing. Uses
//! `std::thread::spawn` + `crossbeam_channel::bounded` instead of
//! `spawn_blocking_then` because search results stream incrementally.

use super::item::SearchResultItem;
use gtk4::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// Callback type for file-open events (path, line_number).
type OpenFileCallback = Box<dyn Fn(&Path, u32)>;

#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/search-panel.ui")]
pub struct LushtextSearchPanel {
    #[template_child]
    pub search_entry: TemplateChild<gtk4::SearchEntry>,
    #[template_child]
    pub results_list: TemplateChild<gtk4::ListView>,
    #[template_child]
    pub results_scroll: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub count_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub error_label: TemplateChild<gtk4::Label>,
    #[template_child]
    pub case_toggle: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub regex_toggle: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub word_toggle: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub more_toggle: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub options_revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub gitignore_toggle: TemplateChild<gtk4::ToggleButton>,
    #[template_child]
    pub glob_entry: TemplateChild<gtk4::Entry>,

    /// Root-level model: contains file header items only.
    pub root_store: gio::ListStore,
    /// Per-file child stores: file_path → (file item, child ListStore).
    pub file_groups: RefCell<HashMap<PathBuf, (SearchResultItem, gio::ListStore)>>,
    /// Cancel token for the in-flight search thread.
    pub cancel_token: RefCell<Option<Arc<AtomicBool>>>,
    /// Generation counter for search debounce (300ms).
    pub search_generation: Cell<u32>,
    /// Workspace roots to search. Updated by the window when workspaces change.
    pub workspace_roots: RefCell<Vec<PathBuf>>,
    /// Running total of matches in the current search.
    pub total_matches: Cell<u32>,
    /// Running total of files with matches in the current search.
    pub total_files: Cell<u32>,
    /// Whether a search is currently active (thread running + polling timer alive).
    pub searching: Cell<bool>,
    /// Whether the result cap was hit in the current search.
    pub result_capped: Cell<bool>,
    /// Generation counter for glob entry debounce (300ms).
    pub glob_generation: Cell<u32>,

    /// GSettings instance for search toggle persistence.
    pub settings: gio::Settings,

    /// Guards against spurious searches during construction (GSettings restore
    /// fires `notify::active` on toggle buttons before workspace roots are set).
    pub constructed_complete: Cell<bool>,

    // Callbacks — set by the window.
    /// Called when the user activates a match result: (file_path, line_number).
    pub open_file_callback: RefCell<Option<OpenFileCallback>>,
    /// Called when the user presses Escape.
    pub close_requested_callback: RefCell<Option<Box<dyn Fn()>>>,
}

impl Default for LushtextSearchPanel {
    fn default() -> Self {
        Self {
            search_entry: TemplateChild::default(),
            results_list: TemplateChild::default(),
            results_scroll: TemplateChild::default(),
            count_label: TemplateChild::default(),
            error_label: TemplateChild::default(),
            case_toggle: TemplateChild::default(),
            regex_toggle: TemplateChild::default(),
            word_toggle: TemplateChild::default(),
            more_toggle: TemplateChild::default(),
            options_revealer: TemplateChild::default(),
            gitignore_toggle: TemplateChild::default(),
            glob_entry: TemplateChild::default(),
            root_store: gio::ListStore::new::<SearchResultItem>(),
            file_groups: RefCell::new(HashMap::new()),
            cancel_token: RefCell::new(None),
            search_generation: Cell::new(0),
            workspace_roots: RefCell::new(Vec::new()),
            total_matches: Cell::new(0),
            total_files: Cell::new(0),
            searching: Cell::new(false),
            result_capped: Cell::new(false),
            glob_generation: Cell::new(0),
            settings: gio::Settings::new(crate::config::APP_ID),
            constructed_complete: Cell::new(false),
            open_file_callback: RefCell::new(None),
            close_requested_callback: RefCell::new(None),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextSearchPanel {
    const NAME: &'static str = "LushtextSearchPanel";
    type Type = super::LushtextSearchPanel;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextSearchPanel {
    fn constructed(&self) {
        self.parent_constructed();
        self.setup_results_list();
        self.setup_search_entry();
        self.setup_toggles();
        self.setup_options();
        self.constructed_complete.set(true);
    }

    fn dispose(&self) {
        // Cancel any in-flight search thread so it stops promptly after
        // the window closes, instead of running until the walker finishes.
        if let Some(cancel) = self.cancel_token.take() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

impl WidgetImpl for LushtextSearchPanel {}
impl BoxImpl for LushtextSearchPanel {}

impl LushtextSearchPanel {
    /// Set up the GtkTreeListModel + factory for the results list.
    fn setup_results_list(&self) {
        let root_store = self.root_store.clone();
        // Use WeakRef to the panel so the callback sees live file_groups,
        // not a stale clone taken at construction time (when the map is empty).
        let panel_weak = self.obj().downgrade();

        // TreeListModel: root items are file headers, children are match items.
        let tree_model = gtk4::TreeListModel::new(
            root_store,
            false, // passthrough = false (we need TreeListRow wrappers)
            false, // autoexpand = false (NEVER true per project rules)
            move |item| -> Option<gio::ListModel> {
                let panel = panel_weak.upgrade()?;
                let result_item = item.downcast_ref::<SearchResultItem>()?;
                if result_item.is_file_item() {
                    let path = PathBuf::from(result_item.file_path());
                    panel
                        .imp()
                        .file_groups
                        .borrow()
                        .get(&path)
                        .map(|(_, store)| store.clone().upcast())
                } else {
                    None
                }
            },
        );

        let selection = gtk4::SingleSelection::new(Some(tree_model));
        self.results_list.set_model(Some(&selection));

        // Factory: create + bind row widgets.
        let factory = gtk4::SignalListItemFactory::new();
        factory.connect_setup(|_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");

            let expander = gtk4::TreeExpander::new();

            // Content box for the row: either file header or match line.
            let content_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            content_box.set_margin_start(4);
            content_box.set_margin_end(4);
            content_box.set_margin_top(2);
            content_box.set_margin_bottom(2);

            // File icon/name label + match count for file rows,
            // line number + content for match rows. Both use the same box;
            // connect_bind swaps visibility.
            let file_label = gtk4::Label::new(None);
            file_label.set_hexpand(true);
            file_label.set_xalign(0.0);
            file_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
            file_label.add_css_class("heading");

            let count_badge = gtk4::Label::new(None);
            count_badge.add_css_class("caption");
            count_badge.add_css_class("dim-label");

            let line_num_label = gtk4::Label::new(None);
            line_num_label.add_css_class("caption");
            line_num_label.add_css_class("dim-label");
            line_num_label.add_css_class("monospace");
            line_num_label.set_width_chars(5);
            line_num_label.set_xalign(1.0);

            let line_content_label = gtk4::Label::new(None);
            line_content_label.set_hexpand(true);
            line_content_label.set_xalign(0.0);
            line_content_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            line_content_label.add_css_class("monospace");

            content_box.append(&file_label);
            content_box.append(&count_badge);
            content_box.append(&line_num_label);
            content_box.append(&line_content_label);

            expander.set_child(Some(&content_box));
            list_item.set_child(Some(&expander));
        });

        factory.connect_bind(|_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            let Some(expander) = list_item
                .child()
                .and_then(|w| w.downcast::<gtk4::TreeExpander>().ok())
            else {
                return;
            };

            let Some(row) = list_item
                .item()
                .and_then(|item| item.downcast::<gtk4::TreeListRow>().ok())
            else {
                return;
            };
            expander.set_list_row(Some(&row));

            let Some(result_item) = row.item().and_downcast::<SearchResultItem>() else {
                return;
            };

            let Some(content_box) = expander
                .child()
                .and_then(|w| w.downcast::<gtk4::Box>().ok())
            else {
                return;
            };

            // Get the four child labels.
            let file_label = content_box
                .first_child()
                .and_then(|w| w.downcast::<gtk4::Label>().ok());
            let count_badge = file_label
                .as_ref()
                .and_then(|w| w.next_sibling())
                .and_then(|w| w.downcast::<gtk4::Label>().ok());
            let line_num_label = count_badge
                .as_ref()
                .and_then(|w| w.next_sibling())
                .and_then(|w| w.downcast::<gtk4::Label>().ok());
            let line_content_label = line_num_label
                .as_ref()
                .and_then(|w| w.next_sibling())
                .and_then(|w| w.downcast::<gtk4::Label>().ok());

            if result_item.is_file_item() {
                // File header row.
                if let Some(ref label) = file_label {
                    label.set_text(&result_item.display_path());
                    label.set_visible(true);
                }
                if let Some(ref badge) = count_badge {
                    badge.set_visible(true);
                    // bind_property keeps the badge text in sync as matches
                    // stream in. sync_create sets the initial value; subsequent
                    // set_match_count() calls emit notify → transform fires.
                    let binding = result_item
                        .bind_property("match-count", badge, "label")
                        .transform_to(|_: &glib::Binding, value: &glib::Value| {
                            let count: u32 = value.get().ok()?;
                            Some(format!("{count}").to_value())
                        })
                        .sync_create()
                        .build();
                    // SAFETY: key is unique per ListItem, type matches steal_data
                    // in connect_unbind. Binding must outlive the bind cycle.
                    unsafe {
                        list_item.set_data("count-binding", binding);
                    }
                }
                if let Some(ref label) = line_num_label {
                    label.set_visible(false);
                }
                if let Some(ref label) = line_content_label {
                    label.set_visible(false);
                }
            } else {
                // Match row.
                if let Some(ref label) = file_label {
                    label.set_visible(false);
                }
                if let Some(ref badge) = count_badge {
                    badge.set_visible(false);
                }
                if let Some(ref label) = line_num_label {
                    label.set_text(&format!("{}", result_item.line_number()));
                    label.set_visible(true);
                }
                if let Some(ref label) = line_content_label {
                    let content = result_item.line_content();
                    let markup = render_match_markup(
                        &content,
                        result_item.match_start() as usize,
                        result_item.match_end() as usize,
                    );
                    label.set_markup(&markup);
                    label.set_visible(true);
                }
            }

            // Disable expander gesture for match rows (same fix as sidebar file tree).
            // TreeExpander installs an internal GtkGestureClick that intercepts all
            // rows; for non-expandable match rows this prevents ListView activation.
            for controller in expander.observe_controllers().into_iter().flatten() {
                if let Ok(gesture) = controller.downcast::<gtk4::GestureClick>() {
                    if result_item.is_match_item() {
                        gesture.set_propagation_phase(gtk4::PropagationPhase::None);
                    } else {
                        gesture.set_propagation_phase(gtk4::PropagationPhase::Bubble);
                    }
                }
            }
        });

        // Disconnect the match-count property binding when a row is recycled,
        // so the old SearchResultItem's updates don't overwrite a reused label.
        factory.connect_unbind(|_, list_item| {
            let list_item = list_item
                .downcast_ref::<gtk4::ListItem>()
                .expect("ListItem");
            // SAFETY: mirrors set_data("count-binding") in connect_bind above.
            // steal_data returns None for match rows (no binding was stored).
            unsafe {
                if let Some(binding) = list_item.steal_data::<glib::Binding>("count-binding") {
                    binding.unbind();
                }
            }
        });

        self.results_list.set_factory(Some(&factory));

        // Result activation: double-click or Enter on a match row.
        let panel_weak = self.obj().downgrade();
        self.results_list.connect_activate(move |list_view, pos| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let Some(model) = list_view.model() else {
                return;
            };
            let Some(item) = model.item(pos) else { return };

            // The model wraps items in TreeListRow.
            if let Some(row) = item.downcast_ref::<gtk4::TreeListRow>()
                && let Some(result_item) = row.item().and_downcast::<SearchResultItem>()
            {
                if result_item.is_match_item() {
                    // Open file at line.
                    let path = PathBuf::from(result_item.file_path());
                    let line = result_item.line_number();
                    if let Some(ref cb) = *panel.imp().open_file_callback.borrow() {
                        cb(&path, line);
                    }
                } else {
                    // File header: toggle expand/collapse.
                    row.set_expanded(!row.is_expanded());
                }
            }
        });
    }

    /// Bind toggle buttons to GSettings for persistence and wire immediate
    /// re-search on toggle change. Must be called AFTER `setup_search_entry`
    /// and BEFORE `constructed_complete` is set to `true`.
    fn setup_toggles(&self) {
        use crate::config::keys;

        // Two-way GSettings bindings: persist toggle state across sessions.
        // These fire notify::active with the restored value.
        self.settings
            .bind(keys::SEARCH_CASE_SENSITIVE, &*self.case_toggle, "active")
            .build();
        self.settings
            .bind(keys::SEARCH_REGEX, &*self.regex_toggle, "active")
            .build();
        self.settings
            .bind(keys::SEARCH_WHOLE_WORD, &*self.word_toggle, "active")
            .build();

        // Immediate re-search when any toggle changes (no debounce — UX-DR12).
        // Connected AFTER bind() so the initial GSettings restore doesn't trigger
        // a search. The `constructed_complete` guard prevents the restore-time
        // notify from reaching start_search.
        for toggle in [&*self.case_toggle, &*self.regex_toggle, &*self.word_toggle] {
            let panel_weak = self.obj().downgrade();
            toggle.connect_notify_local(Some("active"), move |_, _| {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                if !panel.imp().constructed_complete.get() {
                    return; // GSettings restore during construction — skip.
                }
                let query = panel.query();
                if !query.is_empty() {
                    panel.start_search(&query);
                }
            });
        }
    }

    /// Wire the "More" button to the options revealer, bind gitignore toggle
    /// to GSettings, and set up glob entry debounce. Must be called AFTER
    /// `setup_toggles` and BEFORE `constructed_complete` is set to `true`.
    fn setup_options(&self) {
        use crate::config::keys;

        // 1. GSettings bind: restores persisted expanded state to more_toggle.
        self.settings
            .bind(
                keys::SEARCH_PANEL_OPTIONS_EXPANDED,
                &*self.more_toggle,
                "active",
            )
            .build();

        // 2. GSettings bind: restores persisted gitignore state.
        self.settings
            .bind(keys::SEARCH_GITIGNORE, &*self.gitignore_toggle, "active")
            .build();

        // 3. Propagate more_toggle.active → options_revealer.reveal_child.
        // sync_create ensures the revealer reflects the restored GSettings state.
        self.more_toggle
            .bind_property("active", &*self.options_revealer, "reveal-child")
            .sync_create()
            .build();

        // 4. Immediate re-search when gitignore toggle changes (same as case/regex/word).
        let panel_weak = self.obj().downgrade();
        self.gitignore_toggle
            .connect_notify_local(Some("active"), move |_, _| {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                if !panel.imp().constructed_complete.get() {
                    return; // GSettings restore during construction — skip.
                }
                let query = panel.query();
                if !query.is_empty() {
                    panel.start_search(&query);
                }
            });

        // 5. Glob entry: 300ms generation-counter debounce.
        let panel_weak = self.obj().downgrade();
        self.glob_entry.connect_changed(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let imp = panel.imp();
            let current_gen = imp.glob_generation.get().wrapping_add(1);
            imp.glob_generation.set(current_gen);

            let panel_weak = panel.downgrade();
            glib::timeout_add_local_once(Duration::from_millis(300), move || {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                if panel.imp().glob_generation.get() != current_gen {
                    return; // Superseded by a newer keystroke.
                }
                let query = panel.query();
                if !query.is_empty() {
                    panel.start_search(&query);
                }
            });
        });
    }

    /// Set up the search entry with debounced search triggering.
    fn setup_search_entry(&self) {
        let panel_weak = self.obj().downgrade();
        self.search_entry.connect_search_changed(move |entry| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let query = entry.text().to_string();
            let imp = panel.imp();

            // Generation-counter debounce: 300ms.
            let generation = imp.search_generation.get().wrapping_add(1);
            imp.search_generation.set(generation);

            let panel_weak = panel.downgrade();
            glib::timeout_add_local_once(Duration::from_millis(300), move || {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                if panel.imp().search_generation.get() != generation {
                    return; // Superseded by a newer keystroke.
                }
                panel.start_search(&query);
            });
        });

        // Escape key: signal close request.
        let panel_weak = self.obj().downgrade();
        self.search_entry.connect_stop_search(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            if let Some(ref cb) = *panel.imp().close_requested_callback.borrow() {
                cb();
            }
        });
    }
}

/// Build Pango markup highlighting the matched substring with bold.
/// Falls back to plain escaped text when the range is invalid.
fn render_match_markup(content: &str, start: usize, end: usize) -> String {
    // Clamp to content length and snap to char boundaries to avoid panics.
    let start = content.floor_char_boundary(start.min(content.len()));
    let end = content.ceil_char_boundary(end.min(content.len()));
    if start >= end {
        // No valid match range — render plain escaped text.
        return glib::markup_escape_text(content).to_string();
    }
    format!(
        "{}<b>{}</b>{}",
        glib::markup_escape_text(&content[..start]),
        glib::markup_escape_text(&content[start..end]),
        glib::markup_escape_text(&content[end..]),
    )
}

/// Compute a display-friendly relative path for a result file.
/// Strips the workspace root prefix for readability.
pub fn make_display_path(path: &Path, roots: &[PathBuf]) -> String {
    for root in roots {
        if let Ok(relative) = path.strip_prefix(root) {
            return relative.display().to_string();
        }
    }
    path.display().to_string()
}
