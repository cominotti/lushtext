// SPDX-License-Identifier: GPL-3.0-or-later

//! Build script: compile GResources for development builds.
//!
//! In Flatpak builds, Meson compiles and installs GResources separately.
//! This build.rs handles the dev-build case so `cargo run` works directly.

use std::path::Path;

use lushtext_build_support::filesystem as build_fs;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let resource_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources");
    let data_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data");
    let resource_xml = format!("{resource_dir}/dev.cominotti.lushtext.gresource.xml");
    println!("cargo:rerun-if-changed={resource_xml}");
    // Cargo only knows about the manifest unless child resources are listed.
    // Keep bundled style-scheme edits visible to `make run` without requiring
    // a manual `cargo clean`.
    for style_file in ["Adwaita.xml", "Adwaita-dark.xml"] {
        println!("cargo:rerun-if-changed={resource_dir}/gtksourceview/styles/{style_file}");
    }

    // Local development checkouts compile resources here; packaged builds or
    // partial source exports may omit the XML because Meson owns that step.
    if build_fs::exists(Path::new(&resource_xml)) {
        glib_build_tools::compile_resources(
            &[resource_dir, data_dir],
            &resource_xml,
            "lushtext.gresource",
        );
    }

    // Compile GSettings schemas for dev builds only.
    // Meson builds set LUSHTEXT_PKGDATADIR — skip schema compilation there
    // because the source tree may be read-only in Flatpak and Meson handles
    // schema installation and compilation via gnome.post_install().
    if std::env::var("LUSHTEXT_PKGDATADIR").is_err() {
        let schema_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data");
        let schema_file = format!("{schema_dir}/dev.cominotti.lushtext.gschema.xml");
        if build_fs::exists(Path::new(&schema_file)) {
            let status = std::process::Command::new("glib-compile-schemas")
                .arg(schema_dir)
                .status()
                .expect("glib-compile-schemas not found — install glib2-devel");
            assert!(status.success(), "glib-compile-schemas failed");
            println!("cargo:rerun-if-changed={schema_file}");
        }
    }
}
