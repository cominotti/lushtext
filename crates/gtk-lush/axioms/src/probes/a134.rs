// SPDX-License-Identifier: MIT OR Apache-2.0

//! A134: a `GtkPopover`'s popup surface shares its toplevel's frame clock, so
//! the two surfaces' paints in one frame share one `GdkFrameTimings`, while
//! each surface keeps its own Wayland presentation feedbacks. **GTK ≥ 4.24**
//! (4.24.x, `gdk/wayland/gdkwaylandpresentationtime.c` and
//! `gdk/gdkframetimings.c`): a `discarded` for the popover's feedback marks
//! the shared frame `DISCARDED` when the popover has no other feedback for
//! that frame, and the toplevel's `presented` for the same frame then prints,
//! once per process (`g_warning_once`),
//! `Gdk-WARNING: gdk_frame_timings_presented() called on discarded frame.`.
//! **GTK < 4.24** has no such state machine, and prints nothing. GTK `main`
//! removed the transition in d2595c6b ("frameclock: Rework discarded()",
//! MR !10242), which `gtk-4-24` does not carry, so a toolkit with it reaches
//! the bound and fails the `≥ 4.24` branch: the signal to drop the widget
//! runner's classification of the line.
//!
//! The trigger is the compositor's: Mutter 51.1 discards the feedback of a
//! popover's committed but unpainted buffer when the popover is repositioned
//! (`xdg_popup.reposition`). The fixture repositions the popover in the
//! frame clock's `after-paint` of the frame that drew its first buffer, every
//! cycle.
//!
//! The warning is printed once per process, so the fixture runs in a child,
//! this executable re-run with [`CONTROL_ENV`] set to `A134` and
//! `WAYLAND_DEBUG=client`, with its output captured. The control is read from
//! the child's protocol trace: in one paint (no event between the two
//! requests) the toplevel and the popover each asked for a feedback, the
//! popover's was `discarded`, and the toplevel's was `presented` after that.
//! That ordered pair is the precondition of the GDK transition; a discard
//! alone is not (a content change without a reposition draws discards too,
//! and no warning). At or above the bound the pair is a control, so a silent
//! run on a toolkit with the fix is a violation, not a fixture failure; below
//! it the pair is recorded only. The axiom is the warning count on the
//! toolkit's side of the bound.

use std::collections::{HashMap, HashSet};
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use gtk4::prelude::*;

use crate::AxiomId;
use crate::observation::{Observation, Recorder};
use crate::probes::session_names::CONTROL_ENV;
use crate::version::catalogued_bound;

/// The cycles the child runs: popup, content change, popdown, each with the
/// reposition. Every traced run on GTK 4.24.1 showed the ordered pair in its
/// first cycles; a run without one is fixture-invalid.
pub const CYCLES: u32 = 60;
/// The delay before the first cycle, so the toplevel is mapped and painting.
const FIRST_CYCLE_DELAY: Duration = Duration::from_millis(500);
/// From `popup()` to the content change.
const CONTENT_DELAY: Duration = Duration::from_millis(4);
/// From the content change to `popdown()`, and from `popdown()` to the next
/// cycle.
const STEP_DELAY: Duration = Duration::from_millis(25);
/// The child gives up, and exits 2, after this long.
const CHILD_TIMEOUT: Duration = Duration::from_secs(90);
/// The first word of each line the child prints.
const CHILD_LINE_PREFIX: &str = "gtk-lush-axioms popover-frames:";
/// The message GTK 4.24 prints, once per process.
pub const DISCARDED_FRAME_WARNING: &str =
    "gdk_frame_timings_presented() called on discarded frame.";

/// The role a traced `wl_surface` got.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Role {
    Toplevel,
    Popup,
}

/// What the child's protocol trace shows.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct TraceCounts {
    /// `xdg_popup.reposition` requests.
    pub repositions: u32,
    /// Feedbacks the toplevel's surface asked for.
    pub toplevel_feedbacks: u32,
    /// Feedbacks the popover's surface asked for.
    pub popup_feedbacks: u32,
    /// `discarded` events for the popover's feedbacks.
    pub popup_discarded: u32,
    /// Distinct paints in which the popover's feedback was discarded and the
    /// toplevel's feedback of the same paint was presented after it.
    pub ordered_pairs: u32,
}

/// The `#<id>` right after `prefix` in `line`.
fn id_after(line: &str, prefix: &str) -> Option<u32> {
    let start = line.find(prefix)? + prefix.len();
    let digits: String = line[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Read a `WAYLAND_DEBUG=client` trace. Ids are reused once released, so a
/// feedback is owned by the request that last created it, and a surface's
/// role by the last `get_xdg_surface` naming it. A paint is the run of
/// requests between two events: GDK commits every surface it draws in one
/// paint, feedback requests included, and events are only dispatched between
/// paints.
#[must_use]
pub fn parse_trace(trace: &str) -> TraceCounts {
    let mut counts = TraceCounts::default();
    let mut xdg_to_wl: HashMap<u32, u32> = HashMap::new();
    let mut roles: HashMap<u32, Role> = HashMap::new();
    // Live feedbacks: id -> (role of its surface, paint number).
    let mut feedbacks: HashMap<u32, (Role, u32)> = HashMap::new();
    let mut discarded_paints: HashSet<u32> = HashSet::new();
    let mut paired_paints: HashSet<u32> = HashSet::new();
    let mut paint = 0u32;
    let mut last_was_event = true;
    for line in trace.lines() {
        if !line.starts_with('[') {
            continue;
        }
        if line.contains(" -> ") {
            if last_was_event {
                paint += 1;
            }
            last_was_event = false;
            if line.contains(".get_xdg_surface(") {
                if let (Some(xdg), Some(wl)) = (
                    id_after(line, "new id xdg_surface#"),
                    id_after(line, ", wl_surface#"),
                ) {
                    xdg_to_wl.insert(xdg, wl);
                }
            } else if line.contains(".get_toplevel(") || line.contains(".get_popup(") {
                let role = if line.contains(".get_popup(") {
                    Role::Popup
                } else {
                    Role::Toplevel
                };
                if let Some(wl) =
                    id_after(line, "xdg_surface#").and_then(|xdg| xdg_to_wl.get(&xdg).copied())
                {
                    roles.insert(wl, role);
                }
            } else if line.contains("wp_presentation#") && line.contains(".feedback(") {
                if let (Some(wl), Some(feedback)) = (
                    id_after(line, "(wl_surface#"),
                    id_after(line, "new id wp_presentation_feedback#"),
                ) && let Some(&role) = roles.get(&wl)
                {
                    match role {
                        Role::Toplevel => counts.toplevel_feedbacks += 1,
                        Role::Popup => counts.popup_feedbacks += 1,
                    }
                    feedbacks.insert(feedback, (role, paint));
                }
            } else if line.contains("xdg_popup#") && line.contains(".reposition(") {
                counts.repositions += 1;
            }
            continue;
        }
        last_was_event = true;
        let Some(feedback) = id_after(line, "wp_presentation_feedback#") else {
            continue;
        };
        if line.contains(".discarded(") {
            if let Some((role, of_paint)) = feedbacks.remove(&feedback)
                && role == Role::Popup
            {
                counts.popup_discarded += 1;
                discarded_paints.insert(of_paint);
            }
        } else if line.contains(".presented(")
            && let Some((role, of_paint)) = feedbacks.remove(&feedback)
            && role == Role::Toplevel
            && discarded_paints.contains(&of_paint)
        {
            paired_paints.insert(of_paint);
        }
    }
    counts.ordered_pairs = u32::try_from(paired_paints.len()).unwrap_or(u32::MAX);
    counts
}

/// What one fixture child did.
#[derive(Debug)]
pub struct PopoverFramesOutcome {
    /// The cycles the child reported, when it got to the end.
    pub cycles: Option<u32>,
    /// The child's exit code, when it exited.
    pub code: Option<i32>,
    /// What its protocol trace shows.
    pub trace: TraceCounts,
    /// `Gdk-WARNING` lines carrying [`DISCARDED_FRAME_WARNING`].
    pub discarded_frame_warnings: u32,
    /// Every other `gdk_frame_timings_*` warning line.
    pub other_frame_timing_warnings: u32,
}

impl PopoverFramesOutcome {
    /// One line for a readout.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "cycles {}; exit {}; repositions {}; popover feedbacks {} ({} discarded); \
             discarded-then-presented paints {}; discarded-frame warnings {}; other \
             frame-timing warnings {}",
            self.cycles
                .map_or_else(|| "?".to_owned(), |c| c.to_string()),
            self.code.map_or_else(|| "?".to_owned(), |c| c.to_string()),
            self.trace.repositions,
            self.trace.popup_feedbacks,
            self.trace.popup_discarded,
            self.trace.ordered_pairs,
            self.discarded_frame_warnings,
            self.other_frame_timing_warnings,
        )
    }
}

/// Run the fixture child, traced, in this process's display.
///
/// # Errors
///
/// The error text when the child cannot be started.
pub fn popover_frames_in_child() -> Result<PopoverFramesOutcome, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let output = Command::new(executable)
        .env(CONTROL_ENV, AxiomId::new(134).to_string())
        .env("WAYLAND_DEBUG", "client")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let cycles = stdout.lines().find_map(|line| {
        line.strip_prefix(CHILD_LINE_PREFIX)?
            .trim()
            .strip_prefix("cycles=")?
            .parse()
            .ok()
    });
    let mut discarded_frame_warnings = 0;
    let mut other_frame_timing_warnings = 0;
    for line in stderr.lines() {
        if !line.contains("-WARNING **") || !line.contains("gdk_frame_timings_") {
            continue;
        }
        if line.contains("Gdk-WARNING **") && line.ends_with(DISCARDED_FRAME_WARNING) {
            discarded_frame_warnings += 1;
        } else {
            other_frame_timing_warnings += 1;
        }
    }
    Ok(PopoverFramesOutcome {
        cycles,
        code: output.status.code(),
        trace: parse_trace(&stderr),
        discarded_frame_warnings,
        other_frame_timing_warnings,
    })
}

/// The fixture, shared by the child and the sample: a toplevel with a column
/// of rows and a content area, and one popover on the fourth row set up like
/// a file peek (right of the row, no arrow, autohide, a fixed size, a
/// "loading" page and a text page).
#[derive(Debug)]
pub struct PopoverFramesFixture {
    /// The toplevel window.
    pub window: gtk4::Window,
    /// The popover.
    pub popover: gtk4::Popover,
    rows: Vec<gtk4::Label>,
    content: gtk4::Box,
    stack: gtk4::Stack,
    text: gtk4::TextView,
}

impl PopoverFramesFixture {
    /// Build the fixture; nothing is presented.
    #[must_use]
    pub fn new() -> Self {
        let window = gtk4::Window::builder()
            .default_width(1200)
            .default_height(800)
            .title("A134 popover frames")
            .build();
        let outer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let side = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let rows: Vec<gtk4::Label> = (0..30)
            .map(|index| {
                let label = gtk4::Label::new(Some(&format!("file-{index}.txt")));
                label.set_xalign(0.0);
                side.append(&label);
                label
            })
            .collect();
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_hexpand(true);
        outer.append(&side);
        outer.append(&content);
        window.set_child(Some(&outer));

        let popover = gtk4::Popover::new();
        popover.set_parent(&rows[3]);
        popover.set_position(gtk4::PositionType::Right);
        popover.set_has_arrow(false);
        popover.set_autohide(true);
        popover.set_size_request(420, 320);
        popover.add_css_class("card");
        let stack = gtk4::Stack::new();
        stack.add_named(&gtk4::Label::new(Some("Loading preview…")), Some("loading"));
        let text = gtk4::TextView::new();
        text.set_editable(false);
        text.set_monospace(true);
        stack.add_named(&text, Some("text"));
        popover.set_child(Some(&stack));
        Self {
            window,
            popover,
            rows,
            content,
            stack,
            text,
        }
    }

    /// One cycle's first step: the loading page, a row label change, and
    /// `popup()`.
    fn popup(&self, cycle: u32) {
        self.stack.set_visible_child_name("loading");
        self.rows[3].set_label(&format!("file-3.txt (selected {cycle})"));
        self.popover.popup();
    }

    /// The second step: the text page, and `popup()` again.
    fn show_content(&self, cycle: u32) {
        let body: Vec<String> = (0..40)
            .map(|line| format!("line {line} of cycle {cycle}"))
            .collect();
        self.text.buffer().set_text(&body.join("\n"));
        self.stack.set_visible_child_name("text");
        self.popover.popup();
    }

    /// The third step: `popdown()`, and the toplevel's content replaced.
    fn popdown(&self) {
        self.popover.popdown();
        if let Some(child) = self.content.first_child() {
            self.content.remove(&child);
        }
        let view = gtk4::TextView::new();
        view.set_vexpand(true);
        view.buffer().set_text(&"x".repeat(4000));
        self.content.append(&view);
    }

    /// Move the popover's anchor, as a peek following its row does.
    fn reposition(&self, cycle: u32) {
        let shift = i32::try_from(cycle % 5).unwrap_or(0);
        self.popover
            .set_pointing_to(Some(&gtk4::gdk::Rectangle::new(6 + shift, 4, 300, 30)));
    }

    /// Run `cycles` cycles on the presented window and call `done` with the
    /// cycles run and the repositions made. In the frame clock's
    /// `after-paint` of each cycle's first frame with the popover mapped, the
    /// popover is repositioned.
    pub fn run_cycles(self: &std::rc::Rc<Self>, cycles: u32, done: impl Fn(u32, u32) + 'static) {
        use std::cell::Cell;
        use std::rc::Rc;

        let cycle = Rc::new(Cell::new(0u32));
        let armed = Rc::new(Cell::new(false));
        let repositions = Rc::new(Cell::new(0u32));
        if let Some(clock) = self.window.frame_clock() {
            let fixture = Rc::downgrade(self);
            let (cycle, armed, repositions) = (
                Rc::clone(&cycle),
                Rc::clone(&armed),
                Rc::clone(&repositions),
            );
            clock.connect_after_paint(move |_| {
                let Some(fixture) = fixture.upgrade() else {
                    return;
                };
                if armed.get() && fixture.popover.is_mapped() {
                    armed.set(false);
                    fixture.reposition(cycle.get());
                    repositions.set(repositions.get() + 1);
                }
            });
        }
        let done = Rc::new(done);
        Self::schedule_popup(
            self,
            FIRST_CYCLE_DELAY,
            cycles,
            cycle,
            armed,
            repositions,
            done,
        );
    }

    fn schedule_popup(
        fixture: &std::rc::Rc<Self>,
        delay: Duration,
        cycles: u32,
        cycle: std::rc::Rc<std::cell::Cell<u32>>,
        armed: std::rc::Rc<std::cell::Cell<bool>>,
        repositions: std::rc::Rc<std::cell::Cell<u32>>,
        done: std::rc::Rc<dyn Fn(u32, u32)>,
    ) {
        let fixture = std::rc::Rc::clone(fixture);
        glib::timeout_add_local_once(delay, move || {
            armed.set(true);
            fixture.popup(cycle.get());
            glib::timeout_add_local_once(CONTENT_DELAY, move || {
                fixture.show_content(cycle.get());
                glib::timeout_add_local_once(STEP_DELAY, move || {
                    fixture.popdown();
                    cycle.set(cycle.get() + 1);
                    if cycle.get() >= cycles {
                        done(cycle.get(), repositions.get());
                    } else {
                        Self::schedule_popup(
                            &fixture,
                            STEP_DELAY,
                            cycles,
                            cycle,
                            armed,
                            repositions,
                            done,
                        );
                    }
                });
            });
        });
    }
}

impl Default for PopoverFramesFixture {
    fn default() -> Self {
        Self::new()
    }
}

/// The fixture child (`GTK_LUSH_AXIOMS_CONTROL=A134`): present the fixture,
/// run [`CYCLES`] cycles, report them, and exit.
pub(crate) fn popover_frames_child() -> ExitCode {
    use std::cell::Cell;
    use std::rc::Rc;

    if let Err(error) = crate::init_toolkit() {
        eprintln!("{CHILD_LINE_PREFIX} GTK did not initialize: {error}");
        return ExitCode::from(2);
    }
    let fixture = Rc::new(PopoverFramesFixture::new());
    fixture.window.present();
    let main_loop = glib::MainLoop::new(None, false);
    let finished = Rc::new(Cell::new(None::<(u32, u32)>));
    {
        let (main_loop, finished) = (main_loop.clone(), Rc::clone(&finished));
        fixture.run_cycles(CYCLES, move |cycles, repositions| {
            finished.set(Some((cycles, repositions)));
            main_loop.quit();
        });
    }
    let timeout = {
        let main_loop = main_loop.clone();
        glib::timeout_add_local_once(CHILD_TIMEOUT, move || main_loop.quit())
    };
    main_loop.run();
    let Some((cycles, repositions)) = finished.get() else {
        eprintln!("{CHILD_LINE_PREFIX} the cycles did not finish within {CHILD_TIMEOUT:?}");
        return ExitCode::from(2);
    };
    timeout.remove();
    println!("{CHILD_LINE_PREFIX} cycles={cycles}");
    println!("{CHILD_LINE_PREFIX} repositions={repositions}");
    fixture.popover.unparent();
    fixture.window.destroy();
    ExitCode::SUCCESS
}

/// Probe A134. See the module documentation.
#[must_use]
pub fn probe_a134() -> Observation {
    Recorder::run(AxiomId::new(134), |recorder| {
        let outcome = popover_frames_in_child();
        if let Err(error) = &outcome {
            recorder.measure("child_error", error);
        }
        let outcome = recorder.require(outcome.ok(), "control: the fixture child started")?;
        recorder.measure("child", outcome.summary());
        recorder.measure("cycles", outcome.cycles.unwrap_or(0));
        recorder.measure("repositions", outcome.trace.repositions);
        recorder.measure("popup_feedbacks", outcome.trace.popup_feedbacks);
        recorder.measure("popup_discarded", outcome.trace.popup_discarded);
        recorder.measure("ordered_pairs", outcome.trace.ordered_pairs);
        recorder.measure("discarded_frame_warnings", outcome.discarded_frame_warnings);
        recorder.measure(
            "other_frame_timing_warnings",
            outcome.other_frame_timing_warnings,
        );
        recorder.control(
            outcome.code == Some(0) && outcome.cycles == Some(CYCLES),
            "control: the child ran every cycle and exited 0",
        )?;
        recorder.control(
            outcome.trace.repositions > 0 && outcome.trace.popup_feedbacks > 0,
            "control: the trace shows the popover repositioned and asking for feedbacks",
        )?;
        recorder.control(
            outcome.other_frame_timing_warnings == 0,
            "control: no other frame-timing warning was printed",
        )?;
        if catalogued_bound(recorder, AxiomId::new(134))?.reached() {
            // Without the ordered pair a silent run would say nothing about
            // GDK, so the toolkit side needs it; a fixed toolkit then fails
            // the axiom, not the control.
            recorder.control(
                outcome.trace.ordered_pairs > 0,
                "control: in one paint the popover's feedback was discarded and the \
                 toplevel's was presented after it",
            )?;
            recorder.axiom(
                outcome.discarded_frame_warnings == 1,
                "A134 (GTK >= 4.24): the toplevel's presented after the popover's discard \
                 prints `gdk_frame_timings_presented() called on discarded frame.` once",
            )
        } else {
            // Recorded, not required: the claim below the bound is GDK's
            // alone, and the floor's compositor may not discard at all (the
            // GNOME 50 CI image runs Mutter 50, which was not traced).
            recorder.axiom(
                outcome.discarded_frame_warnings == 0,
                "A134 (GTK < 4.24): the same sequence prints no frame-timing warning",
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Abridged from a traced GTK 4.24.1 run of the fixture
    /// (`tab-strip-overflow-navigation` attribution, `out-trace/run-1.log`).
    const TRACE: &str = "\
[18:55:23.774694] {Default Queue}  -> xdg_wm_base#44.get_xdg_surface(new id xdg_surface#46, wl_surface#25)
[18:55:23.774698]  -> xdg_surface#46.get_toplevel(new id xdg_toplevel#47)
[18:55:23.786779] {Default Queue}  -> wp_presentation#28.feedback(wl_surface#25, new id wp_presentation_feedback#55)
[18:55:23.794258] {Default Queue} wp_presentation_feedback#55.presented(0, 1039854, 765773000, 16666667, 0, 1, 4)
[18:55:24.279133] {Default Queue}  -> xdg_wm_base#44.get_xdg_surface(new id xdg_surface#58, wl_surface#55)
[18:55:24.279136]  -> xdg_surface#58.get_popup(new id xdg_popup#59, xdg_surface#46, xdg_positioner#57)
[18:55:24.281000] {Default Queue} wl_callback#54.done(1039855270)
[18:55:24.281419]  -> wl_surface#25.attach(wl_buffer#51, 0, 0)
[18:55:24.281421] {Default Queue}  -> wp_presentation#28.feedback(wl_surface#25, new id wp_presentation_feedback#60)
[18:55:24.281423]  -> wl_surface#25.commit()
[18:55:24.282046]  -> wl_surface#55.attach(wl_buffer#62, 0, 0)
[18:55:24.282048] {Default Queue}  -> wp_presentation#28.feedback(wl_surface#55, new id wp_presentation_feedback#66)
[18:55:24.282050]  -> wl_surface#55.commit()
[18:55:24.282272]  -> xdg_popup#59.reposition(xdg_positioner#67, 1)
[18:55:24.285153] xdg_popup#59.repositioned(1)
[18:55:24.285214] {Default Queue} wp_presentation_feedback#66.discarded()
[18:55:24.298694] {Default Queue} wp_presentation_feedback#60.presented(0, 1039855, 270143000, 16666667, 0, 2, 4)
(process:2808320): Gdk-WARNING **: 15:55:24.298: gdk_frame_timings_presented() called on discarded frame.
[18:55:24.299127] {Default Queue}  -> wp_presentation#28.feedback(wl_surface#25, new id wp_presentation_feedback#66)
[18:55:24.314786] {Default Queue} wp_presentation_feedback#66.presented(0, 1039855, 285819000, 16666667, 0, 3, 4)
";

    #[test]
    fn the_trace_shows_one_discarded_then_presented_paint() {
        assert_eq!(
            parse_trace(TRACE),
            TraceCounts {
                repositions: 1,
                toplevel_feedbacks: 3,
                popup_feedbacks: 1,
                popup_discarded: 1,
                ordered_pairs: 1,
            }
        );
    }

    #[test]
    fn a_presented_before_the_discard_is_not_the_ordered_pair() {
        let swapped = TRACE
            .replace(
                "wp_presentation_feedback#66.discarded()",
                "wp_presentation_feedback#60.presented(0, 1, 2, 3, 0, 2, 4)",
            )
            .replacen(
                "wp_presentation_feedback#60.presented(0, 1039855, 270143000, 16666667, 0, 2, 4)",
                "wp_presentation_feedback#66.discarded()",
                1,
            );
        let counts = parse_trace(&swapped);
        assert_eq!(counts.popup_discarded, 1);
        assert_eq!(counts.ordered_pairs, 0);
    }

    #[test]
    fn feedbacks_of_different_paints_are_not_paired() {
        // An event between the two feedback requests puts them in two paints.
        let split = TRACE.replace(
            "[18:55:24.282046]  -> wl_surface#55.attach",
            "[18:55:24.282000] wl_callback#56.done(1)\n\
             [18:55:24.282046]  -> wl_surface#55.attach",
        );
        assert_eq!(parse_trace(&split).ordered_pairs, 0);
    }
}
