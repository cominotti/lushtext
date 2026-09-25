// SPDX-License-Identifier: MIT OR Apache-2.0

//! A21: `gtk_window_destroy` (`GtkWindowExt::destroy`) on a window that
//! belongs to a registered `GtkApplication` removes it from the application
//! at once — `window-removed` is emitted before `destroy()` returns, and
//! afterwards `application()` is `None` and `windows()` no longer lists it —
//! but it does **not** dispose the window while another strong reference is
//! alive: the dispose vfunc has not run and a `glib::WeakRef` still upgrades.
//! Dispose runs only when the last strong reference drops, just before the
//! window is finalized.
//!
//! So teardown that must happen when a window is destroyed cannot live only
//! in `dispose`, and a registry that finds its peers through weak references
//! must drop a window on `window-removed`, not wait for its dispose.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use crate::observation::{Recorder, Stop};
use crate::session::{REALIZE_BUDGET, flush_events, wait_until};
use crate::{AxiomId, Observation};

/// The size the fixture window is presented at.
pub const WINDOW_SIZE: (i32, i32) = (240, 160);

/// What happened to one [`DisposeCountingWindow`]: how many times its
/// `dispose` ran, and whether (and after how many disposals) it was
/// finalized. It outlives the window, so it can be read after finalization.
#[derive(Clone, Debug, Default)]
pub struct DisposalLog(Rc<Counts>);

#[derive(Debug, Default)]
struct Counts {
    disposals: Cell<u32>,
    finalizations: Cell<u32>,
    disposals_at_finalize: Cell<u32>,
}

impl DisposalLog {
    /// How many times the window's `dispose` has run.
    #[must_use]
    pub fn disposals(&self) -> u32 {
        self.0.disposals.get()
    }

    /// How many times the window has been finalized (0 or 1).
    #[must_use]
    pub fn finalizations(&self) -> u32 {
        self.0.finalizations.get()
    }

    /// How many disposals had run when the window was finalized.
    #[must_use]
    pub fn disposals_at_finalize(&self) -> u32 {
        self.0.disposals_at_finalize.get()
    }

    fn record_dispose(&self) {
        self.0.disposals.set(self.0.disposals.get() + 1);
    }

    fn record_finalize(&self) {
        self.0.finalizations.set(self.0.finalizations.get() + 1);
        self.0.disposals_at_finalize.set(self.0.disposals.get());
    }
}

mod imp {
    use std::cell::RefCell;

    use gtk4::glib;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct DisposeCountingWindow {
        pub log: RefCell<Option<super::DisposalLog>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DisposeCountingWindow {
        const NAME: &str = "GtkLushAxiomsDisposeCountingWindow";
        type Type = super::DisposeCountingWindow;
        type ParentType = gtk4::Window;
    }

    impl ObjectImpl for DisposeCountingWindow {
        fn dispose(&self) {
            if let Some(log) = self.log.borrow().as_ref() {
                log.record_dispose();
            }
        }
    }

    impl WidgetImpl for DisposeCountingWindow {}
    impl WindowImpl for DisposeCountingWindow {}

    impl Drop for DisposeCountingWindow {
        // The instance struct is dropped when GObject finalizes the window.
        fn drop(&mut self) {
            if let Some(log) = self.log.get_mut() {
                log.record_finalize();
            }
        }
    }
}

gtk4::glib::wrapper! {
    /// A plain `GtkWindow` that records each `dispose` and its finalization
    /// in a [`DisposalLog`].
    pub struct DisposeCountingWindow(ObjectSubclass<imp::DisposeCountingWindow>)
        @extends gtk4::Window, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget,
            gtk4::Native, gtk4::Root, gtk4::ShortcutManager;
}

impl DisposeCountingWindow {
    /// A window of [`WINDOW_SIZE`] that records into `log`.
    #[must_use]
    pub fn new(log: &DisposalLog) -> Self {
        let window: Self = gtk4::glib::Object::new();
        window.imp().log.replace(Some(log.clone()));
        let (width, height) = WINDOW_SIZE;
        window.set_default_size(width, height);
        window
    }
}

/// What the application's `window-removed` handler saw for the fixture.
#[derive(Debug, Default)]
struct Removals {
    emissions: u32,
    during_destroy: u32,
    disposals_at_emission: Option<u32>,
}

/// Probe A21. See the module documentation.
#[must_use]
pub fn probe_a21() -> Observation {
    Recorder::run(AxiomId::new(21), |recorder| {
        let application = gtk4::Application::builder()
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        let registered = application.register(None::<&gtk4::gio::Cancellable>);
        recorder.measure("application_registered", application.is_registered());
        recorder.control(
            registered.is_ok() && application.is_registered(),
            "control: the application registers",
        )?;

        let log = DisposalLog::default();
        let window = DisposeCountingWindow::new(&log);
        window.set_application(Some(&application));
        window.present();
        let realized = wait_until(REALIZE_BUDGET, || window.width() > 0);
        recorder.control(realized, "control: the fixture window realizes")?;
        flush_events();
        observe_destroy(recorder, &application, window, &log)
    })
}

/// Destroy `window` while an extra strong reference is held, then drop every
/// strong reference, measuring the application's view and the window's
/// disposals at each step.
fn observe_destroy(
    recorder: &mut Recorder,
    application: &gtk4::Application,
    window: DisposeCountingWindow,
    log: &DisposalLog,
) -> Result<(), Stop> {
    let weak = window.downgrade();
    let destroying = Rc::new(Cell::new(false));
    let removals = Rc::new(RefCell::new(Removals::default()));
    let handler = {
        let weak = weak.clone();
        let destroying = destroying.clone();
        let removals = removals.clone();
        let log = log.clone();
        application.connect_window_removed(move |_, removed| {
            let is_fixture = weak
                .upgrade()
                .is_some_and(|window| window.upcast_ref::<gtk4::Window>() == removed);
            if is_fixture {
                let mut seen = removals.borrow_mut();
                seen.emissions += 1;
                seen.during_destroy += u32::from(destroying.get());
                seen.disposals_at_emission = Some(log.disposals());
            }
        })
    };
    let listed = |application: &gtk4::Application| {
        application.windows().iter().any(|listed| {
            weak.upgrade()
                .is_some_and(|window| window.upcast_ref::<gtk4::Window>() == listed)
        })
    };

    recorder.measure("listed_before_destroy", listed(application));
    recorder.measure("window_removed_before_destroy", removals.borrow().emissions);
    recorder.measure("disposals_before_destroy", log.disposals());
    recorder.control(
        listed(application)
            && window.application().as_ref() == Some(application)
            && removals.borrow().emissions == 0
            && log.disposals() == 0,
        "control: before destroy the window belongs to the application, \
         window-removed has not fired, and dispose has not run",
    )?;

    let extra = window.clone();
    recorder.measure("ref_count_before_destroy", window.ref_count());
    destroying.set(true);
    window.destroy();
    destroying.set(false);
    let (emissions, during_destroy, disposals_at_emission) = {
        let seen = removals.borrow();
        (
            seen.emissions,
            seen.during_destroy,
            seen.disposals_at_emission,
        )
    };
    recorder.measure("window_removed_emissions", emissions);
    recorder.measure("window_removed_during_destroy", during_destroy);
    recorder.measure(
        "disposals_at_window_removed",
        disposals_at_emission.map_or_else(|| "none".to_owned(), |count| count.to_string()),
    );
    recorder.axiom(
        emissions == 1 && during_destroy == 1,
        "A21: destroy emits window-removed for the window once, before it returns",
    )?;
    let application_after = window.application();
    recorder.measure(
        "application_after_destroy_is_none",
        application_after.is_none(),
    );
    recorder.measure("listed_after_destroy", listed(application));
    recorder.axiom(
        application_after.is_none() && !listed(application),
        "A21: after destroy the window has no application and the application no longer lists it",
    )?;
    recorder.measure("disposals_after_destroy", log.disposals());
    recorder.measure("alive_after_destroy", weak.upgrade().is_some());
    recorder.measure("ref_count_after_destroy", window.ref_count());
    recorder.axiom(
        log.disposals() == 0 && weak.upgrade().is_some(),
        "A21: destroy does not dispose a window another strong reference keeps alive",
    )?;

    flush_events();
    drop(window);
    recorder.measure("disposals_after_dropping_the_original", log.disposals());
    recorder.measure(
        "alive_while_the_extra_reference_lives",
        weak.upgrade().is_some(),
    );
    recorder.axiom(
        log.disposals() == 0 && weak.upgrade().is_some(),
        "A21: the window stays undisposed while the extra strong reference lives",
    )?;

    drop(extra);
    flush_events();
    application.disconnect(handler);
    recorder.measure("disposals_final", log.disposals());
    recorder.measure("finalizations", log.finalizations());
    recorder.measure("disposals_at_finalize", log.disposals_at_finalize());
    recorder.measure("alive_final", weak.upgrade().is_some());
    recorder.axiom(
        log.disposals() >= 1
            && log.finalizations() == 1
            && log.disposals_at_finalize() == log.disposals()
            && weak.upgrade().is_none(),
        "A21: dropping the last strong reference disposes the window, then finalizes it",
    )
}
