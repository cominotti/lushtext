// SPDX-License-Identifier: MIT OR Apache-2.0

//! The GSettings backend fixture of A38–A41: a keyfile backend over a private
//! settings file, and the schema GTK itself installs.
//!
//! A probe never touches the process's default settings backend or the
//! user's settings. [`KeyfileSettingsFixture`] creates a directory of its own
//! inside the process's temporary directory (the probe runner gives every run
//! a private one), builds `g_keyfile_settings_backend_new` over a file in it,
//! and removes the directory when dropped. Every `gio::Settings` a probe makes
//! is `g_settings_new_full` with that one backend object, which is how two
//! instances of an application share its default backend.
//!
//! The schema is `org.gtk.gtk4.Settings.FileChooser`, which GTK 4 installs,
//! so the fixture needs no compiled schema of its own. A backend write is
//! observed on the file itself: `g_file_replace_contents` writes a temporary
//! file and renames it over the settings file, so every write gives the file a
//! new inode.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use gtk4::gio;
use gtk4::prelude::*;

/// The GTK 4 schema the fixture's settings use.
pub const SCHEMA_ID: &str = "org.gtk.gtk4.Settings.FileChooser";
/// Boolean keys of [`SCHEMA_ID`] the probes write.
pub const BOOLEAN_KEYS: [&str; 3] = ["show-hidden", "sort-directories-first", "show-size-column"];
/// An integer key of [`SCHEMA_ID`] the probes write.
pub const INTEGER_KEY: &str = "sidebar-width";

/// The identity of the settings file: a backend write changes the inode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    /// The file's inode.
    pub inode: u64,
    /// The file's size in bytes.
    pub size: u64,
}

/// A keyfile settings backend over a private file, removed on drop.
pub struct KeyfileSettingsFixture {
    dir: PathBuf,
    path: PathBuf,
    backend: gio::SettingsBackend,
    schema: gio::SettingsSchema,
}

impl KeyfileSettingsFixture {
    /// Build the fixture under a fresh directory named for `tag`, with the
    /// settings file already holding `padding_bytes` of an unrelated group
    /// (so a write has a known amount of text to serialize). `None` when the
    /// GTK 4 schema is not installed or the directory cannot be created.
    #[must_use]
    pub fn new(tag: &str, padding_bytes: usize) -> Option<Self> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let schema = gio::SettingsSchemaSource::default()?.lookup(SCHEMA_ID, true)?;
        let dir = std::env::temp_dir().join(format!(
            "gtk-lush-axioms-gsettings-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("keyfile");
        let mut seed = String::from("[org/gtk/gtk4/settings/file-chooser]\nshow-hidden=false\n");
        if padding_bytes > 0 {
            seed.push_str("\n[padding]\n");
            let mut index = 0usize;
            while seed.len() < padding_bytes {
                seed.push_str(&format!("key{index}='{}'\n", "x".repeat(96)));
                index += 1;
            }
        }
        std::fs::write(&path, seed).ok()?;
        let backend = gio::keyfile_settings_backend_new(path.to_str()?, "/", None);
        Some(Self {
            dir,
            path,
            backend,
            schema,
        })
    }

    /// A new settings instance on the fixture's one backend.
    #[must_use]
    pub fn settings(&self) -> gio::Settings {
        gio::Settings::new_full(&self.schema, Some(&self.backend), None)
    }

    /// The settings file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The settings file's identity now, `None` when it cannot be read.
    #[must_use]
    pub fn identity(&self) -> Option<FileIdentity> {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(&self.path).ok()?;
        Some(FileIdentity {
            inode: metadata.ino(),
            size: metadata.size(),
        })
    }

    /// The settings file's text now (empty when it cannot be read).
    #[must_use]
    pub fn contents(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }

    /// Whether the settings file holds `key=value` in the schema's group.
    #[must_use]
    pub fn file_has(&self, key: &str, value: &str) -> bool {
        let contents = self.contents();
        let mut in_group = false;
        for line in contents.lines() {
            if line.starts_with('[') {
                in_group = line == "[org/gtk/gtk4/settings/file-chooser]";
            } else if in_group && line == format!("{key}={value}") {
                return true;
            }
        }
        false
    }
}

impl KeyfileSettingsFixture {
    /// Start counting the backend's replacements of the settings file.
    ///
    /// A `GFileMonitor` on the directory (with `WATCH_MOVES`) reports each
    /// temporary file renamed over the settings file. Events reach the calling
    /// thread's main context, so read the count with
    /// [`WriteWatch::settled_count`], which drains it first.
    #[must_use]
    pub fn watch_writes(&self) -> Option<WriteWatch> {
        let directory = gio::File::for_path(&self.dir);
        let monitor = directory
            .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
            .ok()?;
        let count = Rc::new(Cell::new(0u32));
        let counter = Rc::clone(&count);
        let target = self.path.clone();
        monitor.connect_changed(move |_, file, other, event| {
            let renamed_onto_target = match event {
                gio::FileMonitorEvent::Renamed => {
                    other.and_then(gio::File::path) == Some(target.clone())
                }
                gio::FileMonitorEvent::MovedIn => file.path() == Some(target.clone()),
                _ => false,
            };
            if renamed_onto_target {
                counter.set(counter.get() + 1);
            }
        });
        Some(WriteWatch {
            _monitor: monitor,
            count,
        })
    }
}

/// Counts the backend's replacements of the settings file.
pub struct WriteWatch {
    _monitor: gio::FileMonitor,
    count: Rc<Cell<u32>>,
}

impl WriteWatch {
    /// The replacements delivered so far, without waiting for more.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.count.get()
    }

    /// The replacements seen so far, after letting pending monitor events
    /// arrive (the main context is drained for `EVENT_SETTLE`).
    #[must_use]
    pub fn settled_count(&self) -> u32 {
        let deadline = Instant::now() + EVENT_SETTLE;
        while Instant::now() < deadline {
            drain_main_context();
            std::thread::sleep(Duration::from_millis(5));
        }
        drain_main_context();
        self.count.get()
    }
}

/// How long [`WriteWatch::settled_count`] waits for monitor events.
pub const EVENT_SETTLE: Duration = Duration::from_millis(250);

impl Drop for KeyfileSettingsFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Drain every ready source of the default main context.
pub fn drain_main_context() {
    let context = glib::MainContext::default();
    while context.iteration(false) {}
}
