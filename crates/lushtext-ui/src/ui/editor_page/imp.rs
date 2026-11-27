// SPDX-License-Identifier: GPL-3.0-or-later

//! Private GObject implementation for `LushtextEditorPage`.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use gtk_lush_signals::SignalBag;
use gtk4::gio;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use lushtext_model::document::formatting_overrides::FormattingOverrides;
use sourceview5::prelude::*;

use crate::ui::info_bar::LushtextInfoBar;
use crate::ui::search_bar::LushtextSearchBar;

use super::bookmarks::BookmarkState;
use super::buffer_replacement::BufferReplacementState;
use super::callbacks::EditorCallbacks;
use super::document_identity::{DocumentMetadataState, DraftState, MemoryResidencyState};
use super::document_scale::DocumentScaleState;
use super::focus_mode::FocusModeEditorState;
use super::formatting::PreferenceBindingState;
use super::frame_validation::FrameValidationState;
use super::language_attach::LanguageAttachState;
use super::load::{EditorLoadState, LoadState, LoadTrackingState};
use super::local_history::LocalHistoryState;
use super::minimap::MinimapState;
use super::monitor::MonitorState;
use super::overscroll::OverscrollState;
use super::restore_position::RestoreState;
use super::save::SaveState;

/// Private template implementation for one editor tab.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/editor-page.ui")]
pub struct LushtextEditorPage {
    #[template_child]
    pub info_bar: TemplateChild<LushtextInfoBar>,
    #[template_child]
    pub overlay: TemplateChild<gtk4::Overlay>,
    #[template_child]
    pub minimap_overlay: TemplateChild<gtk4::Overlay>,
    #[template_child]
    pub source_view: TemplateChild<sourceview5::View>,
    #[template_child]
    pub scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    #[template_child]
    pub search_revealer: TemplateChild<gtk4::Revealer>,
    #[template_child]
    pub search_bar: TemplateChild<LushtextSearchBar>,
    pub file_path: RefCell<Option<PathBuf>>,
    pub canonical_file_path: RefCell<Option<PathBuf>>,
    pub file_size: Cell<Option<u64>>,
    pub load_state: Cell<EditorLoadState>,
    pub latest_load_failed: Cell<bool>,
    pub document_scale: DocumentScaleState,
    pub language_attach: LanguageAttachState,
    pub reanalysis_pacing: super::reanalysis_pacing::ReanalysisPacingState,
    pub restyle: super::restyle::RestyleState,
    pub edit_lock: super::edit_lock::EditLockState,
    pub held_input: super::held_input::HeldInputState,
    pub(crate) protected_boundaries: crate::ui::protected_fold::BoundaryLedger,
    pub(crate) protected_view: super::protected_view::execution::ProtectedViewRuntime,
    pub(crate) protected_drop_targets:
        super::protected_view::interaction_execution::DetachedDropTargets,
    pub(crate) protected_find: super::protected_view::find_execution::ProtectedFindState,
    pub(crate) protected_source_capture:
        super::protected_view::capture_execution::SourceCaptureState,
    pub(crate) protected_fold_gutter: RefCell<Option<sourceview5::GutterRendererText>>,
    pub(crate) protected_notice: super::protected_view::notice_execution::ProtectedNoticeState,
    pub(crate) line_inspector: super::protected_view::inspector_execution::LineInspectorState,
    pub residency: MemoryResidencyState,
    pub load_tracking: LoadTrackingState,
    pub applied_style_scheme_id: RefCell<Option<String>>,
    pub document_surface_opacity: Cell<f64>,
    pub settings: gio::Settings,
    pub preference_bindings: PreferenceBindingState,
    pub formatting_overrides: Cell<FormattingOverrides>,
    pub document_buffer_signals: SignalBag,
    pub editing_buffer_signals: SignalBag,
    pub(crate) callbacks: EditorCallbacks,
    pub monitor: MonitorState,
    pub draft: DraftState,
    pub save: SaveState,
    pub document_metadata: DocumentMetadataState,
    pub load: LoadState,
    pub replacement: BufferReplacementState,
    pub restore: RestoreState,
    pub overscroll: OverscrollState,
    pub bookmarks: BookmarkState,
    pub local_history: LocalHistoryState,
    pub minimap: MinimapState,
    pub focus_mode: FocusModeEditorState,
    pub frame_validation: FrameValidationState,
    pub(crate) view_attachment: super::view_attachment::ViewAttachmentState,
}

impl Default for LushtextEditorPage {
    fn default() -> Self {
        Self {
            info_bar: TemplateChild::default(),
            overlay: TemplateChild::default(),
            minimap_overlay: TemplateChild::default(),
            source_view: TemplateChild::default(),
            scrolled_window: TemplateChild::default(),
            search_revealer: TemplateChild::default(),
            search_bar: TemplateChild::default(),
            file_path: RefCell::default(),
            canonical_file_path: RefCell::default(),
            file_size: Cell::default(),
            load_state: Cell::new(EditorLoadState::Untitled),
            latest_load_failed: Cell::new(false),
            document_scale: DocumentScaleState::default(),
            language_attach: LanguageAttachState::default(),
            reanalysis_pacing: super::reanalysis_pacing::ReanalysisPacingState::default(),
            restyle: super::restyle::RestyleState::default(),
            edit_lock: super::edit_lock::EditLockState::default(),
            held_input: super::held_input::HeldInputState::default(),
            protected_boundaries: crate::ui::protected_fold::BoundaryLedger::default(),
            protected_view: super::protected_view::execution::ProtectedViewRuntime::default(),
            protected_drop_targets:
                super::protected_view::interaction_execution::DetachedDropTargets::default(),
            protected_find: super::protected_view::find_execution::ProtectedFindState::default(),
            protected_source_capture:
                super::protected_view::capture_execution::SourceCaptureState::default(),
            protected_fold_gutter: RefCell::default(),
            protected_notice:
                super::protected_view::notice_execution::ProtectedNoticeState::default(),
            line_inspector: super::protected_view::inspector_execution::LineInspectorState::default(
            ),
            residency: MemoryResidencyState::default(),
            load_tracking: LoadTrackingState::default(),
            applied_style_scheme_id: RefCell::new(None),
            document_surface_opacity: Cell::new(1.0),
            settings: crate::ui::settings_writes::shared(),
            preference_bindings: PreferenceBindingState::default(),
            formatting_overrides: Cell::new(FormattingOverrides::default()),
            document_buffer_signals: SignalBag::new(),
            editing_buffer_signals: SignalBag::new(),
            callbacks: EditorCallbacks::default(),
            monitor: MonitorState::default(),
            draft: DraftState::default(),
            save: SaveState::default(),
            document_metadata: DocumentMetadataState::default(),
            load: LoadState::default(),
            replacement: BufferReplacementState::default(),
            restore: RestoreState::default(),
            overscroll: OverscrollState::default(),
            bookmarks: BookmarkState::default(),
            local_history: LocalHistoryState::default(),
            minimap: MinimapState::default(),
            focus_mode: FocusModeEditorState::default(),
            frame_validation: FrameValidationState::default(),
            view_attachment: super::view_attachment::ViewAttachmentState::default(),
        }
    }
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextEditorPage {
    const NAME: &str = "LushtextEditorPage";
    type Type = super::LushtextEditorPage;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        LushtextInfoBar::ensure_type();
        LushtextSearchBar::ensure_type();
        sourceview5::View::ensure_type();
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextEditorPage {
    fn signals() -> &'static [glib::subclass::Signal] {
        static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
            std::sync::OnceLock::new();
        SIGNALS
            .get_or_init(|| vec![glib::subclass::Signal::builder("memory-policy-changed").build()])
    }

    fn dispose(&self) {
        self.obj().close_line_inspector();
        super::save::admission::cancel_for_editor(&self.obj());
        self.obj().cancel_buffer_replacement_for_dispose();
        self.obj().dispose_load_resources();
        self.obj().dispose_language_attach();
        self.obj().dispose_restyle();
        self.save.dispose_snapshot();
        if let Some(snapshot) = self.document_metadata.lossy_analysis_snapshot.take() {
            snapshot.dispose();
        }
        if let super::local_history::PeriodicSnapshotCapture::Capturing(snapshot) =
            self.local_history.periodic_snapshot.take()
        {
            snapshot.dispose();
        }
        self.obj().dispose_minimap_analysis();
        self.minimap.dispose();
        let _ = self.local_history.periodic_timer.invalidate();
        self.local_history.periodic_timer_token.set(None);
        self.local_history
            .editor_generation
            .set(self.local_history.editor_generation.get().next());
        self.callbacks.dispose(&self.obj());
        self.preference_bindings.signals.clear();
        self.document_buffer_signals.clear();
        self.editing_buffer_signals.clear();
        self.local_history.buffer_signals.clear();
        self.focus_mode.buffer_signals.clear();
        self.frame_validation.clear();
        self.document_scale.signals.clear();
        self.overscroll.observer.take();
        self.overscroll.reflow_pause.take();
        self.overscroll.scroll_source_signals.clear();
        self.focus_mode.text_origin_guide.take();
        self.obj().cancel_modified_tag_retirement();
        if let Some(monitor) = self.monitor.file_monitor.take() {
            monitor.cancel();
        }
    }

    fn constructed(&self) {
        self.parent_constructed();
        let buffer = self
            .source_view
            .buffer()
            .downcast::<sourceview5::Buffer>()
            .expect("GtkSourceView buffer");
        let _ = self.view_attachment.document.set(buffer.clone());
        buffer.set_highlight_syntax(true);
        self.obj().setup_edit_lock();
        crate::ui::responsiveness_probe::observe_editor(&self.source_view);
        self.obj().setup_settle_watch();
        crate::ui::font_zoom::follow_editor_font(&*self.source_view);
        self.obj().setup_editor_preferences_and_signals(&buffer);
        self.obj().setup_minimap();
        self.obj().setup_allocation_reflow_observers();
        self.obj().setup_focus_mode_text_origin_guide();
        self.obj().setup_focus_mode_presentation();
        self.obj().setup_frame_validation();
        self.obj().setup_document_scale_watch();
        self.obj().setup_reanalysis_pacing();
        self.obj().setup_paragraph_wrap();
        self.obj().setup_bounded_surrounding_text();
        self.obj().setup_logical_cursor_steps();
        self.obj().setup_protected_view_interaction();
        self.obj().install_fold_gutter();
        self.obj().apply_invisible_characters_mode();
        self.obj().refresh_accessibility_metadata();
        self.obj().setup_view_attachment();
    }
}

impl WidgetImpl for LushtextEditorPage {
    fn map(&self) {
        self.obj().attach_view();
        self.parent_map();
    }

    fn unmap(&self) {
        self.parent_unmap();
        self.obj().watch_hidden_validation();
    }

    fn unroot(&self) {
        self.obj().close_line_inspector();
        self.parent_unroot();
    }
}

impl BoxImpl for LushtextEditorPage {}
