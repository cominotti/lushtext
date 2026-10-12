// SPDX-License-Identifier: GPL-3.0-or-later

//! In-process responsiveness instrumentation for the measurement build.
//!
//! **cross-cutting instrumentation with no workflow role and no `WFR-*` row**,
//! like `services::kill_point`. It exists so `make responsiveness-smoke` can
//! measure what the user feels in the real binary: how long the GTK main loop
//! goes without returning to poll, when frames are presented, how long an
//! input event waits for its first presented frame, and what an input method
//! that asks for the surrounding text costs.
//!
//! Without the `responsiveness-probes` Cargo feature — every release, Meson,
//! Flatpak, and Snap build — [`install`] is an empty inline function: no
//! source, handler, controller, input-method type, or environment read exists
//! in the binary. With the feature, [`install`] does nothing unless
//! `LUSHTEXT_RESPONSIVENESS_LOG` names a file; then it installs:
//!
//! 1. **The lateness watchdog** (`LatenessWatchdog`): a custom `GSource` at
//!    `G_PRIORITY_HIGH` whose ready time is set in microseconds
//!    (`g_source_set_ready_time`) every tick `T`
//!    (`LUSHTEXT_RESPONSIVENESS_TICK_US`, default 2,000 µs). Each dispatch
//!    records `L = now − ready time` and re-arms from that same `now`. GLib
//!    cannot preempt a dispatch and dispatches every ready source of the
//!    winning priority back to back, so the largest `L` bounds the longest
//!    main-loop stall `S` as `S − T ≤ L ≤ S + ε`, with `ε` the poll wake-up
//!    slack (GLib rounds the poll timeout to whole milliseconds). A plain
//!    `timeout_source_new` is not used: it has millisecond granularity and
//!    re-arms from the iteration's cached time.
//! 2. **Frame timing**: each window's `GdkFrameClock` `after-paint` queues the
//!    frame, and once its `GdkFrameTimings` are complete the probe records the
//!    frame time, the presentation time when the compositor reported one, and
//!    the refresh interval.
//! 3. **Input stamps**: a capture-phase `GtkEventControllerLegacy` on each
//!    window records, for key, scroll, and button events, both the handler's
//!    monotonic time and the event's own compositor timestamp, so latency can
//!    be measured from the injection or the event time rather than from a
//!    handler that runs only after a stall ends.
//! 4. **The probe input method** (`LUSHTEXT_PROBE_IM=1`): a `GtkIMContext`
//!    registered on the `gtk-im-module` extension point as `lushtext-probe`
//!    (selected with `GTK_IM_MODULE=lushtext-probe`). On focus-in,
//!    `set_cursor_location`, `filter_keypress`, and `reset` it emits
//!    `retrieve-surrounding`, as an IBus engine with the surrounding-text
//!    capability does, and records the call and the surrounding text's byte
//!    length, never the text. It commits printable keys itself so typing
//!    still inserts text.
//!
//! Every observation becomes a `services::responsiveness_log::ProbeRecord` and
//! travels over a bounded channel to one writer thread; a full channel drops
//! and counts records, never blocking the GTK thread.
//!
//! The watchdog compiles under `cfg(any(test, feature = "responsiveness-probes"))`
//! so the default unit-test lane tests it on a private main context; only
//! [`install`] and its toolkit hooks are feature-only.

#[cfg(any(test, feature = "responsiveness-probes"))]
pub(crate) use watchdog::LatenessWatchdog;

/// Install the responsiveness probes into `app` when the measurement build
/// was asked to (`LUSHTEXT_RESPONSIVENESS_LOG`); see the module docs.
///
/// Without the `responsiveness-probes` feature this is an empty function.
#[cfg(not(feature = "responsiveness-probes"))]
#[inline]
pub fn install(_app: &impl gtk4::prelude::IsA<gtk4::Application>) {}

/// Install the responsiveness probes into `app` when the measurement build
/// was asked to (`LUSHTEXT_RESPONSIVENESS_LOG`); see the module docs.
#[cfg(feature = "responsiveness-probes")]
pub fn install(app: &impl gtk4::prelude::IsA<gtk4::Application>) {
    measurement::install(app.as_ref());
}

#[cfg(any(test, feature = "responsiveness-probes"))]
mod watchdog {
    //! The custom `GSource` behind the lateness watchdog.

    use std::ffi::c_uint;
    use std::ptr;

    use glib::ffi;
    use glib::translate::{FromGlibPtrFull, ToGlibPtr};

    /// What the watchdog calls on each dispatch: `(now_us, ready_us)`.
    type TickSink = Box<dyn FnMut(i64, i64)>;

    /// The source's private block: GLib allocates `struct_size` bytes with
    /// the `GSource` first, so the state pointer travels with the source.
    #[repr(C)]
    struct WatchdogSource {
        source: ffi::GSource,
        state: *mut WatchdogState,
    }

    struct WatchdogState {
        tick_us: i64,
        sink: TickSink,
    }

    static WATCHDOG_FUNCS: ffi::GSourceFuncs = ffi::GSourceFuncs {
        prepare: None,
        check: None,
        dispatch: Some(dispatch),
        finalize: Some(finalize),
        closure_callback: None,
        closure_marshal: None,
    };

    unsafe extern "C" fn dispatch(
        source: *mut ffi::GSource,
        _callback: ffi::GSourceFunc,
        _user_data: ffi::gpointer,
    ) -> ffi::gboolean {
        // SAFETY: GLib calls `dispatch` only for sources created by
        // `LatenessWatchdog::attach`, whose block is a `WatchdogSource` with
        // a live `state` until `finalize` runs, and only on the thread that
        // iterates the source's context, which owns the non-`Send` sink.
        unsafe {
            let block = source.cast::<WatchdogSource>();
            let ready_us = ffi::g_source_get_ready_time(source);
            let now_us = ffi::g_get_monotonic_time();
            let state = &mut *(*block).state;
            ffi::g_source_set_ready_time(source, now_us.saturating_add(state.tick_us));
            // A panic must not unwind into GLib's C frames.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                (state.sink)(now_us, ready_us);
            }))
            .is_err()
            {
                std::process::abort();
            }
        }
        ffi::GTRUE
    }

    unsafe extern "C" fn finalize(source: *mut ffi::GSource) {
        // SAFETY: as in `dispatch`; `finalize` runs once, after which GLib
        // frees the block, so the state is reclaimed exactly once.
        unsafe {
            let block = source.cast::<WatchdogSource>();
            let state = (*block).state;
            if !state.is_null() {
                (*block).state = ptr::null_mut();
                drop(Box::from_raw(state));
            }
        }
    }

    /// A `G_PRIORITY_HIGH` source dispatched every tick, re-armed from each
    /// dispatch's own monotonic time; destroyed when dropped.
    pub(crate) struct LatenessWatchdog {
        source: glib::Source,
    }

    impl LatenessWatchdog {
        /// Attach a watchdog with tick `tick_us` to `context`, calling `sink`
        /// with `(now_us, ready_us)` on every dispatch.
        pub(crate) fn attach(
            context: &glib::MainContext,
            tick_us: i64,
            sink: impl FnMut(i64, i64) + 'static,
        ) -> Self {
            let tick_us = tick_us.max(1);
            let state = Box::into_raw(Box::new(WatchdogState {
                tick_us,
                sink: Box::new(sink),
            }));
            // SAFETY: `g_source_new` allocates a zeroed block of
            // `struct_size` bytes beginning with a `GSource`; the static funcs
            // outlive every source. The block is ours until attached, and
            // `from_glib_full` adopts the one reference `g_source_new` returned.
            unsafe {
                let size = c_uint::try_from(size_of::<WatchdogSource>())
                    .expect("the source block fits a c_uint");
                let raw = ffi::g_source_new(ptr::from_ref(&WATCHDOG_FUNCS).cast_mut(), size);
                (*raw.cast::<WatchdogSource>()).state = state;
                ffi::g_source_set_priority(raw, ffi::G_PRIORITY_HIGH);
                ffi::g_source_set_ready_time(
                    raw,
                    ffi::g_get_monotonic_time().saturating_add(tick_us),
                );
                ffi::g_source_attach(raw, context.to_glib_none().0);
                Self {
                    source: glib::Source::from_glib_full(raw),
                }
            }
        }
    }

    impl Drop for LatenessWatchdog {
        fn drop(&mut self) {
            self.source.destroy();
        }
    }

    #[cfg(test)]
    mod tests {
        use std::cell::RefCell;
        use std::rc::Rc;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::{Duration, Instant};

        use super::LatenessWatchdog;

        /// The watchdog tick under test.
        const TICK_US: i64 = 2_000;
        /// How long the blocking source runs.
        const STALL: Duration = Duration::from_millis(40);
        /// Scheduling slack allowed above the stall: generous, because the
        /// lower bound is what the physics guarantees and a loaded runner can
        /// delay the dispatch that follows the stall.
        const SLACK_US: i64 = 30_000;

        #[test]
        fn a_blocking_dispatch_is_recorded_between_its_bounds() {
            let context = glib::MainContext::new();
            let latenesses = Rc::new(RefCell::new(Vec::<i64>::new()));
            let recorder = Rc::clone(&latenesses);
            let watchdog = LatenessWatchdog::attach(&context, TICK_US, move |now, ready| {
                recorder.borrow_mut().push(now - ready);
            });
            // Let the watchdog tick a few times before the stall.
            let warm_up = Instant::now();
            while warm_up.elapsed() < Duration::from_millis(20) {
                context.iteration(true);
            }
            let stalled = Arc::new(AtomicBool::new(false));
            let stall_flag = Arc::clone(&stalled);
            let blocker = glib::idle_source_new(None, glib::Priority::DEFAULT, move || {
                std::thread::sleep(STALL);
                stall_flag.store(true, Ordering::SeqCst);
                glib::ControlFlow::Break
            });
            blocker.attach(Some(&context));
            let started = Instant::now();
            while started.elapsed() < STALL + Duration::from_millis(30) {
                context.iteration(true);
            }
            drop(watchdog);
            assert!(stalled.load(Ordering::SeqCst), "the blocking source ran");
            let stall_us = i64::try_from(STALL.as_micros()).expect("fits");
            let worst = latenesses.borrow().iter().copied().max().unwrap_or(0);
            assert!(
                worst >= stall_us - TICK_US,
                "lateness {worst} µs below the lower bound {} µs",
                stall_us - TICK_US
            );
            assert!(
                worst <= stall_us + SLACK_US,
                "lateness {worst} µs above the upper bound {} µs",
                stall_us + SLACK_US
            );
        }

        #[test]
        fn an_idle_context_ticks_on_time() {
            let context = glib::MainContext::new();
            let ticks = Rc::new(RefCell::new(0u32));
            let counter = Rc::clone(&ticks);
            let _watchdog = LatenessWatchdog::attach(&context, TICK_US, move |now, ready| {
                assert!(now >= ready, "never dispatched before its ready time");
                *counter.borrow_mut() += 1;
            });
            let started = Instant::now();
            while started.elapsed() < Duration::from_millis(50) {
                context.iteration(true);
            }
            assert!(*ticks.borrow() >= 5, "{} ticks in 50 ms", ticks.borrow());
        }
    }
}

#[cfg(feature = "responsiveness-probes")]
mod measurement {
    //! The feature-only half: environment, toolkit hooks, and the probe IM.

    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::rc::Rc;

    use gtk4::prelude::*;

    use super::LatenessWatchdog;
    use crate::services::responsiveness_log::{
        DEFAULT_LATE_THRESHOLD_US, EVIDENCE_CHANNEL_CAPACITY, EvidenceWriter, LatenessAggregator,
        ProbeImTrigger, ProbeInputKind, ProbeRecord,
    };

    const LOG_ENV: &str = "LUSHTEXT_RESPONSIVENESS_LOG";
    const TICK_ENV: &str = "LUSHTEXT_RESPONSIVENESS_TICK_US";
    const PROBE_IM_ENV: &str = "LUSHTEXT_PROBE_IM";
    const DEFAULT_TICK_US: i64 = 2_000;
    /// The input-method module id the lane selects with `GTK_IM_MODULE`.
    pub(super) const PROBE_IM_ID: &str = "lushtext-probe";
    /// Frames a window may have queued before the oldest is written without
    /// its presentation time.
    const MAX_PENDING_FRAMES: usize = 64;

    /// The process-wide probe, owned by the GTK thread.
    struct Probe {
        writer: EvidenceWriter,
        aggregator: RefCell<LatenessAggregator>,
        next_window: Cell<u32>,
        _watchdog: RefCell<Option<LatenessWatchdog>>,
    }

    thread_local! {
        static PROBE: RefCell<Option<Rc<Probe>>> = const { RefCell::new(None) };
    }

    fn probe() -> Option<Rc<Probe>> {
        PROBE.with(|probe| probe.borrow().clone())
    }

    /// Send one record if the probe is installed.
    pub(super) fn record(record: ProbeRecord) {
        if let Some(probe) = probe() {
            probe.writer.send(record);
        }
    }

    pub(super) fn install(app: &gtk4::Application) {
        if probe().is_some() {
            return;
        }
        let Some(path) = std::env::var_os(LOG_ENV).map(PathBuf::from) else {
            return;
        };
        let tick_us = std::env::var(TICK_ENV)
            .ok()
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|tick| *tick > 0)
            .unwrap_or(DEFAULT_TICK_US);
        let writer = match EvidenceWriter::to_path(&path, EVIDENCE_CHANNEL_CAPACITY) {
            Ok(writer) => writer,
            Err(error) => {
                tracing::error!(
                    "responsiveness probe: cannot open {}: {error}",
                    path.display()
                );
                return;
            }
        };
        let probe_im = std::env::var(PROBE_IM_ENV).as_deref() == Ok("1");
        if probe_im {
            probe_im::register();
        }
        let probe = Rc::new(Probe {
            writer,
            aggregator: RefCell::new(LatenessAggregator::new(DEFAULT_LATE_THRESHOLD_US)),
            next_window: Cell::new(1),
            _watchdog: RefCell::new(None),
        });
        PROBE.with(|slot| slot.replace(Some(Rc::clone(&probe))));
        probe.writer.send(ProbeRecord::Start {
            t_us: glib::monotonic_time(),
            pid: std::process::id(),
            tick_us: u64::try_from(tick_us).unwrap_or(0),
            late_threshold_us: DEFAULT_LATE_THRESHOLD_US,
            probe_im,
        });
        let weak = Rc::downgrade(&probe);
        let watchdog = LatenessWatchdog::attach(
            &glib::MainContext::default(),
            tick_us,
            move |now_us, ready_us| {
                let Some(probe) = weak.upgrade() else {
                    return;
                };
                let records = probe.aggregator.borrow_mut().tick(now_us, ready_us);
                for record in records {
                    probe.writer.send(record);
                }
            },
        );
        probe._watchdog.replace(Some(watchdog));
        for window in app.windows() {
            hook_window(&window);
        }
        app.connect_window_added(|_, window| hook_window(window));
    }

    fn next_window_id() -> u32 {
        probe().map_or(0, |probe| {
            let id = probe.next_window.get();
            probe.next_window.set(id.saturating_add(1));
            id
        })
    }

    fn hook_window(window: &gtk4::Window) {
        let id = next_window_id();
        let input = gtk4::EventControllerLegacy::new();
        input.set_propagation_phase(gtk4::PropagationPhase::Capture);
        input.connect_event(move |_, event| {
            let kind = match event.event_type() {
                gtk4::gdk::EventType::KeyPress => Some(ProbeInputKind::KeyPress),
                gtk4::gdk::EventType::KeyRelease => Some(ProbeInputKind::KeyRelease),
                gtk4::gdk::EventType::Scroll => Some(ProbeInputKind::Scroll),
                gtk4::gdk::EventType::ButtonPress => Some(ProbeInputKind::ButtonPress),
                gtk4::gdk::EventType::ButtonRelease => Some(ProbeInputKind::ButtonRelease),
                _ => None,
            };
            if let Some(kind) = kind {
                record(ProbeRecord::Input {
                    t_us: glib::monotonic_time(),
                    window: id,
                    event: kind,
                    event_time_ms: event.time(),
                });
            }
            glib::Propagation::Proceed
        });
        window.add_controller(input);
        let hooked = Rc::new(Cell::new(false));
        let hook_clock = move |widget: &gtk4::Window| {
            if hooked.get() {
                return;
            }
            if let Some(clock) = widget.frame_clock() {
                hooked.set(true);
                let pending = Rc::new(RefCell::new(VecDeque::<(i64, i64)>::new()));
                clock.connect_after_paint(move |clock| record_frames(clock, id, &pending));
            }
        };
        if window.is_realized() {
            hook_clock(window);
        }
        window.connect_realize(move |window| hook_clock(window));
    }

    /// Queue the frame that just painted and write every queued frame whose
    /// timings are now final, oldest first.
    fn record_frames(
        clock: &gtk4::gdk::FrameClock,
        window: u32,
        pending: &RefCell<VecDeque<(i64, i64)>>,
    ) {
        let mut pending = pending.borrow_mut();
        if let Some(current) = clock.current_timings() {
            pending.push_back((current.frame_counter(), current.frame_time()));
        }
        let now = glib::monotonic_time();
        while let Some(&(counter, frame_time)) = pending.front() {
            let timings = clock.timings(counter);
            let complete = timings
                .as_ref()
                .is_some_and(gtk4::gdk::FrameTimings::is_complete);
            if !complete && timings.is_some() && pending.len() <= MAX_PENDING_FRAMES {
                break;
            }
            pending.pop_front();
            let (presentation, refresh) = timings
                .filter(gtk4::gdk::FrameTimings::is_complete)
                .map_or((None, None), |timings| {
                    (
                        Some(timings.presentation_time()).filter(|time| *time > 0),
                        Some(timings.refresh_interval()).filter(|interval| *interval > 0),
                    )
                });
            record(ProbeRecord::Frame {
                t_us: now,
                window,
                frame_counter: counter,
                frame_time_us: frame_time,
                presentation_us: presentation,
                refresh_interval_us: refresh,
            });
        }
    }

    pub(super) fn record_im(
        trigger: ProbeImTrigger,
        surrounding: Option<(u64, i64)>,
        handled: bool,
    ) {
        record(ProbeRecord::Im {
            t_us: glib::monotonic_time(),
            trigger,
            surrounding_bytes: surrounding.map(|(bytes, _)| bytes),
            cursor_index: surrounding.map_or(-1, |(_, cursor)| cursor),
            handled,
        });
    }

    mod probe_im {
        //! The deterministic probe input method.

        use gtk4::gio;
        use gtk4::prelude::*;

        use super::PROBE_IM_ID;

        mod imp {
            use std::cell::Cell;

            use gtk4::prelude::*;
            use gtk4::subclass::prelude::*;

            use crate::services::responsiveness_log::ProbeImTrigger;

            #[derive(Default)]
            pub struct ProbeImContext {
                /// What the client supplied in answer to the last request:
                /// `(byte length, cursor byte index)`.
                surrounding: Cell<Option<(u64, i64)>>,
                /// Whether the client widget has focus. Requests are made only
                /// then: an IBus engine asks while it serves a focused client,
                /// and GTK resets a context during text-view construction and
                /// teardown, when the view has no buffer to answer from.
                focused: Cell<bool>,
            }

            #[glib::object_subclass]
            impl ObjectSubclass for ProbeImContext {
                const NAME: &str = "LushtextProbeImContext";
                type Type = super::ProbeImContext;
                type ParentType = gtk4::IMContext;

                fn class_init(klass: &mut Self::Class) {
                    // gtk4-rs installs a `get_preedit_string` trampoline that
                    // writes all three out-parameters unconditionally, but
                    // GTK callers may pass NULL for some of them (GtkText
                    // does while it realizes), which segfaulted the measurement
                    // build. The probe has no preedit, so restore GTK's own
                    // `GtkIMContext` implementation, which checks each
                    // pointer.
                    let parent = Self::type_data();
                    // SAFETY: both pointers are `GtkIMContextClass` structs
                    // for this type and its parent, valid for the program's
                    // lifetime once the type is registered; this runs inside
                    // class initialization, before any instance exists.
                    unsafe {
                        let parent_class =
                            parent.as_ref().parent_class() as *const gtk4::ffi::GtkIMContextClass;
                        let class =
                            std::ptr::from_mut(klass).cast::<gtk4::ffi::GtkIMContextClass>();
                        (*class).get_preedit_string = (*parent_class).get_preedit_string;
                        // GtkTextView answers `retrieve-surrounding` through
                        // `gtk_im_context_set_surrounding_with_selection`,
                        // which calls the class's `set_surrounding_with_selection`
                        // when set (GtkIMContext's own, which only stores the
                        // text privately) and falls back to `set_surrounding`
                        // otherwise. gtk4-rs exposes only the latter here, so
                        // clear the former to route the answer to the probe.
                        (*class).set_surrounding_with_selection = None;
                    }
                }
            }

            impl ObjectImpl for ProbeImContext {}

            impl ProbeImContext {
                fn request_surrounding(&self, trigger: ProbeImTrigger) {
                    if !self.focused.get() {
                        return;
                    }
                    self.surrounding.set(None);
                    let handled = self.obj().emit_by_name::<bool>("retrieve-surrounding", &[]);
                    super::super::record_im(trigger, self.surrounding.take(), handled);
                }

                fn remember(&self, text: &str, cursor_index: i32) {
                    let bytes = u64::try_from(text.len()).unwrap_or(u64::MAX);
                    self.surrounding.set(Some((bytes, i64::from(cursor_index))));
                }
            }

            impl IMContextImpl for ProbeImContext {
                fn focus_in(&self) {
                    self.focused.set(true);
                    self.request_surrounding(ProbeImTrigger::FocusIn);
                }

                fn focus_out(&self) {
                    self.focused.set(false);
                }

                fn set_cursor_location(&self, _area: &gtk4::gdk::Rectangle) {
                    self.request_surrounding(ProbeImTrigger::CursorLocation);
                }

                fn reset(&self) {
                    self.request_surrounding(ProbeImTrigger::Reset);
                }

                fn set_surrounding(&self, text: &str, cursor_index: i32) {
                    self.remember(text, cursor_index);
                }

                fn filter_keypress(&self, event: &gtk4::gdk::Event) -> bool {
                    if event.event_type() != gtk4::gdk::EventType::KeyPress {
                        return false;
                    }
                    self.request_surrounding(ProbeImTrigger::FilterKeypress);
                    let Some(key) = event.downcast_ref::<gtk4::gdk::KeyEvent>() else {
                        return false;
                    };
                    let blocking = gtk4::gdk::ModifierType::CONTROL_MASK
                        | gtk4::gdk::ModifierType::ALT_MASK
                        | gtk4::gdk::ModifierType::SUPER_MASK
                        | gtk4::gdk::ModifierType::META_MASK;
                    if key.modifier_state().intersects(blocking) {
                        return false;
                    }
                    match key.keyval().to_unicode() {
                        Some(character) if !character.is_control() => {
                            self.obj()
                                .emit_by_name::<()>("commit", &[&character.to_string()]);
                            true
                        }
                        _ => false,
                    }
                }
            }
        }

        glib::wrapper! {
            /// A `GtkIMContext` that asks for the surrounding text at every
            /// entry point an IBus engine with that capability would.
            pub struct ProbeImContext(ObjectSubclass<imp::ProbeImContext>)
                @extends gtk4::IMContext;
        }

        /// Register the probe input method on GTK's `gtk-im-module`
        /// extension point under [`PROBE_IM_ID`].
        pub(super) fn register() {
            let _ = gio::IOExtensionPoint::implement(
                "gtk-im-module",
                ProbeImContext::static_type(),
                PROBE_IM_ID,
                0,
            );
        }
    }
}
