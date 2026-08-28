// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — execution. Owns one in-tab search session.
//!
//! A session is the lifetime of one `sourceview5::SearchContext`: created on
//! attach, torn down on detach, and the only thing that makes navigation,
//! replacement, and match reporting possible in between. Every operation here
//! begins by taking the live context and returning early without it, which is
//! what makes the whole workflow safe to drive before the bar has ever been
//! attached — the button handlers and key controllers are wired once in
//! `constructed()` and fire regardless.
//!
//! The one inversion lives here: `connect_occurrences_count_notify`.
//! GtkSourceView scans asynchronously, so the match total arrives later, from
//! the scanner rather than from the keystroke that caused it, and control
//! resumes in `report_match_state`. The census recorded this row as "fully
//! synchronous ... no worker completion seam"; that is wrong, and it is the only
//! resumption point the workflow has.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::gio;
use gtk4::prelude::*;
use sourceview5::prelude::*;

use crate::ui::accessibility;

use super::LushtextSearchBar;
use super::policy::{self, SearchOption};

/// Begin a session against one buffer and view.
pub(super) fn begin_session(
    bar: &LushtextSearchBar,
    buffer: &sourceview5::Buffer,
    view: &sourceview5::View,
) {
    end_session(bar);

    let settings = sourceview5::SearchSettings::builder()
        .wrap_around(true)
        .build();
    let context = sourceview5::SearchContext::new(buffer, Some(&settings));
    context.set_highlight(true);

    apply_option_actions_to_settings(bar, &settings);

    // A query retained from a previous session must highlight immediately rather
    // than waiting for the next keystroke.
    let text = bar.search_entry().text();
    if !text.is_empty() {
        settings.set_search_text(Some(text.as_str()));
    }

    // The inversion: the scan completes later and resumes here.
    let bar_weak = bar.downgrade();
    let handler_id = context.connect_occurrences_count_notify(move |_ctx| {
        if let Some(bar) = bar_weak.upgrade() {
            report_match_state(&bar);
        }
    });

    // Held weakly: the view outlives the session and must not be kept alive by
    // it. Only `scroll_to_current_match` needs it.
    let weak_view = glib::WeakRef::new();
    weak_view.set(Some(view));

    let imp = bar.imp();
    imp.occurrences_signals.track(&context, handler_id);
    imp.search_context.replace(Some(context));
    imp.search_settings.replace(Some(settings));
    imp.view_ref.replace(Some(weak_view));
    imp.navigated.set(false);
    bar.emit_search_state_changed();
}

/// End the current session, clearing highlighting and every session-scoped slot.
pub(super) fn end_session(bar: &LushtextSearchBar) {
    let imp = bar.imp();

    // The occurrences handler holds the bar, so it must be disconnected before
    // the context is dropped or the pair leaks as a reference cycle.
    imp.occurrences_signals.clear();
    if let Some(context) = imp.search_context.borrow().as_ref().cloned() {
        context.set_highlight(false);
    }

    imp.search_context.replace(None);
    imp.search_settings.replace(None);
    imp.view_ref.replace(None);
    imp.navigated.set(false);

    bar.set_match_count(0, 0);
    bar.search_entry().remove_css_class("error");
    accessibility::set_invalid(bar.search_entry(), false);
    bar.emit_search_state_changed();
}

/// Select the next match after the cursor.
pub(super) fn select_next_match(bar: &LushtextSearchBar) {
    let Some(context) = bar.search_context() else {
        return;
    };
    let buffer = context.buffer();
    // Start one character past the insert mark so the current match is advanced
    // past rather than re-found.
    let mut iter = buffer.iter_at_mark(&buffer.get_insert());
    iter.forward_char();

    if let Some((match_start, match_end, _wrapped)) = context.forward(&iter) {
        select_and_reveal(bar, &buffer, &match_start, &match_end);
    }
    report_match_state(bar);
}

/// Select the previous match before the cursor.
pub(super) fn select_previous_match(bar: &LushtextSearchBar) {
    let Some(context) = bar.search_context() else {
        return;
    };
    let buffer = context.buffer();
    let iter = buffer.iter_at_mark(&buffer.get_insert());

    if let Some((match_start, match_end, _wrapped)) = context.backward(&iter) {
        select_and_reveal(bar, &buffer, &match_start, &match_end);
    }
    report_match_state(bar);
}

/// Replace the selected match, then advance to the next one.
pub(super) fn replace_selected_match(bar: &LushtextSearchBar) {
    let Some(context) = bar.search_context() else {
        return;
    };
    let buffer = context.buffer();
    let replace_text = bar.replace_entry().text();

    let (mut match_start, mut match_end) = selection_or_cursor(&buffer);
    if context
        .replace(&mut match_start, &mut match_end, replace_text.as_str())
        .is_ok()
    {
        select_next_match(bar);
    }
}

/// Replace every match in the buffer.
pub(super) fn replace_all_matches(bar: &LushtextSearchBar) {
    let Some(context) = bar.search_context() else {
        return;
    };
    let replace_text = bar.replace_entry().text();
    if let Err(error) = context.replace_all(replace_text.as_str()) {
        tracing::error!("Replace all failed: {error}");
    }
    report_match_state(bar);
}

/// Write the match counter label and its accessible value text.
///
/// Lives here rather than in the facade because it mutates widgets: the facade
/// narrates stages and must not own GTK writes. The *text* is `policy`'s
/// decision, including the blank-while-scanning case that keeps a false
/// "no matches" from flickering on every keystroke.
pub(super) fn project_match_count(bar: &LushtextSearchBar, current: i32, total: i32) {
    let label = &bar.imp().match_label;
    if let Some(count) = policy::match_count_label(current, total) {
        label.set_label(&count);
        accessibility::set_value_text(&**label, &count);
    } else {
        label.set_label("");
        accessibility::set_value_text(&**label, policy::NO_CURRENT_MATCH_VALUE_TEXT);
    }
}

/// Project the live scan state onto the counter, the query styling, and a
/// screen-reader announcement.
///
/// This is where the scan inversion resumes, and it is also called directly
/// after every navigation and replacement so the counter never lags the cursor.
pub(super) fn report_match_state(bar: &LushtextSearchBar) {
    let Some(context) = bar.search_context() else {
        return;
    };
    let total = context.occurrences_count();
    let search_text = bar.search_entry().text();

    let current = if total > 0 {
        let buffer = context.buffer();
        let (selection_start, selection_end) = selection_or_cursor(&buffer);
        policy::current_occurrence(
            total,
            context.occurrence_position(&selection_start, &selection_end),
        )
    } else {
        policy::current_occurrence(total, 0)
    };

    bar.set_match_count(current, total);

    let entry = bar.search_entry();
    let no_matches = policy::query_has_no_matches(search_text.as_str(), total);
    if no_matches {
        entry.add_css_class("error");
    } else {
        entry.remove_css_class("error");
    }
    accessibility::set_invalid(entry, no_matches);

    if let Some(message) = policy::match_count_announcement(search_text.as_str(), current, total) {
        bar.imp().match_announcement_throttler.announce_if_allowed(
            &*bar.imp().match_label,
            accessibility::AnnouncementLane::DebouncedResults,
            "editor-search-results",
            &message,
        );
    }
}

/// Apply one option toggle to the live session's settings.
pub(super) fn apply_option(bar: &LushtextSearchBar, name: &str, enabled: bool) {
    let Some(settings) = bar.imp().search_settings.borrow().clone() else {
        return;
    };
    if let Some(option) = SearchOption::from_action_name(name) {
        set_option(&settings, option, enabled);
    }
}

/// Seed a fresh session's settings from the option actions' current states.
///
/// Applying the states once per attach is what avoids one `notify::state`
/// handler per attach/detach cycle; later toggles arrive through `apply_option`.
fn apply_option_actions_to_settings(
    bar: &LushtextSearchBar,
    settings: &sourceview5::SearchSettings,
) {
    let Some(group) = bar.imp().options_group.borrow().clone() else {
        return;
    };

    for name in policy::SEARCH_OPTION_ACTION_NAMES {
        let Some(option) = SearchOption::from_action_name(name) else {
            continue;
        };
        let Some(action) = group.lookup_action(name) else {
            continue;
        };
        let Ok(simple) = action.downcast::<gio::SimpleAction>() else {
            continue;
        };
        let enabled: bool = simple
            .state()
            .and_then(|state| state.get())
            .unwrap_or(false);
        set_option(settings, option, enabled);
    }
}

/// The one place a typed option becomes a GtkSourceView setter.
fn set_option(settings: &sourceview5::SearchSettings, option: SearchOption, enabled: bool) {
    match option {
        SearchOption::Regex => settings.set_regex_enabled(enabled),
        SearchOption::CaseSensitive => settings.set_case_sensitive(enabled),
        SearchOption::WholeWord => settings.set_at_word_boundaries(enabled),
    }
}

/// Select a match, scroll it into view, and record that the user navigated.
fn select_and_reveal(
    bar: &LushtextSearchBar,
    buffer: &sourceview5::Buffer,
    match_start: &gtk4::TextIter,
    match_end: &gtk4::TextIter,
) {
    buffer.select_range(match_start, match_end);
    scroll_to_current_match(bar);
    bar.imp().navigated.set(true);
}

/// Scroll the attached view so the insert mark is on screen.
fn scroll_to_current_match(bar: &LushtextSearchBar) {
    if let Some(ref weak_view) = *bar.imp().view_ref.borrow()
        && let Some(view) = weak_view.upgrade()
    {
        let buffer = view.buffer();
        view.scroll_mark_onscreen(&buffer.get_insert());
    }
}

/// The selection bounds, collapsing to the cursor when nothing is selected.
fn selection_or_cursor(buffer: &sourceview5::Buffer) -> (gtk4::TextIter, gtk4::TextIter) {
    buffer.selection_bounds().unwrap_or_else(|| {
        let iter = buffer.iter_at_mark(&buffer.get_insert());
        (iter, iter)
    })
}
