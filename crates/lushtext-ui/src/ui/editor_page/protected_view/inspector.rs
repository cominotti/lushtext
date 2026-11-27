// SPDX-License-Identifier: GPL-3.0-or-later

//! The Line Inspector dialog (`protected-view-inspector-and-surfaces`, design
//! D6): a **called presentation surface** of the Protected View workflow, with
//! no role. It shows one window of at most `N_INSPECT` characters that
//! `inspector_execution` read on a worker, and forwards its buttons to that
//! coordination module; it decides nothing and reads no buffer.
//!
//! Its text view holds only the window, so caret, selection, copy (bounded to
//! the window), input-method and accessibility paths stay within the
//! `L_PROTECT` basis.

use std::cell::RefCell;

use gtk4::prelude::*;
use libadwaita::subclass::prelude::*;

use crate::ui::accessibility::{self, AnnouncementLane};

/// What the inspector shows: the window's place in the document and its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InspectorPresentation {
    /// 1-based line number.
    pub(crate) line: u64,
    pub(crate) lines: u64,
    /// The window's half-open column range, 0-based.
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) paragraph_chars: u64,
    pub(crate) has_previous: bool,
    pub(crate) has_next: bool,
    /// Characters of this line the editor hides (0 when it is not folded):
    /// what the fold marker's tooltip says, reachable from the keyboard here.
    pub(crate) hidden_chars: u64,
    /// A folded line before / after this one exists.
    pub(crate) has_previous_folded: bool,
    pub(crate) has_next_folded: bool,
}

/// The heading's subtitle, also the window's accessible label and the
/// announcement when the window moves. Bounded: numbers only, no text.
#[must_use]
pub(crate) fn presentation_summary(presentation: &InspectorPresentation) -> String {
    if presentation.paragraph_chars == 0 {
        return format!("Line {}, empty", presentation.line);
    }
    let window = format!(
        "Line {}, columns {}–{} of {}",
        presentation.line,
        presentation.start + 1,
        presentation.end,
        presentation.paragraph_chars
    );
    if presentation.hidden_chars == 0 {
        return window;
    }
    format!(
        "{window}; {} characters hidden in the editor",
        presentation.hidden_chars
    )
}

/// The buttons and entries the coordination module answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectorRequest {
    PreviousWindow,
    NextWindow,
    /// Go to the line and column in the spin buttons (both 1-based).
    GoTo {
        line: u64,
        column: u64,
    },
    CopyWindow,
    CopyWholeLine,
    /// The nearest folded line before the shown one (Alt+Up).
    PreviousFoldedLine,
    /// The nearest folded line after the shown one (Alt+Down).
    NextFoldedLine,
}

type RequestHandler = Box<dyn Fn(InspectorRequest)>;

mod imp {
    use super::{RefCell, RequestHandler};
    use gtk4::CompositeTemplate;
    use libadwaita::subclass::prelude::*;

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/dev/cominotti/lushtext/ui/line-inspector.ui")]
    pub struct LushtextLineInspector {
        #[template_child]
        pub window_title: TemplateChild<libadwaita::WindowTitle>,
        #[template_child]
        pub window_scroller: TemplateChild<gtk4::ScrolledWindow>,
        #[template_child]
        pub window_view: TemplateChild<gtk4::TextView>,
        #[template_child]
        pub previous_button: TemplateChild<gtk4::Button>,
        #[template_child]
        pub next_button: TemplateChild<gtk4::Button>,
        #[template_child]
        pub previous_folded_button: TemplateChild<gtk4::Button>,
        #[template_child]
        pub next_folded_button: TemplateChild<gtk4::Button>,
        #[template_child]
        pub line_spin: TemplateChild<gtk4::SpinButton>,
        #[template_child]
        pub column_spin: TemplateChild<gtk4::SpinButton>,
        #[template_child]
        pub go_button: TemplateChild<gtk4::Button>,
        #[template_child]
        pub copy_window_button: TemplateChild<gtk4::Button>,
        #[template_child]
        pub copy_line_button: TemplateChild<gtk4::Button>,
        pub(super) handler: RefCell<Option<RequestHandler>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LushtextLineInspector {
        const NAME: &'static str = "LushtextLineInspector";
        type Type = super::LushtextLineInspector;
        type ParentType = libadwaita::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LushtextLineInspector {
        fn constructed(&self) {
            self.parent_constructed();
            // `monospace: true` in line-inspector.blp: the view follows the
            // editor font and zoom (`ui/font_zoom.rs`).
            crate::ui::font_zoom::follow_editor_font(&*self.window_view);
            crate::ui::entry_paste_cap::cap_single_line_inserts(&*self.line_spin);
            crate::ui::entry_paste_cap::cap_single_line_inserts(&*self.column_spin);
            self.obj().wire();
        }
    }
    impl WidgetImpl for LushtextLineInspector {}
    impl AdwDialogImpl for LushtextLineInspector {}
}

glib::wrapper! {
    pub struct LushtextLineInspector(ObjectSubclass<imp::LushtextLineInspector>)
        @extends libadwaita::Dialog, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::ShortcutManager;
}

impl LushtextLineInspector {
    #[must_use]
    pub(crate) fn new() -> Self {
        glib::Object::new()
    }

    /// Route the dialog's requests to `handler` (the coordination module).
    pub(crate) fn connect_request(&self, handler: impl Fn(InspectorRequest) + 'static) {
        self.imp().handler.replace(Some(Box::new(handler)));
    }

    /// Run exactly what the matching button or entry runs (the buttons'
    /// handlers call this too).
    pub fn activate(&self, request: InspectorRequest) {
        self.request(request);
    }

    fn request(&self, request: InspectorRequest) {
        if let Some(handler) = self.imp().handler.borrow().as_ref() {
            handler(request);
        }
    }

    fn wire(&self) {
        let imp = self.imp();
        accessibility::set_label(&*imp.previous_button, "Previous Window");
        accessibility::set_label(&*imp.next_button, "Next Window");
        accessibility::set_label(&*imp.previous_folded_button, "Previous Folded Line");
        accessibility::set_label(&*imp.next_folded_button, "Next Folded Line");
        accessibility::set_label(&*imp.line_spin, "Line");
        accessibility::set_label(&*imp.column_spin, "Column");
        accessibility::set_label(&*imp.go_button, "Go to Line and Column");
        accessibility::set_label(&*imp.copy_window_button, "Copy Window");
        accessibility::set_label(&*imp.copy_line_button, "Copy Whole Line");
        accessibility::set_read_only(&*imp.window_view, true);
        accessibility::set_multi_line(&*imp.window_view, true);
        let connect = |button: &gtk4::Button, request: fn(&Self) -> InspectorRequest| {
            let dialog = self.downgrade();
            button.connect_clicked(move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.request(request(&dialog));
                }
            });
        };
        connect(&imp.previous_button, |_| InspectorRequest::PreviousWindow);
        connect(&imp.next_button, |_| InspectorRequest::NextWindow);
        connect(&imp.previous_folded_button, |_| {
            InspectorRequest::PreviousFoldedLine
        });
        connect(&imp.next_folded_button, |_| {
            InspectorRequest::NextFoldedLine
        });
        connect(&imp.go_button, Self::go_to_request);
        connect(&imp.copy_window_button, |_| InspectorRequest::CopyWindow);
        connect(&imp.copy_line_button, |_| InspectorRequest::CopyWholeLine);
        // Page Down / Page Up (and Alt+Right / Alt+Left) page through the
        // line, and Alt+Down / Alt+Up move to the next or previous folded
        // line, from anywhere in the dialog: the window text is at most
        // N_INSPECT characters, so its own paging has nothing to do. Capture
        // phase, so the text view's bindings never see the keys.
        let paging = gtk4::EventControllerKey::new();
        paging.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let dialog = self.downgrade();
        paging.connect_key_pressed(move |_, key, _, state| {
            let Some(dialog) = dialog.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let alt = state.contains(gtk4::gdk::ModifierType::ALT_MASK);
            let request = match key {
                gtk4::gdk::Key::Page_Down | gtk4::gdk::Key::KP_Page_Down => {
                    InspectorRequest::NextWindow
                }
                gtk4::gdk::Key::Page_Up | gtk4::gdk::Key::KP_Page_Up => {
                    InspectorRequest::PreviousWindow
                }
                gtk4::gdk::Key::Right if alt => InspectorRequest::NextWindow,
                gtk4::gdk::Key::Left if alt => InspectorRequest::PreviousWindow,
                gtk4::gdk::Key::Down | gtk4::gdk::Key::KP_Down if alt => {
                    InspectorRequest::NextFoldedLine
                }
                gtk4::gdk::Key::Up | gtk4::gdk::Key::KP_Up if alt => {
                    InspectorRequest::PreviousFoldedLine
                }
                _ => return glib::Propagation::Proceed,
            };
            dialog.request(request);
            glib::Propagation::Stop
        });
        self.add_controller(paging);
        for spin in [&*imp.line_spin, &*imp.column_spin] {
            let dialog = self.downgrade();
            spin.connect_activate(move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.request(dialog.go_to_request());
                }
            });
        }
    }

    fn go_to_request(&self) -> InspectorRequest {
        let imp = self.imp();
        let line = u64::try_from(imp.line_spin.value_as_int())
            .unwrap_or(1)
            .max(1);
        let column = u64::try_from(imp.column_spin.value_as_int())
            .unwrap_or(1)
            .max(1);
        InspectorRequest::GoTo { line, column }
    }

    /// Show a window that a worker read. `announce` moves the polite
    /// announcement (a window move, not the first show).
    pub(crate) fn present_window(
        &self,
        presentation: &InspectorPresentation,
        text: &str,
        announce: bool,
    ) {
        let imp = self.imp();
        let summary = presentation_summary(presentation);
        imp.window_title.set_subtitle(&summary);
        imp.window_view.buffer().set_text(text);
        accessibility::set_label(&*imp.window_view, &summary);
        imp.previous_button.set_sensitive(presentation.has_previous);
        imp.next_button.set_sensitive(presentation.has_next);
        imp.previous_folded_button
            .set_sensitive(presentation.has_previous_folded);
        imp.next_folded_button
            .set_sensitive(presentation.has_next_folded);
        let to_f64 = |value: u64| f64::from(u32::try_from(value).unwrap_or(u32::MAX));
        imp.line_spin
            .adjustment()
            .set_upper(to_f64(presentation.lines.max(1)));
        imp.line_spin.set_value(to_f64(presentation.line));
        imp.column_spin
            .adjustment()
            .set_upper(to_f64(presentation.paragraph_chars.max(1)));
        imp.column_spin.set_value(to_f64(presentation.start + 1));
        imp.copy_window_button.set_sensitive(!text.is_empty());
        imp.copy_line_button
            .set_sensitive(presentation.paragraph_chars > 0);
        if announce {
            accessibility::announce_with_lane(
                &*imp.window_view,
                &summary,
                AnnouncementLane::StatusUpdate,
            );
        }
    }

    /// Select `chars` characters from window column `offset` (clamped to the
    /// window), and scroll to them: a find match inside the window.
    pub(crate) fn select_window_range(&self, offset: u64, chars: u64) {
        let view = self.imp().window_view.get();
        let buffer = view.buffer();
        let (Ok(offset), Ok(chars)) = (i32::try_from(offset), i32::try_from(chars)) else {
            return;
        };
        let start = buffer.iter_at_offset(offset);
        let end = buffer.iter_at_offset(offset.saturating_add(chars));
        buffer.select_range(&start, &end);
        view.scroll_to_iter(&mut start.clone(), 0.1, false, 0.0, 0.0);
    }

    /// The window's text view (the copy-window path and tests read it).
    #[must_use]
    pub fn window_view(&self) -> gtk4::TextView {
        self.imp().window_view.get()
    }

    /// The title's subtitle: the window summary.
    #[must_use]
    pub fn summary(&self) -> String {
        self.imp().window_title.subtitle().to_string()
    }
}
