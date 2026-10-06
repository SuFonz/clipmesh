// Prevents an extra console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Binary entry point. Android never runs this - the mobile runtime calls
//! [`clipmesh_android_lib::run`] through the generated `MainActivity` - but
//! keeping it means the crate behaves like a normal Tauri app on every platform.

fn main() {
    clipmesh_android_lib::run()
}
