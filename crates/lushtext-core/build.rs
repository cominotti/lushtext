// SPDX-License-Identifier: GPL-3.0-or-later

//! Build script: compile GResources for development builds.
//!
//! In Flatpak builds, Meson compiles and installs GResources separately.
//! This build.rs handles the dev-build case so `cargo run` works directly.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let resource_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources");
    let resource_xml = format!("{resource_dir}/dev.cominotti.lushtext.gresource.xml");

    // Only compile resources if the XML exists (skip during early scaffold)
    if std::path::Path::new(&resource_xml).exists() {
        glib_build_tools::compile_resources(&[resource_dir], &resource_xml, "lushtext.gresource");
    }

    // Compile GSettings schemas for dev builds.
    // For installed builds, Meson handles schema compilation and installation.
    let schema_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data");
    let schema_file = format!("{schema_dir}/dev.cominotti.lushtext.gschema.xml");
    if std::path::Path::new(&schema_file).exists() {
        let status = std::process::Command::new("glib-compile-schemas")
            .arg(schema_dir)
            .status()
            .expect("glib-compile-schemas not found — install glib2-devel");
        assert!(status.success(), "glib-compile-schemas failed");
        println!("cargo:rerun-if-changed={schema_file}");
    }
}
