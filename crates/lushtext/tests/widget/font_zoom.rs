// SPDX-License-Identifier: GPL-3.0-or-later

//! Widget tests for `scope-font-zoom-restyle`: a zoom step swaps one class on
//! the registered font-following widgets and loads no provider; a base-font
//! change loads one provider in a dispatch that swaps nothing and swaps later;
//! and every surface computes the font `main`'s display-wide rules gave it
//! (ledger A86, `ui/font_zoom.rs`).

use std::path::Path;
use std::time::Duration;

use crate::common::{
    activate_action, ensure_gtk_init, fixture, flush_after_delay, flush_events, present_window,
    test_window, wait_for_painted_frame, wait_until,
};
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio::prelude::SettingsExt;
use gtk4::prelude::*;
use lushtext_core::config::keys;
use lushtext_core::ui::editor_page::{EditorLoadState, LushtextEditorPage};
use lushtext_core::ui::font_zoom::{self, FontZoomEvidence};
use lushtext_core::ui::settings_writes;
use lushtext_core::ui::window::LushtextWindow;

/// The editor surface's line-height guard (as `ui/font_zoom/policy.rs`).
const LINE_HEIGHT: f64 = 1.18;

fn editor_at(window: &LushtextWindow, index: i32) -> LushtextEditorPage {
    window
        .imp()
        .tab_view
        .nth_page(index)
        .child()
        .downcast::<LushtextEditorPage>()
        .expect("editor page")
}

fn open_loaded(window: &LushtextWindow, path: &Path) -> LushtextEditorPage {
    window.open_document(path);
    let index = window.imp().tab_view.n_pages() - 1;
    let editor = editor_at(window, index);
    wait_until(Duration::from_secs(10), || {
        matches!(editor.load_state(), EditorLoadState::Loaded)
    });
    flush_events();
    editor
}

fn zoom_classes(widget: &impl IsA<gtk4::Widget>) -> Vec<String> {
    widget
        .css_classes()
        .iter()
        .filter(|class| class.starts_with("lushtext-font-"))
        .map(ToString::to_string)
        .collect()
}

fn set_zoom(zoom: u32) {
    settings_writes::set_uint(keys::ZOOM_LEVEL, zoom);
}

/// Wait until no generation is pending (the swap of a base-font change ran).
fn wait_swapped() {
    wait_until(Duration::from_secs(5), || {
        font_zoom::evidence().pending_generation.is_none()
    });
    flush_after_delay(Duration::from_millis(50));
}

fn pango_description(widget: &impl IsA<gtk4::Widget>) -> String {
    widget
        .pango_context()
        .font_description()
        .map(|description| description.to_string())
        .unwrap_or_default()
}

/// The desktop monospace font the theme reads with the system font.
fn desktop_monospace() -> String {
    let source = gtk4::gio::SettingsSchemaSource::default();
    match source.and_then(|source| source.lookup("org.gnome.desktop.interface", true)) {
        Some(_) => gtk4::gio::Settings::new("org.gnome.desktop.interface")
            .string("monospace-font-name")
            .to_string(),
        None => "Monospace 11".to_owned(),
    }
}

/// `apply_font_css`'s rules on `main`, transcribed, for the current settings.
fn legacy_css() -> String {
    let settings = settings_writes::shared();
    let zoom = settings.uint(keys::ZOOM_LEVEL).clamp(50, 400);
    let use_system = settings.boolean(keys::USE_SYSTEM_FONT);
    let desc = if use_system {
        gtk4::pango::FontDescription::from_string(&desktop_monospace())
    } else {
        gtk4::pango::FontDescription::from_string(&settings.string(keys::CUSTOM_FONT))
    };
    let family = desc.family().unwrap_or_else(|| "Monospace".into());
    let base_pt = {
        let raw = f64::from(desc.size()) / f64::from(gtk4::pango::SCALE);
        if raw > 0.0 { raw } else { 11.0 }
    };
    let zoomed_pt = base_pt * f64::from(zoom) / 100.0;
    let editor_css = format!(
        "textview.tab-content-editor-surface, textview.tab-content-editor-surface text {{ font-family: \"{family}\"; font-size: {zoomed_pt:.1}pt; line-height: {LINE_HEIGHT:.2}; }}"
    );
    if use_system && zoom == 100 {
        editor_css
    } else {
        format!(
            ".monospace {{ font-family: \"{family}\"; font-size: {zoomed_pt:.1}pt; }}\n{editor_css}"
        )
    }
}

/// What a widget computes: its Pango font and, for a text view, the first
/// line's height (the editor's line height).
#[derive(Debug, PartialEq, Eq)]
struct Computed {
    font: String,
    line_height: Option<i32>,
}

fn computed(widget: &gtk4::Widget) -> Computed {
    let line_height = widget.downcast_ref::<gtk4::TextView>().map(|view| {
        let start = view.buffer().start_iter();
        view.line_yrange(&start).1
    });
    Computed {
        font: pango_description(widget),
        line_height,
    }
}

/// Read `widgets` with the zoom classes (this change), then with the classes
/// removed and `main`'s display-wide rules loaded instead, and compare.
fn assert_matches_legacy(window: &LushtextWindow, widgets: &[gtk4::Widget], state: &str) {
    wait_for_painted_frame(window);
    flush_after_delay(Duration::from_millis(50));
    let ours: Vec<Computed> = widgets.iter().map(computed).collect();
    let class = font_zoom::evidence()
        .current_class
        .expect("a current class");
    for widget in widgets {
        widget.remove_css_class(&class);
    }
    let display = gtk4::gdk::Display::default().expect("display");
    let legacy = gtk4::CssProvider::new();
    legacy.load_from_string(&legacy_css());
    gtk4::style_context_add_provider_for_display(
        &display,
        &legacy,
        gtk4::STYLE_PROVIDER_PRIORITY_USER,
    );
    window.queue_draw();
    wait_for_painted_frame(window);
    flush_after_delay(Duration::from_millis(50));
    let theirs: Vec<Computed> = widgets.iter().map(computed).collect();
    gtk4::style_context_remove_provider_for_display(&display, &legacy);
    for widget in widgets {
        widget.add_css_class(&class);
    }
    window.queue_draw();
    wait_for_painted_frame(window);
    assert_eq!(
        ours, theirs,
        "{state}: the zoom classes compute main's fonts"
    );
}

fn markdown_document(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("notes.md");
    fixture::write_text(
        &path,
        "# Title\n\n```rust\nfn main() {}\n```\n\n![missing](does-not-exist.png)\n",
    );
    path
}

/// The registered font-following widgets of `window`'s first editor and its
/// Markdown preview, plus a `.monospace` label registered after the window.
fn surfaces(editor: &LushtextEditorPage) -> Vec<gtk4::Widget> {
    let mut widgets: Vec<gtk4::Widget> = vec![editor.source_view().clone().upcast()];
    for widget in crate::common::every_widget() {
        if widget.has_css_class("monospace") && font_zoom::is_registered(&widget) {
            widgets.push(widget);
        }
    }
    widgets
}

/// Set `gtk-interface-contrast` (GTK 4.20+) by its nick.
fn set_contrast(settings: &gtk4::Settings, nick: &str) {
    let pspec = settings
        .find_property("gtk-interface-contrast")
        .expect("gtk-interface-contrast");
    let class = glib::EnumClass::with_type(pspec.value_type()).expect("an enum property");
    let value = class.to_value_by_nick(nick).expect("a known contrast");
    settings.set_property_from_value("gtk-interface-contrast", &value);
}

fn reset_font_settings() {
    settings_writes::set_boolean(keys::USE_SYSTEM_FONT, true);
    settings_writes::set_string(keys::CUSTOM_FONT, "Monospace 11");
    set_zoom(100);
    wait_swapped();
}

#[test]
fn test_a_zoom_step_reloads_no_provider_and_swaps_exactly_the_registered_classes() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("a.txt");
    fixture::write_text(&path, "zoom me\n");
    let window = test_window();
    present_window(&window);
    reset_font_settings();
    let editor = open_loaded(&window, &path);
    // Many unrelated widgets in another window, none registered.
    let unrelated = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    for row in 0..500 {
        unrelated.append(&gtk4::Label::new(Some(&format!("row {row}"))));
    }
    let other = gtk4::Window::builder().child(&unrelated).build();
    present_window(&other);

    let texts = font_zoom::provider_texts();
    let before: FontZoomEvidence = font_zoom::evidence();
    assert_eq!(
        zoom_classes(editor.source_view()),
        vec![before.current_class.clone().expect("class")]
    );
    activate_action(&window, "zoom-in");
    let after = font_zoom::evidence();
    assert_eq!(
        font_zoom::provider_texts(),
        texts,
        "a zoom step reloads nothing"
    );
    assert_eq!(after.provider_loads, before.provider_loads);
    assert_eq!(after.swaps, before.swaps + 1);
    assert_eq!(
        after.last_swap_widgets, after.live,
        "every live registered widget"
    );
    let class = after.current_class.expect("class");
    assert!(class.ends_with("-zoom-110"), "{class}");
    assert_eq!(zoom_classes(editor.source_view()), vec![class.clone()]);
    for label in crate::common::every_widget()
        .into_iter()
        .filter(|widget| widget.is_ancestor(&other))
    {
        assert!(
            zoom_classes(&label).is_empty(),
            "unrelated widgets keep no class"
        );
    }
    // The minimap (shown by default) follows too.
    assert!(
        crate::common::every_widget()
            .iter()
            .filter(|widget| widget.has_css_class("minimap-view"))
            .all(|map| zoom_classes(map) == vec![class.clone()])
    );
    activate_action(&window, "zoom-reset");
    other.destroy();
}

#[test]
fn test_widgets_created_after_a_zoom_step_carry_the_current_class_and_size() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let first = dir.path().join("first.txt");
    fixture::write_text(&first, "first\n");
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    reset_font_settings();
    let _first = open_loaded(&window, &first);
    set_zoom(130);
    flush_events();
    let class = font_zoom::evidence().current_class.expect("class");
    assert!(class.ends_with("-zoom-130"));

    // A new tab, a Markdown preview with a code block and an image fallback,
    // and a search-result-like label built through the registering helper.
    let editor = open_loaded(&window, &markdown_document(dir.path()));
    activate_action(&window, "toggle-preview-pane");
    wait_until(Duration::from_secs(10), || {
        crate::common::every_widget()
            .iter()
            .any(|widget| widget.has_css_class("markdown-code-block-view"))
    });
    let label = gtk4::Label::new(Some("12"));
    font_zoom::follow_editor_font_monospace(&label);
    assert_eq!(zoom_classes(&label), vec![class.clone()]);
    assert_eq!(zoom_classes(editor.source_view()), vec![class.clone()]);
    let code = crate::common::every_widget()
        .into_iter()
        .find(|widget| widget.has_css_class("markdown-code-block-view"))
        .expect("code block");
    assert_eq!(zoom_classes(&code), vec![class]);
    assert!(crate::common::assert_monospace_widgets_registered("preview") >= 2);
    assert_matches_legacy(&window, &surfaces(&editor), "130 % system font");
    reset_font_settings();
}

#[test]
fn test_every_surface_computes_main_s_font_in_every_state() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    reset_font_settings();
    let editor = open_loaded(&window, &markdown_document(dir.path()));
    activate_action(&window, "toggle-preview-pane");
    wait_until(Duration::from_secs(10), || {
        crate::common::every_widget()
            .iter()
            .any(|widget| widget.has_css_class("markdown-code-block-view"))
    });
    let widgets = surfaces(&editor);
    assert!(widgets.len() >= 3, "editor, minimap and preview widgets");

    assert_matches_legacy(&window, &widgets, "100 % system font");
    settings_writes::set_boolean(keys::USE_SYSTEM_FONT, false);
    settings_writes::set_string(keys::CUSTOM_FONT, "Monospace 13");
    wait_swapped();
    assert_matches_legacy(&window, &widgets, "100 % custom font");
    set_zoom(130);
    flush_events();
    assert_matches_legacy(&window, &widgets, "130 % custom font");
    set_zoom(50);
    flush_events();
    assert_matches_legacy(&window, &widgets, "50 % custom font");

    set_zoom(130);
    let gtk_settings = gtk4::Settings::default().expect("settings");
    let dpi = gtk_settings.gtk_xft_dpi();
    gtk_settings.set_gtk_xft_dpi(96 * 1024 * 3 / 2);
    assert_matches_legacy(&window, &widgets, "130 % under a 1.5 text scale");
    gtk_settings.set_gtk_xft_dpi(dpi);
    if gtk_settings
        .find_property("gtk-interface-contrast")
        .is_some()
    {
        set_contrast(&gtk_settings, "more");
        assert_matches_legacy(&window, &widgets, "130 % with high contrast");
        set_contrast(&gtk_settings, "no-preference");
    }
    reset_font_settings();
}

#[test]
fn test_focus_mode_margins_follow_the_zoom_as_on_main() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("focus.txt");
    fixture::write_text(&path, &"focus words ".repeat(40));
    let window = test_window();
    window.set_default_size(1400, 900);
    present_window(&window);
    reset_font_settings();
    let editor = open_loaded(&window, &path);
    gtk4::prelude::WidgetExt::activate_action(
        &window,
        "win.set-focus-mode",
        Some(&true.to_variant()),
    )
    .expect("set-focus-mode");
    wait_for_painted_frame(&window);
    flush_after_delay(Duration::from_millis(100));
    let char_width = |editor: &LushtextEditorPage| {
        editor
            .source_view()
            .pango_context()
            .metrics(None, None)
            .approximate_char_width()
    };
    let at_100 = (editor.focus_mode_left_margin(), char_width(&editor));
    set_zoom(130);
    wait_for_painted_frame(&window);
    flush_after_delay(Duration::from_millis(100));
    wait_until(Duration::from_secs(2), || {
        editor.focus_mode_left_margin() != at_100.0
    });
    let at_130 = (editor.focus_mode_left_margin(), char_width(&editor));
    assert!(
        at_130.1 > at_100.1,
        "the zoom widens the character: {at_100:?} {at_130:?}"
    );
    assert!(
        at_130.0 < at_100.0,
        "a wider column leaves smaller margins: {at_100:?} {at_130:?}"
    );
    // The margins derive from the view's Pango character width alone, which
    // is main's under main's rules (the same width the readable-column policy
    // reads there).
    let widget: gtk4::Widget = editor.source_view().clone().upcast();
    assert_matches_legacy(&window, std::slice::from_ref(&widget), "Focus Mode 130 %");
    gtk4::prelude::WidgetExt::activate_action(
        &window,
        "win.set-focus-mode",
        Some(&false.to_variant()),
    )
    .expect("set-focus-mode");
    reset_font_settings();
}

#[test]
fn test_two_windows_font_change_and_an_off_grid_zoom() {
    ensure_gtk_init();
    let dir = tempfile::tempdir().expect("tempdir");
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    fixture::write_text(&a, "a\n");
    fixture::write_text(&b, "b\n");
    let first = test_window();
    present_window(&first);
    reset_font_settings();
    let app = first.application().expect("application");
    let second = LushtextWindow::new(app.downcast_ref().expect("adwaita application"));
    present_window(&second);
    let one = open_loaded(&first, &a);
    let two = open_loaded(&second, &b);

    // A zoom step in one window swaps both and loads nothing.
    let before = font_zoom::evidence();
    activate_action(&first, "zoom-in");
    let after = font_zoom::evidence();
    assert_eq!(after.provider_loads, before.provider_loads);
    let class = after.current_class.clone().expect("class");
    assert_eq!(zoom_classes(one.source_view()), vec![class.clone()]);
    assert_eq!(zoom_classes(two.source_view()), vec![class.clone()]);

    // A font change: one load, in a dispatch that swaps nothing ...
    settings_writes::set_boolean(keys::USE_SYSTEM_FONT, false);
    let loading = font_zoom::evidence();
    assert_eq!(loading.provider_loads, after.provider_loads + 1);
    assert_eq!(loading.swaps, after.swaps, "the load swaps no class");
    assert!(loading.pending_generation.is_some());
    assert_eq!(zoom_classes(one.source_view()), vec![class.clone()]);
    // ... and the swap in a later one.
    wait_swapped();
    let swapped = font_zoom::evidence();
    assert_eq!(swapped.provider_loads, loading.provider_loads);
    assert_eq!(swapped.swaps, loading.swaps + 1);
    let new_class = swapped.current_class.expect("class");
    assert_ne!(new_class, class);
    assert!(new_class.ends_with("-zoom-110"));
    assert_eq!(zoom_classes(one.source_view()), vec![new_class.clone()]);
    assert_eq!(zoom_classes(two.source_view()), vec![new_class]);

    // A second change before the swap supersedes the first: one swap.
    let superseding = font_zoom::evidence();
    settings_writes::set_string(keys::CUSTOM_FONT, "Monospace 12");
    settings_writes::set_string(keys::CUSTOM_FONT, "Monospace 14");
    wait_swapped();
    let superseded = font_zoom::evidence();
    assert_eq!(superseded.provider_loads, superseding.provider_loads + 2);
    assert_eq!(superseded.swaps, superseding.swaps + 1, "latest wins, once");

    // 55 % from outside the UI loads once; two steps after it load nothing.
    let off_grid = font_zoom::evidence();
    set_zoom(55);
    wait_swapped();
    assert_eq!(
        font_zoom::evidence().provider_loads,
        off_grid.provider_loads + 1
    );
    activate_action(&first, "zoom-in");
    activate_action(&first, "zoom-in");
    let stepped = font_zoom::evidence();
    assert_eq!(stepped.provider_loads, off_grid.provider_loads + 1);
    assert!(stepped.current_class.expect("class").ends_with("-zoom-75"));
    assert_eq!(settings_writes::shared().uint(keys::ZOOM_LEVEL), 75);
    second.destroy();
    reset_font_settings();
}
