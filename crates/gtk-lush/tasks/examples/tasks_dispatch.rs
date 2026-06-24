// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal background-task dispatch example for GTK Lush task freshness.
//!
//! It demonstrates how a blocking worker result returns to the main loop and is
//! accepted only when its freshness token still matches the current request.

use std::cell::Cell;
use std::rc::Rc;

use gtk_lush_tasks::{FreshnessToken, spawn_blocking_then};

fn main() {
    let main_loop = glib::MainLoop::new(None, false);
    let completed = Rc::new(Cell::new(false));

    let requested = FreshnessToken::new(1);
    let current = FreshnessToken::new(1);
    let completed_for_callback = Rc::clone(&completed);
    let main_loop_for_callback = main_loop.clone();

    spawn_blocking_then(
        requested,
        || String::from("background result"),
        move |token, result| {
            if let Ok(fresh) = token.accept(current, result) {
                println!("{}", fresh.into_inner());
                completed_for_callback.set(true);
            }
            main_loop_for_callback.quit();
        },
    );

    main_loop.run();
    assert!(completed.get());
}
