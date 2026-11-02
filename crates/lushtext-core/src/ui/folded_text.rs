// SPDX-License-Identifier: GPL-3.0-or-later

//! Cross-cutting coordination (no role): copying text that a folded surface
//! hides, from the text the surface was installed from
//! (`protected-view-inspector-and-surfaces` review M1 and S1).
//!
//! A folded `GtkTextView` holds every character of its text, the tails of its
//! giant paragraphs under the invisible fold tag (`ui/protected_fold.rs`).
//! GTK's own copy of a selection across such a tail first copies the whole
//! selected range into a second buffer on the GTK thread, hidden tails
//! included (unbounded), and then offers only the visible characters, so the
//! clipboard silently loses every hidden tail. A folded surface therefore
//! copies from its **source**, the text it was installed from:
//!
//! - **Ctrl+C, Ctrl+X and the context menu's Copy** (the view's
//!   `copy-clipboard` and `cut-clipboard` signals, which the menu actions emit)
//!   are stopped; the selection's character offsets, which are the source's
//!   because the buffer holds every character, go to a worker that cuts the
//!   range out of the source into its own bytes, and a [`SourceTextProvider`]
//!   holding only those bytes goes on the clipboard.
//! - **The primary selection** is the surface's own deferred
//!   [`SourceTextProvider`]: GTK's buffer provider (visible text only) is taken
//!   off, and a read of the primary selection cuts the then-current selection
//!   out of the source on a worker. The provider holds the source weakly, so it
//!   never keeps a closed surface's text alive.
//! - **Dragging a selection that reaches a hidden tail** is not started: a
//!   press inside such a selection collapses it at the press, so GtkTextView's
//!   own gesture starts a new selection instead of a drag of visible-only text.
//!
//! Callers: the local-history preview (`ui/window/local_history/`) and the
//! read-only note editors (`ui/window/notes/editor_execution.rs`). The
//! Protected View editor has its own copy path over its `ProtectedTextSource`
//! (`ui/editor_page/protected_view/`), and shares this module's provider.
//!
//! The shared sliced install step, `install_slice`, is here too: both surfaces
//! install a long text in bounded turns, one hidden tail per main-loop
//! iteration process-wide (`ui/protected_fold.rs`).

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::ops::Range;
use std::rc::Rc;
use std::sync::{Arc, Weak};
use std::task::{Poll, Waker};

use gtk4::prelude::*;

use crate::model::buffer_replacement::SliceBudget;
use crate::model::buffer_replacement::next_replacement_boundary;
use crate::model::protected_fold::FoldPlan;
use crate::ui::editor_page::WHOLE_LINE_CHUNK_BYTES as COPY_CHUNK_BYTES;
use crate::ui::protected_fold::{BoundaryLedger, FoldedInstall, tail_insert_admitted};

/// MIME types a [`SourceTextProvider`] offers: UTF-8 text only. No `GType` is
/// offered, so no reader can make GTK materialize the text as one `GValue`.
pub const SOURCE_TEXT_MIME_TYPES: [&str; 2] = ["text/plain;charset=utf-8", "text/plain"];

/// Text a folded surface shows, readable from a worker.
pub(crate) trait SurfaceText: Send + Sync + 'static {
    /// The whole text, exactly as the surface's buffer holds it.
    fn surface_text(&self) -> &str;
}

impl SurfaceText for String {
    fn surface_text(&self) -> &str {
        self
    }
}

/// The byte range of characters `start..end` of `text` (clamped to its end).
/// One forward scan over at most `end` characters: **worker only** for a
/// document-sized text.
#[must_use]
pub(crate) fn char_range_bytes(text: &str, start: usize, end: usize) -> Range<usize> {
    let mut from = text.len();
    let mut to = text.len();
    for (index, (byte, _)) in text.char_indices().enumerate() {
        if index == start {
            from = byte;
        }
        if index == end {
            to = byte;
            break;
        }
    }
    from.min(to)..to
}

/// Cut characters `start..end` of `text` into their own bytes. Worker only.
#[must_use]
pub(crate) fn cut_chars(text: &str, start: usize, end: usize) -> Arc<[u8]> {
    let range = char_range_bytes(text, start, end.max(start));
    Arc::from(&text.as_bytes()[range])
}

// ---------------------------------------------------------------------------
// Running one worker job from an async clipboard write
// ---------------------------------------------------------------------------

/// The completion slot of one worker job awaited by a clipboard write.
struct WorkerSlot<T> {
    value: RefCell<Option<T>>,
    waker: RefCell<Option<Waker>>,
}

/// Run `work` through the bounded worker pool (`spawn_blocking_then`) and
/// resolve on the GTK thread with its result.
fn run_on_worker<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = T> {
    let slot = Rc::new(WorkerSlot {
        value: RefCell::new(None),
        waker: RefCell::new(None),
    });
    gtk_lush_tasks::spawn_blocking_then(Rc::clone(&slot), work, |slot, value| {
        slot.value.replace(Some(value));
        if let Some(waker) = slot.waker.take() {
            waker.wake();
        }
    });
    std::future::poll_fn(move |context| {
        if let Some(value) = slot.value.take() {
            return Poll::Ready(value);
        }
        slot.waker.replace(Some(context.waker().clone()));
        Poll::Pending
    })
}

// ---------------------------------------------------------------------------
// The lazy, chunked clipboard provider
// ---------------------------------------------------------------------------

/// A worker job that cuts the bytes to serve out of a source.
pub(crate) type DeferredRead = Box<dyn FnOnce() -> Option<Arc<[u8]>> + Send>;

/// What a provider serves.
enum ProviderContent {
    /// Bytes already cut on a worker (a copy).
    Ready(Arc<[u8]>),
    /// Bytes cut when a reader asks (the primary selection): the resolver runs
    /// on the GTK thread at read time and returns the worker job, or `None`
    /// when there is nothing to serve.
    Deferred(Box<dyn Fn() -> Option<DeferredRead>>),
}

mod provider_imp {
    use std::cell::{Cell, RefCell};
    use std::future::Future;
    use std::pin::Pin;
    use std::rc::Rc;
    use std::sync::Arc;

    use gtk4::gdk;
    use gtk4::gdk::subclass::prelude::*;
    use gtk4::prelude::*;

    use super::{COPY_CHUNK_BYTES, ProviderContent, SOURCE_TEXT_MIME_TYPES, run_on_worker};

    #[derive(Default)]
    pub struct SourceTextProvider {
        pub(super) content: RefCell<Option<ProviderContent>>,
        pub(super) chunks_written: Rc<Cell<u64>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SourceTextProvider {
        const NAME: &'static str = "LushtextSourceTextProvider";
        type Type = super::SourceTextProvider;
        type ParentType = gdk::ContentProvider;
    }

    impl ObjectImpl for SourceTextProvider {}

    impl ContentProviderImpl for SourceTextProvider {
        fn formats(&self) -> gdk::ContentFormats {
            gdk::ContentFormats::new(&SOURCE_TEXT_MIME_TYPES)
        }

        fn write_mime_type_future(
            &self,
            mime_type: &str,
            stream: &gio::OutputStream,
            io_priority: glib::Priority,
        ) -> Pin<Box<dyn Future<Output = Result<(), glib::Error>> + 'static>> {
            enum Pending {
                Ready(Arc<[u8]>),
                Read(super::DeferredRead),
                Nothing,
            }
            let known = SOURCE_TEXT_MIME_TYPES.contains(&mime_type);
            let pending = match self.content.borrow().as_ref() {
                _ if !known => Pending::Nothing,
                Some(ProviderContent::Ready(bytes)) => Pending::Ready(Arc::clone(bytes)),
                Some(ProviderContent::Deferred(resolve)) => {
                    resolve().map_or(Pending::Nothing, Pending::Read)
                }
                None => Pending::Nothing,
            };
            let stream = stream.clone();
            let chunks = Rc::clone(&self.chunks_written);
            Box::pin(async move {
                let unavailable = || {
                    glib::Error::new(gio::IOErrorEnum::NotSupported, "no clipboard text to serve")
                };
                let bytes = match pending {
                    Pending::Ready(bytes) => bytes,
                    Pending::Read(read) => run_on_worker(read).await.ok_or_else(unavailable)?,
                    Pending::Nothing => return Err(unavailable()),
                };
                let mut at = 0;
                // One chunk per main-loop dispatch: each write is awaited.
                while at < bytes.len() {
                    let end = (at + COPY_CHUNK_BYTES).min(bytes.len());
                    let chunk = glib::Bytes::from(&bytes[at..end]);
                    let written = stream.write_bytes_future(&chunk, io_priority).await?;
                    chunks.set(chunks.get().saturating_add(1));
                    let written = usize::try_from(written).unwrap_or(0);
                    if written == 0 {
                        return Err(glib::Error::new(
                            gio::IOErrorEnum::BrokenPipe,
                            "clipboard reader stopped",
                        ));
                    }
                    at += written;
                }
                Ok(())
            })
        }
    }
}

glib::wrapper! {
    /// A clipboard provider that serves text cut out of a document source on a
    /// worker, writing it to a reader in `COPY_CHUNK_BYTES` chunks, one per
    /// main-loop dispatch. It holds only the bytes it serves (a copy), or a
    /// weak way back to the source (the primary selection), never a strong
    /// reference to a whole document.
    pub struct SourceTextProvider(ObjectSubclass<provider_imp::SourceTextProvider>)
        @extends gtk4::gdk::ContentProvider;
}

impl SourceTextProvider {
    /// A provider serving `bytes`, already cut on a worker.
    #[must_use]
    pub(crate) fn ready(bytes: Arc<[u8]>) -> Self {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        let provider: Self = glib::Object::new();
        provider
            .imp()
            .content
            .replace(Some(ProviderContent::Ready(bytes)));
        provider
    }

    /// A provider whose bytes are cut when a reader asks: `resolve` runs on
    /// the GTK thread at read time and returns the worker job.
    #[must_use]
    pub(crate) fn deferred(resolve: impl Fn() -> Option<DeferredRead> + 'static) -> Self {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        let provider: Self = glib::Object::new();
        provider
            .imp()
            .content
            .replace(Some(ProviderContent::Deferred(Box::new(resolve))));
        provider
    }

    /// Chunks written to readers so far.
    #[must_use]
    pub fn chunks_written(&self) -> u64 {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        self.imp().chunks_written.get()
    }

    /// Bytes a ready provider serves (0 for a deferred one).
    #[must_use]
    pub fn byte_len(&self) -> usize {
        use glib::subclass::prelude::ObjectSubclassIsExt;
        match self.imp().content.borrow().as_ref() {
            Some(ProviderContent::Ready(bytes)) => bytes.len(),
            _ => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// The folded surface's copy routing
// ---------------------------------------------------------------------------

/// The source and fold plan of the text a surface currently shows folded.
struct FoldedSource {
    source: Weak<dyn SurfaceText>,
    plan: Arc<FoldPlan>,
}

/// Copy routing for one read-only folded `GtkTextView` (module docs).
pub(crate) struct FoldedTextSurface {
    view: glib::WeakRef<gtk4::TextView>,
    folded: RefCell<Option<FoldedSource>>,
    primary: RefCell<Option<SourceTextProvider>>,
    primary_removed: Cell<bool>,
}

impl std::fmt::Debug for FoldedTextSurface {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FoldedTextSurface")
            .field("folded", &self.folded.borrow().is_some())
            .finish()
    }
}

impl FoldedTextSurface {
    /// Install the copy routing on `view`. It does nothing until a folded
    /// source is set ([`Self::set_folded_source`]).
    pub(crate) fn attach(view: &gtk4::TextView) -> Rc<Self> {
        let surface = Rc::new(Self {
            view: view.downgrade(),
            folded: RefCell::new(None),
            primary: RefCell::new(None),
            primary_removed: Cell::new(false),
        });
        for name in ["copy-clipboard", "cut-clipboard"] {
            let weak = Rc::downgrade(&surface);
            view.connect_local(name, false, move |args| {
                if let Some(surface) = weak.upgrade()
                    && surface.is_folded()
                    && let Some(Ok(view)) = args.first().map(glib::Value::get::<glib::Object>)
                {
                    // The view is read-only: a cut copies, as GTK's would.
                    glib::signal::signal_stop_emission_by_name(&view, name);
                    surface.copy_selection();
                }
                None
            });
        }
        // GtkTextView adds the primary clipboard to its buffer on realize and
        // removes it on unrealize (removing an absent one is a CRITICAL): put
        // it back before an unrealize, take it off again after a realize.
        let weak = Rc::downgrade(&surface);
        view.connect_unrealize(move |_| {
            if let Some(surface) = weak.upgrade() {
                surface.set_buffer_primary_off(false);
            }
        });
        let weak = Rc::downgrade(&surface);
        view.connect_realize(move |_| {
            if let Some(surface) = weak.upgrade()
                && surface.is_folded()
            {
                surface.set_buffer_primary_off(true);
                surface.sync_primary();
            }
        });
        let weak = Rc::downgrade(&surface);
        view.buffer().connect_has_selection_notify(move |_| {
            if let Some(surface) = weak.upgrade() {
                surface.sync_primary();
            }
        });
        surface.install_drag_guard(view);
        surface
    }

    /// Show `source` folded by `plan` from now on, or (`None`) an unfolded
    /// text GTK may copy itself.
    pub(crate) fn set_folded_source(
        self: &Rc<Self>,
        source: Option<(Weak<dyn SurfaceText>, Arc<FoldPlan>)>,
    ) {
        let folded = source.is_some();
        self.folded
            .replace(source.map(|(source, plan)| FoldedSource { source, plan }));
        self.set_buffer_primary_off(folded);
        self.sync_primary();
    }

    /// Whether the surface currently routes copies through a source.
    pub(crate) fn is_folded(&self) -> bool {
        self.folded.borrow().is_some()
    }

    /// The selection's character offsets, while there is one.
    fn selection_offsets(&self) -> Option<(usize, usize)> {
        let view = self.view.upgrade()?;
        let (start, end) = view.buffer().selection_bounds()?;
        Some((
            usize::try_from(start.offset()).ok()?,
            usize::try_from(end.offset()).ok()?,
        ))
    }

    fn copy_selection(self: &Rc<Self>) {
        let Some((start, end)) = self.selection_offsets() else {
            return;
        };
        let Some(source) = self
            .folded
            .borrow()
            .as_ref()
            .and_then(|folded| folded.source.upgrade())
        else {
            return;
        };
        let Some(view) = self.view.upgrade() else {
            return;
        };
        gtk_lush_tasks::spawn_blocking_then(
            view,
            move || cut_chars(source.surface_text(), start, end),
            move |view, bytes| {
                let provider = SourceTextProvider::ready(bytes);
                view.clipboard()
                    .set_content(Some(provider.upcast_ref::<gtk4::gdk::ContentProvider>()))
                    .ok();
            },
        );
    }

    /// Take GTK's buffer provider off the primary selection (`off`), or put it
    /// back. Only a realized view has it.
    fn set_buffer_primary_off(&self, off: bool) {
        let Some(view) = self.view.upgrade() else {
            return;
        };
        if off == self.primary_removed.get() || (off && !view.is_realized()) {
            return;
        }
        let primary = view.primary_clipboard();
        if off {
            view.buffer().remove_selection_clipboard(&primary);
        } else {
            self.release_primary(&primary);
            view.buffer().add_selection_clipboard(&primary);
        }
        self.primary_removed.set(off);
    }

    /// Offer the surface's deferred provider on the primary selection while a
    /// folded selection exists; withdraw it when the selection goes, as GTK
    /// does for its own provider.
    fn sync_primary(self: &Rc<Self>) {
        let Some(view) = self.view.upgrade() else {
            return;
        };
        if !self.primary_removed.get() {
            return;
        }
        let primary = view.primary_clipboard();
        if !view.buffer().has_selection() {
            self.release_primary(&primary);
            return;
        }
        let provider = self
            .primary
            .borrow_mut()
            .get_or_insert_with(|| {
                let weak = Rc::downgrade(self);
                SourceTextProvider::deferred(move || {
                    let surface = weak.upgrade()?;
                    let (start, end) = surface.selection_offsets()?;
                    let source = surface.folded.borrow().as_ref()?.source.clone();
                    Some(Box::new(move || {
                        let source = source.upgrade()?;
                        Some(cut_chars(source.surface_text(), start, end))
                    }) as DeferredRead)
                })
            })
            .clone();
        let ours = primary
            .content()
            .is_some_and(|content| content == *provider.upcast_ref::<gtk4::gdk::ContentProvider>());
        if !ours {
            primary
                .set_content(Some(provider.upcast_ref::<gtk4::gdk::ContentProvider>()))
                .ok();
        }
    }

    fn release_primary(&self, primary: &gtk4::gdk::Clipboard) {
        let ours = self.primary.borrow().as_ref().is_some_and(|provider| {
            primary.content().is_some_and(|content| {
                content == *provider.upcast_ref::<gtk4::gdk::ContentProvider>()
            })
        });
        if ours {
            primary
                .set_content(None::<&gtk4::gdk::ContentProvider>)
                .ok();
        }
    }

    /// A press inside a selection that reaches a hidden tail collapses the
    /// selection at the press before GtkTextView's own gesture sees it, so no
    /// drag of visible-only text starts.
    fn install_drag_guard(self: &Rc<Self>, view: &gtk4::TextView) {
        let click = gtk4::GestureClick::new();
        click.set_button(gtk4::gdk::BUTTON_PRIMARY);
        click.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |gesture, n_press, x, y| {
            let Some(surface) = weak.upgrade() else {
                return;
            };
            let Some(view) = gesture.widget().and_downcast::<gtk4::TextView>() else {
                return;
            };
            if n_press == 1
                && surface.press_starts_a_folded_drag(&view, x, y)
                && let Some(iter) = iter_at_widget_point(&view, x, y)
            {
                view.buffer().place_cursor(&iter);
            }
        });
        view.add_controller(click);
    }

    fn press_starts_a_folded_drag(&self, view: &gtk4::TextView, x: f64, y: f64) -> bool {
        let folded = self.folded.borrow();
        let Some(folded) = folded.as_ref() else {
            return false;
        };
        let buffer = view.buffer();
        let Some((start, end)) = buffer.selection_bounds() else {
            return false;
        };
        let Some(at) = iter_at_widget_point(view, x, y) else {
            return false;
        };
        if !at.in_range(&start, &end) {
            return false;
        }
        selection_reaches_hidden_text(
            &folded.plan,
            (
                u64::try_from(start.line()).unwrap_or(0),
                u64::try_from(start.line_offset()).unwrap_or(0),
            ),
            (
                u64::try_from(end.line()).unwrap_or(0),
                u64::try_from(end.line_offset()).unwrap_or(0),
            ),
        )
    }
}

/// The buffer position under widget point `(x, y)` of `view`.
fn iter_at_widget_point(view: &gtk4::TextView, x: f64, y: f64) -> Option<gtk4::TextIter> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a gesture's widget coordinate converted to GtkTextView's integer window coordinates"
    )]
    let (x, y) = (x.round() as i32, y.round() as i32);
    let (bx, by) = view.window_to_buffer_coords(gtk4::TextWindowType::Widget, x, y);
    view.iter_at_location(bx, by)
}

/// Whether a selection from `(line, column)` `start` to `end` covers a hidden
/// tail character of `plan`. A folded paragraph's tail is its columns from
/// `head_chars` on, so the selection covers it when it runs past that column
/// of the paragraph's line and starts before the line's end.
#[must_use]
pub(crate) fn selection_reaches_hidden_text(
    plan: &FoldPlan,
    start: (u64, u64),
    end: (u64, u64),
) -> bool {
    let ranges = plan.ranges();
    let first = ranges.partition_point(|range| range.paragraph < start.0);
    ranges[first..]
        .iter()
        .take_while(|range| range.paragraph <= end.0)
        .any(|range| {
            let line_end = range.head_chars + range.tail_chars;
            let starts_before_end = range.paragraph > start.0 || start.1 < line_end;
            let ends_past_head = range.paragraph < end.0 || end.1 > range.head_chars;
            starts_before_end && ends_past_head
        })
}

// ---------------------------------------------------------------------------
// The shared sliced install step
// ---------------------------------------------------------------------------

/// What one install turn did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InstallSlice {
    /// Another document already inserted a hidden tail in this main-loop
    /// iteration: nothing was inserted, retry in the next one.
    Deferred,
    /// One slice was inserted; `offset` advanced.
    Inserted,
    /// The sliced protected install broke (`FoldedInstallBroken`): nothing
    /// more may be inserted, and the caller cancels the install.
    Broken,
}

/// Insert the next slice of `text` (from `*offset`) at the end of `buffer`.
/// Without a fold plan the slice ends after a paragraph delimiter or at a
/// UTF-8-safe slice budget (`next_replacement_boundary`); with one it is the
/// next step of the sliced protected install (`ui::protected_fold::
/// FoldedInstall`, C3), whose boundary marks `ledger` records, and it runs only
/// when no other tail went in during this iteration (`tail_insert_admitted`).
pub(crate) fn install_slice(
    buffer: &gtk4::TextBuffer,
    text: &str,
    offset: &mut usize,
    fold: &mut Option<FoldedInstall>,
    ledger: &BoundaryLedger,
) -> InstallSlice {
    if let Some(install) = fold.as_mut() {
        if !tail_insert_admitted() {
            return InstallSlice::Deferred;
        }
        let append = || buffer.end_iter();
        match install.step(&append, text, SliceBudget::BYTES_ONLY, ledger) {
            Ok(end) => *offset = end,
            Err(_) => return InstallSlice::Broken,
        }
    } else {
        let end = next_replacement_boundary(text, *offset);
        let mut end_iter = buffer.end_iter();
        buffer.insert(&mut end_iter, &text[*offset..end]);
        *offset = end;
    }
    InstallSlice::Inserted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::protected_fold::FoldRange;

    #[test]
    fn char_ranges_cut_on_character_boundaries() {
        let text = "aé€😀b";
        assert_eq!(&text[char_range_bytes(text, 1, 4)], "é€😀");
        assert_eq!(&text[char_range_bytes(text, 0, 5)], text);
        assert_eq!(&text[char_range_bytes(text, 4, 99)], "b");
        assert_eq!(&text[char_range_bytes(text, 9, 99)], "");
        assert_eq!(&*cut_chars(text, 2, 3), "€".as_bytes());
    }

    #[test]
    fn a_selection_reaches_hidden_text_only_past_a_head() {
        let plan = FoldPlan::from_ranges(vec![FoldRange {
            paragraph: 2,
            head_chars: 10,
            tail_chars: 90,
        }]);
        // Inside the head only.
        assert!(!selection_reaches_hidden_text(&plan, (2, 0), (2, 10)));
        // Past the head.
        assert!(selection_reaches_hidden_text(&plan, (2, 0), (2, 11)));
        // Across the whole folded line.
        assert!(selection_reaches_hidden_text(&plan, (1, 0), (3, 0)));
        // Before and after it.
        assert!(!selection_reaches_hidden_text(&plan, (0, 0), (1, 5)));
        assert!(!selection_reaches_hidden_text(&plan, (3, 0), (4, 5)));
        // Starting at the folded line's end.
        assert!(!selection_reaches_hidden_text(&plan, (2, 100), (3, 1)));
    }
}
