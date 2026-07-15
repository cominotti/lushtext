// SPDX-License-Identifier: GPL-3.0-or-later

//! Worker-side destruction for large plain-Rust UI payloads.
//!
//! Unlike ordinary GTK task dispatch, this lane releases worker capacity as
//! soon as destruction finishes and never waits for a main-loop callback. Its
//! bounded queue isolates supersession churn from file-I/O worker admission.

use std::sync::OnceLock;

use crossbeam_channel::{Sender, TrySendError, bounded};

const DISPOSAL_WORKERS: usize = 2;
const DISPOSAL_QUEUE_CAPACITY: usize = 64;

type DisposalJob = Box<dyn FnOnce() + Send + 'static>;

fn disposal_sender() -> &'static Sender<DisposalJob> {
    static SENDER: OnceLock<Sender<DisposalJob>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, receiver) = bounded::<DisposalJob>(DISPOSAL_QUEUE_CAPACITY);
        for index in 0..DISPOSAL_WORKERS {
            let receiver = receiver.clone();
            std::thread::Builder::new()
                .name(format!("lushtext-disposal-{index}"))
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        job();
                    }
                })
                .expect("plain disposal worker should start");
        }
        sender
    })
}

/// Destroy one Send payload off GTK without consuming the general task pool.
pub(crate) fn spawn(job: impl FnOnce() + Send + 'static) {
    match disposal_sender().try_send(Box::new(job)) {
        Ok(()) => {}
        Err(TrySendError::Full(job)) => {
            // Saturation is exceptional because the two consumers perform only
            // destruction. Backpressure here keeps both retained payloads and
            // OS threads hard-bounded without falling back to a large GTK drop.
            disposal_sender()
                .send(job)
                .expect("plain disposal workers should remain connected");
        }
        Err(TrySendError::Disconnected(_job)) => {
            panic!("plain disposal workers unexpectedly disconnected");
        }
    }
}

/// Destroy plain data off GTK, then publish compact state on the main loop.
pub(crate) fn spawn_then_main<S>(
    state: S,
    dispose: impl FnOnce() + Send + 'static,
    then: impl FnOnce(S) + 'static,
) where
    S: 'static,
{
    let guarded_state = glib::thread_guard::ThreadGuard::new(state);
    let guarded_then = glib::thread_guard::ThreadGuard::new(then);
    spawn(move || {
        dispose();
        glib::idle_add_once(move || {
            guarded_then.into_inner()(guarded_state.into_inner());
        });
    });
}

#[cfg(test)]
mod tests {
    use super::{DISPOSAL_QUEUE_CAPACITY, DISPOSAL_WORKERS, spawn};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn saturated_lane_applies_fixed_worker_backpressure_and_drains() {
        let release = Arc::new(AtomicBool::new(false));
        let started = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(AtomicUsize::new(0));
        for _ in 0..DISPOSAL_WORKERS {
            let release = Arc::clone(&release);
            let started = Arc::clone(&started);
            let completed = Arc::clone(&completed);
            spawn(move || {
                started.fetch_add(1, Ordering::AcqRel);
                while !release.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
                completed.fetch_add(1, Ordering::AcqRel);
            });
        }
        let start_deadline = Instant::now() + Duration::from_secs(2);
        while started.load(Ordering::Acquire) != DISPOSAL_WORKERS && Instant::now() < start_deadline
        {
            std::thread::yield_now();
        }
        assert_eq!(started.load(Ordering::Acquire), DISPOSAL_WORKERS);

        for _ in 0..DISPOSAL_QUEUE_CAPACITY {
            let completed = Arc::clone(&completed);
            spawn(move || {
                completed.fetch_add(1, Ordering::AcqRel);
            });
        }

        let (attempted_tx, attempted_rx) = mpsc::channel();
        let (returned_tx, returned_rx) = mpsc::channel();
        let overflow_completed = Arc::clone(&completed);
        let producer = std::thread::spawn(move || {
            attempted_tx.send(()).expect("signal overflow attempt");
            spawn(move || {
                overflow_completed.fetch_add(1, Ordering::AcqRel);
            });
            returned_tx.send(()).expect("signal overflow return");
        });
        attempted_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("overflow producer should attempt admission");
        assert!(
            returned_rx.recv_timeout(Duration::from_millis(20)).is_err(),
            "overflow admission should wait while both workers and the queue are full"
        );
        release.store(true, Ordering::Release);
        returned_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("overflow admission should resume when capacity returns");
        producer.join().expect("disposal producer should finish");

        let deadline = Instant::now() + Duration::from_secs(2);
        let expected = DISPOSAL_QUEUE_CAPACITY + DISPOSAL_WORKERS + 1;
        while completed.load(Ordering::Acquire) != expected && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(completed.load(Ordering::Acquire), expected);
    }
}
