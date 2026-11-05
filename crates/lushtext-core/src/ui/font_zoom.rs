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
    number: u64,
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

#[derive(Default)]
struct State {
    providers: Option<[gtk4::CssProvider; 2]>,
    /// The generation every registered widget's class belongs to.
    current: Option<Generation>,
    /// The zoom of the registered widgets' class.
    zoom: u32,
    /// A loaded generation not yet on any widget, and the zoom to swap to.
    pending: Option<(Generation, u32)>,
    /// The scheduled swap: after-paint handlers and the fallback timeout.
    scheduled: Vec<(gtk4::gdk::FrameClock, glib::SignalHandlerId)>,
    fallback: Option<glib::SourceId>,
    idle: Option<glib::SourceId>,
    widgets: Vec<glib::WeakRef<gtk4::Widget>>,
    pruned_len: usize,
    evidence: FontZoomEvidence,
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
    pub generation: u64,
    /// The generation loaded but not yet swapped in, if any.
    pub pending_generation: Option<u64>,
    /// The class every registered widget carries.
    pub current_class: Option<String>,
}

/// Create the two display providers, once per process. Later calls (another
/// application instance in the same process) reuse them.
pub(crate) fn install(display: &gtk4::gdk::Display) {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.providers.is_some() {
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
        state.providers = Some(providers);
        state.zoom = policy::ZOOM_DEFAULT;
    });
}

/// Whether a loaded generation still waits for its class swap: the font
/// change has not reached the widgets yet (a `document-settled` blocker, and
/// the measurement build's settle sentinel holds back while it is true).
#[must_use]
pub fn swap_pending() -> bool {
    STATE.with(|state| state.borrow().pending.is_some())
}

/// Whether the providers exist (`install` ran).
pub(crate) fn is_installed() -> bool {
    STATE.with(|state| state.borrow().providers.is_some())
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
        evidence.pending_generation = state.pending.as_ref().map(|(g, _)| g.number);
        evidence
    })
}

/// The two providers' rule text, for tests that check a zoom step changes
/// none of it.
#[must_use]
pub fn provider_texts() -> Vec<String> {
    STATE.with(|state| {
        state
            .borrow()
            .providers
            .as_ref()
            .map(|providers| providers.iter().map(|p| p.to_str().to_string()).collect())
            .unwrap_or_default()
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
        if state.providers.is_none() {
            return Plan::Nothing;
        }
        if let Some((pending, _)) = state.pending.as_ref()
            && pending.carries(base, zoom)
        {
            if let Some((_, target)) = state.pending.as_mut() {
                *target = zoom;
            }
            return Plan::RetargetPending;
        }
        let current_number = state
            .current
            .as_ref()
            .filter(|current| current.carries(base, zoom))
            .map(|current| current.number);
        match current_number {
            Some(number) => {
                if state.pending.take().is_some() {
                    cancel_schedule(&mut state);
                }
                let from = state.evidence.current_class.clone().unwrap_or_default();
                let to = policy::zoom_class(number, zoom);
                if from == to {
                    Plan::Nothing
                } else {
                    Plan::Swap(from, to)
                }
            }
            None => Plan::Load,
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
        let (number, provider) = match state.current.as_ref() {
            Some(current) => (
                state
                    .pending
                    .as_ref()
                    .map_or(current.number, |(pending, _)| pending.number)
                    .wrapping_add(1),
                1 - current.provider,
            ),
            None => (1, 0),
        };
        let generation = Generation {
            number,
            provider,
            base: base.clone(),
            lattice: policy::lattice_of(zoom),
        };
        let text = policy::provider_text(number, base, generation.lattice);
        if let Some(providers) = state.providers.as_ref() {
            providers[provider].load_from_string(&text);
        }
        state.evidence.provider_loads = state.evidence.provider_loads.saturating_add(1);
        if state.current.is_none() {
            // The first load: nothing carries a class yet, so the class
            // becomes current at once (registration adds it).
            state.evidence.generation = number;
            state.evidence.current_class = Some(policy::zoom_class(number, zoom));
            state.zoom = zoom;
            state.current = Some(generation);
            true
        } else {
            state.pending = Some((generation, zoom));
            false
        }
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
    STATE.with(|state| cancel_schedule(&mut state.borrow_mut()));
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
        STATE.with(|state| state.borrow_mut().fallback = None);
        painted();
    });
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.scheduled = scheduled;
        state.fallback = Some(fallback);
    });
}

/// A frame painted after the load (or the wait ran out): swap from a later
/// dispatch, so the swap's re-layout never shares the revalidation's frame.
fn painted() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        for (clock, handler) in std::mem::take(&mut state.scheduled) {
            clock.disconnect(handler);
        }
        if let Some(source) = state.fallback.take() {
            source.remove();
        }
        if state.idle.is_none() && state.pending.is_some() {
            let idle = glib::idle_add_local_once(|| {
                STATE.with(|state| state.borrow_mut().idle = None);
                swap_pending_generation();
            });
            state.idle = Some(idle);
        }
    });
}

fn swap_pending_generation() {
    let swap_to = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let (generation, zoom) = state.pending.take()?;
        let from = state.evidence.current_class.clone().unwrap_or_default();
        let to = policy::zoom_class(generation.number, zoom);
        state.evidence.generation = generation.number;
        state.current = Some(generation);
        Some((from, to, zoom))
    });
    if let Some((from, to, zoom)) = swap_to {
        crate::ui::editor_page::restyle::restyle_everywhere(|| swap(&from, &to, zoom));
    }
}

fn cancel_schedule(state: &mut State) {
    for (clock, handler) in std::mem::take(&mut state.scheduled) {
        clock.disconnect(handler);
    }
    if let Some(source) = state.fallback.take() {
        source.remove();
    }
    if let Some(source) = state.idle.take() {
        source.remove();
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
        state.zoom = zoom;
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
