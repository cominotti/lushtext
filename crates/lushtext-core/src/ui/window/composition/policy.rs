// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: pure policy of the window-composition workflow.
//!
//! Every decision about *which* composition stage runs *when* lives here: the
//! stage order, which stages a window schedules, which stage a staged surface
//! needs, which stages a forced reveal completes, what a painted frame decides,
//! how many activation opens one turn admits, and how a second window request
//! merges into the first. The coordination half (`execution`) owns the idle
//! sources, the frame-clock handler, and the widgets; it asks this module and
//! does what it says.
//!
//! The stage order is finite and small, so the unit tests below enumerate every
//! reachable plan instead of sampling it (design D3 of `first-frame-window-shell`
//! explains why Kani adds nothing here).

use crate::ui::window::session_restore::policy::SESSION_RESTORE_PAGES_PER_TURN;

/// One stage of a window's composition, in the order they run.
///
/// `Warm`, `Construct`, `Realize` and `Present` are **lifecycle** stages: they
/// run from successive idle dispatches and are never forced. `Content`,
/// `Workspace` and `Properties` are **reveal** stages: each makes one staged
/// surface visible, runs only after the frame showing its predecessor painted,
/// and may be forced by an operation that needs its surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompositionStage {
    /// W: one-time text and icon initialisation, once per process.
    Warm,
    /// C: `LushtextWindow::new`, the whole template with the staged content
    /// invisible, its wiring, the restores, and the breakpoints.
    Construct,
    /// R: `gtk_widget_realize`, which carries GTK's one-time first-realize cost.
    Realize,
    /// P: `present`, which validates the visible tree's CSS and maps it.
    Present,
    /// B1: the content stack (the tab view or the empty page) and the
    /// activation's file opens.
    Content,
    /// B2: the workspace sidebar, always: into a hidden rail when the
    /// restored layout hides it.
    Workspace,
    /// B3: the document-properties panel, always: into a hidden pane or sheet
    /// when the restored layout hides it.
    Properties,
}

impl CompositionStage {
    /// Every stage, in run order.
    pub const ALL: [Self; 7] = [
        Self::Warm,
        Self::Construct,
        Self::Realize,
        Self::Present,
        Self::Content,
        Self::Workspace,
        Self::Properties,
    ];

    /// The stage's bit in a [`StageSet`].
    const fn bit(self) -> u8 {
        1 << (self as u8)
    }

    /// Whether this stage reveals a staged surface (and may be forced).
    #[must_use]
    pub const fn is_reveal(self) -> bool {
        matches!(self, Self::Content | Self::Workspace | Self::Properties)
    }

    /// The stable label of the stage, used in evidence and documentation.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Warm => "warm",
            Self::Construct => "construct",
            Self::Realize => "realize",
            Self::Present => "present",
            Self::Content => "content",
            Self::Workspace => "workspace",
            Self::Properties => "properties",
        }
    }

    /// The measurement build's span name for this stage's dispatch; the
    /// responsiveness lane reads every `composition_<label>` span as one stage.
    #[must_use]
    pub const fn span_name(self) -> &'static str {
        match self {
            Self::Warm => "composition_warm",
            Self::Construct => "composition_construct",
            Self::Realize => "composition_realize",
            Self::Present => "composition_present",
            Self::Content => "composition_content",
            Self::Workspace => "composition_workspace",
            Self::Properties => "composition_properties",
        }
    }

    /// The reveal stage a reveal stage depends on, if any.
    ///
    /// The sidebar and the properties panel frame the content, so forcing
    /// either completes the content first: a forced reveal never shows a side
    /// rail beside a still-empty centre.
    const fn reveal_dependency(self) -> Option<Self> {
        match self {
            Self::Workspace | Self::Properties => Some(Self::Content),
            _ => None,
        }
    }
}

/// A set of composition stages, one bit each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StageSet(u8);

impl StageSet {
    /// No stage.
    pub const EMPTY: Self = Self(0);

    /// Whether `stage` is in the set.
    #[must_use]
    pub const fn contains(self, stage: CompositionStage) -> bool {
        self.0 & stage.bit() != 0
    }

    /// The set with `stage` added.
    #[must_use]
    pub const fn with(self, stage: CompositionStage) -> Self {
        Self(self.0 | stage.bit())
    }

    /// The stages in `self` and not in `other`.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// How many stages the set holds.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// Whether the set is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The set's stages, in run order.
    pub fn iter(self) -> impl Iterator<Item = CompositionStage> {
        CompositionStage::ALL
            .into_iter()
            .filter(move |stage| self.contains(*stage))
    }
}

/// Whether a window's composition blocks readiness: it began (stage C ran)
/// and neither completed nor left its application.
#[must_use]
pub const fn composition_blocks_readiness(begun: bool, complete: bool, torn_down: bool) -> bool {
    begun && !complete && !torn_down
}

/// What a window's composition plan is decided from, read once in stage C.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositionFacts {
    /// Whether this window's request ran stage W (the first window of the
    /// process created through the application's request path).
    pub warmed: bool,
}

/// Which stages a window schedules, and which have run.
///
/// Every reveal stage is scheduled, whether or not the restored layout shows
/// its surface: a sidebar or panel the layout hides is revealed into its hidden
/// rail, which draws nothing, so once composition completes the window is
/// exactly the one an unstaged construction built. A reveal stage may also run
/// earlier, forced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositionPlan {
    scheduled: StageSet,
    done: StageSet,
    forced: u32,
}

impl CompositionPlan {
    /// The plan of a window constructed from `facts`, with W (when it ran) and
    /// C already done: the plan exists only once the window does.
    #[must_use]
    pub fn new(facts: CompositionFacts) -> Self {
        let mut scheduled = StageSet::EMPTY
            .with(CompositionStage::Construct)
            .with(CompositionStage::Realize)
            .with(CompositionStage::Present)
            .with(CompositionStage::Content)
            .with(CompositionStage::Workspace)
            .with(CompositionStage::Properties);
        let mut done = StageSet::EMPTY.with(CompositionStage::Construct);
        if facts.warmed {
            scheduled = scheduled.with(CompositionStage::Warm);
            done = done.with(CompositionStage::Warm);
        }
        Self {
            scheduled,
            done,
            forced: 0,
        }
    }

    /// The stages this window schedules.
    #[must_use]
    pub const fn scheduled(self) -> StageSet {
        self.scheduled
    }

    /// The stages that have run, scheduled or forced.
    #[must_use]
    pub const fn done(self) -> StageSet {
        self.done
    }

    /// Whether `stage` has run.
    #[must_use]
    pub const fn is_done(self, stage: CompositionStage) -> bool {
        self.done.contains(stage)
    }

    /// Record that `stage` ran; `false` when it already had.
    pub fn mark_done(&mut self, stage: CompositionStage) -> bool {
        if self.done.contains(stage) {
            return false;
        }
        self.done = self.done.with(stage);
        true
    }

    /// Record that `stage` ran because an operation forced it.
    pub fn mark_forced(&mut self, stage: CompositionStage) -> bool {
        let newly = self.mark_done(stage);
        if newly {
            self.forced = self.forced.saturating_add(1);
        }
        newly
    }

    /// Scheduled stages still to run.
    #[must_use]
    pub const fn pending(self) -> StageSet {
        self.scheduled.without(self.done)
    }

    /// How many stages this window schedules.
    #[must_use]
    pub const fn stages_total(self) -> u32 {
        self.scheduled.len()
    }

    /// How many scheduled stages are still to run.
    #[must_use]
    pub const fn stages_pending(self) -> u32 {
        self.pending().len()
    }

    /// How many stages were forced ahead of their schedule, or without one.
    #[must_use]
    pub const fn forced_stages(self) -> u32 {
        self.forced
    }

    /// The next scheduled reveal stage, once the window is presented.
    #[must_use]
    pub fn next_scheduled_reveal(self) -> Option<CompositionStage> {
        if !self.done.contains(CompositionStage::Present) {
            return None;
        }
        self.pending().iter().find(|stage| stage.is_reveal())
    }

    /// The stages `ensure_composed(target)` runs now, in order: `target`'s
    /// reveal dependency and `target` itself, each only if it has not run.
    /// A lifecycle stage is never forced, so its chain is empty.
    #[must_use]
    pub fn forced_chain(self, target: CompositionStage) -> StageSet {
        if !target.is_reveal() {
            return StageSet::EMPTY;
        }
        let mut chain = StageSet::EMPTY.with(target);
        if let Some(dependency) = target.reveal_dependency() {
            chain = chain.with(dependency);
        }
        chain.without(self.done)
    }

    /// Whether every scheduled stage has run. Composition is *complete* only
    /// once the frame after the last one has also painted
    /// ([`frame_painted_decision`]).
    #[must_use]
    pub const fn all_scheduled_done(self) -> bool {
        self.pending().is_empty()
    }
}

/// A staged surface an operation needs visible, focusable, or allocated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StagedSurface {
    /// The content stack: the tab view and its editors, or the empty page.
    Content,
    /// The workspace sidebar.
    Workspace,
    /// The document-properties panel.
    Properties,
}

/// The reveal stage that makes `surface` visible.
#[must_use]
pub const fn stage_for_surface(surface: StagedSurface) -> CompositionStage {
    match surface {
        StagedSurface::Content => CompositionStage::Content,
        StagedSurface::Workspace => CompositionStage::Workspace,
        StagedSurface::Properties => CompositionStage::Properties,
    }
}

/// What the coordination half observed when the window's frame painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FramePaintedFacts {
    /// Whether a stage dispatch is already queued.
    pub dispatch_queued: bool,
    /// The stage a test policy holds composition before, if any: no reveal at
    /// or after it is queued until the hold is released.
    pub hold_before: Option<CompositionStage>,
    /// Whether composition already completed.
    pub complete: bool,
}

/// What a painted frame decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FramePaintedDecision {
    /// Nothing to do: not presented yet, already queued, held, or complete.
    Wait,
    /// Queue `stage`'s dispatch: the frame showing its predecessor painted.
    Queue(CompositionStage),
    /// Every scheduled stage ran and this frame showed the last of them.
    Complete,
}

/// Decide what one painted frame does to composition.
///
/// A reveal stage runs only after a frame painted, so no frame carries two
/// stages' first-measure work; composition completes on the first frame painted
/// after every scheduled stage ran (each reveal queues the frame that shows it).
#[must_use]
pub fn frame_painted_decision(
    plan: CompositionPlan,
    facts: FramePaintedFacts,
) -> FramePaintedDecision {
    if facts.complete || facts.dispatch_queued || !plan.is_done(CompositionStage::Present) {
        return FramePaintedDecision::Wait;
    }
    if let Some(stage) = plan.next_scheduled_reveal() {
        if facts.hold_before.is_some_and(|hold| stage >= hold) {
            return FramePaintedDecision::Wait;
        }
        return FramePaintedDecision::Queue(stage);
    }
    if plan.all_scheduled_done() {
        FramePaintedDecision::Complete
    } else {
        FramePaintedDecision::Wait
    }
}

/// How many queued activation opens one turn admits: the session restore's
/// page turn (`SESSION_RESTORE_PAGES_PER_TURN`), so a command line naming many
/// files opens in bounded dispatches exactly as a restored session does.
#[must_use]
pub fn activation_opens_this_turn(queued: usize) -> usize {
    queued.min(SESSION_RESTORE_PAGES_PER_TURN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_facts() -> impl Iterator<Item = CompositionFacts> {
        [false, true]
            .into_iter()
            .map(|warmed| CompositionFacts { warmed })
    }

    /// Every plan reachable from a constructed window: every subset of the
    /// stages after C may have run, scheduled or forced.
    fn reachable_plans() -> impl Iterator<Item = CompositionPlan> {
        all_facts().flat_map(|facts| {
            (0..(1u8 << 7)).filter_map(move |extra| {
                let mut plan = CompositionPlan::new(facts);
                for stage in CompositionStage::ALL {
                    if extra & stage.bit() != 0 {
                        if stage == CompositionStage::Warm && !facts.warmed {
                            return None;
                        }
                        if stage.is_reveal() {
                            plan.mark_forced(stage);
                        } else {
                            plan.mark_done(stage);
                        }
                    }
                }
                Some(plan)
            })
        })
    }

    #[test]
    fn a_new_plan_schedules_every_stage_but_an_unrun_warm() {
        for facts in all_facts() {
            let plan = CompositionPlan::new(facts);
            let scheduled = plan.scheduled();
            assert!(scheduled.contains(CompositionStage::Construct));
            assert!(scheduled.contains(CompositionStage::Realize));
            assert!(scheduled.contains(CompositionStage::Present));
            assert!(scheduled.contains(CompositionStage::Content));
            assert_eq!(scheduled.contains(CompositionStage::Warm), facts.warmed);
            assert!(scheduled.contains(CompositionStage::Workspace));
            assert!(scheduled.contains(CompositionStage::Properties));
            assert!(plan.is_done(CompositionStage::Construct));
            assert_eq!(plan.is_done(CompositionStage::Warm), facts.warmed);
            assert_eq!(
                plan.stages_pending(),
                plan.stages_total() - plan.done().len()
            );
            assert_eq!(plan.forced_stages(), 0);
        }
    }

    #[test]
    fn no_reveal_is_scheduled_before_the_window_is_presented() {
        for plan in reachable_plans() {
            if !plan.is_done(CompositionStage::Present) {
                assert_eq!(plan.next_scheduled_reveal(), None);
                assert_eq!(
                    frame_painted_decision(
                        plan,
                        FramePaintedFacts {
                            dispatch_queued: false,
                            hold_before: None,
                            complete: false,
                        }
                    ),
                    FramePaintedDecision::Wait
                );
            }
        }
    }

    #[test]
    fn the_next_reveal_is_the_first_pending_scheduled_reveal_in_run_order() {
        for plan in reachable_plans() {
            let expected = plan
                .is_done(CompositionStage::Present)
                .then(|| {
                    CompositionStage::ALL
                        .into_iter()
                        .find(|stage| stage.is_reveal() && plan.pending().contains(*stage))
                })
                .flatten();
            assert_eq!(plan.next_scheduled_reveal(), expected, "{plan:?}");
            if let Some(stage) = expected {
                assert!(plan.scheduled().contains(stage) && !plan.is_done(stage));
            }
        }
    }

    #[test]
    fn a_forced_chain_holds_only_reveals_that_have_not_run_and_their_dependency() {
        for plan in reachable_plans() {
            for target in CompositionStage::ALL {
                let chain = plan.forced_chain(target);
                if !target.is_reveal() {
                    assert!(chain.is_empty(), "{target:?} is never forced");
                    continue;
                }
                assert!(chain.iter().all(CompositionStage::is_reveal));
                assert!(plan.done().iter().all(|stage| !chain.contains(stage)));
                assert_eq!(chain.contains(target), !plan.is_done(target));
                if target != CompositionStage::Content {
                    assert_eq!(
                        chain.contains(CompositionStage::Content),
                        !plan.is_done(CompositionStage::Content)
                    );
                }
                // Running the chain in order leaves the target and its
                // dependency done, whatever was done before.
                let mut after = plan;
                for stage in chain.iter() {
                    assert!(after.mark_forced(stage));
                }
                assert!(after.is_done(target));
                assert!(after.forced_chain(target).is_empty());
                assert_eq!(after.forced_stages(), plan.forced_stages() + chain.len());
            }
        }
    }

    #[test]
    fn a_painted_frame_queues_the_next_reveal_or_completes() {
        let mut holds = vec![None];
        holds.extend(CompositionStage::ALL.into_iter().map(Some));
        let cases: Vec<(u8, Option<CompositionStage>)> = (0..4u8)
            .flat_map(|bits| holds.iter().map(move |hold| (bits, *hold)))
            .collect();
        for plan in reachable_plans() {
            for &(bits, hold_before) in &cases {
                let facts = FramePaintedFacts {
                    dispatch_queued: bits & 1 != 0,
                    hold_before,
                    complete: bits & 2 != 0,
                };
                let decision = frame_painted_decision(plan, facts);
                let presented = plan.is_done(CompositionStage::Present);
                if facts.complete || facts.dispatch_queued || !presented {
                    assert_eq!(decision, FramePaintedDecision::Wait);
                    continue;
                }
                match plan.next_scheduled_reveal() {
                    Some(stage) if hold_before.is_some_and(|hold| stage >= hold) => {
                        assert_eq!(decision, FramePaintedDecision::Wait);
                    }
                    Some(stage) => assert_eq!(decision, FramePaintedDecision::Queue(stage)),
                    None if plan.all_scheduled_done() => {
                        assert_eq!(decision, FramePaintedDecision::Complete);
                    }
                    None => assert_eq!(decision, FramePaintedDecision::Wait),
                }
            }
        }
    }

    #[test]
    fn marking_twice_is_idempotent_and_counts_a_forced_stage_once() {
        let mut plan = CompositionPlan::new(CompositionFacts { warmed: false });
        assert!(plan.mark_forced(CompositionStage::Properties));
        assert!(!plan.mark_forced(CompositionStage::Properties));
        assert!(!plan.mark_done(CompositionStage::Properties));
        assert_eq!(plan.forced_stages(), 1);
        assert!(
            plan.forced_chain(CompositionStage::Workspace)
                .contains(CompositionStage::Content)
        );
    }

    #[test]
    fn a_stage_set_iterates_in_run_order() {
        let set = StageSet::EMPTY
            .with(CompositionStage::Properties)
            .with(CompositionStage::Warm)
            .with(CompositionStage::Content);
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            [
                CompositionStage::Warm,
                CompositionStage::Content,
                CompositionStage::Properties
            ]
        );
        assert_eq!(set.len(), 3);
        assert!(StageSet::EMPTY.iter().next().is_none());
    }

    #[test]
    fn surfaces_map_to_their_reveal_stage() {
        assert_eq!(
            stage_for_surface(StagedSurface::Content),
            CompositionStage::Content
        );
        assert_eq!(
            stage_for_surface(StagedSurface::Workspace),
            CompositionStage::Workspace
        );
        assert_eq!(
            stage_for_surface(StagedSurface::Properties),
            CompositionStage::Properties
        );
        for stage in CompositionStage::ALL {
            assert!(stage.span_name().ends_with(stage.label()));
            assert!(stage.span_name().starts_with("composition_"));
        }
    }

    #[test]
    fn an_activation_turn_is_the_session_restore_page_turn() {
        assert_eq!(activation_opens_this_turn(0), 0);
        assert_eq!(activation_opens_this_turn(1), 1);
        assert_eq!(
            activation_opens_this_turn(SESSION_RESTORE_PAGES_PER_TURN),
            SESSION_RESTORE_PAGES_PER_TURN
        );
        assert_eq!(
            activation_opens_this_turn(SESSION_RESTORE_PAGES_PER_TURN * 10 + 1),
            SESSION_RESTORE_PAGES_PER_TURN
        );
    }
}
