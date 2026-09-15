// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget coverage for the workspace entry visibility contract: the hidden-files
//! view mode and the always-excluded names list, applied identically by the
//! sidebar tree, its empty-folder probes, the command palette index, and
//! workspace content search.
//!
//! Every test resets both keys on exit because the widget harness shares one
//! in-memory GSettings backend across the whole process.

use crate::common::{
    ensure_gtk_init, find_descendant, fixture, flush_after_delay, flush_events, present_window,
    test_window, wait_for_palette_index, wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use libadwaita::prelude::*;
use lushtext_core::config::{self, keys};
use lushtext_core::model::content_search::{ContentSearchOptions, SavedSearch, SearchQuerySpec};
use lushtext_core::model::workspace::{WorkspaceConfig, WorkspaceId, WorkspacesFile};
use lushtext_core::model::workspace_visibility::WorkspaceEntryVisibility;
use lushtext_core::services::{bookmark_service, json_store, workspace_manager};
use lushtext_core::ui::automation::app_snapshot;
use lushtext_core::ui::command_palette::item::PaletteItem;
use lushtext_core::ui::preferences::LushtextPreferences;
use lushtext_core::ui::sidebar::file_tree_item::FileTreeItem;
use lushtext_core::ui::sidebar::workspace_section::LushtextWorkspaceSection;
use lushtext_core::ui::window::LushtextWindow;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// Reset both visibility keys when the test ends so later tests start clean.
struct VisibilityKeysReset(gio::Settings);

impl VisibilityKeysReset {
    fn new() -> Self {
        ensure_gtk_init();
        let settings = gio::Settings::new(config::APP_ID);
        settings.reset(keys::WORKSPACE_SHOW_HIDDEN_FILES);
        settings.reset(keys::WORKSPACE_EXCLUDED_NAMES);
        Self(settings)
    }

    fn settings(&self) -> &gio::Settings {
        &self.0
    }

    fn set_show_hidden(&self, value: bool) {
        self.0
            .set_boolean(keys::WORKSPACE_SHOW_HIDDEN_FILES, value)
            .expect("set show-hidden key");
        flush_events();
    }

    fn set_excluded(&self, names: &[&str]) {
        self.0
            .set_strv(keys::WORKSPACE_EXCLUDED_NAMES, names)
            .expect("set excluded-names key");
        flush_events();
    }

    fn excluded(&self) -> Vec<String> {
        self.0
            .strv(keys::WORKSPACE_EXCLUDED_NAMES)
            .iter()
            .map(ToString::to_string)
            .collect()
    }
}

impl Drop for VisibilityKeysReset {
    fn drop(&mut self) {
        self.0.reset(keys::WORKSPACE_SHOW_HIDDEN_FILES);
        self.0.reset(keys::WORKSPACE_EXCLUDED_NAMES);
        flush_events();
    }
}

/// One workspace folder seeded with hidden, excluded, and ordinary entries.
struct VisibilityFixture {
    _dir: tempfile::TempDir,
    folder: PathBuf,
}

impl VisibilityFixture {
    /// Layout (every file contains the token `needle`):
    ///
    /// ```text
    /// folder/
    ///   visible.rs
    ///   .env
    ///   .git/HEAD
    ///   .github/workflows/ci.yml
    ///   dot_only/.gitkeep
    ///   src/main.rs
    ///   src/.hidden.rs
    ///   node_modules/pkg.js
    /// ```
    fn seed() -> Self {
        Self::with_files(
            "project",
            &[
                "visible.rs",
                ".env",
                ".git/HEAD",
                ".github/workflows/ci.yml",
                "dot_only/.gitkeep",
                "src/main.rs",
                "src/.hidden.rs",
                "node_modules/pkg.js",
            ],
        )
    }

    /// One workspace folder named `folder_name` holding `files`, each containing `needle`.
    fn with_files(folder_name: &str, files: &[&str]) -> Self {
        ensure_gtk_init();
        let dir = tempfile::tempdir().expect("visibility fixture tempdir");
        let folder = dir.path().join(folder_name);
        fixture::create_dir_all(&folder);
        for relative in files {
            let path = folder.join(relative);
            fixture::create_dir_all(path.parent().expect("fixture parent"));
            fixture::write_text(&path, "needle\n");
        }
        Self { _dir: dir, folder }
    }

    fn save_as_workspace(&self) {
        let mut workspaces = WorkspacesFile::default();
        workspaces.workspaces.push(WorkspaceConfig::with_one_folder(
            WorkspaceId::new("visibility"),
            "visibility",
            self.folder.clone(),
        ));
        workspace_manager::save(&json_store::data_dir(), &workspaces).expect("save workspaces");
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.folder.join(relative)
    }
}

fn present_workspace_window(
    fixture: &VisibilityFixture,
) -> (LushtextWindow, LushtextWorkspaceSection) {
    fixture.save_as_workspace();
    let window = test_window();
    window.set_default_size(1200, 800);
    present_window(&window);
    let sidebar = window.imp().sidebar.clone();
    sidebar.load_workspaces();
    flush_after_delay(Duration::from_millis(300));
    wait_until(Duration::from_secs(10), || {
        let sections = sidebar.imp().sections.borrow();
        sections.len() == 1 && sections.iter().all(WidgetExt::is_mapped)
    });
    let section = sidebar.imp().sections.borrow()[0].clone();
    // The workspace folder is the top-level row; its entries appear once it is
    // expanded, exactly as the other sidebar tests drive it.
    wait_until(Duration::from_secs(10), || {
        tree_row_for(&section, &fixture.folder).is_some()
    });
    section.expand_folders();
    if fixture.path("visible.rs").is_file() {
        wait_until(Duration::from_secs(10), || {
            tree_paths(&section).contains(&fixture.path("visible.rs"))
        });
    }
    (window, section)
}

fn tree_model(section: &LushtextWorkspaceSection) -> Option<gtk4::TreeListModel> {
    section.imp().tree_model.borrow().as_ref().cloned()
}

fn tree_rows(section: &LushtextWorkspaceSection) -> Vec<(gtk4::TreeListRow, FileTreeItem)> {
    let Some(model) = tree_model(section) else {
        return Vec::new();
    };
    (0..model.n_items())
        .filter_map(|index| {
            let row = model.item(index).and_downcast::<gtk4::TreeListRow>()?;
            let item = row.item().and_downcast::<FileTreeItem>()?;
            Some((row, item))
        })
        .collect()
}

fn tree_paths(section: &LushtextWorkspaceSection) -> BTreeSet<PathBuf> {
    tree_rows(section)
        .into_iter()
        .filter_map(|(_, item)| item.path())
        .collect()
}

fn tree_row_for(
    section: &LushtextWorkspaceSection,
    path: &Path,
) -> Option<(gtk4::TreeListRow, FileTreeItem)> {
    tree_rows(section)
        .into_iter()
        .find(|(_, item)| item.path().as_deref() == Some(path))
}

fn expand_row(section: &LushtextWorkspaceSection, path: &Path) {
    wait_until(Duration::from_secs(10), || {
        tree_row_for(section, path).is_some()
    });
    let (row, _) = tree_row_for(section, path).expect("row to expand");
    row.set_expanded(true);
    flush_events();
}

/// Every file (not directory) reachable in the tree after expanding all folders.
fn sidebar_visible_files(section: &LushtextWorkspaceSection, folder: &Path) -> BTreeSet<PathBuf> {
    section.expand_folders();
    flush_after_delay(Duration::from_millis(200));
    let mut progress = true;
    while progress {
        progress = false;
        for (row, item) in tree_rows(section) {
            if item.is_dir() && row.is_expandable() && !row.is_expanded() {
                row.set_expanded(true);
                progress = true;
            }
        }
        flush_after_delay(Duration::from_millis(300));
    }
    tree_rows(section)
        .into_iter()
        .filter(|(_, item)| !item.is_dir())
        .filter_map(|(_, item)| item.path())
        .filter(|path| path.starts_with(folder))
        .collect()
}

fn app_of(window: &LushtextWindow) -> lushtext_core::app::LushtextApplication {
    window
        .application()
        .expect("window should have an application")
        .downcast::<lushtext_core::app::LushtextApplication>()
        .expect("test app should be LushtextApplication")
}

fn show_hidden_action(window: &LushtextWindow) -> gio::Action {
    app_of(window)
        .lookup_action("show-hidden-files")
        .expect("app.show-hidden-files must be registered")
}

fn action_state(action: &gio::Action) -> bool {
    action
        .state()
        .and_then(|state| state.get::<bool>())
        .expect("show-hidden-files should carry bool state")
}

fn menu_model_entries(model: &gio::MenuModel) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for index in 0..model.n_items() {
        let label = model
            .item_attribute_value(index, "label", Some(glib::VariantTy::STRING))
            .and_then(|variant| variant.get::<String>());
        let action = model
            .item_attribute_value(index, "action", Some(glib::VariantTy::STRING))
            .and_then(|variant| variant.get::<String>());
        if let (Some(label), Some(action)) = (label, action) {
            entries.push((label, action));
        }
        for link_name in ["section", "submenu"] {
            if let Some(link) = model.item_link(index, link_name) {
                entries.extend(menu_model_entries(&link));
            }
        }
    }
    entries
}

/// File names the palette index currently exposes, read through one settled
/// empty query (which returns sources in order without scoring).
fn palette_file_names(window: &LushtextWindow) -> BTreeSet<String> {
    let palette = window.imp().command_palette.clone();
    palette.restart_query_for_test("");
    flush_after_delay(Duration::from_millis(200));
    wait_until(Duration::from_secs(10), || !palette.evidence().searching);
    let store = palette.imp().results_store.clone();
    (0..store.n_items())
        .filter_map(|index| store.item(index).and_downcast::<PaletteItem>())
        .filter(PaletteItem::is_file)
        .map(|item| item.display_name())
        .collect()
}

fn search_and_collect_files(window: &LushtextWindow, spec: &SearchQuerySpec) -> BTreeSet<PathBuf> {
    let panel = window.imp().search_panel.clone();
    panel.start_search(spec);
    wait_until(Duration::from_secs(10), || !panel.is_searching());
    let evidence = panel.evidence();
    assert!(
        !evidence.searching,
        "search must have completed before collecting results"
    );
    panel
        .imp()
        .runtime
        .file_groups
        .borrow()
        .keys()
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

#[test]
fn test_sidebar_toggle_reveals_dotfiles_preserves_expansion_and_stays_silent() {
    let reset = VisibilityKeysReset::new();
    let fixture = VisibilityFixture::seed();
    let (_window, section) = present_workspace_window(&fixture);

    let messages: Rc<RefCell<Vec<String>>> = Rc::default();
    let sink = Rc::clone(&messages);
    section.connect_message(move |message, _| sink.borrow_mut().push(message.to_owned()));

    expand_row(&section, &fixture.path("src"));
    wait_until(Duration::from_secs(10), || {
        tree_paths(&section).contains(&fixture.path("src/main.rs"))
    });
    let before = tree_paths(&section);
    assert!(!before.contains(&fixture.path(".env")));
    assert!(!before.contains(&fixture.path(".git")));
    assert!(!before.contains(&fixture.path("src/.hidden.rs")));
    assert!(before.contains(&fixture.path("node_modules")));
    let mounted = tree_model(&section).expect("tree model mounted");
    let (_, dot_only) = tree_row_for(&section, &fixture.path("dot_only")).expect("dot_only row");
    wait_until(Duration::from_secs(10), || {
        dot_only.is_empty() == Some(true)
    });

    reset.set_show_hidden(true);
    wait_until(Duration::from_secs(10), || {
        let paths = tree_paths(&section);
        paths.contains(&fixture.path(".env")) && paths.contains(&fixture.path("src/.hidden.rs"))
    });
    let after = tree_paths(&section);
    assert!(
        !after.contains(&fixture.path(".git")),
        ".git stays excluded even with hidden files on"
    );
    assert!(after.contains(&fixture.path(".github")));
    assert_eq!(
        tree_model(&section).as_ref(),
        Some(&mounted),
        "toggling visibility must reconcile in place, not remount the tree model"
    );
    let (src_row, _) = tree_row_for(&section, &fixture.path("src")).expect("src row");
    assert!(src_row.is_expanded(), "expanded directory stays expanded");
    let (_, dot_only) = tree_row_for(&section, &fixture.path("dot_only")).expect("dot_only row");
    wait_until(Duration::from_secs(10), || {
        dot_only.is_empty() == Some(false)
    });

    reset.set_show_hidden(false);
    wait_until(Duration::from_secs(10), || {
        !tree_paths(&section).contains(&fixture.path(".env"))
    });

    assert!(
        !messages
            .borrow()
            .iter()
            .any(|message| message.contains("Refreshing workspace folders")),
        "a visibility toggle must not announce a manual refresh: {:?}",
        messages.borrow()
    );
}

#[test]
fn test_top_level_dot_only_workspace_folder_stops_being_empty() {
    let reset = VisibilityKeysReset::new();
    let fixture = VisibilityFixture::with_files("dot-only", &[".gitkeep"]);
    let folder = fixture.folder.clone();
    let (_window, section) = present_workspace_window(&fixture);
    wait_until(Duration::from_secs(10), || {
        tree_row_for(&section, &folder).is_some_and(|(_, item)| item.is_empty() == Some(true))
    });

    reset.set_show_hidden(true);
    wait_until(Duration::from_secs(10), || {
        tree_row_for(&section, &folder).is_some_and(|(_, item)| item.is_empty() == Some(false))
    });
    let (row, _) = tree_row_for(&section, &folder).expect("top-level folder row");
    assert!(
        row.is_expandable(),
        "revealed top-level folder becomes expandable"
    );
}

#[test]
fn test_excluded_name_hides_non_dot_entries_and_subscription_is_owned_once() {
    let reset = VisibilityKeysReset::new();
    let fixture = VisibilityFixture::seed();
    let (window, section) = present_workspace_window(&fixture);
    assert!(tree_paths(&section).contains(&fixture.path("node_modules")));

    assert_eq!(
        window.imp().sidebar.imp().visibility_signals.len(),
        2,
        "the sidebar owns exactly one handler per visibility key"
    );

    reset.set_excluded(&[".git", "node_modules"]);
    wait_until(Duration::from_secs(10), || {
        !tree_paths(&section).contains(&fixture.path("node_modules"))
    });
    assert!(tree_paths(&section).contains(&fixture.path("visible.rs")));

    // Removing every name re-reveals it and, because .git is no longer excluded
    // but is still a dotfolder, .git stays hidden while the mode is off.
    reset.set_excluded(&[]);
    wait_until(Duration::from_secs(10), || {
        tree_paths(&section).contains(&fixture.path("node_modules"))
    });
    assert!(!tree_paths(&section).contains(&fixture.path(".git")));
}

#[test]
fn test_dot_name_renamed_in_place_stays_visible_until_next_refresh() {
    let _reset = VisibilityKeysReset::new();
    let fixture = VisibilityFixture::seed();
    let (_window, section) = present_workspace_window(&fixture);
    let plain = fixture.path("visible.rs");
    let renamed = fixture.path(".visible.rs");

    let (_, item) = tree_row_for(&section, &plain).expect("visible.rs row");
    fixture::rename(&plain, &renamed);
    item.set_path(renamed.clone());
    flush_events();
    assert!(
        tree_paths(&section).contains(&renamed),
        "an in-place rename keeps the row until the next refresh"
    );

    section.queue_auto_refresh_for_test(vec![fixture.folder]);
    wait_until(Duration::from_secs(10), || {
        !tree_paths(&section).contains(&renamed)
    });
}

#[test]
fn test_sidebar_header_row_keeps_only_selector_and_new_workspace_button() {
    let _reset = VisibilityKeysReset::new();
    let window = test_window();
    present_window(&window);
    let sidebar = window.imp().sidebar.clone();
    let header = &*sidebar.imp().new_workspace_box;
    let mut children = Vec::new();
    let mut child = header.first_child();
    while let Some(widget) = child {
        children.push(widget.type_().name().to_owned());
        child = widget.next_sibling();
    }
    assert_eq!(children, vec!["GtkDropDown", "GtkButton"]);
}

// ---------------------------------------------------------------------------
// Command palette
// ---------------------------------------------------------------------------

#[test]
fn test_palette_index_follows_visibility_keys_and_keeps_internal_skips() {
    let reset = VisibilityKeysReset::new();
    let fixture = VisibilityFixture::seed();
    let (window, section) = present_workspace_window(&fixture);

    // node_modules is skipped by the palette's internal performance list while
    // remaining visible in the sidebar; dotfiles are hidden by the mode.
    wait_for_palette_index(&window, 2);
    assert_eq!(
        palette_file_names(&window),
        BTreeSet::from(["main.rs".to_owned(), "visible.rs".to_owned()])
    );
    assert!(tree_paths(&section).contains(&fixture.path("node_modules")));

    reset.set_show_hidden(true);
    wait_for_palette_index(&window, 6);
    assert_eq!(
        palette_file_names(&window),
        BTreeSet::from([
            ".env".to_owned(),
            ".gitkeep".to_owned(),
            ".hidden.rs".to_owned(),
            "ci.yml".to_owned(),
            "main.rs".to_owned(),
            "visible.rs".to_owned(),
        ]),
        "dotfiles are indexed, .git stays excluded, node_modules stays internally skipped"
    );

    reset.set_excluded(&[".git", ".github"]);
    wait_for_palette_index(&window, 5);
    assert!(!palette_file_names(&window).contains("ci.yml"));
}

// ---------------------------------------------------------------------------
// App action, menu, shortcut, snapshot
// ---------------------------------------------------------------------------

#[test]
fn test_show_hidden_files_action_mirrors_key_menu_shortcut_and_snapshot() {
    let reset = VisibilityKeysReset::new();
    let window = test_window();
    present_window(&window);
    let app = app_of(&window);
    let action = show_hidden_action(&window);
    assert!(!action_state(&action));

    action.activate(None);
    flush_events();
    assert!(reset.settings().boolean(keys::WORKSPACE_SHOW_HIDDEN_FILES));
    assert!(action_state(&action));

    reset.set_show_hidden(false);
    assert!(
        !action_state(&action),
        "key writes flow back into action state"
    );

    action.change_state(&true.to_variant());
    flush_events();
    assert!(reset.settings().boolean(keys::WORKSPACE_SHOW_HIDDEN_FILES));

    // GTK normalizes modifier order when it stores accelerators.
    assert_eq!(
        app.accels_for_action("app.show-hidden-files"),
        vec!["<Shift><Control>h"]
    );
    assert!(
        !app.accels_for_action("win.begin-replace")
            .iter()
            .any(|accel| accel.contains("Shift")),
        "Find and Replace keeps its own Ctrl+H binding"
    );

    let menu = window
        .imp()
        .primary_menu_button
        .menu_model()
        .expect("primary menu model");
    assert!(
        menu_model_entries(&menu)
            .iter()
            .any(|(label, action)| label == "Show _Hidden Files"
                && action == "app.show-hidden-files"),
        "main menu carries the check item"
    );

    reset.set_excluded(&[".git", "vendor"]);
    let snapshot = app_snapshot(&app);
    let workspace = snapshot.window.expect("window snapshot").workspace;
    assert!(workspace.show_hidden_files);
    assert_eq!(workspace.excluded_names, vec![".git", "vendor"]);
    assert!(!workspace.excluded_names_truncated);
}

// ---------------------------------------------------------------------------
// Preferences editor
// ---------------------------------------------------------------------------

fn excluded_row_titles(prefs: &LushtextPreferences) -> Vec<String> {
    prefs
        .imp()
        .workspace_excluded_rows
        .borrow()
        .iter()
        .map(|row| row.title().to_string())
        .collect()
}

fn submit_excluded_name(prefs: &LushtextPreferences, text: &str) {
    let row = &*prefs.imp().workspace_excluded_add_row;
    row.set_text(text);
    row.emit_by_name::<()>("apply", &[]);
    flush_events();
}

#[test]
fn test_excluded_names_editor_adds_removes_resets_and_reprojects() {
    let reset = VisibilityKeysReset::new();
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();
    assert_eq!(excluded_row_titles(&prefs), vec![".git"]);
    assert!(imp.workspace_excluded_empty_row.borrow().is_none());

    submit_excluded_name(&prefs, " build ");
    assert_eq!(reset.excluded(), vec![".git", "build"]);
    assert_eq!(excluded_row_titles(&prefs), vec![".git", "build"]);
    assert_eq!(imp.workspace_excluded_add_row.text().as_str(), "");
    assert!(!imp.workspace_excluded_add_row.has_css_class("error"));

    submit_excluded_name(&prefs, "a/b");
    assert_eq!(reset.excluded(), vec![".git", "build"]);
    assert!(imp.workspace_excluded_add_row.has_css_class("error"));
    submit_excluded_name(&prefs, "   ");
    assert_eq!(reset.excluded(), vec![".git", "build"]);
    assert!(imp.workspace_excluded_add_row.has_css_class("error"));

    submit_excluded_name(&prefs, "build");
    assert_eq!(
        reset.excluded(),
        vec![".git", "build"],
        "duplicates add nothing"
    );
    assert!(
        !imp.workspace_excluded_add_row.has_css_class("error"),
        "a duplicate focuses the existing row instead of erroring"
    );

    let git_row = imp.workspace_excluded_rows.borrow()[0].clone();
    let remove = find_descendant(git_row.upcast_ref(), |widget| {
        widget
            .downcast_ref::<gtk4::Button>()
            .is_some_and(|button| button.icon_name().as_deref() == Some("list-remove-symbolic"))
    })
    .and_downcast::<gtk4::Button>()
    .expect("remove button on the .git row");
    remove.emit_clicked();
    flush_events();
    assert_eq!(reset.excluded(), vec!["build"]);
    assert_eq!(excluded_row_titles(&prefs), vec!["build"]);

    // External edits re-project without duplicating rows.
    reset.set_excluded(&[]);
    assert!(excluded_row_titles(&prefs).is_empty());
    assert!(imp.workspace_excluded_empty_row.borrow().is_some());
    reset.set_excluded(&["one", "two"]);
    assert_eq!(excluded_row_titles(&prefs), vec!["one", "two"]);
    assert!(imp.workspace_excluded_empty_row.borrow().is_none());

    imp.workspace_excluded_reset_button.emit_clicked();
    flush_events();
    assert_eq!(reset.excluded(), vec![".git"]);
    assert_eq!(excluded_row_titles(&prefs), vec![".git"]);
}

#[test]
fn test_preferences_switch_and_app_action_agree() {
    let reset = VisibilityKeysReset::new();
    let window = test_window();
    present_window(&window);
    let action = show_hidden_action(&window);
    let prefs = LushtextPreferences::new();
    let imp = prefs.imp();
    assert!(!imp.workspace_show_hidden_files_row.is_active());

    imp.workspace_show_hidden_files_row.set_active(true);
    flush_events();
    assert!(reset.settings().boolean(keys::WORKSPACE_SHOW_HIDDEN_FILES));
    assert!(action_state(&action));

    action.activate(None);
    flush_events();
    assert!(!imp.workspace_show_hidden_files_row.is_active());
}

// ---------------------------------------------------------------------------
// Search panel
// ---------------------------------------------------------------------------

#[test]
fn test_search_panel_hidden_toggle_seeds_overrides_and_restores() {
    let reset = VisibilityKeysReset::new();
    let fixture = VisibilityFixture::seed();
    let (window, _section) = present_workspace_window(&fixture);
    let panel = window.imp().search_panel.clone();

    reset.set_show_hidden(true);
    ActionGroupExt::activate_action(&window, "toggle-search-panel", None);
    flush_events();
    wait_until(Duration::from_secs(5), || panel.is_mapped());
    assert!(
        panel.imp().hidden_toggle.is_active(),
        "seeded from the global key on open"
    );
    assert!(panel.evidence().hidden_enabled);

    panel.imp().hidden_toggle.set_active(false);
    flush_events();
    assert!(
        reset.settings().boolean(keys::WORKSPACE_SHOW_HIDDEN_FILES),
        "the per-search override never writes the global key"
    );
    assert!(!panel.evidence().hidden_enabled);

    // An idle open panel follows a global change.
    reset.set_show_hidden(false);
    reset.set_show_hidden(true);
    wait_until(Duration::from_secs(2), || {
        panel.imp().hidden_toggle.is_active()
    });

    let files = search_and_collect_files(
        &window,
        &SearchQuerySpec::new(
            "needle".to_owned(),
            ContentSearchOptions::default().with_hidden(true),
        ),
    );
    assert!(files.contains(&fixture.path(".env")));
    assert!(files.contains(&fixture.path("visible.rs")));
    assert!(
        !files
            .iter()
            .any(|path| path.starts_with(fixture.path(".git"))),
        "excluded names are never searched: {files:?}"
    );

    let saved = SavedSearch::from_spec(
        "hidden search".to_owned(),
        SearchQuerySpec::new(
            "needle".to_owned(),
            ContentSearchOptions::default().with_hidden(true),
        ),
    );
    panel.imp().hidden_toggle.set_active(false);
    flush_events();
    panel.restore_from_saved_search(&saved);
    flush_events();
    assert!(
        panel.imp().hidden_toggle.is_active(),
        "saved search restores the toggle"
    );
    assert!(saved.spec.options.toggle_summary().contains("hidden"));
    wait_until(Duration::from_secs(10), || !panel.is_searching());
}

// ---------------------------------------------------------------------------
// Notes and bookmarks inside hidden folders
// ---------------------------------------------------------------------------

#[test]
fn test_bookmark_inside_hidden_folder_is_browsable_and_row_appears_when_revealed() {
    let reset = VisibilityKeysReset::new();
    let fixture = VisibilityFixture::seed();
    let path = fixture.path(".github/workflows/ci.yml");
    bookmark_service::save_for_path(
        &json_store::data_dir(),
        &path,
        &[lushtext_core::model::bookmark::BookmarkRecord::new(
            0,
            Some("hidden bookmark".to_owned()),
        )],
    )
    .expect("save bookmark sidecar");
    let (window, section) = present_workspace_window(&fixture);

    window.open_document(&path);
    wait_until(Duration::from_secs(10), || {
        window.notes_evidence().active_document_bookmark_count == 1
    });
    assert!(window.notes_evidence().active_document_file_backed);

    ActionGroupExt::activate_action(&window, "show-bookmarks", None);
    flush_events();
    wait_until(Duration::from_secs(10), || {
        window
            .notes_evidence()
            .browser
            .is_some_and(|browser| browser.source.active == 0 && browser.source.pending == 0)
    });
    assert!(
        window.notes_evidence().browser.is_some(),
        "Browse Bookmarks opens for a file inside a hidden folder"
    );

    assert!(!tree_paths(&section).contains(&path));
    reset.set_show_hidden(true);
    expand_row(&section, &fixture.path(".github"));
    expand_row(&section, &fixture.path(".github/workflows"));
    wait_until(Duration::from_secs(10), || {
        tree_paths(&section).contains(&path)
    });
    assert_eq!(window.notes_evidence().active_document_bookmark_count, 1);
}

// ---------------------------------------------------------------------------
// Cross-surface parity
// ---------------------------------------------------------------------------

#[test]
fn test_sidebar_palette_and_search_agree_on_visible_files_in_both_modes() {
    let reset = VisibilityKeysReset::new();
    // Deliberately no palette-internal skip names (node_modules, vendor, …) so
    // the three surfaces are governed by the visibility rule alone.
    let files = [
        "visible.rs",
        ".env",
        ".git/HEAD",
        "build/out.txt",
        ".config/app.toml",
        "nested/.dot/deep.txt",
    ];
    let fixture = VisibilityFixture::with_files(".config-workspace", &files);
    let folder = fixture.folder.clone();
    reset.set_excluded(&[".git", "build"]);
    let (window, section) = present_workspace_window(&fixture);

    for show_hidden in [false, true] {
        reset.set_show_hidden(show_hidden);
        let rule = WorkspaceEntryVisibility::new(show_hidden, [".git", "build"]);
        let expected: BTreeSet<PathBuf> = files
            .iter()
            .map(|relative| folder.join(relative))
            .filter(|path| {
                path.strip_prefix(&folder)
                    .expect("inside folder")
                    .components()
                    .all(|component| rule.admits(component.as_os_str()))
            })
            .collect();
        let expected_names: BTreeSet<String> = expected
            .iter()
            .map(|path| {
                path.file_name()
                    .expect("fixture file name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();

        wait_for_palette_index(&window, expected.len());
        let observed = RefCell::new(BTreeSet::new());
        wait_until(Duration::from_secs(10), || {
            *observed.borrow_mut() = sidebar_visible_files(&section, &folder);
            *observed.borrow() == expected
        });
        assert_eq!(
            observed.into_inner(),
            expected,
            "sidebar, hidden={show_hidden}"
        );
        assert_eq!(
            palette_file_names(&window),
            expected_names,
            "palette, hidden={show_hidden}"
        );
        let searched = search_and_collect_files(
            &window,
            &SearchQuerySpec::new(
                "needle".to_owned(),
                ContentSearchOptions::default().with_hidden(show_hidden),
            ),
        );
        assert_eq!(searched, expected, "search, hidden={show_hidden}");
    }
}
