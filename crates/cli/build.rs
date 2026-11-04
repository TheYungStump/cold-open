//! The version details Explorer shows for `coldopen.exe`, as the app has
//! them: SignPath's terms ask that every signed program name the product
//! and carry the release's version (docs/specs/release.md §8).

fn main() {
    windows_resources();
}

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
        .set("FileDescription", "Cold Open command-line tool")
        .set("CompanyName", "The Cold Open contributors")
        .set("LegalCopyright", "GPL-3.0-or-later")
        .set("OriginalFilename", "coldopen.exe")
        .set("ProductVersion", &version)
        .set("FileVersion", &version);
    if let Err(e) = res.compile() {
        panic!("the Windows resources: {e}");
    }
}

#[cfg(not(windows))]
fn windows_resources() {}
