// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the fixtures that make a test's files. Each is checked against the filesystem
//! itself, under the directory cargo gives the tests to write in.

use std::fs;
use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
use std::path::Path;

use stella_test_support::{Lock, fifo, fresh_directory, write};

/// Returns the directory cargo gives these tests to write in.
fn parent() -> &'static Path {
    Path::new(env!("CARGO_TARGET_TMPDIR"))
}

#[test]
fn a_fresh_directory_starts_empty_every_time() {
    // What an earlier run left behind is gone, so the directory is there and holds nothing.
    let directory = fresh_directory(parent(), "files-fresh");
    write(&directory, "left/behind.txt", b"x");
    let again = fresh_directory(parent(), "files-fresh");
    assert_eq!(again, directory);
    assert_eq!(fs::read_dir(&again).expect("the directory is there").count(), 0);
}

#[test]
fn write_makes_the_directories_a_file_needs() {
    // A file two directories down is written with its contents, and a second write replaces it.
    let directory = fresh_directory(parent(), "files-write");
    write(&directory, "a/b/c.txt", b"one");
    assert_eq!(fs::read(directory.join("a/b/c.txt")).expect("the file was written"), b"one");
    write(&directory, "a/b/c.txt", b"two");
    assert_eq!(fs::read(directory.join("a/b/c.txt")).expect("the file was written"), b"two");
}

#[test]
fn fifo_makes_a_fifo() {
    // What's made is a FIFO, not a regular file.
    let directory = fresh_directory(parent(), "files-fifo");
    fifo(&directory.join("pipe"));
    let file_type = fs::symlink_metadata(directory.join("pipe")).expect("it was made").file_type();
    assert!(file_type.is_fifo());
}

#[test]
fn a_lock_holds_until_its_dropped() {
    // While the lock is held the file has the mode it was given, and once it's dropped the file
    // can be read and removed again.
    let directory = fresh_directory(parent(), "files-lock");
    write(&directory, "locked.txt", b"x");
    let path = directory.join("locked.txt");
    let mode =
        |path: &Path| fs::metadata(path).expect("the file is there").permissions().mode() & 0o777;
    let lock = Lock::new(path.clone(), 0o000);
    assert_eq!(mode(&path), 0o000);
    drop(lock);
    assert_eq!(mode(&path), 0o755);
}
