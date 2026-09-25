// SPDX-License-Identifier: MIT OR Apache-2.0

//! The shared frame of every axiom sample: the two modes, and the window.
//!
//! **Interactive (default):** a window shows the axiom's statement, the
//! fixture its probe measures, buttons that trigger the behaviour, and the
//! values the probe reads, refreshed live. It is for a person on a desktop
//! session; nothing automated runs it on one.
//!
//! **`--check`:** run the probe, print its observation as one JSON line, and
//! exit `0` (holds), `1` (violated) or `2` (fixture invalid). It refuses with
//! exit `77`, before touching GTK, unless `GTK_LUSH_AXIOMS_HEADLESS=1` is set,
//! so it can never flash probe windows across a live desktop.
//! `make gtk-axiom-sample AXIOM=<id> CHECK=1` sets it inside a private
//! headless session.
//!
//! This module is example code, not public API: each sample includes it with
//! `mod support;`.

use std::process::ExitCode;
use std::time::Duration;

use gtk_lush_axioms::{AxiomId, find, init_toolkit};
use gtk4::prelude::*;

/// The variable that authorizes `--check` to open probe windows.
const HEADLESS_ENV: &str = "GTK_LUSH_AXIOMS_HEADLESS";
/// When set, the interactive window is captured to this PNG and closed.
const SCREENSHOT_ENV: &str = "GTK_LUSH_AXIOMS_SCREENSHOT";
/// Exit code for "this host or session cannot run the check".
const REFUSED_EXIT_CODE: u8 = 77;
/// How often the live readout refreshes.
const READOUT_INTERVAL: Duration = Duration::from_millis(100);
/// How long the screenshot mode lets the window settle before capturing.
const SCREENSHOT_SETTLE: Duration = Duration::from_millis(1_500);

/// The interactive window a sample fills in.
pub struct SampleUi {
    controls: libadwaita::WrapBox,
    readout: gtk4::Label,
    fixture_frame: gtk4::Frame,
}

impl SampleUi {
    /// Show `fixture`, the widget the probe measures.
    pub fn set_fixture(&self, fixture: &impl IsA<gtk4::Widget>) {
        self.fixture_frame.set_child(Some(fixture));
    }

    /// Add a button that runs `action` when clicked.
    pub fn add_control(&self, label: &str, action: impl Fn() + 'static) {
        let button = gtk4::Button::with_label(label);
        button.connect_clicked(move |_| action());
        self.controls.append(&button);
    }

    /// Show the values `describe` returns, refreshed live.
    pub fn set_readout(&self, describe: impl Fn() -> String + 'static) {
        let weak = self.readout.downgrade();
        self.readout.set_text(&describe());
        gtk4::glib::timeout_add_local(READOUT_INTERVAL, move || {
            let Some(label) = weak.upgrade() else {
                return gtk4::glib::ControlFlow::Break;
            };
            let text = describe();
            if label.text() != text {
                label.set_text(&text);
            }
            gtk4::glib::ControlFlow::Continue
        });
    }
}

/// Run the sample for `axiom`: `--check` runs its probe, anything else opens
/// the interactive window that `build` fills in.
pub fn run(axiom: AxiomId, build: fn(&SampleUi)) -> ExitCode {
    if std::env::args()
        .skip(1)
        .any(|argument| argument == "--check")
    {
        check(axiom)
    } else {
        interactive(axiom, build)
    }
}

fn check(axiom: AxiomId) -> ExitCode {
    if std::env::var(HEADLESS_ENV).as_deref() != Ok("1") {
        eprintln!(
            "refusing --check: it opens probe windows, so it runs only in a private headless \
             session. Use `make gtk-axiom-sample AXIOM={axiom} CHECK=1`."
        );
        return ExitCode::from(REFUSED_EXIT_CODE);
    }
    let Some(probe) = find(axiom).and_then(|entry| entry.probe) else {
        eprintln!("{axiom} has no probe in the catalogue");
        return ExitCode::from(REFUSED_EXIT_CODE);
    };
    if let Err(error) = init_toolkit() {
        eprintln!("GTK did not initialize: {error}");
        return ExitCode::from(REFUSED_EXIT_CODE);
    }
    let observation = probe();
    println!("{}", observation.to_json_line());
    ExitCode::from(observation.verdict.exit_code())
}

fn interactive(axiom: AxiomId, build: fn(&SampleUi)) -> ExitCode {
    let application = libadwaita::Application::builder()
        .application_id(format!("dev.gtk_lush.Axioms.A{:02}", axiom.number()))
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.connect_activate(move |application| {
        let window = sample_window(application, axiom, build);
        window.present();
        if let Some(path) = std::env::var_os(SCREENSHOT_ENV) {
            capture_after_settle(&window, path.into());
        }
    });
    let status = application.run_with_args::<&str>(&[]);
    ExitCode::from(status.get())
}

fn sample_window(
    application: &libadwaita::Application,
    axiom: AxiomId,
    build: fn(&SampleUi),
) -> libadwaita::ApplicationWindow {
    let entry = find(axiom);
    let title = format!("Axiom {axiom}");
    let statement = gtk4::Label::builder()
        .label(entry.map_or("not catalogued", |entry| entry.statement))
        .wrap(true)
        .xalign(0.0)
        .css_classes(["title-4"])
        .build();
    let designs = gtk4::Label::builder()
        .label(format!(
            "Relied on by: {}",
            entry.map_or(String::new(), |entry| entry.dependent_designs.join("; "))
        ))
        .wrap(true)
        .xalign(0.0)
        .css_classes(["dim-label"])
        .build();
    let controls = libadwaita::WrapBox::builder()
        .child_spacing(6)
        .line_spacing(6)
        .build();
    let readout = gtk4::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .css_classes(["monospace"])
        .build();
    let fixture_frame = gtk4::Frame::builder().vexpand(true).build();
    let ui = SampleUi {
        controls: controls.clone(),
        readout: readout.clone(),
        fixture_frame: fixture_frame.clone(),
    };
    build(&ui);

    let content = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    content.append(&statement);
    content.append(&designs);
    content.append(&controls);
    content.append(&readout);
    content.append(&fixture_frame);

    let toolbar = libadwaita::ToolbarView::new();
    toolbar.add_top_bar(
        &libadwaita::HeaderBar::builder()
            .title_widget(&libadwaita::WindowTitle::new(
                &title,
                "gtk-lush-axioms sample",
            ))
            .build(),
    );
    toolbar.set_content(Some(&content));
    libadwaita::ApplicationWindow::builder()
        .application(application)
        .title(title)
        .default_width(760)
        .default_height(720)
        .content(&toolbar)
        .build()
}

/// Render `window` to a PNG at `path` once it has settled, then close it.
fn capture_after_settle(window: &libadwaita::ApplicationWindow, path: std::path::PathBuf) {
    let window = window.clone();
    gtk4::glib::timeout_add_local_once(SCREENSHOT_SETTLE, move || {
        match render_png(&window, &path) {
            Ok(()) => println!("screenshot: {}", path.display()),
            Err(error) => eprintln!("screenshot failed: {error}"),
        }
        window.close();
    });
}

fn render_png(
    window: &libadwaita::ApplicationWindow,
    path: &std::path::Path,
) -> Result<(), String> {
    let paintable = gtk4::WidgetPaintable::new(Some(window));
    let (width, height) = (window.width(), window.height());
    let snapshot = gtk4::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(width), f64::from(height));
    let node = snapshot.to_node().ok_or("the window drew nothing")?;
    let renderer = window.renderer().ok_or("the window has no renderer")?;
    let texture = renderer.render_texture(&node, None);
    texture.save_to_png(path).map_err(|error| error.to_string())
}
