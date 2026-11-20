//! This build script copies the `memory.x` file from the crate root into
//! a directory where the linker can always find it at build time.
//! For many projects this is optional, as the linker always searches the
//! project root directory -- wherever `Cargo.toml` is. However, if you
//! are using a workspace or have a more complicated build setup, this
//! build script becomes required. Additionally, by requesting that
//! Cargo re-run the build script whenever `memory.x` is changed,
//! updating `memory.x` ensures a rebuild of the application with the
//! new memory settings.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use usbd_human_interface_device::device::joystick::JOYSTICK_DESCRIPTOR;

fn main() {
    // Put `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("../../memory.x"))
        .unwrap();
    println!("cargo:rustc-link-search={}", out.display());

    // Write the generated SDL mapping into the same profile output
    // directory as the binary, so consumers can load it from the
    // executable location.
    let target_dir = env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).parent().unwrap().parent().unwrap().join("target"));
    let target_triple = env::var("TARGET").unwrap();
    let profile = env::var("PROFILE").unwrap();
    let mapping_path = target_dir
        .join(target_triple)
        .join(profile)
        .join("generated_sdl_mapping.txt");
    std::fs::create_dir_all(mapping_path.parent().unwrap()).unwrap();
    std::fs::write(mapping_path, generate_sdl_mapping()).unwrap();

    // By default, Cargo will re-run a build script whenever
    // any file in the project changes. By specifying `memory.x`
    // here, we ensure the build script is only re-run when
    // `memory.x` is changed.
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=build.rs");
}

fn generate_sdl_mapping() -> &'static str {
    // Keep the generated mapping tied to the HID joystick descriptor.
    debug_assert_eq!(JOYSTICK_DESCRIPTOR[0], 0x05);
    debug_assert_eq!(JOYSTICK_DESCRIPTOR[1], 0x01);
    debug_assert!(JOYSTICK_DESCRIPTOR.len() >= 34);

    "03000000091200000100000011010000,Atari CX40,a:b0,b:b1,x:b2,y:b3,leftshoulder:b4,rightshoulder:b5,back:b6,start:b7,leftx:a0,lefty:a1"
}
