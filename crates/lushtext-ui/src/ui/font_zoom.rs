// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor font and zoom as CSS classes on the widgets that follow them
//! (`scope-font-zoom-restyle`).
//!
//! **Cross-cutting coordination, no role** (`WFR-FONT-ZOOM`, cross-cutting). It serves the zoom actions, the
//! font preferences (`ui/theme.rs`) and every surface that renders text in
//! the editor font: each editor's source view, and every application widget
//! carrying `.monospace` (search-result labels, the file peek, the minimap,
//! Markdown code blocks and image fallbacks, the notes and local-history
//! previews, the Line Inspector). Its pure half is [`policy`].
//!
//! A reload of a display `GtkCssProvider` revalidates every widget of every
//! window, even with unchanged text, and its cost grows with the widgets open
//! (ledger A86). So the font rules are loaded **once per base font**, with a
//! rule set per zoom level keyed by the class `lushtext-font-<g>-zoom-<z>`
//! (design D1), and a zoom step swaps that class on the registered widgets
//! (D2): only they re-match, and their re-layout at the new size is all the
//! step costs. Widgets register through [`follow_editor_font`] (the editor
//! surface) or [`follow_editor_font_monospace`] (every `.monospace` widget,
//! the only way application code may add `.monospace`; `make check-policy`
//! enforces it), which add the current class at once, so a widget created
//! after a zoom step shows the current size. The registry holds them weakly
//! and prunes the dead on every swap and when it doubles.
//!
//! **A base-font change is two dispatches** (D4). It needs new rules, so it
//! loads generation `g + 1` into the provider that holds no class any widget
//! carries: no computed value changes, so that dispatch is the whole-tree
//! revalidation alone. After the next frame has painted, a later dispatch
//! swaps every widget from its `g` class to its `g + 1` class inside the paced
//! restyle (`restyle_everywhere`), as a zoom step. A zoom level off the menu's
//! grid (set from outside the UI, say 55 %) takes the same path once for its
//! mod-10 lattice. A second change before the swap supersedes it: one pending
//! generation, latest wins, and it is always loaded into the provider the
//! widgets do not use.

pub mod policy;

use std::cell::RefCell;

use gtk4::glib;
use gtk4::prelude::*;
use lushtext_model::values::FontZoomGeneration;

use self::policy::BaseFont;

/// How long the swap of a newly loaded generation waits, at most, for a frame
/// to paint after the load (128 ms, about fifteen 120 Hz frames; kind:
/// wall-clock interval), past which no window is taken to be painting and the
/// swap runs anyway.
const SWAP_FRAME_WAIT: std::time::Duration = std::time::Duration::from_millis(128);

/// The registry size below which registration never prunes.
///
/// Kind: cap; a power of two, rounded down.
const PRUNE_FLOOR: usize = 64;

/// One loaded generation of font rules.
#[derive(Clone, Debug, PartialEq)]
struct Generation {
    number: FontZoomGeneration,
    /// Index into the two providers.
    provider: usize,
    base: BaseFont,
    lattice: Option<u32>,
}

impl Generation {
    fn carries(&self, base: &BaseFont, zoom: u32) -> bool {
        &self.base == base && policy::carries(self.lattice, zoom)
    }
}

/// Lifecycle phase of the process-wide font-zoom CSS providers and generations.
#[derive(Default)]
enum RegistryPhase {
    #[default]
    Uninitialized,
    Installed {
        providers: [gtk4::CssProvider; 2],
        active: ActiveFontPhase,
    },
}

/// Generation state once the two display providers have been installed.
enum ActiveFontPhase {
    AwaitingFirstLoad,
    Loaded {
        current: Generation,
        zoom: u32,
        swap: SwapPhase,
    },
}

/// Pending generation-swap schedule after a base-font or lattice load.
#[derive(Default)]
enum SwapPhase {
    #[default]
    Idle,
    WaitingForFrame {
        generation: Generation,
        zoom: u32,
        clocks: Vec<(gtk4::gdk::FrameClock, glib::SignalHandlerId)>,
        fallback: Option<glib::SourceId>,
    },
    IdleQueued {
        generation: Generation,
        zoom: u32,
        idle: glib::SourceId,
    },
}

impl SwapPhase {
    fn pending_generation(&self) -> Option<&Generation> {
        match self {
            Self::Idle => None,
            Self::WaitingForFrame { generation, .. } | Self::IdleQueued { generation, .. } => {
                Some(generation)
            }
        }
    }

    fn target_zoom_mut(&mut self) -> Option<&mut u32> {
        match self {
            Self::Idle => None,
            Self::WaitingForFrame { zoom, .. } | Self::IdleQueued { zoom, .. } => Some(zoom),
        }
    }

    fn cancel(self) {
        match self {
            Self::Idle => {}
            Self::WaitingForFrame {
                clocks, fallback, ..
            } => {
                for (clock, handler) in clocks {
                    clock.disconnect(handler);
                }
                if let Some(source) = fallback {
                    source.remove();
                }
            }
            Self::IdleQueued { idle, .. } => {
                idle.remove();
            }
        }
    }
}

#[derive(Default)]
struct State {
    phase: RegistryPhase,
    widgets: Vec<glib::WeakRef<gtk4::Widget>>,
    pruned_len: usize,
    evidence: FontZoomEvidence,
}

impl State {
    fn swap_phase(&self) -> Option<&SwapPhase> {
        match &self.phase {
            RegistryPhase::Installed {
                active: ActiveFontPhase::Loaded { swap, .. },
                ..
            } => Some(swap),
            _ => None,
        }
    }

    fn swap_phase_mut(&mut self) -> Option<&mut SwapPhase> {
        match &mut self.phase {
            RegistryPhase::Installed {
                active: ActiveFontPhase::Loaded { swap, .. },
                ..
            } => Some(swap),
            _ => None,
        }
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Observable state of the font zoom registry and providers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontZoomEvidence {
    /// Registrations made.
    pub registered: u64,
    /// Registered widgets still alive at the last count.
    pub live: usize,
    /// Class swaps run (one per zoom step or generation change).
    pub swaps: u64,
    /// Widgets whose class the last swap changed.
    pub last_swap_widgets: usize,
    /// How long the last swap's class changes took, µs (the re-layout runs in
    /// the next frame).
    pub last_swap_us: u64,
    /// Provider loads (`load_from_string`) made.
    pub provider_loads: u64,
    /// The generation the registered widgets carry, 0 before the first load.
    pub generation: FontZoomGeneration,
    /// The generation loaded but not yet swapped in, if any.
    pub pending_generation: Option<FontZoomGeneration>,
    /// The class every registered widget carries.
    pub current_class: Option<String>,
}

/// Create the two display providers, once per process. Later calls (another
/// application instance in the same process) reuse them.
pub(crate) fn install(display: &gtk4::gdk::Display) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if matches!(state.phase, RegistryPhase::Installed { .. }) {
            return;
        }
        let providers = [gtk4::CssProvider::new(), gtk4::CssProvider::new()];
        for provider in &providers {
            gtk4::style_context_add_provider_for_display(
                display,
                provider,
                gtk4::STYLE_PROVIDER_PRIORITY_USER,
            );
        }
        state.phase = RegistryPhase::Installed {
            providers,
            active: ActiveFontPhase::AwaitingFirstLoad,
        };
    });
}

/// Whether a loaded generation still waits for its class swap: the font
/// change has not reached the widgets yet (a `document-settled` blocker, and
/// the measurement build's settle sentinel holds back while it is true).
#[must_use]
pub fn swap_pending() -> bool {
    STATE.with(|state| {
        state
            .borrow()
            .swap_phase()
            .is_some_and(|swap| !matches!(swap, SwapPhase::Idle))
    })
}

/// Whether the providers exist (`install` ran).
pub(crate) fn is_installed() -> bool {
    STATE.with(|state| matches!(state.borrow().phase, RegistryPhase::Installed { .. }))
}

/// Make `widget` (an editor surface view, carrying
/// `tab-content-editor-surface`) follow the editor font and zoom: it takes the
/// current zoom class now and every later one.
pub fn follow_editor_font(widget: &impl IsA<gtk4::Widget>) {
    let widget = widget.upcast_ref::<gtk4::Widget>();
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(class) = state.evidence.current_class.clone() {
            widget.add_css_class(&class);
        }
        state.widgets.push(widget.downgrade());
        state.evidence.registered = state.evidence.registered.saturating_add(1);
        if state.widgets.len() >= state.pruned_len.max(PRUNE_FLOOR).saturating_mul(2) {
            prune(&mut state);
        }
    });
}

/// Give `widget` GTK's monospace font and make it follow the editor font and
/// zoom: `gtk_text_view_set_monospace` for a text view, the `monospace` class
/// otherwise. The only way application code adds `.monospace`.
pub fn follow_editor_font_monospace(widget: &impl IsA<gtk4::Widget>) {
    let widget = widget.upcast_ref::<gtk4::Widget>();
    if let Some(view) = widget.downcast_ref::<gtk4::TextView>() {
        view.set_monospace(true);
    }
    widget.add_css_class("monospace");
    follow_editor_font(widget);
}

/// Whether `widget` is registered.
#[must_use]
pub fn is_registered(widget: &impl IsA<gtk4::Widget>) -> bool {
    let widget = widget.upcast_ref::<gtk4::Widget>();
    STATE.with(|state| {
        state
            .borrow()
            .widgets
            .iter()
            .any(|weak| weak.upgrade().as_ref() == Some(widget))
    })
}

/// The registry's observable state (counts the live widgets; changes nothing).
#[must_use]
pub fn evidence() -> FontZoomEvidence {
    STATE.with(|state| {
        let state = state.borrow();
        let mut evidence = state.evidence.clone();
        evidence.live = state
            .widgets
            .iter()
            .filter(|weak| weak.upgrade().is_some())
            .count();
        evidence.pending_generation = state
            .swap_phase()
            .and_then(SwapPhase::pending_generation)
            .map(|g| g.number);
        evidence
    })
}

/// The two providers' rule text, for tests that check a zoom step changes
/// none of it.
#[must_use]
pub fn provider_texts() -> Vec<String> {
    STATE.with(|state| match &state.borrow().phase {
        RegistryPhase::Uninitialized => Vec::new(),
        RegistryPhase::Installed { providers, .. } => {
            providers.iter().map(|p| p.to_str().to_string()).collect()
        }
    })
}

/// Apply the font `base` at `zoom`: a class swap when the current (or pending)
/// generation carries it, else a load of a new generation and a later swap.
pub(crate) fn apply(base: &BaseFont, zoom: u32) {
    let zoom = policy::clamp_zoom(zoom);
    enum Plan {
        Nothing,
        Swap(String, String),
        Load,
        RetargetPending,
    }
    let plan = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let from = state.evidence.current_class.clone().unwrap_or_default();
        let RegistryPhase::Installed { active, .. } = &mut state.phase else {
            return Plan::Nothing;
        };
        let ActiveFontPhase::Loaded { current, swap, .. } = active else {
            return Plan::Load;
        };
        if swap
            .pending_generation()
            .is_some_and(|pending| pending.carries(base, zoom))
        {
            if let Some(target) = swap.target_zoom_mut() {
                *target = zoom;
            }
            return Plan::RetargetPending;
        }
        if current.carries(base, zoom) {
            std::mem::take(swap).cancel();
            let to = policy::zoom_class(current.number, zoom);
            if from == to {
                Plan::Nothing
            } else {
                Plan::Swap(from, to)
            }
        } else {
            Plan::Load
        }
    });
    match plan {
        Plan::Nothing | Plan::RetargetPending => {}
        Plan::Swap(from, to) => {
            crate::ui::editor_page::restyle::restyle_everywhere(|| swap(&from, &to, zoom));
        }
        Plan::Load => load(base, zoom),
    }
}

/// Load a new generation into the provider the widgets do not use.
fn load(base: &BaseFont, zoom: u32) {
    let _span = crate::ui::responsiveness_probe::PostOpenSpan::begin("post_open_font_load");
    let first = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let current_class;
        let current_gen;
        let first = {
            let RegistryPhase::Installed { providers, active } = &mut state.phase else {
                return false;
            };
            match active {
                ActiveFontPhase::AwaitingFirstLoad => {
                    let number = FontZoomGeneration::new(1);
                    let generation = Generation {
                        number,
                        provider: 0,
                        base: base.clone(),
                        lattice: policy::lattice_of(zoom),
                    };
                    let text = policy::provider_text(number, base, generation.lattice);
                    providers[0].load_from_string(&text);
                    current_gen = Some(number);
                    current_class = Some(policy::zoom_class(number, zoom));
                    *active = ActiveFontPhase::Loaded {
                        current: generation,
                        zoom,
                        swap: SwapPhase::Idle,
                    };
                    true
                }
                ActiveFontPhase::Loaded { current, swap, .. } => {
                    let number = swap
                        .pending_generation()
                        .map_or(current.number, |pending| pending.number)
                        .next();
                    let provider = 1 - current.provider;
                    let generation = Generation {
                        number,
                        provider,
                        base: base.clone(),
                        lattice: policy::lattice_of(zoom),
                    };
                    let text = policy::provider_text(number, base, generation.lattice);
                    providers[provider].load_from_string(&text);
                    current_gen = None;
                    current_class = None;
                    std::mem::replace(
                        swap,
                        SwapPhase::WaitingForFrame {
                            generation,
                            zoom,
                            clocks: Vec::new(),
                            fallback: None,
                        },
                    )
                    .cancel();
                    false
                }
            }
        };
        state.evidence.provider_loads = state.evidence.provider_loads.saturating_add(1);
        if let Some(number) = current_gen {
            state.evidence.generation = number;
            state.evidence.current_class = current_class;
        }
        first
    });
    if first {
        // Widgets registered before the first load take the class now.
        let class = STATE.with(|state| state.borrow().evidence.current_class.clone());
        if let Some(class) = class {
            for widget in live_widgets() {
                widget.add_css_class(&class);
            }
        }
        return;
    }
    schedule_swap();
}

/// Swap the pending generation in after the next painted frame.
fn schedule_swap() {
    let clocks: Vec<gtk4::gdk::FrameClock> = gtk4::Window::list_toplevels()
        .into_iter()
        .filter(WidgetExt::is_mapped)
        .filter_map(|window| window.frame_clock())
        .collect();
    let mut scheduled = Vec::new();
    for clock in clocks {
        let handler = clock.connect_after_paint(|_| painted());
        scheduled.push((clock, handler));
    }
    let fallback = glib::timeout_add_local_once(SWAP_FRAME_WAIT, || {
        STATE.with(|state| {
            if let Some(SwapPhase::WaitingForFrame { fallback, .. }) =
                state.borrow_mut().swap_phase_mut()
            {
                *fallback = None;
            }
        });
        painted();
    });
    STATE.with(|state| {
        if let Some(SwapPhase::WaitingForFrame {
            clocks,
            fallback: slot,
            ..
        }) = state.borrow_mut().swap_phase_mut()
        {
            *clocks = scheduled;
            *slot = Some(fallback);
        } else {
            for (clock, handler) in scheduled {
                clock.disconnect(handler);
            }
            fallback.remove();
        }
    });
}

/// A frame painted after the load (or the wait ran out): swap from a later
/// dispatch, so the swap's re-layout never shares the revalidation's frame.
fn painted() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(swap) = state.swap_phase_mut() else {
            return;
        };
        if !matches!(swap, SwapPhase::WaitingForFrame { .. }) {
            return;
        }
        let SwapPhase::WaitingForFrame {
            generation,
            zoom,
            clocks,
            fallback,
        } = std::mem::take(swap)
        else {
            return;
        };
        for (clock, handler) in clocks {
            clock.disconnect(handler);
        }
        if let Some(source) = fallback {
            source.remove();
        }
        let idle = glib::idle_add_local_once(swap_pending_generation);
        *swap = SwapPhase::IdleQueued {
            generation,
            zoom,
            idle,
        };
    });
}

fn swap_pending_generation() {
    let swap_to = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let from = state.evidence.current_class.clone().unwrap_or_default();
        let RegistryPhase::Installed {
            active:
                ActiveFontPhase::Loaded {
                    current,
                    zoom: active_zoom,
                    swap,
                },
            ..
        } = &mut state.phase
        else {
            return None;
        };
        let (generation, target_zoom) = match std::mem::take(swap) {
            SwapPhase::Idle => return None,
            SwapPhase::WaitingForFrame {
                generation,
                zoom,
                clocks,
                fallback,
            } => {
                for (clock, handler) in clocks {
                    clock.disconnect(handler);
                }
                if let Some(source) = fallback {
                    source.remove();
                }
                (generation, zoom)
            }
            SwapPhase::IdleQueued {
                generation, zoom, ..
            } => (generation, zoom),
        };
        let generation_number = generation.number;
        let to = policy::zoom_class(generation_number, target_zoom);
        *current = generation;
        *active_zoom = target_zoom;
        state.evidence.generation = generation_number;
        Some((from, to, target_zoom))
    });
    if let Some((from, to, zoom)) = swap_to {
        crate::ui::editor_page::restyle::restyle_everywhere(|| swap(&from, &to, zoom));
    }
}

/// Move every live registered widget from class `from` to `to`.
fn swap(from: &str, to: &str, zoom: u32) {
    let _span = crate::ui::responsiveness_probe::PostOpenSpan::begin("post_open_font_swap");
    let started = std::time::Instant::now();
    let widgets = STATE.with(|state| {
        let mut state = state.borrow_mut();
        prune(&mut state);
        state.evidence.current_class = Some(to.to_owned());
        if let RegistryPhase::Installed {
            active: ActiveFontPhase::Loaded { zoom: active, .. },
            ..
        } = &mut state.phase
        {
            *active = zoom;
        }
        state
            .widgets
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .collect::<Vec<_>>()
    });
    for widget in &widgets {
        if !from.is_empty() {
            widget.remove_css_class(from);
        }
        widget.add_css_class(to);
    }
    let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
    STATE.with(|state| {
        let evidence = &mut state.borrow_mut().evidence;
        evidence.swaps = evidence.swaps.saturating_add(1);
        evidence.last_swap_widgets = widgets.len();
        evidence.last_swap_us = micros;
    });
}

fn live_widgets() -> Vec<gtk4::Widget> {
    STATE.with(|state| {
        state
            .borrow()
            .widgets
            .iter()
            .filter_map(glib::WeakRef::upgrade)
            .collect()
    })
}

fn prune(state: &mut State) {
    state.widgets.retain(|weak| weak.upgrade().is_some());
    state.pruned_len = state.widgets.len();
}
