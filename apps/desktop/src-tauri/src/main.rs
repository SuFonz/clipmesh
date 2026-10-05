// Prevents an extra console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Binary entry point. All the interesting work lives in the library so it can
//! be exercised by tests and by the Android host.

fn main() {
    clipmesh_desktop_lib::run()
}
