// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sample for A21: destroying a window of the application removes it from
//! the application at once, but does not dispose it while the sample still
//! holds a strong reference; dropping that reference disposes it. Open a
//! window, destroy it, and watch `window-removed`, the disposals, and the
//! weak reference before and after dropping the sample's own reference.

mod support;

use std::cell::{Cell, RefCell};
use std::process::ExitCode;
use std::rc::Rc;

use gtk_lush_axioms::AxiomId;
use gtk_lush_axioms::fixtures::{DisposalLog, DisposeCountingWindow};
use gtk4::prelude::*;

/// The window under observation and what the sample knows about it.
#[derive(Default)]
struct Watched {
    log: RefCell<DisposalLog>,
    strong: RefCell<Option<DisposeCountingWindow>>,
    weak: RefCell<gtk4::glib::WeakRef<DisposeCountingWindow>>,
    removed: Cell<u32>,
}

fn build(ui: &support::SampleUi) {
    let explanation = gtk4::Label::builder()
        .label(
            "The sample keeps one strong reference to the window it opens. Destroy it: \
             window-removed fires and the window leaves the application, yet dispose \
             has not run. Drop the reference: dispose runs, then finalize.",
        )
        .wrap(true)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    ui.set_fixture(&explanation);
    let watched = Rc::new(Watched::default());

    let opener = watched.clone();
    let anchor = explanation;
    ui.add_control("Open a window of the application", move || {
        let Some(application) = anchor
            .root()
            .and_downcast::<gtk4::Window>()
            .and_then(|root| root.application())
        else {
            return;
        };
        let log = DisposalLog::default();
        let window = DisposeCountingWindow::new(&log);
        window.set_title(Some("A21 fixture"));
        window.set_application(Some(&application));
        let counter = opener.clone();
        let target = window.downgrade();
        application.connect_window_removed(move |_, removed| {
            if target
                .upgrade()
                .is_some_and(|window| window.upcast_ref::<gtk4::Window>() == removed)
            {
                counter.removed.set(counter.removed.get() + 1);
            }
        });
        opener.removed.set(0);
        opener.log.replace(log);
        opener.weak.replace(window.downgrade());
        window.present();
        opener.strong.replace(Some(window));
    });
    let destroyer = watched.clone();
    ui.add_control("Destroy it (the sample keeps its reference)", move || {
        if let Some(window) = destroyer.strong.borrow().as_ref() {
            window.destroy();
        }
    });
    let dropper = watched.clone();
    ui.add_control("Drop the sample's reference", move || {
        dropper.strong.replace(None);
    });
    ui.set_readout(move || {
        let log = watched.log.borrow();
        let alive = watched.weak.borrow().upgrade();
        format!(
            "window-removed ×{}   in application {}   disposals {}   finalized {}   \
             weak ref upgrades {}   sample holds a reference {}",
            watched.removed.get(),
            alive
                .as_ref()
                .is_some_and(|window| window.application().is_some()),
            log.disposals(),
            log.finalizations() > 0,
            alive.is_some(),
            watched.strong.borrow().is_some(),
        )
    });
}

fn main() -> ExitCode {
    support::run(AxiomId::new(21), build)
}
