// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the Markdown preview widget.
//!
//! Contains a read-only `GtkTextView` inside a `GtkScrolledWindow` for rendered
//! Markdown output, and an `AdwStatusPage` placeholder for non-Markdown files.
//! TextTags are created in `constructed()` and updated on dark/light mode changes
//! via `StyleManager::connect_dark_notify()`. Tables and local image blocks are
//! embedded as anchored child widgets so the preview can stay GTK-native
//! without replacing the surrounding text-buffer renderer.

use gtk_lush_signals::SignalBag;
use gtk4::subclass::prelude::*;
use gtk4::{self, CompositeTemplate, glib};
use libadwaita::prelude::*;
use lushtext_model::values::{CodeBlockRefreshGeneration, EmbedGeneration};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use super::projection_execution::create_or_update_tags;
use crate::ui::accessibility;
use crate::ui::buffer_snapshot::BufferSnapshotHandle;

/// Stored override used by widget tests to observe link activation without
/// launching an external desktop handler.
type LinkActivationCallback = Box<dyn Fn(String)>;
/// One-shot callback fired after the latest deferred code-block width repair.
pub(super) type CodeBlockRefreshCompletionCallback = Box<dyn Fn()>;

/// Private template implementation for the Markdown preview widget.
///
/// Owns the preview text view, placeholder, and render-time anchor state so
/// rerenders can clean up embedded GTK children deterministically.
#[derive(CompositeTemplate, Default)]
#[template(resource = "/dev/cominotti/lushtext/ui/markdown-preview.ui")]
pub struct LushtextMarkdownPreview {
    /// Read-only text surface receiving rendered Markdown tags and anchors.
    #[template_child]
    pub text_view: TemplateChild<gtk4::TextView>,
    /// Scroller that provides preview viewport geometry.
    #[template_child]
    pub scrolled_window: TemplateChild<gtk4::ScrolledWindow>,
    /// Placeholder shown for non-Markdown files or empty preview states.
    #[template_child]
    pub placeholder: TemplateChild<libadwaita::StatusPage>,

    /// Whether we're currently showing the rendered content (true) or the placeholder (false).
    pub showing_content: Cell<bool>,
    /// Anchored widgets currently embedded into the text view.
    ///
    /// `GtkTextChildAnchor` makes tables and image blocks pleasantly native,
    /// but GTK does not manage rerender cleanup for us at the application
    /// level. We keep strong refs plus the layout context captured at insertion
    /// time so `render_markdown`, `clear`, and `show_placeholder` can remove
    /// stale embeds and resize code blocks after later allocations.
    pub(super) rendered_embeds: RefCell<Vec<super::seams::RenderedEmbed>>,
    /// Generation advanced whenever the rendered embed membership changes.
    pub(super) rendered_embed_generation: Cell<EmbedGeneration>,
    /// Last valid text-column width and embed generation fully traversed.
    ///
    /// Hidden or not-yet-allocated previews deliberately leave this untouched
    /// so a later valid allocation still repairs every current code block.
    pub(super) last_code_block_layout: Cell<Option<(i32, EmbedGeneration)>>,
    /// Number of full embed traversals, exposed only to performance fixtures.
    #[cfg(feature = "test-utils")]
    pub(super) code_block_width_traversal_count: Cell<u64>,
    /// Launchable link spans rendered directly into the text buffer.
    ///
    /// The preview rerenders whole documents, so this list is rebuilt from
    /// scratch on every render and then reused by the click and hover
    /// controllers for hit-testing.
    pub(super) text_link_targets: RefCell<Vec<super::seams::RenderedTextLink>>,
    /// Optional override used by tests to capture preview link activations
    /// without spawning an external desktop handler.
    pub(super) link_activation_callback: RefCell<Option<LinkActivationCallback>>,
    /// Current document-surface opacity used for the preview background.
    pub background_opacity: Cell<f64>,
    /// Generation counter for deferred code-block width refreshes.
    ///
    /// Preview shell transitions can emit several allocation and margin changes
    /// before the embedded `GtkTextView` column settles. The counter lets later
    /// refresh requests supersede older idle/timer callbacks so only the most
    /// recent layout pass can resize anchored code blocks.
    pub(super) code_block_refresh_generation: Cell<CodeBlockRefreshGeneration>,
    /// Pending idle refresh for child-anchor code-block widths.
    ///
    /// Layout and render paths can request many refreshes in one GTK turn. Keep
    /// only the newest idle callback so resize storms do not queue no-op work.
    pub(super) code_block_idle_source_id: RefCell<Option<glib::SourceId>>,
    /// Pending timed refresh for child-anchor code-block widths.
    ///
    /// The timed pass catches preview shell allocations that settle just after
    /// idle callbacks while still letting newer refresh requests replace it.
    pub(super) code_block_timeout_source_id: RefCell<Option<glib::SourceId>>,
    /// The text view's horizontal adjustment observer that repairs
    /// code-block widths after the text view's width changes; dropping it
    /// disconnects it.
    pub(super) code_block_width_observer: RefCell<Option<gtk_lush_viewport::ViewportObserver>>,
    /// One-shot callbacks waiting for the current deferred repair sequence.
    ///
    /// Window-level visual readiness uses this to avoid reporting ready before
    /// the preview's own idle and timed child-anchor repairs have run.
    pub(super) code_block_refresh_completion_callbacks:
        RefCell<Vec<CodeBlockRefreshCompletionCallback>>,
    /// Global theme/settings handlers that must disconnect when the preview is disposed.
    pub global_signals: SignalBag,
    /// Active app-local source-buffer capture for note preview rendering.
    pub source_snapshot: RefCell<Option<BufferSnapshotHandle>>,
    /// GTK-free generation, terminal, and readiness ownership state.
    pub(super) render_session:
        RefCell<lushtext_services::notes::markdown_render::MarkdownRenderSession>,
    /// Whether the one document-sized Markdown planner is still draining.
    pub(super) planning_worker_running: Cell<bool>,
    /// Cancellation token for the active planner.
    pub(super) planning_cancel_token:
        RefCell<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    /// Replaceable latest request retained while the active planner disconnects.
    pub(super) queued_plan: RefCell<Option<super::seams::GuardedPendingMarkdownPlan>>,
    /// Latest render or projection work deferred by detached-generation pressure.
    pub(super) deferred_work: RefCell<Option<super::seams::PendingMarkdownWork>>,
    /// Atomic mirror used by workers to destroy stale plans before GTK delivery.
    pub(super) render_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Count of plan, source, or batch-tail payloads destroyed away from GTK.
    pub(super) plain_retirement_jobs: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Plain payloads queued or active on the bounded worker facility.
    pub(super) plain_retirement_pending: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Largest queued/active plain-payload retirement count observed.
    pub(super) plain_retirement_pending_high_water: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Number of bounded projection turns used by the current generation.
    #[cfg(feature = "test-utils")]
    pub(super) projection_dispatch_count: Cell<u64>,
    /// Largest event count observed in one projection turn.
    #[cfg(feature = "test-utils")]
    pub(super) projection_high_water_events: Cell<usize>,
    /// Compact image descriptors waiting behind at most one active decoder.
    pub(super) image_queue: RefCell<VecDeque<super::PendingImageWork>>,
    /// Scalar identity and charge for the single active image worker.
    pub(super) active_image: RefCell<Option<super::ActiveImageWork>>,
    /// Current-generation image work that still blocks render readiness.
    pub(super) current_image_work_count: Cell<usize>,
    /// GTK-free count/byte ownership and high-water image admission state.
    pub(super) image_admission:
        RefCell<lushtext_services::notes::markdown_render::MarkdownImageAdmission>,
    /// Detached buffers/widgets/links awaiting bounded main-loop retirement.
    pub(super) retirement: RefCell<Option<super::seams::MarkdownRetirementSession>>,
    /// Whether the one retirement idle source is armed.
    pub(super) retirement_armed: Cell<bool>,
    /// Largest ordinary detached-generation backlog observed.
    #[cfg(feature = "test-utils")]
    pub(super) retirement_generations_high_water: Cell<usize>,
    /// Whether latest-work backpressure was observed during the current widget lifetime.
    #[cfg(feature = "test-utils")]
    pub(super) deferred_work_high_water: Cell<usize>,
    /// Largest detached characters and object references retired in one turn.
    #[cfg(feature = "test-utils")]
    pub(super) retirement_chars_high_water: Cell<usize>,
    #[cfg(feature = "test-utils")]
    pub(super) retirement_items_high_water: Cell<usize>,
}

#[glib::object_subclass]
// Register the preview as a GtkBox subclass so anchored child widgets and
// deferred layout repairs stay owned by one template-backed widget instance.
impl ObjectSubclass for LushtextMarkdownPreview {
    const NAME: &str = "LushtextMarkdownPreview";
    type Type = super::LushtextMarkdownPreview;
    type ParentType = gtk4::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextMarkdownPreview {
    fn constructed(&self) {
        self.parent_constructed();

        // Create the initial tag table based on current theme.
        let is_dark = libadwaita::StyleManager::default().is_dark();
        create_or_update_tags(&self.text_view.buffer(), is_dark);
        self.apply_accessibility_metadata();
        self.obj().setup_link_interaction();
        self.background_opacity.set(
            crate::ui::settings_writes::shared()
                .double(crate::config::keys::TAB_CONTENT_OPACITY)
                .clamp(0.0, 1.0),
        );

        crate::ui::settings_writes::connect_changed_for(
            &*self.obj(),
            crate::config::keys::TAB_CONTENT_OPACITY,
            |preview, s| {
                preview.imp().background_opacity.set(
                    s.double(crate::config::keys::TAB_CONTENT_OPACITY)
                        .clamp(0.0, 1.0),
                );
            },
        );

        // Re-create tags when the dark/light mode changes so colors stay correct.
        let obj_weak = self.obj().downgrade();
        let style_manager = libadwaita::StyleManager::default();
        let handler_id = style_manager.connect_dark_notify(move |sm| {
            if let Some(obj) = obj_weak.upgrade() {
                create_or_update_tags(&obj.imp().text_view.buffer(), sm.is_dark());
            }
        });
        self.global_signals.track(&style_manager, handler_id);

        // Code blocks are embedded as child-anchor widgets, so they need to
        // follow the final text-view column width rather than the outer box.
        // The width itself is observed through the text view's horizontal
        // adjustment, whose page size GTK sets from the text view's allocated
        // width: this `GtkBox` subclass is allocated by its `GtkBoxLayout`,
        // which calls no `size_allocate` override, and `GtkWidget` has no
        // `width` property to notify (`widget-wiring.md`, "Size-Dependent
        // Constraints"). The adjustment changes inside the text view's
        // allocation, so the repair runs from the deferred passes only.
        let obj_weak = self.obj().downgrade();
        let width_observer = gtk_lush_viewport::ViewportObserver::for_scrollable(
            &*self.text_view,
            move |change| {
                if change.axis() == gtk_lush_viewport::ViewportAxis::Horizontal
                    && let Some(obj) = obj_weak.upgrade()
                {
                    obj.queue_code_block_width_refresh_after_allocation();
                }
            },
            |_| {},
        );
        debug_assert!(
            width_observer.is_some(),
            "the template's scrolled window gives the preview text view its adjustments"
        );
        self.code_block_width_observer.replace(width_observer);
        for property_name in ["left-margin", "right-margin"] {
            let obj_weak = self.obj().downgrade();
            self.text_view
                .connect_notify_local(Some(property_name), move |_, _| {
                    if let Some(obj) = obj_weak.upgrade() {
                        obj.queue_code_block_width_refresh();
                    }
                });
        }

        let obj_weak = self.obj().downgrade();
        self.obj().connect_map(move |_| {
            if let Some(obj) = obj_weak.upgrade() {
                obj.queue_code_block_width_refresh();
            }
        });
    }

    fn dispose(&self) {
        if let Some(cancel) = self.planning_cancel_token.take() {
            cancel.store(true, std::sync::atomic::Ordering::Release);
        }
        self.obj().cancel_queued_image_work();
        if let Some(retired) = self.queued_plan.take() {
            self.obj().retire_guarded_markdown(retired);
        }
        if let Some(retired) = self.deferred_work.take() {
            self.obj().retire_markdown_work(retired);
        }
        let generation = self.render_session.borrow_mut().cancel().get();
        self.render_generation
            .store(generation, std::sync::atomic::Ordering::Release);
        if let Some(snapshot) = self.source_snapshot.take() {
            snapshot.dispose();
        }
        self.global_signals.clear();
        self.code_block_width_observer.take();
        if let Some(source_id) = self.code_block_idle_source_id.take() {
            source_id.remove();
        }
        if let Some(source_id) = self.code_block_timeout_source_id.take() {
            source_id.remove();
        }
        self.code_block_refresh_completion_callbacks
            .borrow_mut()
            .clear();
    }
}

impl WidgetImpl for LushtextMarkdownPreview {}
impl BoxImpl for LushtextMarkdownPreview {}

impl LushtextMarkdownPreview {
    /// Mark the preview as a read-only document surface before any content is rendered.
    fn apply_accessibility_metadata(&self) {
        accessibility::set_role(&*self.obj(), gtk4::AccessibleRole::Document);
        accessibility::set_labelled_description(
            &*self.obj(),
            "Markdown preview",
            "Read-only rendered Markdown document preview",
        );
        accessibility::set_role(&*self.scrolled_window, gtk4::AccessibleRole::Region);
        accessibility::set_labelled_description(
            &*self.scrolled_window,
            "Markdown preview scroll area",
            "Scrollable read-only rendered Markdown content",
        );
        // GtkTextView already owns GTK_ACCESSIBLE_ROLE_TEXT_BOX; keep the
        // projection to state and names so GTK does not emit duplicate-role criticals.
        accessibility::set_labelled_description(
            &*self.text_view,
            "Rendered Markdown content",
            "Read-only rendered Markdown text",
        );
        accessibility::set_read_only(&*self.text_view, true);
        accessibility::set_multi_line(&*self.text_view, true);
        accessibility::set_role(&*self.placeholder, gtk4::AccessibleRole::Status);
        accessibility::set_labelled_description(
            &*self.placeholder,
            "Markdown preview placeholder",
            "Open a Markdown file to see a rendered preview",
        );
        accessibility::set_hidden(&*self.placeholder, true);
    }
}
