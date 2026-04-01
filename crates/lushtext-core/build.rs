// SPDX-License-Identifier: GPL-3.0-or-later

//! Build script: compile GResources for development builds.
//!
//! In Flatpak builds, Meson compiles and installs GResources separately.
//! This build.rs handles the dev-build case so `cargo run` works directly.

fn main() {
    let resource_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources");
    let resource_xml = format!("{resource_dir}/dev.cominotti.lushtext.gresource.xml");

    // Only compile resources if the XML exists (skip during early scaffold)
    if std::path::Path::new(&resource_xml).exists() {
        glib_build_tools::compile_resources(
            &[resource_dir],
            &resource_xml,
            "lushtext.gresource",
        );
    }
}
