// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the fixtures that make a test's files. Each is checked against the filesystem
//! itself, in the tests' scratch directory.

use std::fs;
use std::os::unix::fs::{FileTypeExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use stella_test_support::{ModeGuard, Scratch, fifo, write};

/// Returns a new, empty directory for one test to work in.
fn fresh_directory(name: &str) -> PathBuf {
    Scratch::of("files").fresh(name)
}

#[test]
fn a_fresh_directory_starts_empty_every_time() {
    // What an earlier run left behind is gone, so the directory is there and holds nothing.
    let directory = fresh_directory("fresh");
    write(&directory, "left/behind.txt", b"x");
    let again = fresh_directory("fresh");
    assert_eq!(again, directory);
    assert_eq!(fs::read_dir(&again).expect("the directory is there").count(), 0);
}

#[test]
fn write_makes_the_directories_a_file_needs() {
    // A file two directories down is written with its contents, and a second write replaces it.
    let directory = fresh_directory("write");
    write(&directory, "a/b/c.txt", b"one");
    assert_eq!(fs::read(directory.join("a/b/c.txt")).expect("the file was written"), b"one");
    write(&directory, "a/b/c.txt", b"two");
    assert_eq!(fs::read(directory.join("a/b/c.txt")).expect("the file was written"), b"two");
}

#[test]
fn fifo_makes_a_fifo() {
    // What's made is a FIFO, not a regular file.
    let directory = fresh_directory("fifo");
    fifo(&directory.join("pipe"));
    let file_type = fs::symlink_metadata(directory.join("pipe")).expect("it was made").file_type();
    assert!(file_type.is_fifo());
}

#[test]
fn a_mode_guard_holds_until_its_dropped() {
    // While the guard is held the file has the mode it was given. Once it's dropped the file has
    // the mode it had before, whatever that was, so it can be read and removed again.
    let directory = fresh_directory("lock");
    write(&directory, "locked.txt", b"x");
    let path = directory.join("locked.txt");
    let mode =
        |path: &Path| fs::metadata(path).expect("the file is there").permissions().mode() & 0o777;
    for mode_before in [0o640, 0o755] {
        fs::set_permissions(&path, fs::Permissions::from_mode(mode_before))
            .expect("a test's own file takes the permissions it's given");
        let guard = ModeGuard::new(path.clone(), 0o000);
        assert_eq!(mode(&path), 0o000);
        drop(guard);
        assert_eq!(mode(&path), mode_before);
    }
}

#[test]
fn a_scratch_directory_is_in_the_target_directory() {
    // A group's directory is `tmp/<group>` beside the profile's directory the test was built
    // into, so `cargo clean` removes it and two checkouts never share one.
    let scratch = Scratch::of("files");
    let test = std::env::current_exe().expect("a running test has a path");
    let target = test.ancestors().nth(3).expect("a test is built three directories down");
    assert_eq!(scratch.path(), target.join("tmp/files"));
}

#[test]
#[should_panic(expected = "should be a plain name")]
fn a_fresh_directory_named_for_its_parent_is_a_bug() {
    // `..` would have the directory above the scratch directory removed.
    _ = Scratch::of("files").fresh("..");
}

#[test]
#[should_panic(expected = "should be a plain name")]
fn a_fresh_directory_with_no_name_is_a_bug() {
    // No name at all would have the whole scratch directory removed.
    _ = Scratch::of("files").fresh("");
}

#[test]
#[should_panic(expected = "should be one name")]
fn a_fresh_directory_named_by_a_path_is_a_bug() {
    // A path could reach into another test's directory.
    _ = Scratch::of("files").fresh("fresh/inside");
}

#[test]
#[should_panic(expected = "should be a plain name")]
fn a_scratch_group_named_by_an_absolute_path_is_a_bug() {
    // An absolute path would replace the scratch directory with any directory at all.
    _ = Scratch::of("/tmp");
}

#[test]
#[should_panic(expected = "so it stays in its directory")]
fn writing_through_a_parent_is_a_bug() {
    // `..` would write outside the directory the test was given.
    let directory = fresh_directory("write-parent");
    write(&directory, "../outside.txt", b"x");
}

#[test]
#[should_panic(expected = "so it stays in its directory")]
fn writing_to_an_absolute_path_is_a_bug() {
    // An absolute path replaces the directory it's joined to.
    let directory = fresh_directory("write-absolute");
    write(&directory, "/tmp/stella-test-support-outside.txt", b"x");
}
