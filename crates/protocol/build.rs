//! Compiles `schema/clipmesh.proto` into Rust.
//!
//! We deliberately avoid the `protoc` binary requirement by using `protox`, a
//! pure Rust protobuf compiler. That means a fresh clone builds on any machine
//! (including CI and Android cross-compilation hosts) with nothing but a Rust
//! toolchain installed.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let schema_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schema");
    let proto_file = schema_dir.join("clipmesh.proto");

    println!("cargo:rerun-if-changed={}", proto_file.display());
    println!("cargo:rerun-if-changed={}", schema_dir.display());

    let file_descriptor_set = protox::compile([&proto_file], [&schema_dir])?;

    let mut config = prost_build::Config::new();
    // `bytes = "."` would switch every `bytes` field to `Bytes`; we keep `Vec<u8>`
    // because payloads are handed to platform clipboard APIs that want a slice.
    config
        .out_dir(std::env::var("OUT_DIR").map(PathBuf::from)?)
        .compile_fds(file_descriptor_set)?;

    Ok(())
}
