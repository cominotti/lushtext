// SPDX-License-Identifier: GPL-3.0-or-later

//! **Called presentation surface** — not a role.
//!
//! Every grouped-row dialog this workflow presents, plus the shared row chrome
//! they are built from. This module owns no ordered stage and no coordination
//! state: the facade calls it to present a surface, and each activatable row
//! calls back into the facade's next stage.
//!
//! It is deliberately dumb about *meaning*. Every subtitle, description, and
//! selected/activatable decision comes from `policy`, so the wording contract in
//! the Decision And Detail Dialogs rule (`.agents/rules/ui.md`) is testable
//! without constructing a widget. What stays here is Adwaita structure:
//!
//! * one `AdwPreferencesGroup` per conceptual set (`Current Document`,
//!   `Actions`, `Encoding Options`, `Future Save Style`, `Mode Options`),
//! * `AdwActionRow` rows with `title_lines(0)` / `subtitle_lines(0)` so long
//!   copy wraps instead of being truncated,
//! * activatable rows for choices and commands, non-activatable rows for facts,
//! * explicit inner content padding on the content box, per the Dialog Text
//!   Surface Padding rule.

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use crate::model::encoding::{DocumentEncoding, LineEnding};
use crate::ui::accessibility;

use super::super::LushtextWindow;
use super::policy::{self, ChoiceRowState};

const RESPONSE_CLOSE: &str = "close";

/// Present the summary encoding surface for one editor.
pub(super) fn present_encoding_controls(window: &LushtextWindow, has_path: bool) {
    let dialog = build_dialog(
        "Text Encoding",
        "Review and change how this document reads and writes text bytes.",
    );
    let content = standard_dialog_content();

    let Some(editor) = window.active_editor() else {
        return;
    };

    let current_group = libadwaita::PreferencesGroup::builder()
        .title("Current Document")
        .build();
    current_group.add(&static_dialog_row(
        "Opened As",
        editor.opened_encoding().label(),
    ));
    current_group.add(&static_dialog_row(
        "Next Save",
        editor.save_encoding().label(),
    ));
    content.append(&current_group);

    let actions_group = libadwaita::PreferencesGroup::builder()
        .title("Actions")
        .build();
    append_action_row_with_sensitivity(
        &actions_group,
        "Reopen with Encoding…",
        if has_path {
            "Reinterpret the bytes currently on disk with a different encoding."
        } else {
            "Save this document before reopening it with another encoding."
        },
        has_path,
        window.downgrade(),
        &dialog,
        |window| present_reopen_encoding(&window),
    );
    append_action_row(
        &actions_group,
        "Save Using Encoding…",
        "Change how future saves encode this document.",
        window.downgrade(),
        &dialog,
        |window| present_save_encoding(&window),
    );
    append_action_row(
        &actions_group,
        "Invisible Characters…",
        "Choose whether whitespace and hidden encoding-adjacent characters are drawn.",
        window.downgrade(),
        &dialog,
        |window| present_invisible_characters(&window),
    );
    content.append(&actions_group);

    dialog.set_extra_child(Some(&content));
    dialog.present(Some(window));
}

/// Present the chooser for reinterpreting the bytes currently on disk.
pub(super) fn present_reopen_encoding(window: &LushtextWindow) {
    let Some(editor) = window.active_editor() else {
        return;
    };

    let dialog = build_dialog(
        "Reopen with Encoding",
        "Choose a decoding for the bytes currently on disk.",
    );
    let content = standard_dialog_content();
    let current_group = libadwaita::PreferencesGroup::builder()
        .title("Current Decoding")
        .build();
    current_group.add(&static_dialog_row(
        "Opened As",
        editor.opened_encoding().label(),
    ));
    content.append(&current_group);

    let options_group = libadwaita::PreferencesGroup::builder()
        .title("Encoding Options")
        .build();
    let opened = editor.opened_encoding();
    for encoding in DocumentEncoding::COMMON {
        let state = policy::encoding_choice_state(encoding, opened);
        append_choice_row(
            &options_group,
            encoding.label(),
            policy::reopen_encoding_subtitle(encoding, state.selected),
            state,
            move |window| window.begin_reopen_with_encoding(encoding),
            window.downgrade(),
            &dialog,
        );
    }
    content.append(&options_group);

    dialog.set_extra_child(Some(&content));
    dialog.present(Some(window));
}

/// Present the chooser for the document's next-save encoding policy.
pub(super) fn present_save_encoding(window: &LushtextWindow) {
    let Some(editor) = window.active_editor() else {
        return;
    };

    let dialog = build_dialog(
        "Save Using Encoding",
        "Choose the encoding used by future saves.",
    );
    let content = standard_dialog_content();
    let current_group = libadwaita::PreferencesGroup::builder()
        .title("Current Save Encoding")
        .build();
    current_group.add(&static_dialog_row(
        "Next Save",
        editor.save_encoding().label(),
    ));
    content.append(&current_group);

    let options_group = libadwaita::PreferencesGroup::builder()
        .title("Encoding Options")
        .build();
    let current = editor.save_encoding();
    for encoding in DocumentEncoding::COMMON {
        let state = policy::encoding_choice_state(encoding, current);
        append_choice_row(
            &options_group,
            encoding.label(),
            policy::save_encoding_subtitle(encoding, state.selected),
            state,
            move |window| window.begin_save_encoding_change(encoding),
            window.downgrade(),
            &dialog,
        );
    }
    content.append(&options_group);

    dialog.set_extra_child(Some(&content));
    dialog.present(Some(window));
}

/// Present the chooser for invisible-character display mode.
pub(super) fn present_invisible_characters(window: &LushtextWindow) {
    let Some(editor) = window.active_editor() else {
        return;
    };

    let dialog = build_dialog(
        "Invisible Characters",
        "Choose how much whitespace and encoding-adjacent detail the editor should draw.",
    );
    let content = standard_dialog_content();
    let options_group = libadwaita::PreferencesGroup::builder()
        .title("Mode Options")
        .build();

    let current = editor.invisible_characters_mode();
    for mode in policy::INVISIBLE_MODE_CHOICES {
        let state = policy::invisible_mode_choice_state(mode, current);
        append_choice_row(
            &options_group,
            mode.label(),
            policy::invisible_mode_subtitle(mode, state.selected),
            state,
            move |window| window.apply_invisible_characters_mode(mode),
            window.downgrade(),
            &dialog,
        );
    }
    content.append(&options_group);

    dialog.set_extra_child(Some(&content));
    dialog.present(Some(window));
}

/// Present line-ending controls for one editor.
pub(super) fn present_line_ending_controls(window: &LushtextWindow) {
    let Some(editor) = window.active_editor() else {
        return;
    };

    let dialog = build_dialog(
        "Line Endings",
        "Choose the line-ending style LushText should use on future saves.",
    );

    let content = standard_dialog_content();
    let current_group = libadwaita::PreferencesGroup::builder()
        .title("Current Document")
        .build();
    let detected = editor.detected_line_ending();
    current_group.add(&static_dialog_row(
        "Opened With",
        policy::opened_line_ending_subtitle(detected),
    ));
    current_group.add(&static_dialog_row(
        "Next Save",
        editor.save_line_ending().label(),
    ));
    content.append(&current_group);

    let options_group = libadwaita::PreferencesGroup::builder()
        .title("Future Save Style")
        .build();
    let save_line_ending = editor.save_line_ending();
    for line_ending in LineEnding::SAVE_CHOICES {
        let state = policy::line_ending_choice_state(line_ending, save_line_ending, detected);
        append_choice_row(
            &options_group,
            line_ending.label(),
            policy::line_ending_subtitle(line_ending, state.selected, detected),
            state,
            move |window| window.apply_line_ending_choice(line_ending),
            window.downgrade(),
            &dialog,
        );
    }
    content.append(&options_group);

    dialog.set_extra_child(Some(&content));
    dialog.present(Some(window));
}

/// Present the current file-health findings for one editor.
///
/// The empty state is a wrapped label rather than a group, because there is no
/// conceptual set to title — one message is the whole content.
pub(super) fn present_file_health(window: &LushtextWindow) {
    let Some(editor) = window.active_editor() else {
        return;
    };

    let dialog = build_dialog(
        "File Health",
        "Review encoding-adjacent findings and any slower follow-up actions for the active document.",
    );
    let content = standard_dialog_content();

    let findings = editor.file_health();
    if findings.is_empty() {
        let label = gtk4::Label::new(Some(policy::NO_FILE_HEALTH_FINDINGS_BODY));
        label.set_wrap(true);
        label.set_xalign(0.0);
        content.append(&label);
    } else {
        for finding in findings {
            let row = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            let title = gtk4::Label::new(Some(&finding.title));
            title.set_xalign(0.0);
            title.add_css_class("heading");
            let body = gtk4::Label::new(Some(&finding.body));
            body.set_wrap(true);
            body.set_xalign(0.0);
            body.add_css_class("dim-label");
            row.append(&title);
            row.append(&body);
            content.append(&row);
        }
    }

    dialog.set_extra_child(Some(&content));
    dialog.present(Some(window));
}

// ─── Shared row chrome ────────────────────────────────────────────────

/// Build a standard dialog shell for document-local format workflows.
fn build_dialog(heading: &str, body: &str) -> libadwaita::AlertDialog {
    let dialog = libadwaita::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .build();
    dialog.add_response(RESPONSE_CLOSE, "_Close");
    dialog.set_default_response(Some(RESPONSE_CLOSE));
    dialog.set_close_response(RESPONSE_CLOSE);
    // A stable accessible name for the transient surface, matching the visible
    // heading so an AT-SPI anchor does not depend on Adwaita's internal labelling.
    accessibility::set_label(&dialog, heading);
    dialog
}

/// Create the standard content box used by the encoding toolkit dialogs.
///
/// The vertical margins are the inner content padding the Dialog Text Surface
/// Padding rule requires; the dialog shell's own margins do not pad content.
fn standard_dialog_content() -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    content
}

/// Create a read-only row for current document facts.
fn static_dialog_row(
    title: impl Into<glib::GString>,
    subtitle: impl Into<glib::GString>,
) -> libadwaita::ActionRow {
    dialog_row(title, subtitle, false)
}

/// Append one activatable row that closes the current dialog before running an action.
fn append_action_row(
    group: &libadwaita::PreferencesGroup,
    title: &str,
    subtitle: &str,
    window_weak: glib::WeakRef<LushtextWindow>,
    dialog: &libadwaita::AlertDialog,
    action: impl Fn(LushtextWindow) + 'static,
) {
    append_action_row_with_sensitivity(group, title, subtitle, true, window_weak, dialog, action);
}

/// Append one activatable row with an explicit sensitivity override.
fn append_action_row_with_sensitivity(
    group: &libadwaita::PreferencesGroup,
    title: &str,
    subtitle: &str,
    sensitive: bool,
    window_weak: glib::WeakRef<LushtextWindow>,
    dialog: &libadwaita::AlertDialog,
    action: impl Fn(LushtextWindow) + 'static,
) {
    let row = dialog_row(title, subtitle, sensitive);
    if sensitive {
        // Decorative: the row's own title and subtitle already say that
        // activating it opens a further surface, so announcing a bare "image"
        // beside them would be noise. `has-popup` carries the same fact to a
        // screen reader in the way ATK expects.
        let chevron = gtk4::Image::from_icon_name("go-next-symbolic");
        accessibility::set_role(&chevron, gtk4::AccessibleRole::Presentation);
        accessibility::set_has_popup(&row, true);
        row.add_suffix(&chevron);
        let dialog_weak = dialog.downgrade();
        row.connect_activated(move |_| {
            if let Some(dialog) = dialog_weak.upgrade() {
                dialog.close();
            }
            if let Some(window) = window_weak.upgrade() {
                action(window);
            }
        });
    } else {
        row.set_sensitive(false);
    }
    group.add(&row);
}

/// Append one option row that either shows the current choice or applies a new one.
fn append_choice_row(
    group: &libadwaita::PreferencesGroup,
    title: &str,
    subtitle: String,
    state: ChoiceRowState,
    action: impl Fn(LushtextWindow) + 'static,
    window_weak: glib::WeakRef<LushtextWindow>,
    dialog: &libadwaita::AlertDialog,
) {
    let row = dialog_row(title, subtitle, state.enabled);
    // Publish which option is current as accessible state rather than leaving it
    // to the checkmark glyph below. The subtitle from `policy` already says
    // "Current save encoding." in prose, so this is not the only channel — but
    // prose a user must parse is weaker than a state an assistive technology can
    // report directly, and the glyph alone carries nothing.
    //
    if state.selected {
        let check = gtk4::Image::from_icon_name("object-select-symbolic");
        accessibility::set_role(&check, gtk4::AccessibleRole::Presentation);
        row.add_suffix(&check);
    }
    if state.enabled {
        let dialog_weak = dialog.downgrade();
        row.connect_activated(move |_| {
            if let Some(dialog) = dialog_weak.upgrade() {
                dialog.close();
            }
            if let Some(window) = window_weak.upgrade() {
                action(window);
            }
        });
    }
    group.add(&row);

    // Published **after** the row is parented. `AdwActionRow`'s default role does
    // not carry `Selected`, and metadata set before the row joins the group is
    // dropped when GTK builds its accessible context, so both calls happen here —
    // proved by the widget assertion, which failed with them set earlier.
    // `Radio` + `Checked`, not `ListItem` + `Selected`: `AdwActionRow` subclasses
    // `GtkListBoxRow`, whose accessible context owns selection and silently drops
    // an app-supplied `Selected` — measured, after that first attempt failed this
    // row's own widget assertion. A one-of-many option is a radio in ARIA terms
    // anyway, so this is both the treatment that lands and the accurate one.
    accessibility::set_role(&row, gtk4::AccessibleRole::Radio);
    accessibility::set_checked(&row, state.selected);
}

/// Build one row with title/subtitle typography and optional activation.
fn dialog_row(
    title: impl Into<glib::GString>,
    subtitle: impl Into<glib::GString>,
    activatable: bool,
) -> libadwaita::ActionRow {
    libadwaita::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .title_lines(0)
        .subtitle_lines(0)
        .activatable(activatable)
        .selectable(false)
        .build()
}
