use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo::rustc-link-search={}", out.display());

    #[cfg(not(feature = "skip-cyw43-firmware"))]
    download_cyw43_firmware();

    println!("cargo::rerun-if-changed=memory.x");
    // The workspace .cargo/config.toml already supplies link.x and defmt.x.
    // Re-adding link.x here loads memory.x twice and duplicates BOOT2.
    println!("cargo::rustc-link-arg-bins=-Tlink-rp.x");
}

#[cfg(not(feature = "skip-cyw43-firmware"))]
fn download_cyw43_firmware() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let download_folder = manifest_dir.join("cyw43-firmware");
    let url_base = "https://github.com/embassy-rs/embassy/raw/refs/heads/main/cyw43-firmware";
    let file_names = [
        "43439A0.bin",
        "43439A0_btfw.bin",
        "43439A0_clm.bin",
        "nvram_rp2040.bin",
    ];

    println!("cargo::rerun-if-changed=build.rs");
    std::fs::create_dir_all(&download_folder).expect("Failed to create CYW43 firmware directory");
    for file_name in file_names {
        let file_path = download_folder.join(file_name);
        if file_path.exists() {
            continue;
        }
        let url = format!("{url_base}/{file_name}");
        let response = reqwest::blocking::get(url).expect("Failed to download CYW43 firmware");
        std::fs::write(
            file_path,
            response.bytes().expect("Failed to read CYW43 firmware"),
        )
        .expect("Failed to save CYW43 firmware");
    }
}
