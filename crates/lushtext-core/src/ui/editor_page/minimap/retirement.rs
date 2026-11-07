// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination — **retirement**.
//!
//! Destroying payloads the workflow is finished with: an in-flight analysis
//! session and its buffer cursor mark, the sole idle continuation source, the
//! accepted content cache, the retained long-line marker identities, and the
//! modified-since-save tags.
//!
//! **Modified-since-save tracking** is one property-less `GtkTextTag` per
//! generation (ledger A98: a B-tree walk to apply, no relayout, no undo entry,
//! moves with its text). A clean transition does not untag the buffer, which
//! costs O(toggles) in one call (30.6–32.4 ms at 131,072 runs): it **swaps**
//! in a fresh tag, so the strip stops reading the old one in the same
//! dispatch, and queues the old one here. Retiring tags form a FIFO, so a
//! second save while one still retires queues a second. Each slice, at
//! `G_PRIORITY_LOW`, advances a `GtkTextMark` cursor across at most
//! `MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES` toggles, removes the tag over that
//! range, and resumes; a tag with no toggles left leaves the tag table.
//! Marks survive edits between slices, where iterators would not.
//! **Inversion:** the slice loop yields the main loop each turn and resumes in
//! `run_modified_tag_retirement_slice`, guarded by `retirement_generation`.
//!
//! Cancellation is the one place the analysis generation advances outside
//! admission, and it must: a cancelled generation can never publish, so every
//! in-flight slice observing the new value stops on its next resumption. The
//! editor *lifetime* advances only in `retire_minimap_analysis`, which is the
//! stronger statement that no slice from this editor may ever run again.
//!
//! `apply_minimap_tracking_suspension` lives here because suspension is what stops
//! a programmatic buffer replacement from being recorded as a user edit; the
//! guard suspends and exactly restores `imp().minimap.tracking_suspended`, and
//! splitting the suspend from the restore across modules is how that exactness
//! would be lost.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use glib::subclass::prelude::ObjectSubclassIsExt;
use sourceview5::prelude::*;

use super::LushtextEditorPage;
use super::policy::MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES;
use crate::config::keys;

/// Name prefix of the modified-since-save tags; the generation follows.
const MODIFIED_TAG_PREFIX: &str = "lushtext-minimap-modified-";

/// A swapped-out modified tag and the cursor its retirement resumes from.
pub(crate) struct RetiringModifiedTag {
    buffer: gtk4::TextBuffer,
    tag: gtk4::TextTag,
    cursor: gtk4::TextMark,
}

/// Modified-since-save tracking state of one editor (`MinimapState::modified`).
#[derive(Default)]
pub(crate) struct MinimapModifiedTracking {
    /// The tag edits apply now, created on the first edit after a swap.
    pub(super) current: RefCell<Option<gtk4::TextTag>>,
    /// Advances at each swap; names the next tag.
    pub(super) tag_generation: Cell<u64>,
    /// Swapped-out tags still holding toggles, oldest first.
    pub(super) retiring: RefCell<VecDeque<RetiringModifiedTag>>,
    /// The sole low-priority source retiring them.
    pub(super) retirement_source: RefCell<Option<glib::SourceId>>,
    /// Invalidates a slice queued before a cancellation.
    pub(super) retirement_generation: Cell<u64>,
    /// Retirement slices run during this editor's lifetime.
    #[cfg(feature = "test-utils")]
    pub(super) retirement_slices: Cell<usize>,
}

impl LushtextEditorPage {
    pub(super) fn cancel_minimap_analysis(&self, clear_cache: bool, release_markers: bool) {
        let imp = self.imp();
        let had_session = imp.minimap.analysis_session.borrow().is_some();
        let source_id = imp.minimap.analysis_source_id.take();
        let had_source = source_id.is_some();
        if had_session || had_source || clear_cache {
            imp.minimap
                .analysis_generation
                .set(imp.minimap.analysis_generation.get().wrapping_add(1));
        }
        if let Some(source_id) = source_id {
            source_id.remove();
        }
        if let Some(session) = imp.minimap.analysis_session.take() {
            session.buffer.delete_mark(&session.cursor_mark);
        }
        if clear_cache {
            imp.minimap.analysis_cache.take();
        } else if release_markers
            && let Some(cache) = imp.minimap.analysis_cache.borrow_mut().as_mut()
        {
            cache.result.long_line_lines.clear();
            cache.markers_collected = false;
        }
        #[cfg(feature = "test-utils")]
        if had_session || had_source {
            imp.minimap
                .analysis_cancellations
                .set(imp.minimap.analysis_cancellations.get().saturating_add(1));
        }
    }

    pub(super) fn discard_minimap_analysis_content(&self) {
        self.cancel_minimap_analysis(true, false);
    }

    pub(super) fn discard_minimap_analysis_request(&self, marker_preference_changed: bool) {
        let release_markers = marker_preference_changed
            && !self
                .imp()
                .settings
                .boolean(keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE);
        self.cancel_minimap_analysis(false, release_markers);
    }

    pub(super) fn retire_minimap_analysis(&self) {
        let lifetime = &self.imp().minimap.analysis_lifetime;
        lifetime.set(lifetime.get().wrapping_add(1));
        self.cancel_minimap_analysis(true, false);
    }

    /// Temporarily suspend edit tracking while programmatic buffer mutations run.
    pub(super) fn apply_minimap_tracking_suspension(&self, suspended: bool) {
        if suspended && !self.imp().minimap.tracking_suspended.get() {
            self.discard_minimap_analysis_content();
        }
        self.imp().minimap.tracking_suspended.set(suspended);
    }

    /// Retire the native source map while installation or the scale policy
    /// keeps it away from the buffer.
    ///
    /// `GtkSourceMap` is a second text view, so a hidden but bound map still
    /// lays out every paragraph. Clearing its `view` while its update tick may
    /// be armed makes the tick run against no view (ledger A32: a
    /// `Gtk-CRITICAL` and a `GLib-GObject-CRITICAL`), and LushText cannot read
    /// that tick. So the map is **retired**: the overlay is hidden, the map is
    /// unparented, which unrealizes it and detaches it from the frame clock so
    /// an armed tick never runs (ledger A45), and LushText drops its
    /// reference. `attach_native_source_map` builds a fresh one when the
    /// minimap is shown again.
    pub(super) fn detach_minimap_projection(&self) {
        self.discard_minimap_analysis_content();
        self.retire_native_source_map();
    }

    /// Unparent and drop the current map, if any.
    pub(super) fn retire_native_source_map(&self) {
        let imp = self.imp();
        imp.minimap_overlay.set_visible(false);
        let Some(source_map) = imp.minimap.source_map.borrow_mut().take() else {
            return;
        };
        imp.minimap.map_signals.clear();
        if let Some(render_hold) = imp.minimap.render_hold.borrow().as_ref() {
            render_hold.clear();
        }
        if source_map.parent().is_some() {
            imp.minimap_overlay.set_child(None::<&gtk4::Widget>);
        }
        // Nothing else may keep the map: a retired map that stays alive also
        // stays bound to the document buffer (through its view's `buffer`
        // binding) and re-lays out every edited paragraph from its validation
        // idle. The reflow freeze refers to its live child weakly for this
        // reason. The weak reference below is evidence only.
        let mut retired = imp.minimap.retired_maps.borrow_mut();
        retired.retain(|map| map.upgrade().is_some());
        retired.push(source_map.downgrade());
        drop(retired);
        drop(source_map);
    }

    /// The current modified tag, created and added to the buffer's table on
    /// first use after a swap.
    pub(super) fn current_modified_tag(&self) -> gtk4::TextTag {
        let tracking = &self.imp().minimap.modified;
        if let Some(tag) = tracking.current.borrow().as_ref() {
            return tag.clone();
        }
        let generation = tracking.tag_generation.get();
        let tag = gtk4::TextTag::new(Some(&format!("{MODIFIED_TAG_PREFIX}{generation}")));
        self.buffer().tag_table().add(&tag);
        tracking.current.replace(Some(tag.clone()));
        tag
    }

    /// Clear every modified-since-save marker in this dispatch by swapping in a
    /// fresh tag, and queue the old one for sliced retirement.
    pub(super) fn swap_modified_tag(&self) {
        let imp = self.imp();
        let tracking = &imp.minimap.modified;
        tracking
            .tag_generation
            .set(tracking.tag_generation.get().wrapping_add(1));
        let Some(tag) = tracking.current.take() else {
            return;
        };
        let buffer: gtk4::TextBuffer = self.buffer().upcast();
        let cursor = buffer.create_mark(None, &buffer.start_iter(), true);
        tracking
            .retiring
            .borrow_mut()
            .push_back(RetiringModifiedTag {
                buffer,
                tag,
                cursor,
            });
        self.note_marker_change();
        self.arm_modified_tag_retirement();
    }

    /// Arm the sole retirement source unless one is already armed.
    fn arm_modified_tag_retirement(&self) {
        let tracking = &self.imp().minimap.modified;
        if tracking.retirement_source.borrow().is_some() {
            return;
        }
        let generation = tracking.retirement_generation.get();
        let editor_weak = self.downgrade();
        let source = glib::idle_add_local_full(glib::Priority::LOW, move || {
            let Some(editor) = editor_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            editor.run_modified_tag_retirement_slice(generation)
        });
        tracking.retirement_source.replace(Some(source));
    }

    /// One retirement slice: at most `MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES`
    /// toggles of the oldest retiring tag.
    fn run_modified_tag_retirement_slice(&self, generation: u64) -> glib::ControlFlow {
        let tracking = &self.imp().minimap.modified;
        if tracking.retirement_generation.get() != generation {
            return glib::ControlFlow::Break;
        }
        #[cfg(feature = "test-utils")]
        tracking
            .retirement_slices
            .set(tracking.retirement_slices.get().saturating_add(1));
        let finished = {
            let retiring = tracking.retiring.borrow();
            let Some(front) = retiring.front() else {
                drop(retiring);
                tracking.retirement_source.take();
                return glib::ControlFlow::Break;
            };
            retire_tag_slice(front)
        };
        if finished {
            let done = tracking.retiring.borrow_mut().pop_front();
            if let Some(done) = done {
                done.buffer.delete_mark(&done.cursor);
                // No toggles remain, so leaving the table walks nothing.
                done.buffer.tag_table().remove(&done.tag);
            }
        }
        if tracking.retiring.borrow().is_empty() {
            tracking.retirement_source.take();
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    }

    /// Cancel retirement on dispose: the source is removed and the cursors are
    /// deleted. The swapped-out tags stay in the table of a buffer that goes
    /// away with this editor, rather than being untagged in one O(toggles)
    /// call during teardown.
    pub(crate) fn cancel_modified_tag_retirement(&self) {
        let tracking = &self.imp().minimap.modified;
        tracking
            .retirement_generation
            .set(tracking.retirement_generation.get().wrapping_add(1));
        if let Some(source) = tracking.retirement_source.take() {
            source.remove();
        }
        for retiring in tracking.retiring.borrow_mut().drain(..) {
            retiring.buffer.delete_mark(&retiring.cursor);
        }
    }

    /// Whether a swapped-out tag is still retiring.
    pub(super) fn modified_tag_retirement_pending(&self) -> bool {
        !self.imp().minimap.modified.retiring.borrow().is_empty()
    }
}

/// Remove `retiring.tag` across at most `MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES`
/// toggles from its cursor and advance the cursor; `true` once the tag has no
/// toggles left.
fn retire_tag_slice(retiring: &RetiringModifiedTag) -> bool {
    let buffer = &retiring.buffer;
    let start = buffer.iter_at_mark(&retiring.cursor);
    let mut end = start;
    let mut toggles = 0;
    while toggles < MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES
        && end.forward_to_tag_toggle(Some(&retiring.tag))
    {
        toggles += 1;
    }
    let finished = toggles < MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES;
    if finished {
        end = buffer.end_iter();
    }
    buffer.remove_tag(&retiring.tag, &start, &end);
    buffer.move_mark(&retiring.cursor, &end);
    finished
}
