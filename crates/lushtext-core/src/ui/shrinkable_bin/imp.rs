// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the shrinkable main-content wrapper.

use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{glib, graphene};
use std::cell::RefCell;
use std::sync::LazyLock;

#[derive(Default)]
pub struct LushtextShrinkableBin {
    pub child: RefCell<Option<gtk4::Widget>>,
}

#[glib::object_subclass]
impl ObjectSubclass for LushtextShrinkableBin {
    const NAME: &'static str = "LushtextShrinkableBin";
    type Type = super::LushtextShrinkableBin;
    type ParentType = gtk4::Widget;
}

impl ObjectImpl for LushtextShrinkableBin {
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

impl WidgetImpl for LushtextShrinkableBin {
    fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
        let Some(child) = self.child.borrow().as_ref().cloned() else {
            return (0, 0, -1, -1);
        };
        if !child.should_layout() {
            return (0, 0, -1, -1);
        }

        let (_, natural, _, natural_baseline) = child.measure(orientation, for_size);
        (0, natural, -1, natural_baseline)
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
        if child.should_layout() {
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
}

impl LushtextShrinkableBin {
    fn set_child(&self, child: Option<&gtk4::Widget>) {
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
