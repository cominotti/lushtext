// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination (`execution`, stage-order-qualified: stage 6, list →
//! filter → select) of `WFR-TAB-OVERFLOW-NAVIGATION`: the window side of the
//! All Tabs popover (design D6).
//!
//! It owns the stage's timing and lifecycle: it takes the snapshot the
//! popover lists (one compact row per tab, no document text, no buffer
//! read), re-takes it through one zero-delay `Debounce` while the popover is
//! open and the tabs change (with the per-page title and pinned handlers that
//! feed it), saves the focus in the menu button's create-popup function and
//! restores it from a generation-guarded idle when the popover closes without
//! an activation, latches an Escape the popover's search entry consumed for
//! the window's dismissal ladder, and selects an activated row's tab through
//! `select_tab_from_all_tabs`, whose editor focus goes through
//! `ui/window/editor_focus/`. The decisions (ranking, highlight, copy) are
//! `policy.rs`'s; the popover widget is `all_tabs_popover/`; the stages are
//! narrated in `mod.rs`.

use std::time::Duration;

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::super::LushtextWindow;
use super::all_tabs_popover::AllTabsRow;
use super::policy::all_tabs_folder_label;
use super::watch::release_page_watch;
use crate::ui::editor_page::LushtextEditorPage;

/// `win.toggle-all-tabs`; `actions.rs` registers it under this name.
pub(in crate::ui::window) const TOGGLE_ALL_TABS_ACTION: &str = "toggle-all-tabs";
/// `win.set-all-tabs-visible`; `actions.rs` registers it under this name.
pub(in crate::ui::window) const SET_ALL_TABS_VISIBLE_ACTION: &str = "set-all-tabs-visible";
/// `win.set-all-tabs-query`; `actions.rs` registers it under this name.
pub(in crate::ui::window) const SET_ALL_TABS_QUERY_ACTION: &str = "set-all-tabs-query";
/// The accelerator of `win.toggle-all-tabs`: `actions.rs` binds it, and the
/// open popover closes on it (the shortcuts dialog's Blueprint and the
/// button's ARIA key shortcut spell it out).
pub(in crate::ui::window) const TOGGLE_ALL_TABS_ACCEL: &str = "<Control><Shift>o";

impl LushtextWindow {
    /// Wire the popover, its button and the live re-snapshot. Runs once, from
    /// the workflow's install stage.
    pub(super) fn install_all_tabs(&self) {
        let Some(controls) = self.imp().tab_overflow.controls() else {
            return;
        };
        // Runs before the popover shows, on a click and on `popup()` alike:
        // the focus to restore is still the window's.
        let window_weak = self.downgrade();
        controls.all_tabs.set_create_popup_func(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.prepare_all_tabs_popup();
            }
        });
        let window_weak = self.downgrade();
        controls.popover.connect_closed(move |popover| {
            if let Some(window) = window_weak.upgrade() {
                window.set_all_tabs_query_enabled(false);
                if !popover.closed_by_activation() {
                    window.restore_focus_after_all_tabs();
                }
                window.schedule_tab_overflow_reconcile();
            }
        });
        // Escape in the search entry closes the popover through
        // `stop-search`, and the window's Bubble-phase Escape controller then
        // sees the same key press: latch it, so the ladder closes nothing
        // under the popover (`transient_dismissal`'s one inversion).
        let window_weak = self.downgrade();
        controls
            .popover
            .search_entry()
            .connect_stop_search(move |_| {
                if let Some(window) = window_weak.upgrade() {
                    window.mark_child_transient_escape_handled();
                }
            });
        let window_weak = self.downgrade();
        controls.popover.connect_row_activated(move |page| {
            if let Some(window) = window_weak.upgrade() {
                window.select_tab_from_all_tabs(page);
            }
        });

        // Live updates while the popover is open: the tab set, order and
        // selection here, each page's title (rename, save, modified) and
        // pinned state on the page itself.
        let tab_view = self.imp().tab_view.get();
        let window_weak = self.downgrade();
        tab_view.connect_page_attached(move |_, page, _| {
            if let Some(window) = window_weak.upgrade() {
                window.watch_all_tabs_page(page);
                window.schedule_all_tabs_resnapshot();
            }
        });
        let window_weak = self.downgrade();
        tab_view.connect_page_detached(move |_, page, _| {
            if let Some(window) = window_weak.upgrade() {
                window.unwatch_all_tabs_page(page);
                window.schedule_all_tabs_resnapshot();
            }
        });
        let window_weak = self.downgrade();
        tab_view.connect_page_reordered(move |_, _, _| {
            if let Some(window) = window_weak.upgrade() {
                window.schedule_all_tabs_resnapshot();
            }
        });
        let window_weak = self.downgrade();
        tab_view.connect_selected_page_notify(move |_| {
            if let Some(window) = window_weak.upgrade()
                && !window.tab_overflow_reveals_selection()
            {
                window.schedule_all_tabs_resnapshot();
            }
        });
        for index in 0..tab_view.n_pages() {
            self.watch_all_tabs_page(&tab_view.nth_page(index));
        }
    }

    fn watch_all_tabs_page(&self, page: &libadwaita::TabPage) {
        let window_weak = self.downgrade();
        let title = page.connect_title_notify(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.schedule_all_tabs_resnapshot();
            }
        });
        let window_weak = self.downgrade();
        let pinned = page.connect_pinned_notify(move |_| {
            if let Some(window) = window_weak.upgrade() {
                window.schedule_all_tabs_resnapshot();
            }
        });
        let previous = self
            .imp()
            .tab_overflow
            .page_watches
            .borrow_mut()
            .insert(page.as_ptr() as usize, (page.downgrade(), [title, pinned]));
        if let Some(previous) = previous {
            release_page_watch(previous);
        }
    }

    fn unwatch_all_tabs_page(&self, page: &libadwaita::TabPage) {
        let watch = self
            .imp()
            .tab_overflow
            .page_watches
            .borrow_mut()
            .remove(&(page.as_ptr() as usize));
        if let Some(watch) = watch {
            release_page_watch(watch);
        }
    }

    /// While the popover is open, re-take the snapshot once the current
    /// dispatch's tab changes are done (zero delay, latest wins).
    fn schedule_all_tabs_resnapshot(&self) {
        if !self.all_tabs_popover_visible() {
            return;
        }
        let _token =
            self.imp()
                .tab_overflow
                .resnapshot
                .schedule(self, Duration::ZERO, |window, _| {
                    if let Some(controls) = window.imp().tab_overflow.controls()
                        && controls.popover.is_visible()
                    {
                        controls.popover.set_rows(window.all_tabs_snapshot());
                    }
                });
    }

    /// One compact row per tab, in strip order: no document text.
    pub(super) fn all_tabs_snapshot(&self) -> Vec<AllTabsRow> {
        let tab_view = &self.imp().tab_view;
        let selected = tab_view.selected_page();
        let home = glib::home_dir();
        (0..tab_view.n_pages())
            .map(|index| {
                let page = tab_view.nth_page(index);
                let editor = page.child().downcast::<LushtextEditorPage>().ok();
                let path = editor.as_ref().and_then(LushtextEditorPage::file_path);
                let folder = path.as_deref().and_then(std::path::Path::parent);
                let title = editor
                    .as_ref()
                    .map_or_else(|| page.title().to_string(), LushtextEditorPage::title);
                AllTabsRow {
                    page: page.downgrade(),
                    title,
                    folder: folder.map(|folder| all_tabs_folder_label(folder, &home)),
                    folder_path: folder.map(|folder| folder.display().to_string()),
                    modified: editor.as_ref().is_some_and(LushtextEditorPage::is_modified),
                    pinned: page.is_pinned(),
                    current: selected.as_ref() == Some(&page),
                }
            })
            .collect()
    }

    /// The menu button's create-popup function: save the focus to restore,
    /// then hand the popover a fresh snapshot. A reopen before the last
    /// close's restore ran cancels that restore and keeps its saved focus,
    /// which is still the focus from before the popover first opened.
    fn prepare_all_tabs_popup(&self) {
        if !self.cancel_all_tabs_focus_restore() {
            let focus = gtk4::prelude::GtkWindowExt::focus(self);
            self.imp()
                .tab_overflow
                .saved_focus
                .replace(focus.map(|widget| widget.downgrade()));
        }
        if let Some(controls) = self.imp().tab_overflow.controls() {
            controls.popover.prepare_to_show(self.all_tabs_snapshot());
        }
        self.set_all_tabs_query_enabled(true);
    }

    /// A close without an activation (Escape, a click outside, the shortcut
    /// again): give the focus back to the widget that had it, else the
    /// selected editor, else nobody. After the menu button's own close
    /// handling, which would focus the button. Until the restore runs, the
    /// reconcile's focus hand-off stands down (`focus_restore_pending`), and
    /// a reopen or disposal cancels it (`cancel_all_tabs_focus_restore`).
    fn restore_focus_after_all_tabs(&self) {
        if self.imp().disposing.get() {
            return;
        }
        let state = &self.imp().tab_overflow;
        let generation = state.focus_restore_generation.get().wrapping_add(1);
        state.focus_restore_generation.set(generation);
        state.focus_restore_pending.set(true);
        let window_weak = self.downgrade();
        glib::idle_add_local_once(move || {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            let state = &window.imp().tab_overflow;
            if state.focus_restore_generation.get() != generation
                || !state.focus_restore_pending.replace(false)
                || window.imp().disposing.get()
            {
                return;
            }
            let saved = state.saved_focus.take();
            let target = saved
                .as_ref()
                .and_then(glib::WeakRef::upgrade)
                .filter(|widget| {
                    widget.is_mapped()
                        && widget
                            .root()
                            .as_ref()
                            .map(glib::object::Cast::upcast_ref::<gtk4::Widget>)
                            == Some(window.upcast_ref::<gtk4::Widget>())
                });
            if let Some(target) = target
                && target.grab_focus()
            {
                return;
            }
            if window.active_editor().is_some() {
                window.focus_selected_editor_after_action();
            } else {
                gtk4::prelude::GtkWindowExt::set_focus(&window, gtk4::Widget::NONE);
            }
        });
    }

    /// Stage: an activated row. The popover has closed. A tab that closed
    /// meanwhile only closes the popover (its focus comes back as on Escape);
    /// otherwise the tab is selected, the strip scrolls it into view (ledger
    /// A133, after D3's guard cancelled any step of ours; the already
    /// selected tab through the selection reveal), and its editor takes the
    /// keyboard.
    fn select_tab_from_all_tabs(&self, page: Option<libadwaita::TabPage>) {
        let tab_view = &self.imp().tab_view;
        let page = page
            .filter(|page| (0..tab_view.n_pages()).any(|index| tab_view.nth_page(index) == *page));
        let Some(page) = page else {
            self.restore_focus_after_all_tabs();
            return;
        };
        self.imp().tab_overflow.saved_focus.replace(None);
        if tab_view.selected_page().as_ref() == Some(&page) {
            // No selection change would scroll the strip to it.
            self.reveal_selected_tab();
        } else {
            tab_view.set_selected_page(&page);
        }
        self.focus_selected_editor_after_action();
    }

    /// Cancel a focus restore whose idle has not run yet (a reopen,
    /// disposal). Returns whether one was pending.
    pub(super) fn cancel_all_tabs_focus_restore(&self) -> bool {
        let state = &self.imp().tab_overflow;
        state
            .focus_restore_generation
            .set(state.focus_restore_generation.get().wrapping_add(1));
        state.focus_restore_pending.replace(false)
    }

    /// Whether the All Tabs popover shows.
    pub(crate) fn all_tabs_popover_visible(&self) -> bool {
        self.imp()
            .tab_overflow
            .controls()
            .is_some_and(|controls| controls.popover.is_visible())
    }

    /// `win.toggle-all-tabs`: open the popover, or close it when open.
    pub(super) fn toggle_all_tabs(&self) {
        self.set_all_tabs_visible(!self.all_tabs_popover_visible());
    }

    /// `win.set-all-tabs-visible`: the target-state form of the toggle,
    /// through the same menu button path.
    pub(super) fn set_all_tabs_visible(&self, visible: bool) {
        let Some(controls) = self.imp().tab_overflow.controls() else {
            return;
        };
        if visible {
            if self.imp().tab_overflow.all_tabs_visible.get() && !controls.popover.is_visible() {
                controls.all_tabs.popup();
            }
        } else if controls.popover.is_visible() {
            controls.popover.popdown();
        }
    }

    /// `win.set-all-tabs-query`: type into the open popover's search entry.
    pub(super) fn set_all_tabs_query(&self, query: &str) {
        if let Some(controls) = self.imp().tab_overflow.controls()
            && controls.popover.is_visible()
        {
            controls.popover.set_query(query);
        }
    }

    pub(in crate::ui::window) fn set_all_tabs_query_enabled(&self, enabled: bool) {
        self.set_tab_overflow_action_enabled(SET_ALL_TABS_QUERY_ACTION, enabled);
    }
}
