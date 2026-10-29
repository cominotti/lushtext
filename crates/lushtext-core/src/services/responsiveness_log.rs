// SPDX-License-Identifier: GPL-3.0-or-later

//! The bounded evidence stream of the smoke-only responsiveness probe.
//!
//! `ui/responsiveness_probe.rs` measures the running application (main-loop
//! lateness, frame timing, input stamps, probe input-method calls) and hands
//! every observation here as a [`ProbeRecord`]. This module owns everything
//! about that evidence that needs no toolkit:
//!
//! - the one-line JSON encoding the `make responsiveness-smoke` driver reads
//!   (`scripts/responsiveness-smoke-driver.py`), with a stable `kind` per
//!   record and every time in `g_get_monotonic_time()` microseconds, which is
//!   `CLOCK_MONOTONIC` and therefore Python's `time.monotonic()`;
//! - [`LatenessAggregator`], which turns watchdog ticks into bounded evidence:
//!   one `late` record per tick at or above a threshold, a per-second log2
//!   histogram, and the top-K worst stalls;
//! - [`EvidenceWriter`], a bounded channel to one writer thread. Sending never
//!   blocks the GTK thread: a full channel drops the record and counts it, and
//!   the writer emits a `dropped` line before the next record it writes.
//!
//! No record carries document, note, draft, or search text: only times,
//! counts, sizes, and identifiers the probe assigns.
//!
//! Compiled only under `cfg(any(test, feature = "responsiveness-probes"))`:
//! shipped builds carry none of it, and the default unit-test lane tests it
//! without the feature.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::JoinHandle;

use crate::model::responsiveness_budget::{HISTOGRAM_BUCKETS, histogram_bucket};
use crate::services::filesystem::write::EvidenceAppendLog;

/// Version of the record schema; the driver refuses a log it does not know.
pub const PROBE_LOG_VERSION: u32 = 1;

/// Records the channel holds before sends start dropping (the fixed-capacity
/// ring between the GTK thread and the writer).
pub const EVIDENCE_CHANNEL_CAPACITY: usize = 4_096;

/// Default lateness at or above which one `late` record is written per tick.
pub const DEFAULT_LATE_THRESHOLD_US: u64 = 1_000;

/// Watchdog time between two `second` histogram records.
pub const SECOND_RECORD_INTERVAL_US: i64 = 1_000_000;

/// Watchdog time between two `topk` records.
pub const TOP_K_RECORD_INTERVAL_US: i64 = 5_000_000;

/// How many worst stalls a `topk` record carries.
pub const TOP_K: usize = 16;

/// The input event kinds the probe stamps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeInputKind {
    /// A key press.
    KeyPress,
    /// A key release.
    KeyRelease,
    /// A scroll (wheel or smooth) event.
    Scroll,
    /// A pointer button press.
    ButtonPress,
    /// A pointer button release.
    ButtonRelease,
}

impl ProbeInputKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::KeyPress => "key-press",
            Self::KeyRelease => "key-release",
            Self::Scroll => "scroll",
            Self::ButtonPress => "button-press",
            Self::ButtonRelease => "button-release",
        }
    }
}

/// Which probe input-method entry point asked for the surrounding text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeImTrigger {
    /// The text widget gained focus.
    FocusIn,
    /// The text widget reported a new cursor location.
    CursorLocation,
    /// A key event reached the input method.
    FilterKeypress,
    /// The text widget reset the input method.
    Reset,
}

impl ProbeImTrigger {
    const fn as_str(self) -> &'static str {
        match self {
            Self::FocusIn => "focus-in",
            Self::CursorLocation => "cursor-location",
            Self::FilterKeypress => "filter-keypress",
            Self::Reset => "reset",
        }
    }
}

/// One line of probe evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeRecord {
    /// The probe started: its configuration.
    Start {
        /// When the probe installed.
        t_us: i64,
        /// Process id of the measured application.
        pid: u32,
        /// The watchdog tick.
        tick_us: u64,
        /// The lateness at or above which a `late` record is written.
        late_threshold_us: u64,
        /// Whether the probe input method is registered.
        probe_im: bool,
    },
    /// One watchdog dispatch at least the threshold late.
    Late {
        /// When the watchdog was dispatched.
        t_us: i64,
        /// The ready time it was dispatched against.
        ready_us: i64,
        /// `t_us − ready_us`.
        lateness_us: u64,
    },
    /// The watchdog ticks since the previous `second` record.
    Second {
        /// When the interval closed.
        t_us: i64,
        /// Ticks in the interval.
        ticks: u64,
        /// The largest lateness in the interval.
        max_lateness_us: u64,
        /// Log2 histogram of the interval's latenesses (see
        /// `model::responsiveness_budget::histogram_bucket`).
        buckets: [u64; HISTOGRAM_BUCKETS],
    },
    /// The worst stalls since the probe started.
    TopK {
        /// When the record was taken.
        t_us: i64,
        /// `(dispatch time, lateness)` pairs, worst first.
        worst: Vec<(i64, u64)>,
    },
    /// One frame whose timings are final (or fell out of the clock's
    /// history).
    Frame {
        /// When the probe read the timings.
        t_us: i64,
        /// The probe's id for the window.
        window: u32,
        /// The frame clock's counter for the frame.
        frame_counter: i64,
        /// The frame's start time.
        frame_time_us: i64,
        /// When the compositor presented it, if it reported that.
        presentation_us: Option<i64>,
        /// The refresh interval the clock reported, if any.
        refresh_interval_us: Option<i64>,
    },
    /// One input event, stamped when the window's capture handler saw it.
    Input {
        /// The handler's monotonic time.
        t_us: i64,
        /// The probe's id for the window.
        window: u32,
        /// The event kind.
        event: ProbeInputKind,
        /// The event's own compositor timestamp (`gdk_event_get_time`, ms).
        event_time_ms: u32,
    },
    /// One probe input-method request for the surrounding text.
    Im {
        /// When the request returned.
        t_us: i64,
        /// What asked.
        trigger: ProbeImTrigger,
        /// The surrounding text's length in bytes, or `None` when the client
        /// supplied none.
        surrounding_bytes: Option<u64>,
        /// The cursor's byte index in the surrounding text, `-1` with none.
        cursor_index: i64,
        /// Whether the client answered the request.
        handled: bool,
    },
    /// One named startup or window-construction span
    /// (`bound-window-construction-dispatch`): an identifier the code chose,
    /// never content.
    Span {
        /// When the probe recorded it.
        t_us: i64,
        /// The span's name (`startup_toolkit`, `window_construction`,
        /// `stage:<name>`, `first_frame`, ...).
        name: &'static str,
        /// When the span started.
        start_us: i64,
        /// When it ended.
        end_us: i64,
    },
    /// Records the channel dropped since the previous written record.
    Dropped {
        /// When the writer noticed.
        t_us: i64,
        /// How many records were dropped.
        count: u64,
    },
}

impl ProbeRecord {
    /// Encode the record as one JSON line, newline included.
    #[must_use]
    pub fn to_json_line(&self) -> String {
        let mut line = String::with_capacity(128);
        self.write_json_line(&mut line);
        line
    }

    /// Append the record's JSON line, newline included, to `line` (the
    /// writer thread reuses one buffer for every record).
    pub fn write_json_line(&self, line: &mut String) {
        match self {
            Self::Start {
                t_us,
                pid,
                tick_us,
                late_threshold_us,
                probe_im,
            } => {
                let _ = write!(
                    line,
                    "{{\"kind\":\"start\",\"t_us\":{t_us},\"version\":{PROBE_LOG_VERSION},\
                     \"pid\":{pid},\"tick_us\":{tick_us},\"late_threshold_us\":{late_threshold_us},\
                     \"probe_im\":{probe_im}}}"
                );
            }
            Self::Late {
                t_us,
                ready_us,
                lateness_us,
            } => {
                let _ = write!(
                    line,
                    "{{\"kind\":\"late\",\"t_us\":{t_us},\"ready_us\":{ready_us},\
                     \"lateness_us\":{lateness_us}}}"
                );
            }
            Self::Second {
                t_us,
                ticks,
                max_lateness_us,
                buckets,
            } => {
                let _ = write!(
                    line,
                    "{{\"kind\":\"second\",\"t_us\":{t_us},\"ticks\":{ticks},\
                     \"max_lateness_us\":{max_lateness_us},\"buckets\":["
                );
                for (index, bucket) in buckets.iter().enumerate() {
                    let separator = if index > 0 { "," } else { "" };
                    let _ = write!(line, "{separator}{bucket}");
                }
                line.push_str("]}");
            }
            Self::TopK { t_us, worst } => {
                let _ = write!(line, "{{\"kind\":\"topk\",\"t_us\":{t_us},\"worst\":[");
                for (index, (at, lateness)) in worst.iter().enumerate() {
                    let separator = if index > 0 { "," } else { "" };
                    let _ = write!(line, "{separator}[{at},{lateness}]");
                }
                line.push_str("]}");
            }
            Self::Frame {
                t_us,
                window,
                frame_counter,
                frame_time_us,
                presentation_us,
                refresh_interval_us,
            } => {
                let source = if presentation_us.is_some() {
                    "presentation"
                } else {
                    "frame-time"
                };
                let _ = write!(
                    line,
                    "{{\"kind\":\"frame\",\"t_us\":{t_us},\"window\":{window},\
                     \"frame_counter\":{frame_counter},\"frame_time_us\":{frame_time_us},\
                     \"presentation_us\":{},\"refresh_interval_us\":{},\"source\":\"{source}\"}}",
                    JsonOption(*presentation_us),
                    JsonOption(*refresh_interval_us),
                );
            }
            Self::Input {
                t_us,
                window,
                event,
                event_time_ms,
            } => {
                let _ = write!(
                    line,
                    "{{\"kind\":\"input\",\"t_us\":{t_us},\"window\":{window},\
                     \"event\":\"{}\",\"event_time_ms\":{event_time_ms}}}",
                    event.as_str()
                );
            }
            Self::Im {
                t_us,
                trigger,
                surrounding_bytes,
                cursor_index,
                handled,
            } => {
                let _ = write!(
                    line,
                    "{{\"kind\":\"im\",\"t_us\":{t_us},\"trigger\":\"{}\",\
                     \"surrounding_bytes\":{},\"cursor_index\":{cursor_index},\
                     \"handled\":{handled}}}",
                    trigger.as_str(),
                    JsonOption(*surrounding_bytes),
                );
            }
            Self::Span {
                t_us,
                name,
                start_us,
                end_us,
            } => {
                let _ = write!(
                    line,
                    "{{\"kind\":\"span\",\"t_us\":{t_us},\"name\":\"{name}\",\
                     \"start_us\":{start_us},\"end_us\":{end_us}}}"
                );
            }
            Self::Dropped { t_us, count } => {
                let _ = write!(
                    line,
                    "{{\"kind\":\"dropped\",\"t_us\":{t_us},\"count\":{count}}}"
                );
            }
        }
        line.push('\n');
    }
}

/// An optional number as JSON: the value, or `null`.
struct JsonOption<T>(Option<T>);

impl<T: std::fmt::Display> std::fmt::Display for JsonOption<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(value) => value.fmt(formatter),
            None => formatter.write_str("null"),
        }
    }
}

/// Turns watchdog ticks into bounded evidence: `late`, `second`, and `topk`
/// records.
#[derive(Debug)]
pub struct LatenessAggregator {
    late_threshold_us: u64,
    interval_start_us: Option<i64>,
    last_top_k_us: Option<i64>,
    ticks: u64,
    max_lateness_us: u64,
    buckets: [u64; HISTOGRAM_BUCKETS],
    /// Worst first; at most [`TOP_K`] entries.
    worst: Vec<(i64, u64)>,
}

impl LatenessAggregator {
    /// An aggregator writing a `late` record per tick at or above
    /// `late_threshold_us`.
    #[must_use]
    pub fn new(late_threshold_us: u64) -> Self {
        Self {
            late_threshold_us,
            interval_start_us: None,
            last_top_k_us: None,
            ticks: 0,
            max_lateness_us: 0,
            buckets: [0; HISTOGRAM_BUCKETS],
            worst: Vec::with_capacity(TOP_K + 1),
        }
    }

    /// Account one watchdog dispatch at `now_us` against `ready_us`, and
    /// return the records it completes, in order.
    pub fn tick(&mut self, now_us: i64, ready_us: i64) -> Vec<ProbeRecord> {
        let lateness_us = u64::try_from(now_us.saturating_sub(ready_us)).unwrap_or(0);
        let mut records = Vec::new();
        let interval_start = *self.interval_start_us.get_or_insert(now_us);
        let last_top_k = *self.last_top_k_us.get_or_insert(now_us);
        self.ticks += 1;
        self.max_lateness_us = self.max_lateness_us.max(lateness_us);
        if let Some(bucket) = self.buckets.get_mut(histogram_bucket(lateness_us)) {
            *bucket += 1;
        }
        self.remember_worst(now_us, lateness_us);
        if lateness_us >= self.late_threshold_us {
            records.push(ProbeRecord::Late {
                t_us: now_us,
                ready_us,
                lateness_us,
            });
        }
        if now_us.saturating_sub(interval_start) >= SECOND_RECORD_INTERVAL_US {
            records.push(ProbeRecord::Second {
                t_us: now_us,
                ticks: self.ticks,
                max_lateness_us: self.max_lateness_us,
                buckets: self.buckets,
            });
            self.interval_start_us = Some(now_us);
            self.ticks = 0;
            self.max_lateness_us = 0;
            self.buckets = [0; HISTOGRAM_BUCKETS];
        }
        if now_us.saturating_sub(last_top_k) >= TOP_K_RECORD_INTERVAL_US {
            records.push(self.top_k_record(now_us));
            self.last_top_k_us = Some(now_us);
        }
        records
    }

    /// The worst stalls so far, as a `topk` record.
    #[must_use]
    pub fn top_k_record(&self, now_us: i64) -> ProbeRecord {
        ProbeRecord::TopK {
            t_us: now_us,
            worst: self.worst.clone(),
        }
    }

    fn remember_worst(&mut self, now_us: i64, lateness_us: u64) {
        if self.worst.len() == TOP_K
            && self
                .worst
                .last()
                .is_some_and(|&(_, smallest)| smallest >= lateness_us)
        {
            return;
        }
        let position = self.worst.partition_point(|&(_, kept)| kept >= lateness_us);
        self.worst.insert(position, (now_us, lateness_us));
        self.worst.truncate(TOP_K);
    }
}

/// What a window's frame clock reports for one queued frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameTimingsState {
    /// The clock no longer holds the frame: it left the clock's history.
    Missing,
    /// The clock holds the frame, but its timings are not final yet.
    Incomplete,
    /// The frame's timings are final.
    Complete {
        /// When the compositor presented the frame, if it reported that.
        presentation_us: Option<i64>,
        /// The refresh interval the clock reported, if any.
        refresh_interval_us: Option<i64>,
    },
}

/// The painted frames of one window whose `frame` records are not written
/// yet, oldest first, and the rule deciding when each one is.
///
/// A frame's timings become final only after its `after-paint`, when the
/// compositor's presentation feedback arrives, so the probe queues each
/// painted frame and writes it once it is final. A queued frame is final when
/// its timings are complete, when the clock no longer holds it, when the queue
/// is over capacity, or when a **later** queued frame is already complete:
/// presentation feedback arrives in commit order, so a frame that is still
/// incomplete behind a complete one never completes (its commit received no
/// feedback). Without that last rule one such frame at the head held every
/// frame behind it until it left the clock's 16-frame history, which a
/// session that paints fewer frames never reaches: staged window composition
/// (`first-frame-window-shell`) made the first frame exactly such a frame, and
/// the measurement build wrote no `frame` record at all.
#[derive(Debug)]
pub struct FrameTimingsQueue {
    /// `(frame counter, frame time)`, oldest first.
    pending: VecDeque<(i64, i64)>,
    capacity: usize,
}

impl FrameTimingsQueue {
    /// A queue holding at most `capacity` frames before the oldest is written
    /// without its timings.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            pending: VecDeque::with_capacity(capacity.saturating_add(1)),
            capacity,
        }
    }

    /// Queue the frame that just painted.
    pub fn painted(&mut self, frame_counter: i64, frame_time_us: i64) {
        self.pending.push_back((frame_counter, frame_time_us));
    }

    /// Frames still waiting for final timings.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Whether no frame is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Remove every queued frame that is now final, oldest first, as `frame`
    /// records of `window` read at `now_us`. `timings` reports a queued
    /// frame's state by its counter; it is asked once per queued frame.
    pub fn take_final(
        &mut self,
        window: u32,
        now_us: i64,
        mut timings: impl FnMut(i64) -> FrameTimingsState,
    ) -> Vec<ProbeRecord> {
        let states: Vec<FrameTimingsState> = self
            .pending
            .iter()
            .map(|&(counter, _)| timings(counter))
            .collect();
        // Every frame up to the newest complete one is final.
        let settled = states
            .iter()
            .rposition(|state| matches!(state, FrameTimingsState::Complete { .. }))
            .map_or(0, |index| index + 1);
        let mut records = Vec::new();
        for (index, state) in states.into_iter().enumerate() {
            let over_capacity = self.pending.len() > self.capacity;
            if index >= settled && state == FrameTimingsState::Incomplete && !over_capacity {
                break;
            }
            let Some((frame_counter, frame_time_us)) = self.pending.pop_front() else {
                break;
            };
            let (presentation_us, refresh_interval_us) = match state {
                FrameTimingsState::Complete {
                    presentation_us,
                    refresh_interval_us,
                } => (presentation_us, refresh_interval_us),
                FrameTimingsState::Missing | FrameTimingsState::Incomplete => (None, None),
            };
            records.push(ProbeRecord::Frame {
                t_us: now_us,
                window,
                frame_counter,
                frame_time_us,
                presentation_us,
                refresh_interval_us,
            });
        }
        records
    }
}

/// The GTK-thread side of the evidence stream: a bounded channel to one
/// writer thread.
#[derive(Debug)]
pub struct EvidenceWriter {
    sender: Option<SyncSender<ProbeRecord>>,
    dropped: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
}

impl EvidenceWriter {
    /// Start a writer appending to `path` through the filesystem boundary.
    ///
    /// # Errors
    ///
    /// Returns the error opening `path` or spawning the thread.
    pub fn to_path(path: &Path, capacity: usize) -> std::io::Result<Self> {
        let mut log = EvidenceAppendLog::open(path)?;
        Self::spawn(capacity, move |line: &[u8]| log.append(line))
    }

    /// Start a writer that hands each encoded line to `append`, on its own
    /// thread. A failing `append` is counted as a drop; the writer keeps
    /// draining so senders never block.
    ///
    /// # Errors
    ///
    /// Returns the error spawning the thread.
    pub fn spawn(
        capacity: usize,
        mut append: impl FnMut(&[u8]) -> std::io::Result<()> + Send + 'static,
    ) -> std::io::Result<Self> {
        let (sender, receiver): (SyncSender<ProbeRecord>, Receiver<ProbeRecord>) =
            mpsc::sync_channel(capacity);
        let dropped = Arc::new(AtomicU64::new(0));
        let worker_dropped = Arc::clone(&dropped);
        let worker = std::thread::Builder::new()
            .name("lushtext-responsiveness-log".to_owned())
            .spawn(move || {
                let mut line = String::with_capacity(512);
                for record in receiver {
                    let pending = worker_dropped.swap(0, Ordering::Relaxed);
                    if pending > 0 {
                        line.clear();
                        ProbeRecord::Dropped {
                            t_us: record_time(&record),
                            count: pending,
                        }
                        .write_json_line(&mut line);
                        if append(line.as_bytes()).is_err() {
                            worker_dropped.fetch_add(pending, Ordering::Relaxed);
                        }
                    }
                    line.clear();
                    record.write_json_line(&mut line);
                    if append(line.as_bytes()).is_err() {
                        worker_dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            dropped,
            worker: Some(worker),
        })
    }

    /// Queue `record` without blocking; a full channel drops and counts it.
    pub fn send(&self, record: ProbeRecord) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        // Full or disconnected: either way the record is dropped and counted.
        if sender.try_send(record).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Records dropped and not yet reported by a `dropped` line.
    #[must_use]
    pub fn pending_drops(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Close the channel and wait for the writer to drain it.
    pub fn finish(mut self) {
        self.close();
    }

    fn close(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for EvidenceWriter {
    fn drop(&mut self) {
        self.close();
    }
}

const fn record_time(record: &ProbeRecord) -> i64 {
    match record {
        ProbeRecord::Start { t_us, .. }
        | ProbeRecord::Late { t_us, .. }
        | ProbeRecord::Second { t_us, .. }
        | ProbeRecord::TopK { t_us, .. }
        | ProbeRecord::Frame { t_us, .. }
        | ProbeRecord::Input { t_us, .. }
        | ProbeRecord::Im { t_us, .. }
        | ProbeRecord::Span { t_us, .. }
        | ProbeRecord::Dropped { t_us, .. } => *t_us,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::mpsc::channel;

    #[test]
    fn every_record_kind_encodes_as_one_json_line() {
        let mut buckets = [0; HISTOGRAM_BUCKETS];
        buckets[2] = 5;
        let lines = [
            ProbeRecord::Start {
                t_us: 1,
                pid: 42,
                tick_us: 2_000,
                late_threshold_us: 1_000,
                probe_im: true,
            },
            ProbeRecord::Late {
                t_us: 10,
                ready_us: 4,
                lateness_us: 6,
            },
            ProbeRecord::Second {
                t_us: 20,
                ticks: 5,
                max_lateness_us: 3,
                buckets,
            },
            ProbeRecord::TopK {
                t_us: 30,
                worst: vec![(9, 70), (8, 60)],
            },
            ProbeRecord::Frame {
                t_us: 40,
                window: 1,
                frame_counter: 7,
                frame_time_us: 35,
                presentation_us: None,
                refresh_interval_us: Some(8_333),
            },
            ProbeRecord::Input {
                t_us: 50,
                window: 1,
                event: ProbeInputKind::Scroll,
                event_time_ms: 123,
            },
            ProbeRecord::Im {
                t_us: 60,
                trigger: ProbeImTrigger::CursorLocation,
                surrounding_bytes: Some(1_894_586),
                cursor_index: 12,
                handled: true,
            },
            ProbeRecord::Dropped { t_us: 70, count: 3 },
            ProbeRecord::Span {
                t_us: 80,
                name: "window_construction",
                start_us: 5,
                end_us: 75,
            },
        ]
        .map(|record| record.to_json_line());
        assert_eq!(
            lines[0],
            "{\"kind\":\"start\",\"t_us\":1,\"version\":1,\"pid\":42,\"tick_us\":2000,\
             \"late_threshold_us\":1000,\"probe_im\":true}\n"
        );
        assert_eq!(
            lines[1],
            "{\"kind\":\"late\",\"t_us\":10,\"ready_us\":4,\"lateness_us\":6}\n"
        );
        assert_eq!(
            lines[2],
            "{\"kind\":\"second\",\"t_us\":20,\"ticks\":5,\"max_lateness_us\":3,\
             \"buckets\":[0,0,5,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}\n"
        );
        assert_eq!(
            lines[3],
            "{\"kind\":\"topk\",\"t_us\":30,\"worst\":[[9,70],[8,60]]}\n"
        );
        assert_eq!(
            lines[4],
            "{\"kind\":\"frame\",\"t_us\":40,\"window\":1,\"frame_counter\":7,\
             \"frame_time_us\":35,\"presentation_us\":null,\"refresh_interval_us\":8333,\
             \"source\":\"frame-time\"}\n"
        );
        assert_eq!(
            lines[5],
            "{\"kind\":\"input\",\"t_us\":50,\"window\":1,\"event\":\"scroll\",\
             \"event_time_ms\":123}\n"
        );
        assert_eq!(
            lines[6],
            "{\"kind\":\"im\",\"t_us\":60,\"trigger\":\"cursor-location\",\
             \"surrounding_bytes\":1894586,\"cursor_index\":12,\"handled\":true}\n"
        );
        assert_eq!(lines[7], "{\"kind\":\"dropped\",\"t_us\":70,\"count\":3}\n");
        assert_eq!(
            lines[8],
            "{\"kind\":\"span\",\"t_us\":80,\"name\":\"window_construction\",\
             \"start_us\":5,\"end_us\":75}\n"
        );
        for line in lines {
            assert_eq!(line.matches('\n').count(), 1, "{line}");
        }
    }

    #[test]
    fn aggregator_writes_late_ticks_seconds_and_the_worst_stalls() {
        let mut aggregator = LatenessAggregator::new(1_000);
        assert!(aggregator.tick(0, 0).is_empty());
        let late = aggregator.tick(2_500, 1_000);
        assert_eq!(
            late,
            vec![ProbeRecord::Late {
                t_us: 2_500,
                ready_us: 1_000,
                lateness_us: 1_500
            }]
        );
        assert!(aggregator.tick(4_000, 3_900).is_empty());
        let closing = aggregator.tick(1_000_000, 999_999);
        let ProbeRecord::Second {
            ticks,
            max_lateness_us,
            buckets,
            ..
        } = closing[0]
        else {
            panic!("expected a second record, got {closing:?}");
        };
        assert_eq!(ticks, 4);
        assert_eq!(max_lateness_us, 1_500);
        assert_eq!(buckets.iter().sum::<u64>(), 4);
        assert_eq!(buckets[histogram_bucket(1_500)], 1);
        let ProbeRecord::TopK { worst, .. } = aggregator.top_k_record(1) else {
            unreachable!("top_k_record returns a topk record");
        };
        assert_eq!(worst[0], (2_500, 1_500));
        assert!(worst.is_sorted_by(|a, b| a.1 >= b.1));
    }

    #[test]
    fn top_k_keeps_only_the_worst() {
        let mut aggregator = LatenessAggregator::new(u64::MAX);
        for step in 0..100i64 {
            let lateness = (step * 37) % 101;
            let _ = aggregator.tick(step * 10, step * 10 - lateness);
        }
        let ProbeRecord::TopK { worst, .. } = aggregator.top_k_record(0) else {
            unreachable!("top_k_record returns a topk record");
        };
        assert_eq!(worst.len(), TOP_K);
        assert!(worst.is_sorted_by(|a, b| a.1 >= b.1));
        assert_eq!(worst[0].1, 100);
    }

    #[test]
    fn a_full_channel_drops_counts_and_reports_before_the_next_record() {
        let written = Arc::new(Mutex::new(Vec::<String>::new()));
        let (entered_tx, entered) = channel::<()>();
        let (release, gate) = channel::<()>();
        let sink = Arc::clone(&written);
        let mut first = true;
        let writer = EvidenceWriter::spawn(2, move |line| {
            if first {
                // Hold the writer inside its first record so the channel fills.
                first = false;
                let _ = entered_tx.send(());
                let _ = gate.recv();
            }
            sink.lock()
                .expect("sink")
                .push(String::from_utf8_lossy(line).into_owned());
            Ok(())
        })
        .expect("writer starts");
        let record = |t_us| ProbeRecord::Late {
            t_us,
            ready_us: 0,
            lateness_us: 0,
        };
        writer.send(record(1));
        entered.recv().expect("the writer takes the first record");
        // Capacity 2: two queue, three drop, and send never blocks.
        for t_us in 2..7 {
            writer.send(record(t_us));
        }
        assert_eq!(writer.pending_drops(), 3);
        release.send(()).expect("release the writer");
        writer.finish();
        let lines = written.lock().expect("sink").clone();
        assert_eq!(
            lines,
            vec![
                record(1).to_json_line(),
                ProbeRecord::Dropped { t_us: 2, count: 3 }.to_json_line(),
                record(2).to_json_line(),
                record(3).to_json_line(),
            ]
        );
    }

    #[test]
    fn a_failing_append_counts_as_dropped_and_the_writer_keeps_draining() {
        let writer = EvidenceWriter::spawn(8, |_| Err(std::io::Error::other("disk full")))
            .expect("writer starts");
        for t_us in 0..4 {
            writer.send(ProbeRecord::Dropped { t_us, count: 0 });
        }
        let dropped = Arc::clone(&writer.dropped);
        writer.finish();
        assert!(dropped.load(Ordering::Relaxed) >= 1);
    }

    /// The frame counters and presentation times of `frame` records.
    fn frames(records: &[ProbeRecord]) -> Vec<(i64, Option<i64>)> {
        records
            .iter()
            .map(|record| match record {
                ProbeRecord::Frame {
                    frame_counter,
                    presentation_us,
                    ..
                } => (*frame_counter, *presentation_us),
                other => panic!("not a frame record: {other:?}"),
            })
            .collect()
    }

    fn presented(frame_counter: i64) -> FrameTimingsState {
        FrameTimingsState::Complete {
            presentation_us: Some(frame_counter * 100 + 50),
            refresh_interval_us: Some(8_333),
        }
    }

    #[test]
    fn a_frame_that_never_completes_does_not_hold_the_frames_behind_it() {
        // Frame 0 gets no presentation feedback; every later frame is
        // presented before the next one paints (the shape staged window
        // composition produces). Fewer than the clock's 16-frame history
        // paint, so no frame ever leaves it.
        let mut queue = FrameTimingsQueue::new(64);
        let mut written = Vec::new();
        for painted in 0..4 {
            queue.painted(painted, painted * 100);
            written.extend(queue.take_final(1, painted, |counter| {
                if counter == 0 || counter == painted {
                    FrameTimingsState::Incomplete
                } else {
                    presented(counter)
                }
            }));
        }
        assert_eq!(
            frames(&written),
            vec![(0, None), (1, Some(150)), (2, Some(250))],
            "the incomplete first frame is written once a later one completed"
        );
        assert_eq!(queue.len(), 1, "the newest frame waits for its feedback");
    }

    #[test]
    fn an_incomplete_frame_waits_while_nothing_after_it_completed() {
        let mut queue = FrameTimingsQueue::new(64);
        queue.painted(0, 0);
        queue.painted(1, 100);
        let written = queue.take_final(1, 5, |_| FrameTimingsState::Incomplete);
        assert!(written.is_empty());
        assert_eq!(queue.len(), 2);
        let written = queue.take_final(1, 6, presented);
        assert_eq!(frames(&written), vec![(0, Some(50)), (1, Some(150))]);
        assert!(queue.is_empty());
    }

    #[test]
    fn a_frame_the_clock_dropped_is_written_without_timings() {
        let mut queue = FrameTimingsQueue::new(64);
        queue.painted(3, 300);
        queue.painted(4, 400);
        let written = queue.take_final(2, 9, |counter| {
            if counter == 3 {
                FrameTimingsState::Missing
            } else {
                FrameTimingsState::Incomplete
            }
        });
        assert_eq!(frames(&written), vec![(3, None)]);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn an_over_capacity_queue_writes_its_oldest_frames() {
        let mut queue = FrameTimingsQueue::new(2);
        for counter in 0..4 {
            queue.painted(counter, counter);
        }
        let written = queue.take_final(1, 9, |_| FrameTimingsState::Incomplete);
        assert_eq!(frames(&written), vec![(0, None), (1, None)]);
        assert_eq!(queue.len(), 2);
    }
}
