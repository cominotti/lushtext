// SPDX-License-Identifier: GPL-3.0-or-later

//! Markdown-preview fenced code-block rendering.
//!
//! Buffers fenced code-block events, resolves a GtkSourceView language, builds
//! the anchored embedded source widget, applies its scoped background, and runs
//! the documented idle-plus-timeout width-repair mechanism. The exact
//! SourceId-pair cancellation/completion timing is preserved from `mod.rs`;
//! only the code location moved.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use pulldown_cmark::{CodeBlockKind, Event};
use sourceview5::prelude::*;
use std::path::Path;

use crate::ui::accessibility;

use super::{
    CODE_BLOCK_BACKGROUND_CSS_PRIORITY, CODE_BLOCK_HORIZONTAL_PADDING, CODE_BLOCK_VERTICAL_PADDING,
    EmbeddedBlockLayout, LushtextMarkdownPreview, MAX_PREVIEW_CODE_BLOCK_BYTES,
    build_preview_limit_fallback_widget,
};

/// Visual inputs shared by all code blocks in one render pass.
#[derive(Debug, Clone)]
pub(super) struct CodeBlockTheme {
    /// GtkSourceView scheme used for syntax token colors.
    style_scheme: Option<sourceview5::StyleScheme>,
    /// CSS background applied to both the outer block and inner text area.
    background_css: String,
}

impl CodeBlockTheme {
    /// Resolve the current editor palette once so many code blocks stay cheap.
    pub(super) fn from_settings(settings: &gtk4::gio::Settings) -> Self {
        let style_scheme = crate::ui::theme::active_sourceview_scheme(settings);
        let palette = crate::ui::theme::resolve_tab_content_palette(settings);
        Self {
            style_scheme,
            background_css: crate::ui::theme::css_rgba_with_alpha(&palette.text_bg, 1.0),
        }
    }
}

/// Buffered Markdown code block collected before we create an anchored source view.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct BufferedCodeBlock {
    /// Original pulldown-cmark block kind, including fenced info string.
    kind: CodeBlockKind<'static>,
    /// Literal code text emitted between code-block start and end tags.
    text: String,
    /// Total source bytes observed, even after preview storage stops.
    source_bytes: usize,
}

impl BufferedCodeBlock {
    /// Start buffering one code block from pulldown-cmark's borrowed event data.
    fn new(kind: CodeBlockKind<'_>) -> Self {
        Self {
            kind: kind.into_static(),
            text: String::new(),
            source_bytes: 0,
        }
    }

    /// Fold one event inside the code block into literal text.
    fn push_event(&mut self, event: Event<'_>) {
        match event {
            Event::Text(text) | Event::Code(text) => self.push_literal(&text),
            Event::SoftBreak | Event::HardBreak => self.push_literal("\n"),
            _ => {}
        }
    }

    /// Add one literal chunk while keeping the preview-owned buffer bounded.
    fn push_literal(&mut self, text: &str) {
        self.source_bytes = self.source_bytes.saturating_add(text.len());
        let remaining = MAX_PREVIEW_CODE_BLOCK_BYTES.saturating_sub(self.text.len());
        if remaining == 0 {
            return;
        }

        let end = text
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= remaining)
            .last()
            .unwrap_or(0);
        let end = if text.len() <= remaining {
            text.len()
        } else {
            end
        };
        self.text.push_str(&text[..end]);
    }

    /// Charge source bytes the planner counted for this block but did not retain.
    ///
    /// A carried-embed crossing stops retaining code text and forwards its true
    /// remaining byte count instead. Charging it here keeps
    /// `exceeds_preview_widget_budget()` and the fallback's reported size
    /// evaluating the block's real total, exactly as when the whole block
    /// arrives in one projection turn.
    fn charge_unretained_source_bytes(&mut self, bytes: usize) {
        self.source_bytes = self.source_bytes.saturating_add(bytes);
    }

    /// Return the first info-string word from a fenced block, if present.
    fn language_hint(&self) -> Option<&str> {
        match &self.kind {
            CodeBlockKind::Fenced(info) => info
                .split_whitespace()
                .next()
                .filter(|hint| !hint.is_empty()),
            CodeBlockKind::Indented => None,
        }
    }

    /// Whether this block is too large for a syntax-highlighted GTK subtree.
    fn exceeds_preview_widget_budget(&self) -> bool {
        self.source_byte_len() > MAX_PREVIEW_CODE_BLOCK_BYTES
    }

    /// Total source bytes represented by this buffered block.
    fn source_byte_len(&self) -> usize {
        self.source_bytes.max(self.text.len())
    }
}

/// Code block being collected together with the layout context active at its start.
pub(super) struct ActiveCodeBlock {
    // (fields below are pub(super) so the render loop in `mod.rs` can build and
    // finalize an active code block.)
    /// Literal code block data collected from pulldown-cmark events.
    pub(super) code_block: BufferedCodeBlock,
    /// Text-column context captured before child-anchor insertion.
    pub(super) layout: EmbeddedBlockLayout,
}

impl ActiveCodeBlock {
    /// Start buffering one code block and remember where it should be laid out.
    pub(super) fn new(kind: CodeBlockKind<'_>, layout: EmbeddedBlockLayout) -> Self {
        Self {
            code_block: BufferedCodeBlock::new(kind),
            layout,
        }
    }

    /// Fold one parser event into the underlying literal code buffer.
    pub(super) fn push_event(&mut self, event: Event<'_>) {
        self.code_block.push_event(event);
    }

    /// Charge counted-but-unretained source bytes onto this in-flight block.
    pub(super) fn charge_unretained_source_bytes(&mut self, bytes: usize) {
        self.code_block.charge_unretained_source_bytes(bytes);
    }

    /// Append one literal marker line standing in for an unprojected text run.
    ///
    /// The marker is preview-owned text rather than source content, so it is
    /// appended through the same bounded literal path and is not charged as
    /// observed source bytes.
    pub(super) fn push_omission_line(&mut self, text: &str) {
        let line = if self.code_block.text.is_empty() || self.code_block.text.ends_with('\n') {
            format!("{text}\n")
        } else {
            format!("\n{text}\n")
        };
        let charged_before = self.code_block.source_bytes;
        self.code_block.push_literal(&line);
        self.code_block.source_bytes = charged_before;
    }
}

/// Return the current Markdown text column width inside the preview text view.
fn preview_text_column_width(text_view: &gtk4::TextView) -> Option<i32> {
    let width = text_view.width();
    if width <= 0 {
        return None;
    }

    let column_width = width.saturating_sub(text_view.left_margin() + text_view.right_margin());
    (column_width > 0).then_some(column_width)
}

/// Resolve one code-block language hint using IDs, common aliases, and filename guessing.
fn resolve_code_block_language_hint(raw_hint: &str) -> Option<sourceview5::Language> {
    let hint = normalize_code_block_language_hint(raw_hint)?;
    let manager = sourceview5::LanguageManager::default();
    if let Some(language) = manager.language(&hint) {
        return Some(language);
    }

    let alias = code_block_language_alias(&hint);
    if alias != hint
        && let Some(language) = manager.language(alias)
    {
        return Some(language);
    }

    let filename = format!("sample.{alias}");
    manager
        .guess_language(Some(Path::new(&filename)), None)
        .or_else(|| manager.guess_language(Some(Path::new(&format!("sample.{hint}"))), None))
}

/// Normalize Markdown renderer language classes and casing into source IDs.
fn normalize_code_block_language_hint(raw_hint: &str) -> Option<String> {
    let hint = raw_hint.trim().trim_start_matches("language-").trim();
    if hint.is_empty() {
        None
    } else {
        Some(hint.to_ascii_lowercase())
    }
}

/// Map common Markdown fence aliases to GtkSourceView language IDs.
fn code_block_language_alias(hint: &str) -> &str {
    match hint {
        "bash" | "zsh" | "shell" => "sh",
        "cjs" | "js" | "mjs" => "javascript",
        "py" => "python3",
        "rs" => "rust",
        "ts" => "typescript",
        other => other,
    }
}

/// Build one native source-view widget for a buffered Markdown code block.
fn build_code_block_widget(code_block: &BufferedCodeBlock, theme: &CodeBlockTheme) -> gtk4::Widget {
    if code_block.exceeds_preview_widget_budget() {
        return build_preview_limit_fallback_widget(
            "Code block not rendered",
            &format!(
                "This code block is {} bytes; the preview renders highlighted code blocks up to {} bytes.",
                code_block.source_byte_len(),
                MAX_PREVIEW_CODE_BLOCK_BYTES
            ),
            "markdown-code-block-fallback",
        );
    }

    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    container.set_margin_top(8);
    container.set_margin_bottom(8);
    container.set_hexpand(true);
    container.set_halign(gtk4::Align::Fill);
    container.add_css_class("markdown-code-block");
    let language_hint = code_block.language_hint().unwrap_or("plain text");
    let code_block_label = format!("Markdown {language_hint} code block");
    accessibility::set_role(&container, gtk4::AccessibleRole::Group);
    accessibility::set_labelled_description(
        &container,
        &code_block_label,
        "Read-only code block embedded in the rendered Markdown preview",
    );

    let source_buffer = sourceview5::Buffer::new(None);
    let language = code_block
        .language_hint()
        .and_then(resolve_code_block_language_hint);
    source_buffer.set_language(language.as_ref());
    source_buffer.set_highlight_syntax(language.is_some());
    source_buffer.set_style_scheme(theme.style_scheme.as_ref());
    source_buffer.set_text(&code_block.text);

    let source_view = sourceview5::View::with_buffer(&source_buffer);
    source_view.set_editable(false);
    source_view.set_cursor_visible(false);
    source_view.set_show_line_numbers(false);
    source_view.set_highlight_current_line(false);
    source_view.set_monospace(true);
    source_view.set_wrap_mode(gtk4::WrapMode::None);
    source_view.set_left_margin(0);
    source_view.set_right_margin(0);
    source_view.set_top_margin(0);
    source_view.set_bottom_margin(0);
    source_view.set_hexpand(true);
    source_view.set_halign(gtk4::Align::Fill);
    source_view.add_css_class("monospace");
    source_view.add_css_class("markdown-code-block-view");
    // GtkSourceView already exposes a text-box role; assigning it again is a
    // GTK critical, so this projection only supplies the code-block name/state.
    accessibility::set_labelled_description(
        &source_view,
        &code_block_label,
        "Read-only source text for this Markdown code block",
    );
    accessibility::set_read_only(&source_view, true);
    accessibility::set_multi_line(&source_view, true);
    apply_code_block_background_css(
        container.upcast_ref::<gtk4::Widget>(),
        &source_view,
        &theme.background_css,
    );

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_child(Some(&source_view));
    scroller.set_policy(gtk4::PolicyType::Automatic, gtk4::PolicyType::Never);
    scroller.set_propagate_natural_height(true);
    scroller.set_propagate_natural_width(false);
    scroller.set_hexpand(true);
    scroller.set_halign(gtk4::Align::Fill);
    scroller.set_margin_top(CODE_BLOCK_VERTICAL_PADDING);
    scroller.set_margin_bottom(CODE_BLOCK_VERTICAL_PADDING);
    scroller.set_margin_start(CODE_BLOCK_HORIZONTAL_PADDING);
    scroller.set_margin_end(CODE_BLOCK_HORIZONTAL_PADDING);
    scroller.add_css_class("markdown-code-block-scroller");

    container.append(&scroller);
    container.upcast()
}

/// Apply one resolved background to both layers of the embedded code surface.
#[expect(
    deprecated,
    reason = "GTK4's non-deprecated provider API is display-wide, but this preview needs a widget-scoped palette override."
)]
fn apply_code_block_background_css(
    container: &gtk4::Widget,
    source_view: &sourceview5::View,
    background: &str,
) {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&code_block_background_css(background));
    container
        .style_context()
        .add_provider(&provider, CODE_BLOCK_BACKGROUND_CSS_PRIORITY);
    source_view
        .style_context()
        .add_provider(&provider, CODE_BLOCK_BACKGROUND_CSS_PRIORITY);
}

/// Build the CSS that keeps the block frame and source text on one surface.
fn code_block_background_css(background: &str) -> String {
    format!(
        r#"
.markdown-code-block {{
  background-color: {background};
}}

.markdown-code-block-view,
.markdown-code-block-view text {{
  background-color: {background};
}}
"#
    )
}

impl LushtextMarkdownPreview {
    /// Insert one buffered Markdown code block into the preview flow.
    pub(super) fn insert_code_block_widget(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        code_block: &BufferedCodeBlock,
        theme: &CodeBlockTheme,
        layout: EmbeddedBlockLayout,
    ) {
        let widget = build_code_block_widget(code_block, theme);
        widget.set_margin_start(layout.margin_start);
        widget.set_margin_end(layout.margin_end);
        self.insert_embedded_widget(buffer, iter, widget.upcast_ref::<gtk4::Widget>(), layout);
    }

    /// Refresh anchored code blocks after GTK has allocated the preview text view.
    ///
    /// `GtkTextView` child anchors do not expand anchored widgets to the text
    /// column automatically, so code-block containers need an explicit width
    /// request based on the current visible text column.
    fn refresh_code_block_widths(&self) {
        let Some(column_width) = preview_text_column_width(&self.imp().text_view.get()) else {
            return;
        };
        let embed_generation = self.imp().rendered_embed_generation.get();
        if self.imp().last_code_block_layout.get() == Some((column_width, embed_generation)) {
            return;
        }

        #[cfg(feature = "test-utils")]
        self.imp().code_block_width_traversal_count.set(
            self.imp()
                .code_block_width_traversal_count
                .get()
                .wrapping_add(1),
        );

        let mut changed = false;
        for embed in self.imp().rendered_embeds.borrow().iter() {
            if embed.widget.has_css_class("markdown-code-block") {
                let width = embed.layout.code_block_width(column_width);
                if embed.widget.width_request() != width {
                    embed.widget.set_width_request(width);
                    embed.widget.queue_resize();
                    changed = true;
                }
            }
        }

        if changed {
            self.imp().text_view.queue_resize();
            self.queue_resize();
        }
        self.imp()
            .last_code_block_layout
            .set(Some((column_width, embed_generation)));
    }

    /// Return the number of full embed traversals for performance assertions.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn code_block_width_traversal_count_for_test(&self) -> u64 {
        self.imp().code_block_width_traversal_count.get()
    }

    /// Reset only the performance counter without changing layout cache state.
    #[cfg(feature = "test-utils")]
    pub fn reset_code_block_width_traversal_count_for_test(&self) {
        self.imp().code_block_width_traversal_count.set(0);
    }

    /// Queue the production deferred repair sequence for widget assertions.
    #[cfg(feature = "test-utils")]
    pub fn queue_code_block_width_refresh_for_test<F: Fn() + 'static>(&self, callback: F) {
        self.queue_code_block_width_refresh_after(callback);
    }

    /// Refresh code-block widths across the current GTK layout turn.
    pub(super) fn queue_code_block_width_refresh(&self) {
        let generation = self
            .imp()
            .code_block_refresh_generation
            .get()
            .wrapping_add(1);
        self.imp().code_block_refresh_generation.set(generation);
        self.refresh_code_block_widths();
        self.replace_deferred_code_block_width_refresh(generation);
    }

    /// Refresh code-block widths and run `callback` after the final deferred pass.
    pub(crate) fn queue_code_block_width_refresh_after<F: Fn() + 'static>(&self, callback: F) {
        self.imp()
            .code_block_refresh_completion_callbacks
            .borrow_mut()
            .push(Box::new(callback));
        self.queue_code_block_width_refresh();
    }

    /// Replace any queued deferred refresh with the latest layout generation.
    fn replace_deferred_code_block_width_refresh(&self, generation: u32) {
        if let Some(source_id) = self.imp().code_block_idle_source_id.take() {
            source_id.remove();
        }
        if let Some(source_id) = self.imp().code_block_timeout_source_id.take() {
            source_id.remove();
        }

        let idle_preview_weak = self.downgrade();
        let idle_source_id = glib::idle_add_local_once(move || {
            let Some(preview) = idle_preview_weak.upgrade() else {
                return;
            };

            let _ = preview.imp().code_block_idle_source_id.take();
            if preview.imp().code_block_refresh_generation.get() == generation {
                preview.refresh_code_block_widths();
            }
        });
        let _ = self
            .imp()
            .code_block_idle_source_id
            .replace(Some(idle_source_id));

        // `GtkPaned` and `GtkTextView` can settle their final child-anchor
        // column one frame after the idle pass in Fedora's headless CI stack.
        // A short replaceable timer keeps production preview geometry honest
        // without accumulating stale callbacks during active resizing.
        let timed_preview_weak = self.downgrade();
        let timeout_source_id =
            glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
                let Some(preview) = timed_preview_weak.upgrade() else {
                    return;
                };

                let _ = preview.imp().code_block_timeout_source_id.take();
                if preview.imp().code_block_refresh_generation.get() == generation {
                    preview.refresh_code_block_widths();
                    let callbacks = preview.imp().code_block_refresh_completion_callbacks.take();
                    for callback in callbacks {
                        callback();
                    }
                }
            });
        let _ = self
            .imp()
            .code_block_timeout_source_id
            .replace(Some(timeout_source_id));
    }

    /// Recheck embedded code-block widths after an outer preview-shell transition.
    ///
    /// The main window can reveal the preview from a hidden Adwaita slot or move
    /// it into a different layout after Markdown has already rendered. Calling
    /// this at shell boundaries keeps child-anchor code blocks tied to the final
    /// text column rather than to an intermediate allocation.
    pub(crate) fn refresh_embedded_code_block_layouts(&self) {
        self.queue_code_block_width_refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_block_preview_budget_flags_large_blocks() {
        let mut code_block = BufferedCodeBlock::new(CodeBlockKind::Indented);
        code_block.text = "x".repeat(MAX_PREVIEW_CODE_BLOCK_BYTES);
        assert!(!code_block.exceeds_preview_widget_budget());

        code_block.text.push('x');
        assert!(code_block.exceeds_preview_widget_budget());
    }

    #[test]
    fn charged_unretained_bytes_still_trigger_the_preview_budget() {
        // The planner stops retaining code text at its carried ceiling and
        // forwards the remainder as a count. Charging it must reproduce the
        // decision a single-turn render would have made.
        let mut code_block = BufferedCodeBlock::new(CodeBlockKind::Indented);
        code_block.push_literal("kept\n");
        assert!(!code_block.exceeds_preview_widget_budget());

        code_block.charge_unretained_source_bytes(MAX_PREVIEW_CODE_BLOCK_BYTES);
        assert!(code_block.exceeds_preview_widget_budget());
        assert_eq!(
            code_block.source_byte_len(),
            MAX_PREVIEW_CODE_BLOCK_BYTES + "kept\n".len()
        );
    }

    #[test]
    fn an_omission_line_is_not_charged_as_observed_source_bytes() {
        let mut active =
            ActiveCodeBlock::new(CodeBlockKind::Indented, EmbeddedBlockLayout::default());
        active.push_event(Event::Text("kept\n".into()));
        let charged = active.code_block.source_bytes;

        active.push_omission_line("[omitted run]");

        assert_eq!(
            active.code_block.source_bytes, charged,
            "a preview-owned marker is not source content"
        );
        assert!(active.code_block.text.contains("[omitted run]"));
        assert!(active.code_block.text.starts_with("kept\n"));
    }

    #[test]
    fn test_code_block_buffer_stops_storing_after_preview_budget() {
        let mut code_block = BufferedCodeBlock::new(CodeBlockKind::Indented);
        code_block.push_event(Event::Text(
            "x".repeat(MAX_PREVIEW_CODE_BLOCK_BYTES + 512).into(),
        ));

        assert_eq!(code_block.text.len(), MAX_PREVIEW_CODE_BLOCK_BYTES);
        assert_eq!(
            code_block.source_byte_len(),
            MAX_PREVIEW_CODE_BLOCK_BYTES + 512
        );
        assert!(code_block.exceeds_preview_widget_budget());
    }

    #[test]
    fn test_code_block_background_css_uses_one_surface_color() {
        let css = code_block_background_css("rgb(1, 2, 3)");

        assert_eq!(
            css.matches("background-color: rgb(1, 2, 3);").count(),
            2,
            "Expected the generated CSS to apply the same background to the outer block and inner source text area"
        );
        assert!(css.contains(".markdown-code-block {"));
        assert!(css.contains(".markdown-code-block-view text"));
    }
}
