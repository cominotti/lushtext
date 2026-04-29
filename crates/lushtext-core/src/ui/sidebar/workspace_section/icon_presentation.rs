// SPDX-License-Identifier: GPL-3.0-or-later

//! Icon presentation for workspace file-tree rows.

use gtk4::gio;
use gtk4::prelude::*;
use std::path::Path;

use super::super::file_tree_item::FileTreeItem;

/// Regular themed folder icon used for actual directory rows in the file tree.
pub(super) const DIRECTORY_ICON_NAME: &str = "folder";
/// Regular themed fallback for files whose content type or theme icon is unavailable.
pub(super) const FILE_FALLBACK_ICON_NAME: &str = "text-x-generic";
/// Symbolic status icon kept for synthetic placeholder rows, not filesystem content.
pub(super) const PLACEHOLDER_ICON_NAME: &str = "dialog-information-symbolic";

/// Presentation choice for the `GtkImage` that starts each recycled tree row.
pub(super) enum FileTreeRowIcon {
    /// Named icons cover deterministic folder, fallback, and placeholder states.
    Named(&'static str),
    /// GIO content-type icons let the current icon theme choose file-kind artwork.
    ContentType(gio::Icon),
}

impl FileTreeRowIcon {
    /// Bind this presentation to a row image, falling back before GTK can show a missing icon.
    pub(super) fn apply_to(&self, image: &gtk4::Image) {
        match self {
            Self::Named(name) => image.set_icon_name(Some(name)),
            Self::ContentType(icon) => {
                let icon_theme = gtk4::IconTheme::for_display(&image.display());
                if icon_theme.has_gicon(icon) {
                    image.set_from_gicon(icon);
                } else {
                    image.set_icon_name(Some(FILE_FALLBACK_ICON_NAME));
                }
            }
        }
    }
}

/// Classify one sidebar tree item without changing the item model or doing file I/O.
pub(super) fn icon_for_file_item(file_item: &FileTreeItem) -> FileTreeRowIcon {
    if file_item.is_placeholder() {
        return FileTreeRowIcon::Named(PLACEHOLDER_ICON_NAME);
    }

    if file_item.is_dir() {
        return FileTreeRowIcon::Named(DIRECTORY_ICON_NAME);
    }

    file_item.path().as_deref().map_or(
        FileTreeRowIcon::Named(FILE_FALLBACK_ICON_NAME),
        icon_for_file_path,
    )
}

fn icon_for_file_path(path: &Path) -> FileTreeRowIcon {
    // GIO can infer useful types from names like `photo.png`; passing only the
    // basename avoids stat or content-sensitive guesses for existing files.
    let filename = path.file_name().map_or(path, Path::new);
    let (content_type, _uncertain) = gio::content_type_guess(Some(filename), Option::<&[u8]>::None);
    if is_generic_unknown_content_type(content_type.as_str()) {
        return FileTreeRowIcon::Named(FILE_FALLBACK_ICON_NAME);
    }

    let icon = gio::content_type_get_icon(content_type.as_str());
    if let Some(themed_icon) = icon.downcast_ref::<gio::ThemedIcon>() {
        themed_icon.append_name(FILE_FALLBACK_ICON_NAME);
    }
    FileTreeRowIcon::ContentType(icon)
}

fn is_generic_unknown_content_type(content_type: &str) -> bool {
    gio::content_type_is_unknown(content_type)
        || content_type == "application/octet-stream"
        || content_type == "application/x-zerosize"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn themed_icon_names(icon: &gio::Icon) -> Vec<String> {
        icon.downcast_ref::<gio::ThemedIcon>()
            .map(|themed_icon| {
                themed_icon
                    .names()
                    .into_iter()
                    .map(|name| name.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn placeholder_rows_keep_symbolic_status_icon() {
        let item = FileTreeItem::new_placeholder("10,000+ items");

        let FileTreeRowIcon::Named(icon_name) = icon_for_file_item(&item) else {
            panic!("placeholder should use a named symbolic icon");
        };

        assert_eq!(icon_name, PLACEHOLDER_ICON_NAME);
    }

    #[test]
    fn directory_rows_use_regular_folder_icon() {
        let item = FileTreeItem::new(PathBuf::from("/tmp/project/src"), true, None);

        let FileTreeRowIcon::Named(icon_name) = icon_for_file_item(&item) else {
            panic!("directory should use a named regular icon");
        };

        assert_eq!(icon_name, DIRECTORY_ICON_NAME);
    }

    #[test]
    fn known_file_rows_use_regular_content_type_icon() {
        let item = FileTreeItem::new(PathBuf::from("/tmp/project/image.png"), false, None);

        let FileTreeRowIcon::ContentType(icon) = icon_for_file_item(&item) else {
            panic!("known file types should use a content-type icon");
        };

        let names = themed_icon_names(&icon);
        assert!(
            names.iter().any(|name| name.contains("image")),
            "image content type should expose image-themed icon names, got {names:?}"
        );
        assert!(
            names
                .first()
                .is_some_and(|name| !name.ends_with("-symbolic")),
            "regular content-type lookup should prefer a regular icon first, got {names:?}"
        );
    }

    #[test]
    fn unknown_file_rows_use_regular_text_fallback() {
        let item = FileTreeItem::new(
            PathBuf::from("/tmp/project/blob.lushtext-unknown-extension"),
            false,
            None,
        );

        let FileTreeRowIcon::Named(icon_name) = icon_for_file_item(&item) else {
            panic!("unknown file type should use a named fallback icon");
        };

        assert_eq!(icon_name, FILE_FALLBACK_ICON_NAME);
    }
}
