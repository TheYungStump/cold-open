//! Compiles the third-party license file into the app's About sheet
//! (docs/specs/release.md §10). The release workflow generates it with
//! cargo-about and names it in `COLDOPEN_THIRD_PARTY`; without that the
//! sheet says where to find it.

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-env-changed=COLDOPEN_THIRD_PARTY");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let packed = match std::env::var_os("COLDOPEN_THIRD_PARTY").filter(|p| !p.is_empty()) {
        Some(path) => {
            let path = PathBuf::from(path);
            println!("cargo:rerun-if-changed={}", path.display());
            let text = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("COLDOPEN_THIRD_PARTY {}: {e}", path.display()));
            assert!(
                !text.is_empty(),
                "COLDOPEN_THIRD_PARTY {} is empty",
                path.display()
            );
            miniz_oxide::deflate::compress_to_vec(&text, 9)
        }
        None => Vec::new(),
    };
    std::fs::write(out.join("third-party.deflate"), packed).expect("write OUT_DIR");
    windows_resources();
}

/// The icon and version details Explorer shows for the .exe
/// (docs/specs/release.md §3.1).
#[cfg(windows)]
fn windows_resources() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon = "../../.github/packaging/icon/coldopen.ico";
    println!("cargo:rerun-if-changed={icon}");
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon)
        .set("ProductName", "Cold Open")
        .set("FileDescription", "Cold Open")
        .set("CompanyName", "The Cold Open contributors")
        .set("LegalCopyright", "GPL-3.0-or-later")
        .set("OriginalFilename", "Cold Open.exe")
        .set("ProductVersion", &version)
        .set("FileVersion", &version);
    if let Err(e) = res.compile() {
        panic!("the Windows resources: {e}");
    }
}

#[cfg(not(windows))]
fn windows_resources() {}
