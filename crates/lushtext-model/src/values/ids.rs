// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed domain identifiers generated via [`crate::define_id!`].

crate::define_id!(
    /// Stable monotonic per-editor-page identifier within the running app.
    pub EditorId(usize)
);
