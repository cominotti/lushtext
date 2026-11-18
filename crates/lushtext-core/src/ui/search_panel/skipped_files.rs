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

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita::prelude::AdwDialogExt;

use crate::model::content_search::{ReplaceSkipReason, ReplaceSkipReport};
use crate::ui::accessibility;
use crate::ui::row_text::LushtextRowText;

use super::LushtextSearchPanel;
use super::policy;

/// Width of the dialog, in sp; it shrinks to the window on narrow layouts.
const SKIPPED_FILES_WIDTH_SP: i32 = 560;
/// Height of the dialog, in sp.
const SKIPPED_FILES_HEIGHT_SP: i32 = 520;

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
    fn new(file: &crate::model::content_search::ReplaceSkippedFile) -> Self {
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
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SkippedFilesModel {
        const NAME: &'static str = "LushtextSkippedFilesModel";
        type Type = super::SkippedFilesModel;
        type Interfaces = (gio::ListModel, gtk4::SectionModel);
    }

    impl ObjectImpl for SkippedFilesModel {}

    impl SkippedFilesModel {
        pub(super) fn len(&self) -> u32 {
            self.report.borrow().as_ref().map_or(0, |report| {
                u32::try_from(report.files().len()).unwrap_or(u32::MAX)
            })
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
                .map_or((len, u32::MAX), |(_, start, end)| (*start, *end))
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
        model
    }

    /// Row objects the model has built: GTK asks only for the rows it realizes.
    #[must_use]
    pub fn item_requests(&self) -> u64 {
        self.imp().item_requests.get()
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
            move |_| {
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
