// SPDX-License-Identifier: MIT OR Apache-2.0

//! Private GObject implementation for [`super::ClipBin`].
//!
//! The public wrapper in `mod.rs` gives Rust callers a normal GTK widget type.
//! This implementation owns the GTK lifecycle details: one optional child,
//! explicit property notification, parent/unparent pairing, zero-minimum
//! measurement, and clipped snapshots.

use std::cell::RefCell;
use std::sync::LazyLock;

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{glib, graphene};

/// Private widget state for `GtkLushClipBin`.
#[derive(Default)]
pub struct ClipBin {
    /// The single child parented by this bin.
    ///
    /// GTK calls widget vfuncs with `&self`, so interior mutability is the
    /// standard gtk-rs shape for replacing or clearing a template/buildable
    /// child while still letting measurement and snapshot borrow it briefly.
    pub child: RefCell<Option<gtk4::Widget>>,
}

#[glib::object_subclass]
impl ObjectSubclass for ClipBin {
    const NAME: &'static str = "GtkLushClipBin";
    type Type = super::ClipBin;
    type ParentType = gtk4::Widget;
}

impl ObjectImpl for ClipBin {
    fn properties() -> &'static [glib::ParamSpec] {
        static PROPERTIES: LazyLock<Vec<glib::ParamSpec>> = LazyLock::new(|| {
            vec![
                glib::ParamSpecObject::builder::<gtk4::Widget>("child")
                    .explicit_notify()
                    .build(),
            ]
        });
        PROPERTIES.as_ref()
    }

    fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
        match pspec.name() {
            "child" => {
                let child = value
                    .get::<Option<gtk4::Widget>>()
                    .expect("child property must be a GtkWidget");
                self.set_child(child.as_ref());
            }
            name => panic!("unknown property {name}"),
        }
    }

    fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
        match pspec.name() {
            "child" => self.child.borrow().to_value(),
            name => panic!("unknown property {name}"),
        }
    }

    fn dispose(&self) {
        if let Some(child) = self.child.borrow_mut().take() {
            child.unparent();
        }
    }
}

impl WidgetImpl for ClipBin {
    fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
        let Some(child) = self.child.borrow().as_ref().cloned() else {
            return (0, 0, -1, -1);
        };
        if !child.should_layout() {
            return (0, 0, -1, -1);
        }

        // ClipBin may be measured at an opposite-axis size below the child's
        // legal minimum because it advertises zero minimum size to protect
        // persistent chrome. Clamp only the size passed into the child query so
        // GTK does not warn about impossible measurement requests; keep our own
        // reported minimum at zero.
        let child_for_size = if for_size >= 0 {
            let opposite = match orientation {
                gtk4::Orientation::Horizontal => gtk4::Orientation::Vertical,
                gtk4::Orientation::Vertical => gtk4::Orientation::Horizontal,
                _ => orientation,
            };
            let (opposite_minimum, _, _, _) = child.measure(opposite, -1);
            for_size.max(opposite_minimum)
        } else {
            for_size
        };
        let (_, natural, _, _) = child.measure(orientation, child_for_size);
        // Baselines are intentionally suppressed. Returning a child natural
        // baseline beside a forced `-1` minimum baseline is internally
        // inconsistent and can trip GTK size-request warnings for labels.
        (0, natural, -1, -1)
    }

    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        let Some(child) = self.child.borrow().as_ref().cloned() else {
            return;
        };
        if child.should_layout() {
            child.allocate(width, height, baseline, None);
        }
    }

    fn snapshot(&self, snapshot: &gtk4::Snapshot) {
        let Some(child) = self.child.borrow().as_ref().cloned() else {
            return;
        };
        if !child.should_layout() {
            return;
        }
        let bounds = graphene::Rect::new(
            0.0,
            0.0,
            self.obj().width() as f32,
            self.obj().height() as f32,
        );
        snapshot.push_clip(&bounds);
        self.obj().snapshot_child(&child, snapshot);
        snapshot.pop();
    }
}

impl ClipBin {
    pub(super) fn set_child(&self, child: Option<&gtk4::Widget>) {
        {
            let current = self.child.borrow();
            match (current.as_ref(), child) {
                (Some(current), Some(new_child)) if current.as_ptr() == new_child.as_ptr() => {
                    return;
                }
                (None, None) => return,
                _ => {}
            }
        }

        if let Some(old_child) = self.child.borrow_mut().take() {
            old_child.unparent();
        }
        if let Some(child) = child {
            child.set_parent(&*self.obj());
            self.child.replace(Some(child.clone()));
        }
        self.obj().queue_resize();
        self.obj().notify("child");
    }
}
