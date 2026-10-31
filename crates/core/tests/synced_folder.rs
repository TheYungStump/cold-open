//! Saves in a folder a sync client watches (docs/specs/release.md §6): on
//! Windows, OneDrive, Dropbox and antivirus briefly open a file just
//! written without letting it be replaced. The atomic save's rename waits
//! about a second for them (`io.rs`), then gives up with the original
//! intact.

#![cfg(windows)]

use std::fs::{self, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

use coldopen_core::io::atomic_write;

/// Opens `path` as a sync client does: reading, sharing reads only.
fn hold(path: &std::path::Path) -> fs::File {
    const FILE_SHARE_READ: u32 = 1;
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
        .unwrap()
}

#[test]
fn a_save_waits_out_a_sync_client_holding_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("pilot.fountain");
    fs::write(&p, "old").unwrap();
    let held = hold(&p);
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        drop(held);
    });
    let t = Instant::now();
    atomic_write(&p, b"new").expect("the save goes through once the file is let go");
    assert!(
        t.elapsed() >= Duration::from_millis(250),
        "{:?}",
        t.elapsed()
    );
    release.join().unwrap();
    assert_eq!(fs::read_to_string(&p).unwrap(), "new");
}

#[test]
fn a_file_held_too_long_fails_the_save_and_keeps_the_original() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("pilot.fountain");
    fs::write(&p, "old").unwrap();
    let held = hold(&p);
    assert!(atomic_write(&p, b"new").is_err());
    drop(held);
    assert_eq!(fs::read_to_string(&p).unwrap(), "old");
    // No temporary file is left behind.
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
