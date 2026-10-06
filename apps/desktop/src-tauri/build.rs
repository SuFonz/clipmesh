//! Build script for the ClipMesh desktop host.
//!
//! # Why this is not just `tauri_build::build()`
//!
//! On Windows `tauri-build` writes a `.rc` resource script that references
//! `icons/icon.ico` and compiles it into the executable. It emits
//! `cargo:rerun-if-changed` for `tauri.conf.json` and `capabilities/`, but
//! **not** for the icons.
//!
//! That matters more than it looks. As soon as a build script emits *any*
//! `rerun-if-changed`, cargo stops using its "re-run when a file in the package
//! changed" fallback and trusts the list instead. So replacing `icons/icon.ico`
//! alone never re-runs this script, the stale `resource.rc` (and the object
//! compiled from it) stays in the build cache, and every later build links the
//! **old icon** into the executable - silently. The build succeeds, the exe
//! runs, and the icon is simply wrong.
//!
//! Emitting the directive below fixes it: change an icon and cargo re-runs this
//! script, which rewrites `resource.rc` against the current file.
//!
//! Keep this list in step with `bundle.icon` in `tauri.conf.json`.

/// The icons this crate bundles, relative to the package root.
const ICONS: &[&str] = &[
    "icons/32x32.png",
    "icons/64x64.png",
    "icons/128x128.png",
    "icons/128x128@2x.png",
    "icons/icon.png",
    "icons/icon.ico",
    "icons/icon.icns",
];

fn main() {
    for icon in ICONS {
        println!("cargo:rerun-if-changed={icon}");
    }

    tauri_build::build()
}
