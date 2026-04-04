// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests for the FileTreeItem GObject (sidebar tree model item).

use crate::common::ensure_gtk_init;
use lushtext_core::ui::sidebar::file_tree_item::FileTreeItem;
use std::path::PathBuf;

#[test]
fn test_new_file() {
    ensure_gtk_init();
    let item = FileTreeItem::new(PathBuf::from("/tmp/hello.rs"), false);
    assert_eq!(item.path(), Some(PathBuf::from("/tmp/hello.rs")));
    assert!(!item.is_dir());
}

#[test]
fn test_new_directory() {
    ensure_gtk_init();
    let item = FileTreeItem::new(PathBuf::from("/tmp/src"), true);
    assert_eq!(item.path(), Some(PathBuf::from("/tmp/src")));
    assert!(item.is_dir());
}

#[test]
fn test_name_returns_filename() {
    ensure_gtk_init();
    let item = FileTreeItem::new(PathBuf::from("/home/user/project/main.rs"), false);
    assert_eq!(item.name(), "main.rs");
}

#[test]
fn test_name_directory() {
    ensure_gtk_init();
    let item = FileTreeItem::new(PathBuf::from("/home/user/project/src"), true);
    assert_eq!(item.name(), "src");
}

#[test]
fn test_name_root_path_returns_display() {
    ensure_gtk_init();
    // "/" has no file_name component, so name() falls back to display()
    let item = FileTreeItem::new(PathBuf::from("/"), false);
    assert_eq!(item.name(), "/");
}

#[test]
fn test_name_empty_path() {
    ensure_gtk_init();
    let item = FileTreeItem::new(PathBuf::from(""), false);
    assert_eq!(item.name(), "");
}

#[test]
fn test_path_preserves_full_path() {
    ensure_gtk_init();
    let long_path = PathBuf::from("/very/deeply/nested/directory/structure/file.txt");
    let item = FileTreeItem::new(long_path.clone(), false);
    assert_eq!(item.path(), Some(long_path));
}

#[test]
fn test_set_path_updates_path_and_name() {
    ensure_gtk_init();
    let item = FileTreeItem::new(PathBuf::from("/tmp/old.txt"), false);
    assert_eq!(item.name(), "old.txt");

    item.set_path(PathBuf::from("/tmp/new.txt"));
    assert_eq!(item.path(), Some(PathBuf::from("/tmp/new.txt")));
    assert_eq!(item.name(), "new.txt");
}
