// SPDX-License-Identifier: MIT OR Apache-2.0

//! The one-child parenting contract shared by the crate's bin widgets.
//!
//! `ClipBin` and `ViewportSliceBin` each hold exactly one optional child. The
//! identity short-circuit, the unparent/parent pairing, and the resize plus
//! `child` notification are the same for both, so they live here once and the
//! bins contribute only their child-specific attach and detach hooks.

use std::cell::RefCell;

use gtk4::prelude::*;

/// Replace `slot`'s child with `child`, parenting it under `host`.
///
/// Returns `false` without doing anything when `child` is already the current
/// child (or both are `None`). Otherwise `on_detach` runs for the outgoing
/// child before it is unparented, `on_attach` runs for the incoming child
/// before it is parented, and the host is queued for resize and notified on
/// its `child` property.
pub(crate) fn replace_child(
    slot: &RefCell<Option<gtk4::Widget>>,
    host: &gtk4::Widget,
    child: Option<&gtk4::Widget>,
    on_detach: impl FnOnce(&gtk4::Widget),
    on_attach: impl FnOnce(&gtk4::Widget),
) -> bool {
    let current = slot
        .borrow()
        .as_ref()
        .map(gtk4::prelude::ObjectType::as_ptr);
    if current == child.map(gtk4::prelude::ObjectType::as_ptr) {
        return false;
    }

    if let Some(old_child) = slot.borrow_mut().take() {
        on_detach(&old_child);
        old_child.unparent();
    }
    if let Some(child) = child {
        on_attach(child);
        child.set_parent(host);
        slot.replace(Some(child.clone()));
    }
    host.queue_resize();
    host.notify("child");
    true
}
