// SPDX-License-Identifier: GPL-3.0-or-later

//! Called presentation surface: the Skipped Files dialog of Replace All.
//!
//! # Role
//!
//! This module carries **no role**. It projects the last run's
//! `ReplaceSkipReport` onto an `AdwDialog` (design D7) and is named in
//! `WFR-SEARCH-REPLACE`'s matrix row. Its wording comes from `policy`.
//!
//! # Why it is lazy
//!
//! A failure-heavy run can skip 16,384 files. The model holds the report as
//! one `Arc` and builds a row object only when GTK asks for a position, which
//! it does for the realized range (about 205 rows, ledger A1), the selection
//! and an activated row; its sections are the report's reason runs
//! (`policy::skip_sections`), one heading per reason, as the notes browser's
//! `GtkSectionModel` does (`ui/window/notes/row_model.rs`). The report never
//! changes while the dialog shows it, so no row is ever retargeted.
//!
//! # Why its first publish is staged
//!
//! Handed all 16,384 rows at once, the list realized GTK's whole band of
//! about 205 rows inside the dispatch that opened the dialog (the lane's
//! opening dispatch read 16.5–20.2 ms; `stream-replace-all` task 9.1). The
//! model lists `model::list_budget::staged_first_rows` first and grows one
//! step per frame (`ui::list_growth::RowGrowth`), as the notes browser does
//! (`.agents/rules/ui.md`, "Realized List Rows"), once the dialog's open
//! animation is over or the list scrolls; any key finishes the growth first,
//! so keyboard navigation lands where it would on the whole list.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita::prelude::AdwDialogExt;

use crate::ui::accessibility;
use crate::ui::list_growth::RowGrowth;
use crate::ui::row_text::LushtextRowText;
use lushtext_model::workspace::content_search::{ReplaceSkipReason, ReplaceSkipReport};

use super::LushtextSearchPanel;
use super::policy;

/// Width of the dialog, in sp; it shrinks to the window on narrow layouts.
const SKIPPED_FILES_WIDTH_SP: i32 = 560;
/// Height of the dialog, in sp.
const SKIPPED_FILES_HEIGHT_SP: i32 = 520;
/// How long the staged first publish waits before it grows: past the
/// dialog's open animation (about 330 ms in the lane), whose frames are
/// paint-bound, so no growth step lands in them. Only how soon the
/// scrollbar's thumb settles depends on it.
const SKIPPED_FILES_GROWTH_DELAY: std::time::Duration = std::time::Duration::from_millis(512);

mod row_imp {
    use super::{Cell, PathBuf, RefCell, glib};
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct SkippedFileRow {
        pub(super) title: RefCell<String>,
        pub(super) subtitle: RefCell<String>,
        pub(super) path: RefCell<PathBuf>,
        pub(super) line: Cell<u32>,
        pub(super) reason: Cell<usize>,
        pub(super) actionable: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SkippedFileRow {
        const NAME: &'static str = "LushtextSkippedFileRow";
        type Type = super::SkippedFileRow;
    }

    impl ObjectImpl for SkippedFileRow {}
}

glib::wrapper! {
    /// One listed skipped file: its name, folder and line, never its text.
    pub struct SkippedFileRow(ObjectSubclass<row_imp::SkippedFileRow>);
}

impl SkippedFileRow {
    fn new(file: &lushtext_model::workspace::content_search::ReplaceSkippedFile) -> Self {
        let row: Self = glib::Object::new();
        let imp = row.imp();
        imp.title.replace(file.path.file_name().map_or_else(
            || file.path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ));
        imp.subtitle.replace(policy::skipped_file_subtitle(file));
        imp.path.replace(file.path.clone());
        imp.line.set(
            file.first_line
                .map_or(1, |line| u32::try_from(line).unwrap_or(u32::MAX)),
        );
        imp.reason.set(file.reason.index());
        imp.actionable
            .set(policy::skipped_file_opens_in_editor(file.reason));
        row
    }

    fn reason(&self) -> ReplaceSkipReason {
        ReplaceSkipReason::ALL[self.imp().reason.get()]
    }
}

mod model_imp {
    use super::{
        Arc, Cell, HashMap, RefCell, ReplaceSkipReason, ReplaceSkipReport, SkippedFileRow, gio,
        glib,
    };
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct SkippedFilesModel {
        pub(super) report: RefCell<Option<Arc<ReplaceSkipReport>>>,
        pub(super) sections: RefCell<Vec<(ReplaceSkipReason, u32, u32)>>,
        pub(super) live: RefCell<HashMap<u32, glib::WeakRef<SkippedFileRow>>>,
        /// Row objects built on demand (evidence: only around the realized rows).
        pub(super) item_requests: Cell<u64>,
        /// Rows a staged first publish lists so far; `None` once every file
        /// is listed.
        pub(super) listed: Cell<Option<u32>>,
        /// When the dialog began opening (monotonic µs), for the growth span.
        pub(super) opened_us: Cell<i64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SkippedFilesModel {
        const NAME: &'static str = "LushtextSkippedFilesModel";
        type Type = super::SkippedFilesModel;
        type Interfaces = (gio::ListModel, gtk4::SectionModel);
    }

    impl ObjectImpl for SkippedFilesModel {}

    impl SkippedFilesModel {
        /// Every file the report lists, whether or not GTK is told yet.
        pub(super) fn full_len(&self) -> u32 {
            self.report.borrow().as_ref().map_or(0, |report| {
                u32::try_from(report.files().len()).unwrap_or(u32::MAX)
            })
        }

        /// The rows GTK is told about: the listed rows of a growing first
        /// publish, then every file.
        pub(super) fn len(&self) -> u32 {
            let full = self.full_len();
            self.listed.get().map_or(full, |listed| listed.min(full))
        }
    }

    impl ListModelImpl for SkippedFilesModel {
        fn item_type(&self) -> glib::Type {
            SkippedFileRow::static_type()
        }

        fn n_items(&self) -> u32 {
            self.len()
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            if position >= self.len() {
                return None;
            }
            let mut live = self.live.borrow_mut();
            if let Some(row) = live.get(&position).and_then(glib::WeakRef::upgrade) {
                return Some(row.upcast());
            }
            let report = self.report.borrow();
            let file = report
                .as_ref()?
                .files()
                .get(usize::try_from(position).ok()?)?;
            let row = SkippedFileRow::new(file);
            self.item_requests
                .set(self.item_requests.get().saturating_add(1));
            if live.len() >= 1_024 {
                live.retain(|_, row| row.upgrade().is_some());
            }
            live.insert(position, row.downgrade());
            Some(row.upcast())
        }
    }

    impl SectionModelImpl for SkippedFilesModel {
        fn section(&self, position: u32) -> (u32, u32) {
            let len = self.len();
            if position >= len {
                return (len, u32::MAX);
            }
            self.sections
                .borrow()
                .iter()
                .find(|(_, start, end)| (*start..*end).contains(&position))
                // A section is cut at the listed rows while the list grows.
                .map_or((len, u32::MAX), |(_, start, end)| (*start, (*end).min(len)))
        }
    }
}

glib::wrapper! {
    /// The dialog's lazy list of skipped files, one section per reason.
    pub struct SkippedFilesModel(ObjectSubclass<model_imp::SkippedFilesModel>)
        @implements gio::ListModel, gtk4::SectionModel;
}

impl SkippedFilesModel {
    fn new(report: Arc<ReplaceSkipReport>) -> Self {
        let model: Self = glib::Object::new();
        model
            .imp()
            .sections
            .replace(policy::skip_sections(report.files()));
        model.imp().report.replace(Some(report));
        let full = model.imp().full_len();
        let first = lushtext_model::budgets::list_budget::staged_first_rows(full);
        model.imp().listed.set((first < full).then_some(first));
        model
            .imp()
            .opened_us
            .set(crate::ui::construction_spans::now_us());
        model
    }

    /// List the next growth step (`model::list_budget::next_growth`); `false`
    /// once every file is listed.
    fn grow_step(&self) -> bool {
        crate::ui::responsiveness_probe::span(
            crate::ui::responsiveness_probe::SKIPPED_FILES_GROWTH_STEP_SPAN,
            || self.grow_one_step(),
        )
    }

    fn grow_one_step(&self) -> bool {
        let imp = self.imp();
        let Some(listed) = imp.listed.get() else {
            return false;
        };
        let full = imp.full_len();
        let Some(next) = lushtext_model::budgets::list_budget::next_growth(listed, full) else {
            imp.listed.set(None);
            self.record_growth_end();
            return false;
        };
        imp.listed.set((next < full).then_some(next));
        self.items_changed(listed, 0, next - listed);
        if next < full {
            true
        } else {
            self.record_growth_end();
            false
        }
    }

    /// List every file a growing first publish has not listed yet.
    fn finish_growth(&self) {
        let imp = self.imp();
        let Some(listed) = imp.listed.take() else {
            return;
        };
        let full = imp.full_len();
        if full > listed {
            self.items_changed(listed, 0, full - listed);
        }
        self.record_growth_end();
    }

    /// Files a growing first publish has still to list.
    fn growing_rows(&self) -> u32 {
        self.imp().full_len() - self.imp().len()
    }

    fn record_growth_end(&self) {
        crate::ui::responsiveness_probe::record_span(
            crate::ui::responsiveness_probe::SKIPPED_FILES_GROWTH_SPAN,
            self.imp().opened_us.get(),
            crate::ui::construction_spans::now_us(),
        );
    }

    /// Row objects the model has built: GTK asks only for the rows it realizes.
    #[must_use]
    pub fn item_requests(&self) -> u64 {
        self.imp().item_requests.get()
    }

    /// Rows the model lists to GTK.
    #[must_use]
    pub fn listed(&self) -> u32 {
        self.imp().len()
    }
}

impl crate::ui::list_growth::GrowingRows for SkippedFilesModel {
    fn grow(&self) -> bool {
        self.grow_step()
    }
}

impl LushtextSearchPanel {
    /// Present the Skipped Files dialog for the last Replace All run.
    ///
    /// Activating a row is Open and Replace in Editor (`win.replace-in-editor`,
    /// design D7): the window opens the file at its first skipped line, or
    /// selects its tab, and prefills the in-tab bar from the run's request. A
    /// file too large to open, or unreadable, has no action.
    pub fn show_skipped_files(&self) {
        crate::ui::responsiveness_probe::span(
            crate::ui::responsiveness_probe::SKIPPED_FILES_OPEN_SPAN,
            || self.present_skipped_files(),
        );
    }

    fn present_skipped_files(&self) {
        let Some(report) = self.last_skip_report() else {
            return;
        };
        let model = SkippedFilesModel::new(Arc::clone(&report));
        let dialog = libadwaita::Dialog::builder()
            .title("Skipped Files")
            .content_width(SKIPPED_FILES_WIDTH_SP)
            .content_height(SKIPPED_FILES_HEIGHT_SP)
            .build();

        let summary = gtk4::Label::new(Some(&policy::skip_summary(&report)));
        summary.set_wrap(true);
        summary.set_xalign(0.0);
        summary.add_css_class("dim-label");
        summary.set_margin_start(12);
        summary.set_margin_end(12);
        summary.set_margin_top(6);
        summary.set_margin_bottom(6);

        let selection = gtk4::SingleSelection::new(Some(model.clone()));
        selection.set_autoselect(false);
        let list_view = gtk4::ListView::new(Some(selection), Some(build_row_factory()));
        list_view.set_header_factory(Some(&build_header_factory()));
        list_view.add_css_class("navigation-sidebar");
        list_view.set_tab_behavior(gtk4::ListTabBehavior::Item);
        list_view.set_single_click_activate(false);
        list_view.set_vexpand(true);
        accessibility::set_role(&list_view, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &list_view,
            "Skipped files",
            "Files Replace All did not replace; activate one to open it in the editor with the replacement ready",
        );
        list_view.connect_activate({
            let panel = self.downgrade();
            let dialog = dialog.downgrade();
            move |list_view, position| {
                let (Some(panel), Some(row)) = (
                    panel.upgrade(),
                    list_view
                        .model()
                        .and_then(|model| model.item(position))
                        .and_then(|item| item.downcast::<SkippedFileRow>().ok()),
                ) else {
                    return;
                };
                if !row.imp().actionable.get() {
                    return;
                }
                let path = row.imp().path.borrow().clone();
                if let Some(dialog) = dialog.upgrade() {
                    dialog.close();
                }
                if let Some(ref callback) =
                    *panel.imp().callbacks.replace_in_editor_callback.borrow()
                {
                    callback(&path);
                }
            }
        });

        crate::ui::responsiveness_probe::observe_first_frame(
            &list_view,
            crate::ui::responsiveness_probe::SKIPPED_FILES_FIRST_FRAME_SPAN,
        );
        // A key that moves the selection lands where it would on the whole
        // list: a first publish still growing lists the rest first.
        let growth: Rc<RefCell<Option<RowGrowth>>> = Rc::default();
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let growth = Rc::clone(&growth);
            let model = model.downgrade();
            move |_, _, _, _| {
                if let Some(model) = model.upgrade()
                    && model.growing_rows() > 0
                {
                    drop(growth.borrow_mut().take());
                    model.finish_growth();
                }
                glib::Propagation::Proceed
            }
        });
        list_view.add_controller(keys);

        let scroller = gtk4::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk4::PolicyType::Never);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&list_view));

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.append(&summary);
        content.append(&scroller);
        let toolbar = libadwaita::ToolbarView::new();
        toolbar.add_top_bar(&libadwaita::HeaderBar::new());
        toolbar.set_content(Some(&content));
        dialog.set_child(Some(&toolbar));
        self.imp()
            .preview
            .skipped_files_model
            .replace(Some(model.downgrade()));
        // The dialog moves focus to its own default when it maps, after a
        // `grab_focus` here, so name the list as the widget it focuses.
        dialog.set_focus(Some(&list_view));
        // Closing returns focus to where it was (Show Skipped Files, or the
        // palette's caller). `closed` runs inside `close()`, before a row's
        // open callback, so opening a file still leaves focus in its editor.
        let previous_focus = self
            .root()
            .and_then(|root| root.focus())
            .map(|widget| widget.downgrade());
        dialog.connect_closed({
            let panel = self.downgrade();
            let growth = Rc::clone(&growth);
            move |_| {
                drop(growth.borrow_mut().take());
                let target = previous_focus
                    .as_ref()
                    .and_then(glib::WeakRef::upgrade)
                    .filter(WidgetExt::is_visible)
                    .or_else(|| {
                        panel
                            .upgrade()
                            .map(|panel| panel.imp().skipped_files_button.get().upcast())
                            .filter(WidgetExt::is_visible)
                    });
                if let Some(target) = target {
                    target.grab_focus();
                }
            }
        });
        dialog.present(Some(self));
        list_view.grab_focus();
        // The growth runs from the list's frame clock once the dialog shows;
        // a panel in no mapped window has no frame to spare, so it lists
        // every file at once (nobody sees that list being built).
        //
        // It waits out the dialog's open animation: under the Cairo renderer
        // each of its frames paints the whole window for 10–14 ms while their
        // layout takes under 1 ms (the lane's `frame_paint` spans, task 9.1),
        // and a growth step adds up to 2.6 ms of layout to its frame. The
        // listed rows already fill the viewport, so nothing on screen waits.
        // A scroll starts it at once, and a key finishes it.
        if model.growing_rows() == 0 {
            return;
        }
        if !self.root().is_some_and(|root| root.is_mapped()) {
            model.finish_growth();
            return;
        }
        let start_growth: Rc<dyn Fn()> = Rc::new({
            let growth = Rc::clone(&growth);
            let model = model.downgrade();
            let list_view = list_view.downgrade();
            move || {
                let (Some(model), Some(list_view)) = (model.upgrade(), list_view.upgrade()) else {
                    return;
                };
                if growth.borrow().is_some() || model.growing_rows() == 0 {
                    return;
                }
                growth
                    .borrow_mut()
                    .replace(RowGrowth::start(Box::new(model), &list_view));
            }
        });
        // A wheel or touchpad scroll starts it at once. (The adjustment's
        // own `value-changed` is no sign of the user: the list moves it while
        // the dialog animates in.)
        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        scroll.set_propagation_phase(gtk4::PropagationPhase::Capture);
        scroll.connect_scroll({
            let start_growth = Rc::clone(&start_growth);
            move |_, _, _| {
                start_growth();
                glib::Propagation::Proceed
            }
        });
        scroller.add_controller(scroll);
        glib::timeout_add_local_once(SKIPPED_FILES_GROWTH_DELAY, move || start_growth());
    }

    /// Connect Open and Replace in Editor, which a Skipped Files row activates.
    pub fn connect_replace_in_editor<F: Fn(&std::path::Path) + 'static>(&self, f: F) {
        self.imp()
            .callbacks
            .replace_in_editor_callback
            .replace(Some(Box::new(f)));
    }

    /// The open dialog's model, read by the evidence surface.
    #[must_use]
    pub(super) fn skipped_files_model(&self) -> Option<SkippedFilesModel> {
        self.imp()
            .preview
            .skipped_files_model
            .borrow()
            .as_ref()
            .and_then(glib::WeakRef::upgrade)
    }
}

/// Two text lines per row: the file name, then its folder and line.
fn build_row_factory() -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let title = LushtextRowText::new();
        title.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        title.set_xalign(0.0);
        let subtitle = LushtextRowText::new();
        subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::Start);
        subtitle.set_xalign(0.0);
        subtitle.add_css_class("dim-label");
        subtitle.add_css_class("caption");
        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        column.set_margin_top(4);
        column.set_margin_bottom(4);
        column.append(&title);
        column.append(&subtitle);
        item.set_child(Some(&column));
    });
    factory.connect_bind(|_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(row) = item
            .item()
            .and_then(|row| row.downcast::<SkippedFileRow>().ok())
        else {
            return;
        };
        let Some(column) = item.child() else {
            return;
        };
        let mut texts = std::iter::successors(column.first_child(), gtk4::Widget::next_sibling)
            .filter_map(|child| child.downcast::<LushtextRowText>().ok());
        let title = row.imp().title.borrow();
        let subtitle = row.imp().subtitle.borrow();
        if let Some(text) = texts.next() {
            text.set_text(&title);
        }
        if let Some(text) = texts.next() {
            text.set_text(&subtitle);
        }
        item.set_activatable(row.imp().actionable.get());
        let action = if row.imp().actionable.get() {
            "Activate to open it in the editor with the replacement ready"
        } else if row.reason() == ReplaceSkipReason::Unreadable {
            "Cannot be read, so it does not open in the editor"
        } else {
            "Too large to open in the editor"
        };
        let description = format!("{subtitle}. {action}");
        accessibility::apply_list_item_row_accessibility(
            item,
            accessibility::RowAccessibility::new(&title).description(&description),
        );
    });
    factory.connect_unbind(|_, object| {
        if let Some(item) = object.downcast_ref::<gtk4::ListItem>() {
            accessibility::clear_row_accessibility_for_list_item(item);
        }
    });
    factory
}

/// One heading per reason, from the section's first row.
fn build_header_factory() -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
            return;
        };
        let title = LushtextRowText::new();
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        title.set_xalign(0.0);
        title.add_css_class("heading");
        title.set_margin_start(6);
        accessibility::set_role(&title, gtk4::AccessibleRole::Heading);
        header.set_child(Some(&title));
    });
    factory.connect_bind(|_, object| {
        let Some(header) = object.downcast_ref::<gtk4::ListHeader>() else {
            return;
        };
        let (Some(row), Some(title)) = (
            header
                .item()
                .and_then(|row| row.downcast::<SkippedFileRow>().ok()),
            header
                .child()
                .and_then(|child| child.downcast::<LushtextRowText>().ok()),
        ) else {
            return;
        };
        title.set_text(policy::skip_reason_heading(row.reason()));
    });
    factory
}
