// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani model of the adaptive-shell breakpoint loop: allocated width -> layout
//! -> breakpoint threshold -> breakpoint application -> layout.
//!
//! The model runs the production decisions — [`derive_adaptive_shell_layout`]
//! and [`plan_shell_reconciliation`] — and mirrors only the GTK sequencing
//! around them, the way the slice-bin model mirrors `size_allocate`:
//!
//! * `allocate(w)` is `imp.rs::size_allocate`: the parent allocation lets the
//!   breakpoint bin re-evaluate its breakpoints, then
//!   `sync_split_view_widths_for_allocation` runs the breakpoint and surface
//!   reconciliation once per new width, unless the sidebar-transition settle is
//!   pending (the `synced_for_width` guard is set either way);
//! * `settle()` is that settle burst's completion
//!   (`sync_secondary_surface_layout_now`);
//! * the `notify::layout-name` handler reconciles the surfaces (ungated), and
//!   the workspace split view's `notify::show-sidebar` and `notify::collapsed`
//!   handlers reinstall the breakpoint threshold unless a settle is pending.
//!
//! Split-view fractions, focus restoration, and action states are not
//! modelled: they write nothing the loop reads.
//!
//! # Envelope
//!
//! Adwaita is restricted only by these assumptions, each citing its row in
//! `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`.
//! **Every one is an A14–A18 fact**, pinned by the `gtk-lush-axioms` probes on
//! GTK 4.22.5 / Libadwaita 1.9.3; the model was first written with the wider
//! alternatives design D5 lists and narrowed to what the probes measured.
//!
//! * **A14** (condition units). `max-width: Nsp` holds at an allocated window
//!   width `w` px iff `w <= N * s` (inclusive), where `s` is the text scale
//!   (`gtk-xft-dpi / 98304`). The harnesses take `s` from {1, 1.25, 1.5, 2}.
//!   The policy compares `w` with whole sp, so the two agree only at `s = 1`.
//! * **A15** (re-evaluation timing). `set_condition` changes nothing
//!   synchronously: the bin schedules its own allocation and re-evaluates there.
//!   Every allocation re-evaluates every breakpoint with its installed
//!   condition, inside the parent allocation, before LushText's own
//!   `size_allocate` code runs. The harnesses run the allocation a
//!   `set_condition` schedules.
//! * **A16** (setters). Setters apply inside the bin's allocation (after it
//!   allocated its child for that frame), and on unapply restore the value the
//!   property had when the setter was added: `layout-name = pane`,
//!   `collapsed = false`. Every later write is lost on unapply. A switch
//!   unapplies the old breakpoint before it applies the new one, so a property
//!   both set ends at the new value (observed by
//!   `shell_geometry::test_doubled_text_scale_keeps_the_workspace_collapsed_at_the_minimum_width`,
//!   being folded into the A16 row). Property writes notify synchronously, and
//!   only when the value changes. `apply` and `unapply` are emitted on every
//!   switch.
//! * **A17** (split views). `show-sidebar` and `collapsed` never change the
//!   toplevel's allocated width, so the width is the harness's choice alone;
//!   with `pin-sidebar = true`, `collapsed` does not write `show-sidebar`; a
//!   bottom sheet with `can-open` and `can-close` false never changes `open` by
//!   itself.
//! * **A18** (minimum width and selection). Breakpoints lower the window's
//!   minimum to its `width-request` (640 px), so the domain is [640, 2560] px.
//!   When several breakpoints match, **only the last added applies**; they are
//!   added properties, workspace, Open button (`max-width: 400sp`), so the
//!   Open-button breakpoint, current at `s = 2` and 640 px, unapplies the other
//!   two.
//!
//! Notify handlers run nested at the write that raised them, as GObject runs
//! them; the recursion is at most three frames deep (`MAX_NOTIFY_DEPTH` guards
//! it, and the unwind bound would report a deeper one).
//!
//! # The design under check
//!
//! [`LayoutWriter::Setter`] is the design before `extend-closed-loop-geometry-
//! verification`: the properties breakpoint carried a `layout-name = sheet`
//! setter, a second writer of the property the reconciliation writes. The
//! breakpoint loop harnesses found it flaps (see
//! `shell_loop_layout_setter_flaps`), and the shipped design is
//! [`LayoutWriter::Trigger`]: the breakpoint has no setter, and its `apply` and
//! `unapply` signals run the reconciliation, so the plan is the only writer.
//! The same harnesses found the Open-button breakpoint, current alone at text
//! scales of 1.6 and up at the narrowest windows (A18), displacing the
//! workspace breakpoint's `collapsed = true` (see
//! `shell_loop_open_button_breakpoint_uncollapsed_the_workspace`); it now
//! carries that setter too (`open_button_collapses`).
//!
//! # Harnesses
//!
//! Proofs: `shell_loop_settles_at_a_stable_width`,
//! `shell_loop_sweep_does_not_flap`, `shell_loop_preserves_requested_visibility`,
//! `shell_loop_layout_agrees_with_policy_at_rest`,
//! `shell_loop_allocation_never_persists`,
//! `shell_loop_workspace_collapses_with_its_breakpoint`, and the step lemma
//! `shell_loop_layout_is_stable_under_its_own_compact_slot`. Pinned
//! counterexamples of the designs before this change (`should_panic`):
//! `shell_loop_layout_setter_flaps` and
//! `shell_loop_open_button_breakpoint_uncollapsed_the_workspace`. The sweeps
//! start from [`Shell::rested`], the state the agreement harness proves every
//! allocation reaches, and every step derives its layout once (`Shell::step`,
//! justified by the step lemma); both keep each harness to a few GiB.
//!
//! Compiled only under `cfg(kani)` (`make kani`); ordinary builds and tests
//! never see this module.

use super::policy::{
    AdaptiveShellInputs, AdaptiveShellLayout, OPEN_BUTTON_BREAKPOINT_MAX_WIDTH_SP,
    PropertiesPresentation, RenderedShellState, SecondarySurface, ShellWrite,
    WORKSPACE_BREAKPOINT_MAX_WIDTH_SP, derive_adaptive_shell_layout, plan_shell_reconciliation,
};
use crate::ui::sidebar::width_preset::WorkspaceSidebarWidthPreset;

/// A layout no step reconciles towards: `step` replaces it before any does.
const PLACEHOLDER_LAYOUT: AdaptiveShellLayout = AdaptiveShellLayout {
    properties_breakpoint_max_width: 0,
    workspace_consumes_width: false,
    properties_presentation: PropertiesPresentation::Pane,
    compact_surface: None,
    render_workspace: false,
    render_properties: false,
};

/// The window's `width-request`: the narrowest allocation (A18).
const MIN_WINDOW_WIDTH: i32 = 640;
/// The widest allocation the harnesses take.
const MAX_WINDOW_WIDTH: i32 = 2560;
/// The deepest nesting of notify handlers the loop may reach before it is
/// treated as non-terminating.
const MAX_NOTIFY_DEPTH: u8 = 5;

/// A text scale factor `num / den` (A14).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextScale {
    num: i64,
    den: i64,
}

impl TextScale {
    /// Whether `max-width: {max_width_sp}sp` holds at `width_px` (A14).
    fn holds(self, width_px: i32, max_width_sp: i32) -> bool {
        i64::from(width_px) * self.den <= i64::from(max_width_sp) * self.num
    }
}

/// Regular text, Large Text, 1.5, and 2 (A14; 2 reaches the Open-button breakpoint, A18).
fn any_text_scale() -> TextScale {
    let choice: u8 = kani::any();
    kani::assume(choice < 4);
    match choice {
        0 => TextScale { num: 1, den: 1 },
        1 => TextScale { num: 5, den: 4 },
        2 => TextScale { num: 3, den: 2 },
        _ => TextScale { num: 2, den: 1 },
    }
}

/// Any allocatable window width (A18).
fn any_width() -> i32 {
    let width: i32 = kani::any();
    kani::assume((MIN_WINDOW_WIDTH..=MAX_WINDOW_WIDTH).contains(&width));
    width
}

/// The design the proof harnesses check: the shipped one.
const SHIPPED: LayoutWriter = LayoutWriter::Trigger;

/// Who writes `layout-name` when the properties breakpoint switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayoutWriter {
    /// A `layout-name = sheet` setter on the properties breakpoint.
    Setter,
    /// No setter; `apply` and `unapply` run the reconciliation.
    Trigger,
}

/// The window's breakpoints, in the order they are added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, kani::Arbitrary)]
enum Breakpoint {
    Properties,
    Workspace,
    OpenButton,
}

/// The breakpoint bin's own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Adwaita {
    /// The current breakpoint.
    current: Option<Breakpoint>,
    /// The properties breakpoint's installed condition, `max-width: Nsp`.
    condition_threshold: i32,
    /// `workspace_split_view.collapsed`.
    collapsed: bool,
}

/// The persisted intent: the GSettings keys the allocation path must not write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Intent {
    preset: WorkspaceSidebarWidthPreset,
    workspace_requested: bool,
    properties_requested: bool,
    focus_mode: bool,
}

/// The shell: intent, rendered widgets, the cached threshold, the allocation
/// guard, and the breakpoint bin, plus ghost counters.
#[derive(Clone, Copy, Debug)]
struct Shell {
    writer: LayoutWriter,
    /// Whether the Open-button breakpoint also sets `collapsed = true`.
    open_button_collapses: bool,
    scale: TextScale,
    intent: Intent,
    /// `imp.secondary_surfaces` and the widgets, as the plan reads them.
    rendered: RenderedShellState,
    /// `imp.properties_breakpoint_max_width`.
    installed_threshold: i32,
    /// `window.width()`.
    width: i32,
    /// `imp.split_width_synced_for_width`.
    synced_for_width: i32,
    /// `workspace_sidebar_transition_settle.pending()`.
    settle_pending: bool,
    adwaita: Adwaita,
    /// The layout the current step reconciles towards: derived once per
    /// allocation or settle, when the step starts (see `step`).
    step_layout: AdaptiveShellLayout,
    /// Ghost: `layout-name` changes in the current allocation.
    presentation_changes: u8,
    /// Ghost: `set_condition` calls in the current allocation.
    reinstalls: u8,
    /// Ghost: the current nesting of notify handlers.
    depth: u8,
}

/// What must not change once the shell is at rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RestState {
    rendered: RenderedShellState,
    installed_threshold: i32,
    adwaita: Adwaita,
    synced_for_width: i32,
}

impl Shell {
    /// Any shell a previous width `previous_width` could have left, about to be
    /// allocated at a different width: arbitrary intent and rendered state, a
    /// cached threshold equal to the installed condition, and a breakpoint bin
    /// that evaluated its conditions at `previous_width`.
    fn any(writer: LayoutWriter, next_width: i32) -> Self {
        let previous_width = any_width();
        let installed_threshold: i32 = kani::any();
        kani::assume((0..=4096).contains(&installed_threshold));
        let synced_for_width: i32 = kani::any();
        kani::assume(synced_for_width != next_width);
        let scale = any_text_scale();
        let mut shell = Self {
            writer,
            open_button_collapses: true,
            scale,
            intent: Intent {
                preset: kani::any(),
                workspace_requested: kani::any(),
                properties_requested: kani::any(),
                focus_mode: kani::any(),
            },
            rendered: kani::any(),
            installed_threshold,
            width: previous_width,
            synced_for_width,
            settle_pending: kani::any(),
            adwaita: Adwaita {
                current: None,
                condition_threshold: installed_threshold,
                collapsed: false,
            },
            step_layout: PLACEHOLDER_LAYOUT,
            presentation_changes: 0,
            reinstalls: 0,
            depth: 0,
        };
        let current = shell.selected_breakpoint();
        shell.adwaita.current = current;
        shell.adwaita.collapsed = shell.collapsed_by(current);
        shell
    }

    /// Any shell at rest at `width`: the state `shell_loop_layout_agrees_with_policy_at_rest`
    /// proves every allocation reaches. Arbitrary intent, text scale, and
    /// compact slot; every rendered surface, the cached threshold, and the
    /// installed condition are what the policy derives, the breakpoint bin has
    /// evaluated at `width`, and no settle is pending. Starting the sweeps here
    /// rather than from `any` is the compositional step that keeps them inside
    /// the solver's budget: the rest they would otherwise re-derive is its own
    /// harness.
    fn rested(writer: LayoutWriter, width: i32) -> Self {
        let mut shell = Self::any(writer, width);
        shell.width = width;
        shell.synced_for_width = width;
        shell.settle_pending = false;
        let layout = shell.layout();
        let sheet = layout.properties_presentation == PropertiesPresentation::Sheet;
        shell.rendered = RenderedShellState {
            presentation: layout.properties_presentation,
            compact_surface: layout.compact_surface,
            workspace_shows_sidebar: layout.render_workspace,
            properties_shows_sidebar: !sheet && layout.render_properties,
            sheet_open: sheet && layout.render_properties,
        };
        let layout = shell.layout();
        shell.installed_threshold = layout.properties_breakpoint_max_width;
        shell.adwaita.condition_threshold = shell.installed_threshold;
        let current = shell.selected_breakpoint();
        shell.adwaita.current = current;
        shell.adwaita.collapsed = shell.collapsed_by(current);
        kani::assume(shell.agrees_with_policy());
        shell
    }

    /// What `collapsed` is while `current` is applied (A16, A18).
    fn collapsed_by(&self, current: Option<Breakpoint>) -> bool {
        match current {
            Some(Breakpoint::Workspace) => true,
            Some(Breakpoint::OpenButton) => self.open_button_collapses,
            Some(Breakpoint::Properties) | None => false,
        }
    }

    fn inputs(&self) -> AdaptiveShellInputs {
        AdaptiveShellInputs {
            window_width: self.width,
            workspace_preset: self.intent.preset,
            workspace_requested_visible: self.intent.workspace_requested,
            properties_requested_visible: self.intent.properties_requested,
            compact_surface: self.rendered.compact_surface,
            focus_mode_active: self.intent.focus_mode,
        }
    }

    /// Start a step: derive the layout it reconciles towards, once.
    ///
    /// Within one allocation or settle the width and intent are fixed, and the
    /// only input a reconciliation changes is the compact slot, which it sets
    /// to the layout's own slot; `shell_loop_layout_is_stable_under_its_own_compact_slot`
    /// proves re-deriving from that slot yields the same layout. So every
    /// reconciliation in the step would derive this layout, and deriving it
    /// once keeps the solver's formula to one copy of the policy per step.
    fn step(&mut self) {
        self.step_layout = self.layout();
    }

    fn layout(&self) -> AdaptiveShellLayout {
        derive_adaptive_shell_layout(self.inputs())
    }

    fn rest_state(&self) -> RestState {
        RestState {
            rendered: self.rendered,
            installed_threshold: self.installed_threshold,
            adwaita: self.adwaita,
            synced_for_width: self.synced_for_width,
        }
    }

    /// Whether every rendered surface, the cached threshold, and the installed
    /// condition are what the policy derives for the current width and intent.
    fn agrees_with_policy(&self) -> bool {
        let layout = self.layout();
        let sheet = layout.properties_presentation == PropertiesPresentation::Sheet;
        let rendered = self.rendered;
        rendered.presentation == layout.properties_presentation
            && rendered.compact_surface == layout.compact_surface
            && rendered.workspace_shows_sidebar == layout.render_workspace
            && rendered.properties_shows_sidebar == (!sheet && layout.render_properties)
            && rendered.sheet_open == (sheet && layout.render_properties)
            && self.installed_threshold == layout.properties_breakpoint_max_width
            && self.adwaita.condition_threshold == self.installed_threshold
    }

    fn enter(&mut self) {
        self.depth += 1;
        assert!(
            self.depth <= MAX_NOTIFY_DEPTH,
            "the notify handlers recurse without terminating"
        );
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    // --- LushText: execution.rs and the imp.rs notify handlers ---------------

    /// `execution::sync_secondary_surfaces`: the plan's surface writes, in order.
    fn sync_secondary_surfaces(&mut self) {
        self.enter();
        let plan =
            plan_shell_reconciliation(self.rendered, self.step_layout, self.installed_threshold);
        // Written out rather than looped, and only the first slot dispatched to
        // the `layout-name` write (the one that notifies into a nested
        // reconciliation), so the recursion is a chain, not a tree. The plan
        // puts `layout-name` first by construction; the other slots assert it.
        let [first, second, third, fourth, fifth] = plan.surface_writes();
        match first {
            Some(ShellWrite::LayoutName(presentation)) => self.set_layout_name(presentation),
            Some(write) => self.apply_non_layout_write(write),
            None => {}
        }
        self.apply_later_slot(second);
        self.apply_later_slot(third);
        self.apply_later_slot(fourth);
        self.apply_later_slot(fifth);
        self.leave();
    }

    fn apply_later_slot(&mut self, slot: Option<ShellWrite>) {
        if let Some(write) = slot {
            self.apply_non_layout_write(write);
        }
    }

    /// `execution::apply_shell_write` for every write but `layout-name`.
    fn apply_non_layout_write(&mut self, write: ShellWrite) {
        match write {
            ShellWrite::LayoutName(_) => {
                panic!("the plan orders `layout-name` before every other write")
            }
            ShellWrite::CompactSurface(surface) => self.rendered.compact_surface = surface,
            ShellWrite::WorkspaceShowSidebar(show) => self.set_workspace_show_sidebar(show),
            ShellWrite::PropertiesShowSidebar(show) => {
                self.rendered.properties_shows_sidebar = show;
            }
            ShellWrite::SheetOpen(open) => self.rendered.sheet_open = open,
        }
    }

    /// `execution::sync_properties_breakpoint`: `set_condition` only when the
    /// integer threshold moved, and the cache is stored after it returns.
    fn sync_properties_breakpoint(&mut self) {
        self.enter();
        let plan =
            plan_shell_reconciliation(self.rendered, self.step_layout, self.installed_threshold);
        if let Some(threshold) = plan.reinstall_threshold {
            self.adwaita.condition_threshold = threshold;
            self.reinstalls = self.reinstalls.saturating_add(1);
            // A15: `set_condition` re-evaluates nothing synchronously; the bin
            // schedules its own allocation, where `evaluate_breakpoints` runs.
            self.installed_threshold = threshold;
        }
        self.leave();
    }

    /// `layout-name` changes notify; the handler reconciles the surfaces (A16).
    fn set_layout_name(&mut self, presentation: PropertiesPresentation) {
        if self.rendered.presentation != presentation {
            self.rendered.presentation = presentation;
            self.presentation_changes = self.presentation_changes.saturating_add(1);
            self.sync_secondary_surfaces();
        }
    }

    /// The workspace `notify::show-sidebar` handler reinstalls the threshold.
    fn set_workspace_show_sidebar(&mut self, show: bool) {
        if self.rendered.workspace_shows_sidebar != show {
            self.rendered.workspace_shows_sidebar = show;
            if !self.settle_pending {
                self.sync_properties_breakpoint();
            }
        }
    }

    /// The `notify::collapsed` handler reinstalls the threshold.
    fn set_collapsed(&mut self, collapsed: bool) {
        if self.adwaita.collapsed != collapsed {
            self.adwaita.collapsed = collapsed;
            if !self.settle_pending {
                self.sync_properties_breakpoint();
            }
        }
    }

    /// `imp.rs::size_allocate` at width `width`.
    fn allocate(&mut self, width: i32) {
        self.width = width;
        self.step();
        self.presentation_changes = 0;
        self.reinstalls = 0;
        // `parent_size_allocate`: the breakpoint bin re-evaluates (A15).
        self.evaluate_breakpoints();
        // `sync_split_view_widths_for_allocation`.
        if self.synced_for_width != width {
            if !self.settle_pending {
                self.sync_properties_breakpoint();
                self.sync_secondary_surfaces();
            }
            self.synced_for_width = width;
        }
    }

    /// The sidebar-transition settle burst completes.
    fn settle(&mut self) {
        self.settle_pending = false;
        self.step();
        self.sync_properties_breakpoint();
        self.sync_secondary_surfaces();
    }

    /// Allocate at `width` until rest: the allocation itself, the settle burst
    /// if one was pending, and the allocation a deferred `set_condition` leaves
    /// behind (A15).
    fn allocate_to_rest(&mut self, width: i32) {
        self.allocate(width);
        if self.settle_pending {
            self.settle();
        }
        self.allocate(width);
    }

    // --- Adwaita: the breakpoint bin and its setters -------------------------

    /// The last added breakpoint whose condition holds (A14, A16).
    fn selected_breakpoint(&self) -> Option<Breakpoint> {
        let width = self.width;
        if self.scale.holds(width, OPEN_BUTTON_BREAKPOINT_MAX_WIDTH_SP) {
            Some(Breakpoint::OpenButton)
        } else if self.scale.holds(width, WORKSPACE_BREAKPOINT_MAX_WIDTH_SP) {
            Some(Breakpoint::Workspace)
        } else if self.scale.holds(width, self.adwaita.condition_threshold) {
            Some(Breakpoint::Properties)
        } else {
            None
        }
    }

    /// Re-evaluate the conditions and switch the current breakpoint (A16).
    fn evaluate_breakpoints(&mut self) {
        self.enter();
        let target = self.selected_breakpoint();
        let from = self.adwaita.current;
        if target != from {
            self.adwaita.current = target;
            // A16: the old breakpoint unapplies before the new one applies, so a
            // property both set ends at the new value (observed by
            // `shell_geometry::test_doubled_text_scale_keeps_the_workspace_collapsed_at_the_minimum_width`).
            self.unapply(from);
            self.apply(target);
        }
        self.leave();
    }

    fn apply(&mut self, breakpoint: Option<Breakpoint>) {
        match breakpoint {
            Some(Breakpoint::Properties) => match self.writer {
                LayoutWriter::Setter => {
                    // A16: the restore value is the install-time `pane`.
                    self.set_layout_name(PropertiesPresentation::Sheet);
                }
                LayoutWriter::Trigger => self.sync_secondary_surfaces(),
            },
            Some(Breakpoint::Workspace) => self.set_collapsed(true),
            Some(Breakpoint::OpenButton) if self.open_button_collapses => self.set_collapsed(true),
            Some(Breakpoint::OpenButton) | None => {}
        }
    }

    fn unapply(&mut self, breakpoint: Option<Breakpoint>) {
        match breakpoint {
            Some(Breakpoint::Properties) => match self.writer {
                // A16: unapply restores the install-time value, `pane`.
                LayoutWriter::Setter => self.set_layout_name(PropertiesPresentation::Pane),
                LayoutWriter::Trigger => self.sync_secondary_surfaces(),
            },
            // A16: and `collapsed = false`.
            Some(Breakpoint::Workspace) => self.set_collapsed(false),
            Some(Breakpoint::OpenButton) if self.open_button_collapses => self.set_collapsed(false),
            Some(Breakpoint::OpenButton) | None => {}
        }
    }
}

/// Three allocatable widths in one direction, all shrinking or all growing.
fn any_monotone_sweep() -> [i32; 3] {
    let widths = [any_width(), any_width(), any_width()];
    let shrinking: bool = kani::any();
    if shrinking {
        kani::assume(widths[0] > widths[1] && widths[1] > widths[2]);
    } else {
        kani::assume(widths[0] < widths[1] && widths[1] < widths[2]);
    }
    widths
}

/// A shell at rest at the first width of a monotone sweep, swept across the
/// other two; returns the rested presentation after each width and asserts no
/// allocation changes `layout-name` more than once.
fn sweep(writer: LayoutWriter) -> [PropertiesPresentation; 3] {
    let widths = any_monotone_sweep();
    let mut shell = Shell::rested(writer, widths[0]);
    let mut rested = [shell.rendered.presentation; 3];
    for (index, width) in widths.into_iter().enumerate().skip(1) {
        shell.allocate(width);
        assert!(
            shell.presentation_changes <= 1,
            "one allocation flips the presentation back and forth"
        );
        shell.allocate(width);
        assert!(
            shell.presentation_changes <= 1,
            "one allocation flips the presentation back and forth"
        );
        rested[index] = shell.rendered.presentation;
        kani::cover!(
            rested[index] != rested[index - 1],
            "the sweep crosses between pane and sheet"
        );
    }
    rested
}

/// **Rest.** From any shell, allocating a new width reaches a fixed point: after
/// the allocation, its settle, and one more allocation at the same width, a
/// further allocation changes no state, reparses no condition, and writes no
/// `layout-name`.
///
/// Domain: every width in [640, 2560] px, every preset, requested visibility,
/// Focus Mode, compact slot, and rendered state, every text scale in
/// {1, 1.25, 1.5, 2}, an arbitrary previous width and cached threshold in
/// [0, 4096] sp, and a settle burst pending or not. Bound: three allocations,
/// one settle, notify recursion three frames deep.
#[kani::proof]
#[kani::unwind(3)]
fn shell_loop_settles_at_a_stable_width() {
    let width = any_width();
    let mut shell = Shell::any(SHIPPED, width);
    shell.allocate_to_rest(width);
    let rested = shell.rest_state();
    shell.allocate(width);
    assert!(shell.rest_state() == rested, "a rested width moves again");
    assert!(shell.presentation_changes == 0 && shell.reinstalls == 0);
}

/// **No flapping.** Across a monotone sweep of three widths, no allocation
/// changes `layout-name` more than once, and the rested presentation changes at
/// most once: the sheet never gives way to the pane while shrinking, nor the
/// pane to the sheet while growing.
///
/// Domain: three strictly monotone widths in [640, 2560] px (either direction),
/// starting at rest at the first (`Shell::rested`: any intent, text scale, and
/// compact slot), no settle pending. Bound: four
/// allocations.
#[kani::proof]
#[kani::unwind(3)]
fn shell_loop_sweep_does_not_flap() {
    let rested = sweep(SHIPPED);
    let changes = u8::from(rested[0] != rested[1]) + u8::from(rested[1] != rested[2]);
    assert!(changes <= 1, "the rested presentation flaps across a sweep");
}

/// **The flap the setter design had**, kept as a pinned counterexample: with a
/// `layout-name = sheet` setter on the properties breakpoint, one allocation
/// changes `layout-name` twice — the setter (or its restore) writes one value
/// and the reconciliation, run from `notify::layout-name`, writes the other
/// back. Two routes reach it: a text scale above 1 applies the breakpoint in
/// `(T, T * s]` px, where the policy wants the pane; and at scale 1, shrinking
/// below 860 sp makes the workspace breakpoint current, which unapplies the
/// properties breakpoint and restores a captured `pane`.
///
/// Domain: as `shell_loop_sweep_does_not_flap`.
#[kani::proof]
#[kani::unwind(3)]
#[kani::should_panic]
fn shell_loop_layout_setter_flaps() {
    let _ = sweep(LayoutWriter::Setter);
}

/// **Requested visibility survives.** After a compact width that suppresses a
/// surface, a wide width renders every requested surface again, and no
/// allocation changes the requested state.
///
/// Domain: a compact width (sheet presentation for the intent) and a wide one
/// (pane presentation, above 860 px) in [640, 2560] px, otherwise as
/// `shell_loop_settles_at_a_stable_width`, starting at rest at the compact
/// width. Bound: two allocations and one settle.
#[kani::proof]
#[kani::unwind(3)]
fn shell_loop_preserves_requested_visibility() {
    let compact = any_width();
    let wide = any_width();
    let mut shell = Shell::rested(SHIPPED, compact);
    let intent = shell.intent;
    kani::assume(shell.layout().properties_presentation == PropertiesPresentation::Sheet);
    kani::assume(wide > WORKSPACE_BREAKPOINT_MAX_WIDTH_SP && wide != compact);
    shell.allocate_to_rest(wide);
    kani::assume(shell.layout().properties_presentation == PropertiesPresentation::Pane);
    let shown = |requested: bool| requested && !intent.focus_mode;
    assert!(shell.rendered.workspace_shows_sidebar == shown(intent.workspace_requested));
    assert!(shell.rendered.properties_shows_sidebar == shown(intent.properties_requested));
    assert!(!shell.rendered.sheet_open);
    assert!(shell.intent == intent);
}

/// **Agreement at rest.** Once a width has rested, every rendered surface, the
/// compact slot, the cached threshold, and the installed condition are exactly
/// what the policy derives for that width and intent.
///
/// Domain: as `shell_loop_settles_at_a_stable_width`.
#[kani::proof]
#[kani::unwind(3)]
fn shell_loop_layout_agrees_with_policy_at_rest() {
    let width = any_width();
    let mut shell = Shell::any(SHIPPED, width);
    shell.allocate_to_rest(width);
    assert!(
        shell.agrees_with_policy(),
        "the rested shell disagrees with the policy"
    );
    kani::cover!(
        shell.rendered.compact_surface == Some(SecondarySurface::DocumentProperties)
            && shell.rendered.sheet_open,
        "the compact slot is handed to document properties"
    );
    kani::cover!(
        width <= WORKSPACE_BREAKPOINT_MAX_WIDTH_SP
            && shell.adwaita.collapsed
            && shell.intent.workspace_requested
            && !shell.rendered.workspace_shows_sidebar,
        "the workspace is collapsed and yields the compact slot"
    );
}

/// **Allocation never persists.** No allocation writes the persisted intent (the
/// requested visibility and preset keys), and an allocation whose threshold is
/// unchanged reparses no condition: `set_condition` runs only when the integer
/// threshold moves.
///
/// Domain: two widths in [640, 2560] px, otherwise as
/// `shell_loop_settles_at_a_stable_width`, starting at rest at the first width.
/// Bound: one allocation.
#[kani::proof]
#[kani::unwind(3)]
fn shell_loop_allocation_never_persists() {
    let first = any_width();
    let second = any_width();
    kani::assume(first != second);
    let mut shell = Shell::rested(SHIPPED, first);
    let intent = shell.intent;
    let threshold = shell.installed_threshold;
    shell.allocate(second);
    assert!(shell.intent == intent);
    kani::cover!(
        shell.reinstalls > 0,
        "an allocation reinstalls the threshold"
    );
    if shell.layout().properties_breakpoint_max_width == threshold {
        assert!(
            shell.reinstalls == 0,
            "an unchanged threshold reparses the breakpoint condition"
        );
    }
}

/// **The step lemma.** Deriving the layout again from the compact slot it chose
/// yields the same layout, for every input. This is what lets a step derive its
/// layout once (`Shell::step`): a reconciliation writes exactly that slot.
///
/// Domain: every `i32` width, preset, requested visibility, Focus Mode, and
/// compact slot.
#[kani::proof]
fn shell_loop_layout_is_stable_under_its_own_compact_slot() {
    let input = AdaptiveShellInputs {
        window_width: kani::any(),
        workspace_preset: kani::any(),
        workspace_requested_visible: kani::any(),
        properties_requested_visible: kani::any(),
        compact_surface: kani::any(),
        focus_mode_active: kani::any(),
    };
    let layout = derive_adaptive_shell_layout(input);
    let again = derive_adaptive_shell_layout(AdaptiveShellInputs {
        compact_surface: layout.compact_surface,
        ..input
    });
    assert!(again == layout);
}

/// **The workspace collapses with its breakpoint.** At rest, the workspace split
/// view is collapsed exactly when the window matches the workspace breakpoint's
/// `max-width: 860sp`, at every text scale — including where the Open-button
/// breakpoint (`max-width: 400sp`) is the one current, because it carries the
/// same `collapsed = true` setter.
///
/// Domain: as `shell_loop_settles_at_a_stable_width`.
#[kani::proof]
#[kani::unwind(3)]
fn shell_loop_workspace_collapses_with_its_breakpoint() {
    let width = any_width();
    let mut shell = Shell::any(SHIPPED, width);
    shell.allocate_to_rest(width);
    let expected = shell.scale.holds(width, WORKSPACE_BREAKPOINT_MAX_WIDTH_SP);
    kani::cover!(
        shell.adwaita.current == Some(Breakpoint::OpenButton),
        "the Open-button breakpoint is the current one"
    );
    assert!(shell.adwaita.collapsed == expected);
}

/// **The collapse the Open-button breakpoint used to drop**, kept as a pinned
/// counterexample: without its own `collapsed = true` setter, a text scale of
/// 1.6 or more at the narrowest windows makes the Open-button breakpoint the
/// only one applied (ledger A18), and the workspace pane stops collapsing.
///
/// Domain: as `shell_loop_settles_at_a_stable_width`.
#[kani::proof]
#[kani::unwind(3)]
#[kani::should_panic]
fn shell_loop_open_button_breakpoint_uncollapsed_the_workspace() {
    let width = any_width();
    let mut shell = Shell::any(SHIPPED, width);
    shell.open_button_collapses = false;
    shell.adwaita.collapsed = shell.collapsed_by(shell.adwaita.current);
    shell.allocate_to_rest(width);
    assert!(shell.adwaita.collapsed == shell.scale.holds(width, WORKSPACE_BREAKPOINT_MAX_WIDTH_SP));
}
