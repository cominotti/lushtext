// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the workspace search panel.
//!
//! Handles search execution with channel-based streaming, result grouping
//! into a `GtkTreeListModel`, and debounced query processing. Uses
//! `std::thread::spawn` + `crossbeam_channel::bounded` instead of
//! `spawn_blocking_then` because search results stream incrementally.

use super::item::SearchResultItem;
use crate::model::content_search::{Replacement, SavedSearch, SearchHistoryEntry};
use gtk4::prelude::*;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// Callback type for file-open events (path, line_number).
type OpenFileCallback = Box<dyn Fn(&Path, u32)>;

/// Callback type for navigation events from F4/Shift+F4 (path, line_number).
type NavigateCallback = Box<dyn Fn(&Path, u32)>;

/// Callback type for search progress events: (files_searched, is_done).
type ProgressCallback = Box<dyn Fn(usize, bool)>;

/// Callback type for Replace All execution: receives checked replacements.
type ReplaceCallback = Box<dyn Fn(Vec<Replacement>)>;

/// Callback type for Undo All: receives the backup map to restore.
type UndoCallback = Box<dyn Fn(HashMap<PathBuf, Vec<u8>>)>;
type MessageCallback = Box<dyn Fn(&str)>;

const SEARCH_INPUT_DEBOUNCE_MS: u64 = 300;

#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/search-panel.ui")]
pub struct LushtextSearchPanel {
    #[template_child]
    pub search_entry: TemplateChild<gtk4::SearchEntry>,
    #[template_child]
    pub results_list: TemplateChild<gtk4::ListView>,
    #[template_child]
    pub results_feedback_revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub results_body_revealer: TemplateChild<gtk4::Revealer>,
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
    #[template_child]
    pub replace_entry: TemplateChild<gtk4::Entry>,
    #[template_child]
    pub replace_all_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub undo_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub save_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub close_button: TemplateChild<gtk4::Button>,
    #[template_child]
    pub results_header_separator: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub results_footer_separator: TemplateChild<gtk4::Separator>,
    #[template_child]
    pub footer_box: TemplateChild<gtk4::Box>,

    /// Dropdown popover for saved searches + recent history. Parented to search_entry.
    pub history_popover: gtk4::Popover,
    /// Container box inside the popover holding both sections.
    pub dropdown_box: gtk4::Box,
    /// "Saved Searches" section header label.
    pub saved_header: gtk4::Label,
    /// List box for saved search entries.
    pub saved_searches_list: gtk4::ListBox,
    /// Separator between saved searches and recent history sections.
    pub dropdown_separator: gtk4::Separator,
    /// "Recent" section header label.
    pub recent_header: gtk4::Label,
    /// List box for recent search history entries.
    pub history_list: gtk4::ListBox,

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

    /// Guards against redundant searches during history/saved-search restore.
    pub restoring_history: Cell<bool>,

    /// Persisted search history entries (most recent first, capped at 20).
    pub history_entries: RefCell<Vec<SearchHistoryEntry>>,

    /// Named saved searches (permanent, user-managed).
    pub saved_searches: RefCell<Vec<SavedSearch>>,

    // --- Replace/Preview state ---
    /// Whether the results list is in preview mode (showing before/after with checkboxes).
    pub preview_mode: Cell<bool>,
    /// In-memory backup of original file content, stored after replace for undo.
    pub undo_backup: RefCell<Option<HashMap<PathBuf, Vec<u8>>>>,
    /// Generation counter that invalidates stale persisted backup loads.
    pub undo_backup_generation: Cell<u32>,
    /// Generated preview data shown in preview mode.
    pub preview_replacements: RefCell<Vec<Replacement>>,
    /// Indices of checked replacements in preview mode.
    pub checked_indices: RefCell<HashSet<usize>>,

    // --- Navigation state (F4/Shift+F4) ---
    /// Flat navigation index of (path, line_number) pairs in match arrival order.
    pub match_positions: RefCell<Vec<(PathBuf, u32)>>,
    /// Current position in `match_positions` for F4/Shift+F4 cycling.
    pub current_match_index: Cell<Option<usize>>,
    /// Last progress count (files visited), forwarded on Done for approximate total.
    pub last_progress_count: Cell<usize>,

    // Callbacks — set by the window.
    /// Called when the user activates a match result: (file_path, line_number).
    pub open_file_callback: RefCell<Option<OpenFileCallback>>,
    /// Called when the user presses Escape.
    pub close_requested_callback: RefCell<Option<Box<dyn Fn()>>>,
    /// Called when F4/Shift+F4 navigates to a match: (path, line_number).
    pub navigate_callback: RefCell<Option<NavigateCallback>>,
    /// Called on search progress and completion: (files_searched, is_done).
    pub progress_callback: RefCell<Option<ProgressCallback>>,
    /// Called when "Confirm Replace" is clicked with checked replacements.
    pub replace_callback: RefCell<Option<ReplaceCallback>>,
    /// Called when "Undo" is clicked with the backup to restore.
    pub undo_callback: RefCell<Option<UndoCallback>>,
    pub message_callback: RefCell<Option<MessageCallback>>,
}

impl Default for LushtextSearchPanel {
    fn default() -> Self {
        Self {
            search_entry: TemplateChild::default(),
            results_list: TemplateChild::default(),
            results_feedback_revealer: TemplateChild::default(),
            results_body_revealer: TemplateChild::default(),
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
            replace_entry: TemplateChild::default(),
            replace_all_button: TemplateChild::default(),
            undo_button: TemplateChild::default(),
            save_button: TemplateChild::default(),
            close_button: TemplateChild::default(),
            results_header_separator: TemplateChild::default(),
            results_footer_separator: TemplateChild::default(),
            footer_box: TemplateChild::default(),
            history_popover: {
                let popover = gtk4::Popover::new();
                popover.set_autohide(true);
                popover.set_has_arrow(false);
                popover
            },
            dropdown_box: gtk4::Box::new(gtk4::Orientation::Vertical, 0),
            saved_header: {
                let label = gtk4::Label::new(Some("Saved Searches"));
                label.add_css_class("heading");
                label.set_halign(gtk4::Align::Start);
                label.set_margin_start(8);
                label.set_margin_top(6);
                label.set_margin_bottom(4);
                label
            },
            saved_searches_list: {
                let list = gtk4::ListBox::new();
                list.set_selection_mode(gtk4::SelectionMode::Single);
                list.set_activate_on_single_click(true);
                list
            },
            dropdown_separator: gtk4::Separator::new(gtk4::Orientation::Horizontal),
            recent_header: {
                let label = gtk4::Label::new(Some("Recent"));
                label.add_css_class("heading");
                label.set_halign(gtk4::Align::Start);
                label.set_margin_start(8);
                label.set_margin_top(6);
                label.set_margin_bottom(4);
                label
            },
            history_list: {
                let list = gtk4::ListBox::new();
                list.set_selection_mode(gtk4::SelectionMode::Single);
                list.set_activate_on_single_click(true);
                list
            },
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
            restoring_history: Cell::new(false),
            history_entries: RefCell::new(Vec::new()),
            saved_searches: RefCell::new(Vec::new()),
            preview_mode: Cell::new(false),
            undo_backup: RefCell::new(None),
            undo_backup_generation: Cell::new(0),
            preview_replacements: RefCell::new(Vec::new()),
            checked_indices: RefCell::new(HashSet::new()),
            match_positions: RefCell::new(Vec::new()),
            current_match_index: Cell::new(None),
            last_progress_count: Cell::new(0),
            open_file_callback: RefCell::new(None),
            close_requested_callback: RefCell::new(None),
            navigate_callback: RefCell::new(None),
            progress_callback: RefCell::new(None),
            replace_callback: RefCell::new(None),
            undo_callback: RefCell::new(None),
            message_callback: RefCell::new(None),
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

        // Assemble dropdown popover: sections → box → scrolled window → popover → parent to entry.
        self.dropdown_box.append(&self.saved_header);
        self.dropdown_box.append(&self.saved_searches_list);
        self.dropdown_box.append(&self.dropdown_separator);
        self.dropdown_box.append(&self.recent_header);
        self.dropdown_box.append(&self.history_list);

        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_max_content_height(300);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&self.dropdown_box));
        self.history_popover.set_child(Some(&scroll));
        self.history_popover.set_parent(&*self.search_entry);

        self.setup_results_list();
        self.setup_search_entry();
        self.setup_toggles();
        self.setup_options();
        self.setup_history();
        self.setup_save_button();
        self.obj().clear_stale_persisted_undo_backup();
        self.constructed_complete.set(true);
    }

    fn dispose(&self) {
        // Cancel any in-flight search thread so it stops promptly after
        // the window closes, instead of running until the walker finishes.
        if let Some(cancel) = self.cancel_token.take() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        // Unparent the programmatically-parented popover to avoid leak warnings.
        self.history_popover.unparent();
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
            // margin-end=24 prevents overlay scrollbar from obscuring the count badge.
            let content_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            content_box.set_margin_start(4);
            content_box.set_margin_end(24);
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

        let bind_panel_weak = self.obj().downgrade();
        factory.connect_bind(move |_, list_item| {
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

            // Clean up any dynamically added preview checkbox from a previous bind
            // (GtkListView recycles ListItems).
            remove_preview_checkbox(&content_box);

            // Get the four child labels.
            let file_label = content_box
                .first_child()
                .and_then(|w| w.downcast::<gtk4::Label>().ok());
            let count_badge = file_label
                .as_ref()
                .and_then(gtk4::prelude::WidgetExt::next_sibling)
                .and_then(|w| w.downcast::<gtk4::Label>().ok());
            let line_num_label = count_badge
                .as_ref()
                .and_then(gtk4::prelude::WidgetExt::next_sibling)
                .and_then(|w| w.downcast::<gtk4::Label>().ok());
            let line_content_label = line_num_label
                .as_ref()
                .and_then(gtk4::prelude::WidgetExt::next_sibling)
                .and_then(|w| w.downcast::<gtk4::Label>().ok());

            if result_item.is_file_item() {
                // File header row.
                if let Some(ref label) = file_label {
                    label.set_text(&result_item.display_path());
                    label.set_visible(true);
                }
                if let Some(ref badge) = count_badge {
                    badge.set_visible(true);
                    let binding = result_item
                        .bind_property("match-count", badge, "label")
                        .transform_to(|_: &glib::Binding, value: &glib::Value| {
                            let count: u32 = value.get().ok()?;
                            Some(format!("{count}").to_value())
                        })
                        .sync_create()
                        .build();
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
                // Match row — check if we're in preview mode.
                let in_preview = bind_panel_weak
                    .upgrade()
                    .is_some_and(|p| p.imp().preview_mode.get());

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

                if in_preview {
                    // Preview mode: show before/after markup with checkbox.
                    if let Some(panel) = bind_panel_weak.upgrade() {
                        let imp = panel.imp();
                        let file_path = result_item.file_path();
                        let line_number = result_item.line_number();

                        // Find the matching replacement by path + line_number + match_start.
                        // Using match_range.start disambiguates multiple matches on the same line.
                        let original_match_start = result_item.original_match_start() as usize;
                        let replacements = imp.preview_replacements.borrow();
                        let match_idx = replacements.iter().position(|r| {
                            r.path.display().to_string() == file_path
                                && r.line_number == u64::from(line_number)
                                && r.match_range.start == original_match_start
                        });

                        if let Some(idx) = match_idx {
                            let r = &replacements[idx];
                            let original = &r.original_line;
                            let replaced = &r.replaced_line;
                            let start = r.match_range.start.min(original.len());
                            let end = r.match_range.end.min(original.len());

                            // Two-line markup: original with match dimmed+strikethrough,
                            // replaced with new text in accent bold.
                            let markup = render_preview_markup(original, replaced, start, end);
                            let is_checked = imp.checked_indices.borrow().contains(&idx);
                            drop(replacements);

                            if let Some(ref label) = line_content_label {
                                label.set_markup(&markup);
                                label.set_visible(true);
                            }

                            // Add checkbox dynamically.
                            let checkbox = gtk4::CheckButton::new();
                            checkbox.set_active(is_checked);
                            checkbox.add_css_class("preview-check");
                            // Insert checkbox at the beginning of content_box.
                            content_box.prepend(&checkbox);

                            // Connect toggled signal to update checked_indices.
                            let panel_weak = panel.downgrade();
                            checkbox.connect_toggled(move |cb| {
                                let Some(panel) = panel_weak.upgrade() else {
                                    return;
                                };
                                let imp = panel.imp();
                                let mut indices = imp.checked_indices.borrow_mut();
                                if cb.is_active() {
                                    indices.insert(idx);
                                } else {
                                    indices.remove(&idx);
                                }
                                let checked = indices.len();
                                let total = imp.preview_replacements.borrow().len();
                                drop(indices);
                                imp.replace_all_button
                                    .set_label(&format!("Replace {checked} of {total}"));
                                imp.replace_all_button.set_sensitive(checked > 0);
                            });
                        } else {
                            drop(replacements);
                            // No matching replacement found — render normally.
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
                    }
                } else {
                    // Normal mode: standard match highlight.
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
            }

            // Disable expander gesture for match rows (same fix as sidebar file tree).
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
                let imp = panel.imp();
                if !imp.constructed_complete.get() || imp.restoring_history.get() {
                    return; // GSettings restore or history restore — skip.
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
                let imp = panel.imp();
                if !imp.constructed_complete.get() || imp.restoring_history.get() {
                    return; // GSettings restore or history restore — skip.
                }
                let query = panel.query();
                if !query.is_empty() {
                    panel.start_search(&query);
                }
            });

        // 5. Replace All / Confirm Replace button.
        let panel_weak = self.obj().downgrade();
        self.replace_all_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let imp = panel.imp();
            if imp.preview_mode.get() {
                // "Confirm Replace" mode: collect checked replacements and fire callback.
                let replacements = imp.preview_replacements.borrow();
                let checked = imp.checked_indices.borrow();
                let selected: Vec<_> = checked
                    .iter()
                    .filter_map(|&idx| replacements.get(idx).cloned())
                    .collect();
                drop(checked);
                drop(replacements);
                panel.exit_preview_mode();
                if let Some(ref cb) = *imp.replace_callback.borrow() {
                    cb(selected);
                }
            } else {
                // "Replace All" mode: enter preview (empty text = delete matches).
                let text = imp.replace_entry.text().to_string();
                if panel.has_results() {
                    panel.enter_preview_mode(&text);
                }
            }
        });

        // 6. Undo button.
        let panel_weak = self.obj().downgrade();
        self.undo_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let imp = panel.imp();
            if let Some(backup) = imp.undo_backup.borrow().clone() {
                panel.hide_undo_button();
                if let Some(ref cb) = *imp.undo_callback.borrow() {
                    cb(backup);
                }
            }
        });

        // 7. Replace entry: update button sensitivity on text change.
        let panel_weak = self.obj().downgrade();
        self.replace_entry.connect_changed(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            panel.update_replace_button_sensitivity();
        });

        // 8. Glob entry: 300ms generation-counter debounce.
        let panel_weak = self.obj().downgrade();
        self.glob_entry.connect_changed(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let imp = panel.imp();
            if imp.restoring_history.get() {
                return; // History restore — skip debounce.
            }
            let current_gen = imp.glob_generation.get().wrapping_add(1);
            imp.glob_generation.set(current_gen);

            schedule_panel_debounce(
                &panel,
                current_gen,
                |panel| panel.imp().glob_generation.get(),
                move |panel| {
                    let query = panel.query();
                    if !query.is_empty() {
                        panel.start_search(&query);
                    }
                },
            );
        });
    }

    /// Set up the search entry with debounced search triggering.
    fn setup_search_entry(&self) {
        let panel_weak = self.obj().downgrade();
        self.search_entry.connect_search_changed(move |entry| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let imp = panel.imp();

            // Dismiss history dropdown when user types (AC #6).
            if imp.history_popover.is_visible() {
                imp.history_popover.popdown();
            }

            // Suppress search during history restore (guard pattern).
            if imp.restoring_history.get() {
                return;
            }

            let query = entry.text().to_string();

            // Generation-counter debounce: 300ms.
            let generation = imp.search_generation.get().wrapping_add(1);
            imp.search_generation.set(generation);

            schedule_panel_debounce(
                &panel,
                generation,
                |panel| panel.imp().search_generation.get(),
                move |panel| panel.start_search(&query),
            );
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

        // Close button: same as Escape.
        let panel_weak = self.obj().downgrade();
        self.close_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            if let Some(ref cb) = *panel.imp().close_requested_callback.borrow() {
                cb();
            }
        });
    }

    /// Wire focus → popover show and row-activated → restore history entry.
    fn setup_history(&self) {
        // Show history dropdown when search_entry gains focus.
        let panel_weak = self.obj().downgrade();
        self.search_entry
            .connect_notify_local(Some("has-focus"), move |entry, _| {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                let imp = panel.imp();
                // Only show on focus-in (not focus-out), when entries exist,
                // and not during preview mode.
                let has_entries = !imp.history_entries.borrow().is_empty()
                    || !imp.saved_searches.borrow().is_empty();
                if entry.has_focus() && has_entries && !imp.preview_mode.get() {
                    panel.populate_dropdown();
                    imp.history_popover.popup();
                }
            });

        // Row activated in history list → restore state and trigger search.
        let panel_weak = self.obj().downgrade();
        self.history_list.connect_row_activated(move |_, row| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            let idx = row.index() as usize;
            let entry = {
                let entries = panel.imp().history_entries.borrow();
                entries.get(idx).cloned()
            };
            if let Some(entry) = entry {
                panel.restore_from_history(&entry);
            }
        });

        // Row activated in saved searches list → restore state and trigger search.
        let panel_weak = self.obj().downgrade();
        self.saved_searches_list
            .connect_row_activated(move |_, row| {
                let Some(panel) = panel_weak.upgrade() else {
                    return;
                };
                let idx = row.index() as usize;
                let entry = {
                    let entries = panel.imp().saved_searches.borrow();
                    entries.get(idx).cloned()
                };
                if let Some(entry) = entry {
                    panel.restore_from_saved_search(&entry);
                }
            });
    }

    /// Wire the save button to open the save search dialog.
    fn setup_save_button(&self) {
        let panel_weak = self.obj().downgrade();
        self.save_button.connect_clicked(move |_| {
            let Some(panel) = panel_weak.upgrade() else {
                return;
            };
            panel.show_save_search_dialog();
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

/// Remove any dynamically added preview checkbox from a content box.
/// Called at the start of `connect_bind` to clean up recycled ListItems.
fn remove_preview_checkbox(content_box: &gtk4::Box) {
    if let Some(first) = content_box.first_child()
        && first.downcast_ref::<gtk4::CheckButton>().is_some()
    {
        content_box.remove(&first);
    }
}

/// Build Pango markup for a preview row: original line with match dimmed/strikethrough,
/// then replacement line with new text accented. Two lines separated by newline.
fn render_preview_markup(
    original: &str,
    replaced: &str,
    match_start: usize,
    match_end: usize,
) -> String {
    let start = original.floor_char_boundary(match_start.min(original.len()));
    let end = original.ceil_char_boundary(match_end.min(original.len()));

    // Line 1: original with match in dim + strikethrough.
    let line1 = if start < end {
        format!(
            "{}<span strikethrough=\"true\" alpha=\"50%\">{}</span>{}",
            glib::markup_escape_text(&original[..start]),
            glib::markup_escape_text(&original[start..end]),
            glib::markup_escape_text(&original[end..]),
        )
    } else {
        glib::markup_escape_text(original).to_string()
    };

    // Line 2: replaced with the replacement region in accent bold.
    // The replacement occupies [start..start+new_len] in the replaced line.
    let new_len =
        replaced.len() as isize - original.len() as isize + (end as isize - start as isize);
    let new_end = (start as isize + new_len).max(start as isize) as usize;
    let new_end = replaced.ceil_char_boundary(new_end.min(replaced.len()));
    let new_start = replaced.floor_char_boundary(start.min(replaced.len()));

    let line2 = if new_start < new_end {
        format!(
            "{}<b>{}</b>{}",
            glib::markup_escape_text(&replaced[..new_start]),
            glib::markup_escape_text(&replaced[new_start..new_end]),
            glib::markup_escape_text(&replaced[new_end..]),
        )
    } else {
        glib::markup_escape_text(replaced).to_string()
    };

    format!("{line1}\n{line2}")
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

fn schedule_panel_debounce<F>(
    panel: &super::LushtextSearchPanel,
    generation: u32,
    current_generation: fn(&super::LushtextSearchPanel) -> u32,
    callback: F,
) where
    F: FnOnce(super::LushtextSearchPanel) + 'static,
{
    let panel_weak = panel.downgrade();
    let callback = RefCell::new(Some(callback));
    glib::timeout_add_local_once(Duration::from_millis(SEARCH_INPUT_DEBOUNCE_MS), move || {
        let Some(panel) = panel_weak.upgrade() else {
            return;
        };
        if current_generation(&panel) != generation {
            return;
        }
        if let Some(callback) = callback.borrow_mut().take() {
            callback(panel);
        }
    });
}
