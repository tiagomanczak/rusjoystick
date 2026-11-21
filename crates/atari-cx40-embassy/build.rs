use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    fs::write(out_dir.join("memory.x"), include_bytes!("../../memory.x"))
        .expect("copy memory.x to the linker output directory");
    println!("cargo:rustc-link-search={}", out_dir.display());
    println!("cargo:rerun-if-changed=../../memory.x");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set"));
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir
                .parent()
                .expect("crate is inside crates/")
                .parent()
                .expect("crates/ is inside the workspace")
                .join("target")
        });
    let mapping_path = target_dir
        .join(env::var("TARGET").expect("TARGET is set by Cargo"))
        .join(env::var("PROFILE").expect("PROFILE is set by Cargo"))
        .join("generated_sdl_mapping.txt");
    fs::create_dir_all(mapping_path.parent().expect("mapping path has a parent"))
        .expect("create SDL mapping directory");
    fs::write(
        mapping_path,
        "03000000091200000100000011010000,Atari CX40,a:b0,b:b1,x:b2,y:b3,leftshoulder:b4,rightshoulder:b5,back:b6,start:b7,leftx:a0,lefty:a1",
    )
    .expect("write SDL mapping");
}
