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
    ///
    /// The document scale policy wins: a document whose paragraphs are too
    /// long for invisible-character drawing gets the drawer switched off,
    /// whatever mode is selected, and the per-document mode is kept so other
    /// documents are unaffected.
    pub(crate) fn apply_invisible_characters_mode(&self) {
        let drawer = self.source_view().space_drawer();
        let mode = if self.document_scale().invisibles_allowed {
            self.invisible_characters_mode()
        } else {
            InvisibleCharactersMode::Off
        };
        match mode {
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
