// SPDX-License-Identifier: GPL-3.0-or-later

//! Invisible-character rendering workflow for one editor tab.
//!
//! GtkSourceView already knows how to draw common whitespace categories without
//! mutating the buffer, so this module keeps the first implementation native:
//! tabs, spaces, non-breaking spaces, and newline markers go through the source
//! view's `SpaceDrawer`, while zero-width/BOM discovery stays in file-health.

use sourceview5::prelude::ViewExt;
use sourceview5::{SpaceLocationFlags, SpaceTypeFlags};

use crate::model::encoding::InvisibleCharactersMode;

use super::LushtextEditorPage;

impl LushtextEditorPage {
    /// Apply the current invisible-character mode to the underlying source view.
    pub(crate) fn apply_invisible_characters_mode(&self) {
        let drawer = self.source_view().space_drawer();
        match self.invisible_characters_mode() {
            InvisibleCharactersMode::Off => {
                drawer.set_enable_matrix(false);
                drawer.set_types_for_locations(SpaceLocationFlags::ALL, SpaceTypeFlags::NONE);
            }
            InvisibleCharactersMode::WhitespaceOnly => {
                drawer.set_enable_matrix(true);
                drawer.set_types_for_locations(
                    SpaceLocationFlags::ALL,
                    SpaceTypeFlags::SPACE | SpaceTypeFlags::TAB,
                );
            }
            InvisibleCharactersMode::All => {
                drawer.set_enable_matrix(true);
                drawer.set_types_for_locations(
                    SpaceLocationFlags::ALL,
                    SpaceTypeFlags::SPACE
                        | SpaceTypeFlags::TAB
                        | SpaceTypeFlags::NBSP
                        | SpaceTypeFlags::NEWLINE,
                );
            }
        }
    }
}
