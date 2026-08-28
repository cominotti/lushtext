// SPDX-License-Identifier: GPL-3.0-or-later

//! Markdown-preview local-image rendering.
//!
//! Owns image target resolution, bounded source reads, off-GTK decode, decoded
//! pixel disposal, the one-at-a-time image work queue, and anchored image/
//! fallback widgets. Admission budgets, generation rejection, worker handoff,
//! and disposal behavior are unchanged from when this lived in `mod.rs`; only
//! the code location moved. Sizing constants and test-only accounting statics
//! stay in `mod.rs` and are imported here.

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{self, gdk};
use pulldown_cmark::Event;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::services::filesystem::{PathStatus, metadata as fs_metadata, read as fs_read};
use crate::ui::accessibility;
use gtk_lush_tasks::spawn_blocking_then_weak;

#[cfg(feature = "test-utils")]
use super::seams::lock_markdown_capacity;
use super::seams::{EmbeddedBlockLayout, MarkdownPreviewRenderContext};
#[cfg(feature = "test-utils")]
use super::test_policy::{
    IMAGE_CANCELLED_WORK, IMAGE_CANDIDATE_INSPECTIONS, IMAGE_DECODED_RESULTS, IMAGE_PIXEL_DROPS,
    IMAGE_PIXEL_DROPS_ON_GTK, IMAGE_POST_DECODE_DELAY_MS, IMAGE_TEST_GTK_THREAD,
    IMAGE_WORK_DELAY_MS,
};
use super::{
    LushtextMarkdownPreview, MAX_PREVIEW_IMAGE_SOURCE_BYTES, MAX_PREVIEW_IMAGE_SOURCE_PIXELS,
    MAX_PREVIEW_IMAGE_WIDTH, MAX_PREVIEW_IMAGE_WORK_BYTES, MAX_PREVIEW_IMAGE_WORK_ITEMS,
    MIN_PREVIEW_IMAGE_SIZE, PREVIEW_IMAGE_WORK_CHARGE_BYTES,
};

/// Result of resolving one Markdown image destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResolvedImageTarget {
    /// Admitted worker work that has not expanded relative candidates on GTK.
    Work(ImageWorkTarget),
    /// A fallback block that should appear inline instead of silently dropping the image.
    Fallback { title: &'static str, body: String },
}

/// Compact image target retained only after image count/byte admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ImageWorkTarget {
    Direct(PathBuf),
    Relative {
        path: PathBuf,
        context: MarkdownPreviewRenderContext,
    },
}

/// Decoded image pixels that can safely cross back from a worker thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DecodedImage {
    /// Image width after preview-size bounding.
    width: i32,
    /// Image height after preview-size bounding.
    height: i32,
    /// Number of bytes between rows.
    stride: usize,
    /// Whether the pixels include an alpha channel.
    has_alpha: bool,
    /// Owned RGB/RGBA bytes copied out of the background pixbuf decode.
    pixels: DecodedPixels,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DecodedPixels(Vec<u8>);

impl AsRef<[u8]> for DecodedPixels {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
impl DecodedPixels {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Drop for DecodedPixels {
    fn drop(&mut self) {
        #[cfg(feature = "test-utils")]
        {
            IMAGE_PIXEL_DROPS.fetch_add(1, Ordering::AcqRel);
            let gtk_thread = lock_markdown_capacity(&IMAGE_TEST_GTK_THREAD);
            if gtk_thread
                .as_ref()
                .is_some_and(|thread| *thread == std::thread::current().id())
            {
                IMAGE_PIXEL_DROPS_ON_GTK.fetch_add(1, Ordering::AcqRel);
            }
        }
    }
}

/// Result of checking workspace-relative image candidates on a background thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OrderedImageCandidateResult {
    /// The first decodable candidate in workspace-folder order, already scaled.
    Loadable { path: PathBuf, image: DecodedImage },
    /// At least one candidate existed, but none could be decoded as an image.
    Unloadable { path: PathBuf, error: String },
    /// None of the candidate paths were present as decodable files.
    Missing { raw_target: String },
    /// The render generation changed before candidate resolution completed.
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct OrderedImageCandidateMetrics {
    inspected: usize,
    peak_retained_candidate_paths: usize,
}

/// Buffered Markdown image collected from pulldown-cmark's event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BufferedImage {
    /// Raw destination URL from the Markdown image syntax.
    destination: String,
    /// Human-readable alternative text built from the image's child events.
    alt_text: String,
}

/// Compact queued local-image work owned by one render generation.
pub(super) struct PendingImageWork {
    generation: u64,
    raw_target: String,
    target: ImageWorkTarget,
    container: glib::WeakRef<gtk4::Box>,
    charge_bytes: u64,
}

/// Scalar completion identity retained while one image worker is active.
pub(super) struct ActiveImageWork {
    generation: u64,
    container: glib::WeakRef<gtk4::Box>,
    charge_bytes: u64,
    cancel: Arc<AtomicBool>,
}

pub(super) type GuardedOrderedImageCandidateResult =
    crate::ui::plain_disposal::DisposalOwned<OrderedImageCandidateResult>;

impl BufferedImage {
    /// Start buffering one Markdown image destination and its alternative text.
    pub(super) fn new(destination: &str) -> Self {
        Self {
            destination: destination.to_string(),
            alt_text: String::new(),
        }
    }

    /// Fold one event inside the image into plain alternative text.
    pub(super) fn push_event(&mut self, event: Event<'_>) {
        match event {
            Event::Text(text) | Event::Code(text) => self.alt_text.push_str(&text),
            Event::SoftBreak | Event::HardBreak => self.alt_text.push(' '),
            _ => {}
        }
    }
}

impl DecodedImage {
    /// Decode and scale one local image on a worker thread.
    fn from_path_cancellable(
        path: &Path,
        is_cancelled: &impl Fn() -> bool,
    ) -> Result<Self, ImageDecodeError> {
        if is_cancelled() {
            return Err(ImageDecodeError::Cancelled);
        }
        let bytes = read_preview_image_bytes_with_limit(
            path,
            MAX_PREVIEW_IMAGE_SOURCE_BYTES,
            || {},
            is_cancelled,
        )
        .map_err(|error| {
            if is_cancelled() {
                ImageDecodeError::Cancelled
            } else {
                ImageDecodeError::Failed(error)
            }
        })?;
        if is_cancelled() {
            return Err(ImageDecodeError::Cancelled);
        }
        let pixbuf = decode_preview_pixbuf_from_bytes(&bytes).map_err(ImageDecodeError::Failed)?;
        if is_cancelled() {
            return Err(ImageDecodeError::Cancelled);
        }
        let (display_width, display_height) = bounded_image_size(pixbuf.width(), pixbuf.height());
        let pixbuf = if display_width != pixbuf.width() || display_height != pixbuf.height() {
            pixbuf
                .scale_simple(
                    display_width,
                    display_height,
                    gtk4::gdk_pixbuf::InterpType::Bilinear,
                )
                .ok_or_else(|| {
                    ImageDecodeError::Failed("failed to scale image for preview".to_string())
                })?
        } else {
            pixbuf
        };
        let channels = pixbuf.n_channels();
        if channels != 3 && channels != 4 {
            return Err(ImageDecodeError::Failed(format!(
                "unsupported image channel count: {channels}"
            )));
        }
        let stride = usize::try_from(pixbuf.rowstride())
            .map_err(|_| ImageDecodeError::Failed("invalid image rowstride".to_string()))?;

        let image = Self {
            width: pixbuf.width(),
            height: pixbuf.height(),
            stride,
            has_alpha: channels == 4,
            pixels: DecodedPixels(pixbuf.read_pixel_bytes().as_ref().to_vec()),
        };
        #[cfg(feature = "test-utils")]
        {
            IMAGE_DECODED_RESULTS.fetch_add(1, Ordering::AcqRel);
            let delay_ms = IMAGE_POST_DECODE_DELAY_MS.load(Ordering::Acquire);
            if delay_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
        }
        if is_cancelled() {
            return Err(ImageDecodeError::Cancelled);
        }
        Ok(image)
    }
}

enum ImageDecodeError {
    Cancelled,
    Failed(String),
}

fn read_preview_image_bytes_with_limit<F, C>(
    path: &Path,
    byte_limit: u64,
    after_facts: F,
    is_cancelled: C,
) -> Result<Vec<u8>, String>
where
    F: FnOnce(),
    C: Fn() -> bool,
{
    if is_cancelled() {
        return Err("image preview read was cancelled".to_string());
    }
    let facts = fs_metadata::file_facts(path).map_err(|error| error.to_string())?;
    if facts.byte_size > byte_limit {
        return Err(format!(
            "image is too large for preview ({} bytes, limit {} bytes)",
            facts.byte_size, byte_limit
        ));
    }
    after_facts();
    let bytes = fs_read::bounded_bytes(path, byte_limit, facts.byte_size, &is_cancelled).map_err(
        |error| match error {
            fs_read::BoundedFileReadError::LimitExceeded { .. } => {
                format!("image grew beyond the {byte_limit}-byte preview limit")
            }
            fs_read::BoundedFileReadError::Cancelled => {
                "image preview read was cancelled".to_string()
            }
            fs_read::BoundedFileReadError::Io(source) => source.to_string(),
        },
    )?;
    if is_cancelled() {
        return Err("image preview read was cancelled".to_string());
    }
    let current = fs_metadata::file_facts(path).map_err(|error| error.to_string())?;
    if current.identity != facts.identity
        || current.byte_size != facts.byte_size
        || current.modified_at_nanos != facts.modified_at_nanos
    {
        return Err("image changed while it was being read for preview".to_string());
    }
    Ok(bytes)
}

/// Decode one preview image from already-boundary-read bytes.
fn decode_preview_pixbuf_from_bytes(bytes: &[u8]) -> Result<gtk4::gdk_pixbuf::Pixbuf, String> {
    let loader = gtk4::gdk_pixbuf::PixbufLoader::new();
    let source_too_large = std::rc::Rc::new(std::cell::Cell::new(false));
    let source_too_large_for_signal = source_too_large.clone();
    loader.connect_size_prepared(move |loader, width, height| {
        let source_pixels = i64::from(width).saturating_mul(i64::from(height));
        if source_pixels > MAX_PREVIEW_IMAGE_SOURCE_PIXELS {
            source_too_large_for_signal.set(true);
            // The loader still needs a legal target size before `close()`, but
            // the result will be rejected; 1x1 avoids allocating the source.
            loader.set_size(1, 1);
            return;
        }

        let (display_width, display_height) = bounded_image_size(width, height);
        loader.set_size(display_width, display_height);
    });
    loader.write(bytes).map_err(|error| error.to_string())?;
    loader.close().map_err(|error| error.to_string())?;

    if source_too_large.get() {
        return Err(format!(
            "image dimensions exceed preview limit ({MAX_PREVIEW_IMAGE_SOURCE_PIXELS} pixels)"
        ));
    }

    loader
        .pixbuf()
        .ok_or_else(|| "image loader did not produce a pixbuf".to_string())
}

/// Resolve one Markdown image destination into a local image or an explicit fallback.
fn resolve_image_target(
    raw_target: &str,
    context: &MarkdownPreviewRenderContext,
) -> ResolvedImageTarget {
    if raw_target.trim().is_empty() {
        return ResolvedImageTarget::Fallback {
            title: "Image path missing",
            body: "Markdown image syntax did not include a usable destination.".to_string(),
        };
    }

    if let Some(scheme) = glib::Uri::parse_scheme(raw_target) {
        if scheme.as_str() == "file" {
            let file = gio::File::for_uri(raw_target);
            return match file.path() {
                Some(path) => ResolvedImageTarget::Work(ImageWorkTarget::Direct(path)),
                _ => ResolvedImageTarget::Fallback {
                    title: "Image file not found",
                    body: raw_target.to_string(),
                },
            };
        }

        return ResolvedImageTarget::Fallback {
            title: "Remote images are not supported",
            body: raw_target.to_string(),
        };
    }

    let path = Path::new(raw_target);
    if path.is_absolute() {
        return ResolvedImageTarget::Work(ImageWorkTarget::Direct(path.to_path_buf()));
    }
    if context.document_path.is_some() || !context.workspace_folders.is_empty() {
        ResolvedImageTarget::Work(ImageWorkTarget::Relative {
            path: path.to_path_buf(),
            context: context.clone(),
        })
    } else {
        ResolvedImageTarget::Fallback {
            title: "Image file not found",
            body: raw_target.to_string(),
        }
    }
}

/// Build one GTK picture from bytes decoded on a worker thread.
fn build_decoded_image_widget(path: &Path, image: DecodedImage) -> gtk4::Widget {
    let format = if image.has_alpha {
        gdk::MemoryFormat::R8g8b8a8
    } else {
        gdk::MemoryFormat::R8g8b8
    };
    let bytes = glib::Bytes::from_owned(image.pixels);
    let texture = gdk::MemoryTexture::new(image.width, image.height, format, &bytes, image.stride);
    let picture = gtk4::Picture::for_paintable(&texture);
    picture.add_css_class("markdown-preview-image");
    accessibility::set_role(&picture, gtk4::AccessibleRole::Img);
    accessibility::set_labelled_description(
        &picture,
        "Markdown image",
        &format!("Rendered image {}", path.display()),
    );
    picture.upcast()
}

/// Remove every current child from a GTK box before replacing async image content.
fn clear_box_children(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn first_loadable_ordered_image_cancellable(
    raw_target: String,
    target: ImageWorkTarget,
    is_cancelled: impl Fn() -> bool,
) -> OrderedImageCandidateResult {
    let result =
        first_loadable_ordered_image_with_metrics_cancellable(raw_target, target, is_cancelled).0;
    #[cfg(feature = "test-utils")]
    if matches!(result, OrderedImageCandidateResult::Cancelled) {
        IMAGE_CANCELLED_WORK.fetch_add(1, Ordering::AcqRel);
    }
    result
}

#[cfg(test)]
fn first_loadable_ordered_image_with_metrics(
    raw_target: String,
    target: ImageWorkTarget,
) -> (OrderedImageCandidateResult, OrderedImageCandidateMetrics) {
    first_loadable_ordered_image_with_metrics_cancellable(raw_target, target, || false)
}

fn first_loadable_ordered_image_with_metrics_cancellable(
    raw_target: String,
    target: ImageWorkTarget,
    is_cancelled: impl Fn() -> bool,
) -> (OrderedImageCandidateResult, OrderedImageCandidateMetrics) {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = IMAGE_WORK_DELAY_MS.load(Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
    let mut first_unloadable = None;
    let mut metrics = OrderedImageCandidateMetrics::default();
    let mut inspect = |path: PathBuf| {
        if is_cancelled() {
            return Some(OrderedImageCandidateResult::Cancelled);
        }
        metrics.inspected = metrics.inspected.saturating_add(1);
        #[cfg(feature = "test-utils")]
        IMAGE_CANDIDATE_INSPECTIONS.fetch_add(1, Ordering::AcqRel);
        metrics.peak_retained_candidate_paths = metrics
            .peak_retained_candidate_paths
            .max(1 + usize::from(first_unloadable.is_some()));
        match fs_metadata::path_status(&path) {
            Ok(PathStatus::File) | Err(_) => {
                match DecodedImage::from_path_cancellable(&path, &is_cancelled) {
                    Ok(image) => Some(OrderedImageCandidateResult::Loadable { path, image }),
                    Err(ImageDecodeError::Cancelled) => {
                        Some(OrderedImageCandidateResult::Cancelled)
                    }
                    Err(ImageDecodeError::Failed(error)) if first_unloadable.is_none() => {
                        first_unloadable = Some((path, error));
                        None
                    }
                    Err(ImageDecodeError::Failed(_)) => None,
                }
            }
            Ok(PathStatus::Directory | PathStatus::Other) => {
                if first_unloadable.is_none() {
                    first_unloadable = Some((path, "not a regular image file".to_string()));
                }
                None
            }
            Ok(PathStatus::Missing) => None,
        }
    };

    match target {
        ImageWorkTarget::Direct(path) => {
            if let Some(result) = inspect(path) {
                return (result, metrics);
            }
        }
        ImageWorkTarget::Relative { path, context } => {
            if let Some(document_path) = context.document_path.as_ref()
                && let Some(parent) = document_path.parent()
                && let Some(result) = inspect(parent.join(&path))
            {
                return (result, metrics);
            }
            for folder in context.workspace_folders.iter() {
                if let Some(result) = inspect(folder.join(&path)) {
                    return (result, metrics);
                }
            }
        }
    }

    if is_cancelled() {
        return (OrderedImageCandidateResult::Cancelled, metrics);
    }
    let result = first_unloadable.map_or(
        OrderedImageCandidateResult::Missing { raw_target },
        |(path, error)| OrderedImageCandidateResult::Unloadable { path, error },
    );
    (result, metrics)
}

/// Build one fallback block for unsupported or unresolved Markdown images.
fn build_image_fallback_widget(title: &str, body: &str) -> gtk4::Widget {
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    container.set_margin_top(8);
    container.set_margin_bottom(8);
    container.set_margin_start(10);
    container.set_margin_end(10);
    container.set_halign(gtk4::Align::Start);
    container.set_width_request(240);
    container.add_css_class("card");
    container.add_css_class("markdown-preview-image-fallback");
    accessibility::set_role(&container, gtk4::AccessibleRole::Img);
    accessibility::set_labelled_description(&container, &format!("Markdown image: {title}"), body);

    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let title_label = gtk4::Label::new(Some(title));
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    title_label.add_css_class("heading");
    title_label.add_css_class("markdown-preview-image-fallback-title");

    let body_label = gtk4::Label::new(Some(body));
    body_label.set_xalign(0.0);
    body_label.set_wrap(true);
    body_label.set_selectable(false);
    body_label.add_css_class("dim-label");
    body_label.add_css_class("monospace");
    body_label.add_css_class("markdown-preview-image-fallback-body");

    content.append(&title_label);
    content.append(&body_label);
    container.append(&content);
    container.upcast()
}

/// Bound one decoded image to a readable preview size while preserving aspect ratio.
fn bounded_image_size(width: i32, height: i32) -> (i32, i32) {
    if width <= 0 || height <= 0 {
        return (MAX_PREVIEW_IMAGE_WIDTH.min(320), 180);
    }

    let max_dimension = width.max(height);
    if max_dimension < MIN_PREVIEW_IMAGE_SIZE {
        let scaled_width = i32::try_from(
            i64::from(width).saturating_mul(i64::from(MIN_PREVIEW_IMAGE_SIZE))
                / i64::from(max_dimension),
        )
        .unwrap_or(width);
        let scaled_height = i32::try_from(
            i64::from(height).saturating_mul(i64::from(MIN_PREVIEW_IMAGE_SIZE))
                / i64::from(max_dimension),
        )
        .unwrap_or(height);
        return (scaled_width.max(1), scaled_height.max(1));
    }

    if max_dimension <= MAX_PREVIEW_IMAGE_WIDTH {
        return (width, height);
    }

    let scaled_width = i32::try_from(
        i64::from(width).saturating_mul(i64::from(MAX_PREVIEW_IMAGE_WIDTH))
            / i64::from(max_dimension),
    )
    .unwrap_or(width);
    let scaled_height = i32::try_from(
        i64::from(height).saturating_mul(i64::from(MAX_PREVIEW_IMAGE_WIDTH))
            / i64::from(max_dimension),
    )
    .unwrap_or(height);
    (scaled_width.max(1), scaled_height.max(1))
}

impl LushtextMarkdownPreview {
    /// Delay image workers so stale-generation completion is deterministic.
    #[cfg(feature = "test-utils")]
    pub fn set_image_work_delay_for_test(delay_ms: u64) {
        IMAGE_WORK_DELAY_MS.store(delay_ms, Ordering::Release);
    }

    /// Delay after worker-side decode so superseded pixel retirement is observable.
    #[cfg(feature = "test-utils")]
    pub fn set_image_post_decode_delay_for_test(delay_ms: u64) {
        IMAGE_POST_DECODE_DELAY_MS.store(delay_ms, Ordering::Release);
    }

    /// Reset direct cancellation, candidate, and decoded-pixel disposal evidence.
    #[cfg(feature = "test-utils")]
    pub fn reset_image_work_observations_for_test() {
        IMAGE_CANDIDATE_INSPECTIONS.store(0, Ordering::Release);
        IMAGE_CANCELLED_WORK.store(0, Ordering::Release);
        IMAGE_DECODED_RESULTS.store(0, Ordering::Release);
        IMAGE_PIXEL_DROPS.store(0, Ordering::Release);
        IMAGE_PIXEL_DROPS_ON_GTK.store(0, Ordering::Release);
        *lock_markdown_capacity(&IMAGE_TEST_GTK_THREAD) = Some(std::thread::current().id());
    }

    /// Insert one buffered Markdown image into the preview flow.
    pub(super) fn insert_image_widget(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        image: &BufferedImage,
        context: &MarkdownPreviewRenderContext,
    ) {
        match resolve_image_target(&image.destination, context) {
            ResolvedImageTarget::Work(target) => {
                self.insert_async_image_placeholder(
                    buffer,
                    iter,
                    &image.destination,
                    target,
                    EmbeddedBlockLayout::default(),
                );
            }
            ResolvedImageTarget::Fallback { title, body } => {
                let widget = build_image_fallback_widget(title, &body);
                self.insert_embedded_widget(
                    buffer,
                    iter,
                    widget.upcast_ref::<gtk4::Widget>(),
                    EmbeddedBlockLayout::default(),
                );
            }
        }
    }

    /// Insert a placeholder while local image decode runs off the GTK thread.
    pub(super) fn insert_async_image_placeholder(
        &self,
        buffer: &gtk4::TextBuffer,
        iter: &mut gtk4::TextIter,
        raw_target: &str,
        target: ImageWorkTarget,
        layout: EmbeddedBlockLayout,
    ) {
        let generation = self.imp().render_session.borrow().generation();
        if !self.imp().image_admission.borrow_mut().try_admit(
            PREVIEW_IMAGE_WORK_CHARGE_BYTES,
            MAX_PREVIEW_IMAGE_WORK_ITEMS,
            MAX_PREVIEW_IMAGE_WORK_BYTES,
        ) {
            let fallback = build_image_fallback_widget(
                "Image preview limited",
                "Only four local images are loaded automatically per render",
            );
            self.insert_embedded_widget(buffer, iter, &fallback, layout);
            return;
        }

        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        container.append(&build_image_fallback_widget("Loading image", raw_target));
        self.insert_embedded_widget(buffer, iter, container.upcast_ref::<gtk4::Widget>(), layout);

        let imp = self.imp();
        imp.current_image_work_count
            .set(imp.current_image_work_count.get().saturating_add(1));
        imp.image_queue.borrow_mut().push_back(PendingImageWork {
            generation,
            raw_target: raw_target.to_string(),
            target,
            container: container.downgrade(),
            charge_bytes: PREVIEW_IMAGE_WORK_CHARGE_BYTES,
        });
        self.start_next_image_work();
    }

    /// Start at most one decoder while queued descriptors remain payload-free.
    pub(super) fn start_next_image_work(&self) {
        let imp = self.imp();
        if imp.active_image.borrow().is_some() {
            return;
        }
        let Some(work) = imp.image_queue.borrow_mut().pop_front() else {
            return;
        };
        let PendingImageWork {
            generation,
            raw_target,
            target,
            container,
            charge_bytes,
        } = work;
        let cancel = Arc::new(AtomicBool::new(false));
        imp.active_image.replace(Some(ActiveImageWork {
            generation,
            container,
            charge_bytes,
            cancel: Arc::clone(&cancel),
        }));
        let render_generation = Arc::clone(&imp.render_generation);
        let Some(reservation) =
            crate::ui::plain_disposal::try_reserve_for_gtk(PREVIEW_IMAGE_WORK_CHARGE_BYTES)
        else {
            self.finish_image_work(crate::ui::plain_disposal::DisposalOwned::small_unreserved(
                OrderedImageCandidateResult::Unloadable {
                    path: PathBuf::from(raw_target),
                    error: "image preview deferred by memory pressure".to_string(),
                },
            ));
            return;
        };
        spawn_blocking_then_weak(
            self,
            move || {
                let result = first_loadable_ordered_image_cancellable(raw_target, target, || {
                    cancel.load(Ordering::Acquire)
                        || render_generation.load(Ordering::Acquire) != generation
                });
                reservation.own(result)
            },
            move |preview, result| preview.finish_image_work(result),
        );
    }

    /// Release exact scalar ownership and apply only a current image completion.
    pub(super) fn finish_image_work(&self, result: GuardedOrderedImageCandidateResult) {
        let imp = self.imp();
        let Some(active) = imp.active_image.take() else {
            return;
        };
        imp.image_admission
            .borrow_mut()
            .release(active.charge_bytes);
        if imp.render_session.borrow().is_current(active.generation) {
            imp.current_image_work_count
                .set(imp.current_image_work_count.get().saturating_sub(1));
            if let Some(container) = active.container.upgrade() {
                Self::replace_ordered_image_placeholder(
                    &container,
                    result.into_inner_for_current_install(),
                );
            }
        }
        self.start_next_image_work();
    }

    /// Release queued descriptor ownership while an active worker drains safely.
    pub(super) fn cancel_queued_image_work(&self) {
        let imp = self.imp();
        if let Some(active) = imp.active_image.borrow().as_ref() {
            active.cancel.store(true, Ordering::Release);
        }
        let queued = imp.image_queue.borrow_mut().drain(..).collect::<Vec<_>>();
        for work in queued {
            imp.image_admission.borrow_mut().release(work.charge_bytes);
        }
        imp.current_image_work_count.set(0);
    }

    /// Replace an async image placeholder with the resolved image or fallback.
    pub(super) fn replace_ordered_image_placeholder(
        container: &gtk4::Box,
        result: OrderedImageCandidateResult,
    ) {
        clear_box_children(container);
        match result {
            OrderedImageCandidateResult::Loadable { path, image } => {
                container.append(&build_decoded_image_widget(&path, image));
            }
            OrderedImageCandidateResult::Unloadable { path, error } => {
                container.append(&build_image_fallback_widget(
                    "Image could not be loaded",
                    &format!("{}\n{error}", path.display()),
                ));
            }
            OrderedImageCandidateResult::Missing { raw_target } => {
                container.append(&build_image_fallback_widget(
                    "Image file not found",
                    &raw_target,
                ));
            }
            OrderedImageCandidateResult::Cancelled => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use crate::ui::markdown_preview::MarkdownPreviewRenderContext;
    use tempfile::tempdir;

    #[test]
    fn preview_image_read_rejects_growth_after_metadata_without_unbounded_read() {
        let dir = tempdir().expect("image growth tempdir");
        let path = dir.path().join("growing-image.bin");
        fixture::write_bytes(&path, b"small");

        let error = read_preview_image_bytes_with_limit(
            &path,
            16,
            || {
                fixture::write_repeated_bytes(&path, b"x", 17);
            },
            || false,
        )
        .expect_err("growth beyond the image limit must fail");

        assert!(error.contains("grew beyond"));
    }

    #[test]
    fn test_resolve_image_target_prefers_document_relative_file_before_workspace() {
        let tempdir = tempdir().expect("tempdir");
        let document_dir = tempdir.path().join("docs");
        let workspace_folder = tempdir.path().join("workspace");
        fixture::create_dir_all(&document_dir);
        fixture::create_dir_all(&workspace_folder);
        fixture::write_bytes(&document_dir.join("logo.png"), b"doc");
        fixture::write_bytes(&workspace_folder.join("logo.png"), b"workspace");

        let context = MarkdownPreviewRenderContext::new(
            Some(document_dir.join("guide.md")),
            vec![workspace_folder],
        );

        assert_eq!(
            resolve_image_target("logo.png", &context),
            ResolvedImageTarget::Work(ImageWorkTarget::Relative {
                path: PathBuf::from("logo.png"),
                context: context.clone(),
            })
        );
    }

    #[test]
    fn test_resolve_image_target_falls_back_to_workspace_when_document_relative_missing() {
        let tempdir = tempdir().expect("tempdir");
        let document_dir = tempdir.path().join("docs");
        let workspace_folder = tempdir.path().join("workspace");
        fixture::create_dir_all(&document_dir);
        fixture::create_dir_all(&workspace_folder.join("images"));
        fixture::write_bytes(&workspace_folder.join("images/logo.png"), b"workspace");

        let context = MarkdownPreviewRenderContext::new(
            Some(document_dir.join("guide.md")),
            vec![workspace_folder],
        );

        assert_eq!(
            resolve_image_target("images/logo.png", &context),
            ResolvedImageTarget::Work(ImageWorkTarget::Relative {
                path: PathBuf::from("images/logo.png"),
                context: context.clone(),
            })
        );
    }

    #[test]
    fn test_resolve_image_target_uses_folder_order_for_workspace_candidates() {
        let tempdir = tempdir().expect("tempdir");
        let folder_a = tempdir.path().join("folder-a");
        let folder_b = tempdir.path().join("folder-b");
        fixture::create_dir_all(&folder_a.join("images"));
        fixture::create_dir_all(&folder_b.join("images"));
        fixture::write_bytes(&folder_a.join("images/logo.png"), b"a");
        fixture::write_bytes(&folder_b.join("images/logo.png"), b"b");

        let context = MarkdownPreviewRenderContext::new(None, vec![folder_b, folder_a]);

        assert_eq!(
            resolve_image_target("images/logo.png", &context),
            ResolvedImageTarget::Work(ImageWorkTarget::Relative {
                path: PathBuf::from("images/logo.png"),
                context: context.clone(),
            })
        );
    }

    #[test]
    fn test_first_loadable_ordered_image_skips_missing_and_unloadable_candidates() {
        let tempdir = tempdir().expect("tempdir");
        let missing_folder = tempdir.path().join("missing-folder");
        let invalid_folder = tempdir.path().join("invalid-folder");
        let folder_b = tempdir.path().join("folder-b");
        fixture::create_dir_all(&invalid_folder.join("images"));
        fixture::create_dir_all(&folder_b.join("images"));
        fixture::write_bytes(&invalid_folder.join("images/logo.svg"), b"not an image");
        fixture::write_text(
            &folder_b.join("images/logo.svg"),
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"##,
        );

        let (result, metrics) = first_loadable_ordered_image_with_metrics(
            "images/logo.svg".to_string(),
            ImageWorkTarget::Relative {
                path: PathBuf::from("images/logo.svg"),
                context: MarkdownPreviewRenderContext::new(
                    None,
                    vec![missing_folder, invalid_folder, folder_b.clone()],
                ),
            },
        );
        match result {
            OrderedImageCandidateResult::Loadable { path, image } => {
                assert_eq!(path, folder_b.join("images/logo.svg"));
                assert_eq!((image.width, image.height), (72, 72));
                assert!(!image.pixels.is_empty());
            }
            other => panic!("expected a decoded workspace image, got {other:?}"),
        }
        assert_eq!(metrics.inspected, 3);
        assert_eq!(metrics.peak_retained_candidate_paths, 2);
    }

    #[test]
    fn ordered_image_resolution_stops_at_early_success_across_many_folders() {
        let tempdir = tempdir().expect("many-folder image tempdir");
        let first = tempdir.path().join("folder-0000");
        fixture::create_dir_all(&first.join("images"));
        fixture::write_text(
            &first.join("images/logo.svg"),
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"##,
        );
        let mut folders = Vec::with_capacity(1_000);
        folders.push(first.clone());
        folders.extend((1..1_000).map(|index| tempdir.path().join(format!("folder-{index:04}"))));

        let (result, metrics) = first_loadable_ordered_image_with_metrics(
            "images/logo.svg".to_string(),
            ImageWorkTarget::Relative {
                path: PathBuf::from("images/logo.svg"),
                context: MarkdownPreviewRenderContext::new(None, folders),
            },
        );

        assert!(matches!(
            result,
            OrderedImageCandidateResult::Loadable { path, .. }
                if path == first.join("images/logo.svg")
        ));
        assert_eq!(metrics.inspected, 1);
        assert_eq!(metrics.peak_retained_candidate_paths, 1);
    }

    #[test]
    fn test_resolve_image_target_reports_missing_for_zero_folder_scope() {
        assert_eq!(
            resolve_image_target(
                "images/logo.png",
                &MarkdownPreviewRenderContext::new(None, Vec::new()),
            ),
            ResolvedImageTarget::Fallback {
                title: "Image file not found",
                body: "images/logo.png".to_string(),
            }
        );
    }

    #[test]
    fn test_resolve_image_target_rejects_remote_urls() {
        assert_eq!(
            resolve_image_target(
                "https://example.com/logo.png",
                &MarkdownPreviewRenderContext::default(),
            ),
            ResolvedImageTarget::Fallback {
                title: "Remote images are not supported",
                body: "https://example.com/logo.png".to_string(),
            }
        );
    }

    #[test]
    fn test_bounded_image_size_scales_down_wide_images() {
        assert_eq!(bounded_image_size(128, 128), (128, 128));
        assert_eq!(bounded_image_size(1280, 640), (640, 320));
        assert_eq!(bounded_image_size(640, 1280), (320, 640));
        assert_eq!(bounded_image_size(16, 16), (72, 72));
        assert_eq!(bounded_image_size(0, 0), (320, 180));
    }
}
