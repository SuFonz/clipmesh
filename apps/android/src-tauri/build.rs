//! Build script for the Android host.
//!
//! Identical to the desktop one, and for the same reason: `tauri-build` emits
//! `rerun-if-changed` for the configuration but not for the icons, and once a
//! build script emits any such directive cargo stops re-running it for
//! unrelated file changes. Without the list below, replacing an icon would
//! leave a stale resource in the build cache - see the desktop `build.rs` for
//! the full explanation.
//!
//! This host also generates the ACL manifests, the mobile entry point glue and
//! (when the Tauri mobile CLI runs) the `gen/android` project.

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
