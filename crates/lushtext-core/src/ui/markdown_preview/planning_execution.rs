// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: coordination — execution, qualified by stage order (planning).
//!
//! Parses Markdown into an immutable plan on a worker, for sources large enough
//! that parsing them on the GTK thread would stall a frame. Ownership is **one
//! active plus one replaceable latest**: a newer request while a worker runs
//! replaces the queued plan rather than starting a second worker, so continuous
//! typing produces one parse per quiet moment instead of one per keystroke.
//!
//! This workflow owns two `execution` stage orders — planning and projection —
//! so both carry the stage-order qualifier the convention requires rather than
//! one of them taking an ill-fitting bounded role name.
//!
//! Resumption points: the worker completion, and a **second** one inside that
//! same completion where a queued superseding plan is re-dispatched.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk_lush_tasks::spawn_blocking_then;
use gtk4::prelude::*;

use crate::services::markdown_render::{markdown_render_options, plan_markdown_cancellable};

use super::policy::inline_footnote_limited_plan;
use super::policy::{InlineFootnoteLowering, lower_inline_footnotes_cancellable};
use super::seams::try_guard_markdown_source;
use super::seams::{GuardedPendingMarkdownPlan, PendingMarkdownPlan};
#[cfg(feature = "test-utils")]
use super::test_policy::MARKDOWN_PLAN_DELAY_MS;
use super::{LushtextMarkdownPreview, MarkdownPreviewRenderContext};

impl LushtextMarkdownPreview {
    /// Keep one document-sized planner active and one replaceable latest source.
    pub(super) fn enqueue_markdown_plan(
        &self,
        generation: u64,
        markdown: &str,
        context: MarkdownPreviewRenderContext,
    ) {
        let imp = self.imp();
        if imp.planning_worker_running.get() {
            if let Some(cancel) = imp.planning_cancel_token.borrow().as_ref() {
                cancel.store(true, Ordering::Release);
            }
            if let Some(queued) = imp.queued_plan.borrow_mut().as_mut() {
                queued.generation = generation;
                queued.source.clear();
                queued.source.push_str(markdown);
                queued.context = context;
                return;
            }
            let Some(request) = try_guard_markdown_source(generation, markdown, &context) else {
                self.finish_markdown_capacity_pressure(generation, markdown.len());
                return;
            };
            if let Some(retired) = imp.queued_plan.replace(Some(request)) {
                self.retire_guarded_markdown(retired);
            }
            return;
        }
        let Some(request) = try_guard_markdown_source(generation, markdown, &context) else {
            self.finish_markdown_capacity_pressure(generation, markdown.len());
            return;
        };
        self.show_pending_markdown_plan();
        self.spawn_markdown_plan(request);
    }

    pub(super) fn enqueue_owned_markdown_plan(&self, request: GuardedPendingMarkdownPlan) {
        let imp = self.imp();
        if imp.planning_worker_running.get() {
            if let Some(cancel) = imp.planning_cancel_token.borrow().as_ref() {
                cancel.store(true, Ordering::Release);
            }
            if let Some(retired) = imp.queued_plan.replace(Some(request)) {
                self.retire_guarded_markdown(retired);
            }
        } else {
            self.spawn_markdown_plan(request);
        }
    }

    pub(super) fn spawn_markdown_plan(&self, request: GuardedPendingMarkdownPlan) {
        let imp = self.imp();
        imp.planning_worker_running.set(true);
        let cancel = Arc::new(AtomicBool::new(false));
        imp.planning_cancel_token.replace(Some(cancel.clone()));
        let generation = request.generation;
        let context = request.context.clone();
        let request = request;
        let generation_counter = imp.render_generation.clone();
        let retirement_jobs = imp.plain_retirement_jobs.clone();
        let preview_weak = self.downgrade();
        spawn_blocking_then(
            preview_weak,
            move || {
                let outcome = request.map_preserving_reservation(|request| {
                    let PendingMarkdownPlan {
                        generation,
                        source,
                        context: _,
                    } = request;
                    #[cfg(feature = "test-utils")]
                    std::thread::sleep(std::time::Duration::from_millis(
                        MARKDOWN_PLAN_DELAY_MS.load(Ordering::Acquire),
                    ));
                    if cancel.load(Ordering::Acquire) {
                        return None;
                    }
                    let plan = match lower_inline_footnotes_cancellable(
                        &source,
                        markdown_render_options(),
                        &cancel,
                    ) {
                        InlineFootnoteLowering::Lowered(lowered) => {
                            plan_markdown_cancellable(&lowered, &cancel)
                        }
                        InlineFootnoteLowering::Unchanged => {
                            plan_markdown_cancellable(&source, &cancel)
                        }
                        InlineFootnoteLowering::Limited => {
                            Some(inline_footnote_limited_plan(source.len()))
                        }
                        InlineFootnoteLowering::Cancelled => None,
                    };
                    if generation_counter.load(Ordering::Acquire) == generation {
                        plan
                    } else {
                        if plan.is_some() {
                            retirement_jobs.fetch_add(1, Ordering::AcqRel);
                        }
                        None
                    }
                });
                if outcome.is_some() {
                    Some(outcome.map_preserving_reservation(|outcome| {
                        outcome.expect("checked Markdown plan outcome exists")
                    }))
                } else {
                    drop(outcome);
                    None
                }
            },
            move |preview_weak, plan| {
                let Some(preview) = preview_weak.upgrade() else {
                    drop(plan);
                    return;
                };
                let imp = preview.imp();
                imp.planning_worker_running.set(false);
                imp.planning_cancel_token.take();
                if let Some(plan) = plan {
                    if imp.render_session.borrow().is_current(generation) {
                        preview.start_render_plan(generation, plan, context);
                    } else {
                        preview.retire_guarded_markdown(plan);
                    }
                }
                let queued = imp.queued_plan.take();
                if let Some(queued) = queued {
                    if imp.render_session.borrow().is_current(queued.generation) {
                        preview.spawn_markdown_plan(queued);
                    } else {
                        preview.retire_guarded_markdown(queued);
                    }
                }
            },
        );
    }

    pub(super) fn cancel_pending_markdown_planning(&self) {
        let imp = self.imp();
        if let Some(cancel) = imp.planning_cancel_token.borrow().as_ref() {
            cancel.store(true, Ordering::Release);
        }
        if let Some(retired) = imp.queued_plan.take() {
            self.retire_guarded_markdown(retired);
        }
    }
}
