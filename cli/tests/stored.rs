// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for finding a path as it's stored. They're a crate of their own, so the compiler
//! lets them reach only what the crate makes public: a path that passed the path lint in, the
//! path as its filesystem stores it out, or why it couldn't be found.
//!
//! Every test works on real files, under the directory cargo gives the tests to write in, since
//! what a filesystem does with a name is the thing being checked. On macOS, one works across a
//! real mount point, made from a small disk image.

#![forbid(unsafe_code)]

use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use stella_cli::{AccessFailure, SearchFailure, search_for_stored};
use stella_linter_path::lint_path;
use stella_test_support::{ModeGuard, Scratch, write};

/// This is a program that passes, though nothing here reads it.
const GOOD: &[u8] = b"function main() returns Void {\n}\n";

/// A disk image mounted at a directory until it's dropped.
#[cfg(target_os = "macos")]
struct Mount {
    mount_point: PathBuf,
}

#[cfg(target_os = "macos")]
impl Mount {
    /// Makes a small disk image in `directory` and mounts it at `volume` there. The volume tells
    /// names apart by case, as the filesystem around it needn't.
    fn in_directory(directory: &Path) -> Self {
        let (image, mount_point) = (directory.join("image.dmg"), directory.join("volume"));
        fs::create_dir(&mount_point).expect("the mount point is ours to make");
        let made = hdiutil(&[
            "create".as_ref(),
            "-size".as_ref(),
            "4m".as_ref(),
            "-fs".as_ref(),
            "Case-sensitive APFS".as_ref(),
            "-volname".as_ref(),
            "stella-test".as_ref(),
            "-quiet".as_ref(),
            image.as_os_str(),
        ]);
        assert!(made, "hdiutil should make a disk image at {}", image.display());
        let attached = hdiutil(&[
            "attach".as_ref(),
            image.as_os_str(),
            "-mountpoint".as_ref(),
            mount_point.as_os_str(),
            "-nobrowse".as_ref(),
            "-quiet".as_ref(),
        ]);
        assert!(attached, "hdiutil should mount the image at {}", mount_point.display());
        Self { mount_point }
    }
}

#[cfg(target_os = "macos")]
impl Drop for Mount {
    fn drop(&mut self) {
        // A test may have failed already, so this mustn't panic too. An image left mounted is
        // unmounted by the next run, before it starts.
        _ = unmount(&self.mount_point);
    }
}

/// Returns a new, empty directory for one test to work in.
fn fresh_directory(name: &str) -> PathBuf {
    Scratch::of("stored").fresh(name)
}

/// Returns `typed`, which must pass the path lint, as it's stored under `root`.
fn stored_of(root: &Path, typed: &str) -> Result<Vec<u8>, SearchFailure> {
    search_for_stored(root, lint_path(typed).expect("the typed path should pass the path lint"))
}

/// Checks that `typed` is stored under `root` as `expected`.
// TS-DISTINCT-ARGS: the path typed comes before the path it's stored by, as in the sentence
// above, and a check with the two swapped asks the filesystem another question, so it fails.
fn stores(root: &Path, typed: &str, expected: &str) {
    let found = stored_of(root, typed).map(|found| String::from_utf8_lossy(&found).into_owned());
    let found = found.map_err(|failure| failure.to_string());
    assert_eq!(found, Ok(String::from(expected)), "for {typed}");
}

/// Says whether the filesystem under `directory` ignores case, as macOS's does by default.
fn ignores_case(directory: &Path) -> bool {
    write(directory, "Probe.txt", b"");
    let ignores = directory.join("probe.txt").exists();
    fs::remove_file(directory.join("Probe.txt")).expect("the probe is ours to remove");
    ignores
}

/// Runs `hdiutil` with `arguments`, and says whether it succeeded.
#[cfg(target_os = "macos")]
fn hdiutil(arguments: &[&std::ffi::OsStr]) -> bool {
    let status = std::process::Command::new("/usr/bin/hdiutil").args(arguments).status();
    status.expect("`/usr/bin/hdiutil` is there to run on macOS").success()
}

/// Unmounts what's mounted at `mount_point`, and says whether anything was.
#[cfg(target_os = "macos")]
fn unmount(mount_point: &Path) -> bool {
    hdiutil(&["detach".as_ref(), mount_point.as_os_str(), "-force".as_ref(), "-quiet".as_ref()])
}

#[test]
fn a_path_stored_as_typed_comes_back_as_typed() {
    // Each name is exactly what was typed, at every depth, so the path comes back unchanged. A
    // leading `./` is no part of the path as it's stored.
    let directory = fresh_directory("as-typed");
    write(&directory, "hello.stella", GOOD);
    write(&directory, "programs/examples/hello-world.stella", GOOD);
    // Other entries beside them, a source and a file that isn't one, are never taken instead.
    write(&directory, "programs/examples/other.stella", GOOD);
    write(&directory, "notes.txt", b"");
    stores(&directory, "hello.stella", "hello.stella");
    stores(&directory, "./hello.stella", "hello.stella");
    stores(
        &directory,
        "programs/examples/hello-world.stella",
        "programs/examples/hello-world.stella",
    );
}

#[test]
fn a_name_stored_in_another_case_comes_back_as_stored() {
    // Where the filesystem ignores case, a name typed in lowercase reaches a file or a directory
    // stored with capitals, and comes back with them.
    let directory = fresh_directory("another-case");
    assert!(
        ignores_case(&directory),
        "this test needs a filesystem that ignores case, as macOS's does by default"
    );
    write(&directory, "Hello.stella", GOOD);
    write(&directory, "Programs/Deep/SHOUT.STELLA", GOOD);
    stores(&directory, "hello.stella", "Hello.stella");
    stores(&directory, "programs/deep/shout.stella", "Programs/Deep/SHOUT.STELLA");
}

#[test]
fn a_name_that_folds_past_ascii_comes_back_as_stored() {
    // Case goes past ASCII: the Kelvin sign is a capital `k`. So `key.stella` reaches a file
    // stored with one, which no comparison of ASCII letters finds. It's found by being the same
    // file, among other names that aren't ASCII and aren't it.
    let directory = fresh_directory("folds");
    assert!(
        ignores_case(&directory),
        "this test needs a filesystem that ignores case, as macOS's does by default"
    );
    for name in ["a.stella", "\u{E9}.stella", "\u{212A}ey.stella", "\u{FC}.stella", "z.stella"] {
        write(&directory, name, GOOD);
    }
    stores(&directory, "key.stella", "\u{212A}ey.stella");
    stores(&directory, "a.stella", "a.stella");
}

#[test]
fn a_file_with_two_names_comes_back_by_the_one_typed() {
    // A hard link gives a file a second name. The name that was typed is the one that comes
    // back, in whichever order the two are listed, and a kebab name for the file never stands in
    // for a name that isn't.
    let directory = fresh_directory("hard-links");
    assert!(
        ignores_case(&directory),
        "this test needs a filesystem that ignores case, as macOS's does by default"
    );
    for (name, other) in [
        ("Hello.stella", "hello-2.stella"),
        ("shout-2.stella", "Shout.stella"),
        ("\u{212A}ey.stella", "a.stella"),
    ] {
        write(&directory, name, GOOD);
        fs::hard_link(directory.join(name), directory.join(other)).expect("make a hard link");
    }
    for (typed, expected) in [
        ("hello-2.stella", "hello-2.stella"),
        ("hello.stella", "Hello.stella"),
        ("shout-2.stella", "shout-2.stella"),
        ("shout.stella", "Shout.stella"),
        ("a.stella", "a.stella"),
        ("key.stella", "\u{212A}ey.stella"),
    ] {
        stores(&directory, typed, expected);
    }
}

#[test]
fn a_symlink_anywhere_in_a_path_is_refused() {
    // A symlink as the file, or as any directory on the way, is refused, whatever it leads to:
    // a file, a directory, nothing, or itself.
    let directory = fresh_directory("symlinks");
    write(&directory, "hello.stella", GOOD);
    write(&directory, "programs/deep/hello.stella", GOOD);
    for (link, target) in [
        ("link.stella", "hello.stella"),
        ("linked", "programs"),
        ("programs/linked", "deep"),
        ("gone.stella", "missing.stella"),
        ("loop.stella", "loop.stella"),
    ] {
        symlink(target, directory.join(link)).expect("make a symlink");
    }
    for typed in [
        "link.stella",
        "linked/deep/hello.stella",
        "programs/linked/hello.stella",
        "gone.stella",
        "loop.stella",
    ] {
        let failure = stored_of(&directory, typed).expect_err("a symlink is refused");
        assert!(matches!(failure, SearchFailure::Symlink), "for {typed}: {failure:?}");
        let sentence =
            "the build path can't be established from a path that goes through a symlink.";
        assert_eq!(failure.to_string(), sentence, "for {typed}");
    }
}

#[test]
fn what_cant_be_found_says_why() {
    // Each path that can't be followed, with what's said of it and the sentence that prints: a
    // name that's nowhere, a file where a directory should be, and directories that can't be
    // searched or listed.
    let directory = fresh_directory("not-found");
    write(&directory, "file", b"");
    write(&directory, "locked/hello.stella", GOOD);
    write(&directory, "listless/hello.stella", GOOD);
    let _locked = ModeGuard::new(directory.join("locked"), 0o000);
    let _listless = ModeGuard::new(directory.join("listless"), 0o100);
    let failure_of = |typed: &str| stored_of(&directory, typed).expect_err("it can't be found");
    for typed in ["missing.stella", "missing/hello.stella"] {
        let missing = failure_of(typed);
        assert!(
            matches!(missing, SearchFailure::Access(AccessFailure::Missing)),
            "for {typed}: {missing:?}"
        );
        assert_eq!(missing.to_string(), "this file doesn't exist.", "for {typed}");
    }
    for (typed, kind, sentence) in [
        (
            "file/hello.stella",
            io::ErrorKind::NotADirectory,
            "this file can't be read: Not a directory (os error 20).",
        ),
        (
            "locked/hello.stella",
            io::ErrorKind::PermissionDenied,
            "this file can't be read: Permission denied (os error 13).",
        ),
    ] {
        let unreadable = failure_of(typed);
        let SearchFailure::Access(AccessFailure::Unreadable(error)) = &unreadable else {
            unreachable!("{typed} can't be looked at, but: {unreadable:?}");
        };
        assert_eq!(error.kind(), kind, "for {typed}");
        assert_eq!(unreadable.to_string(), sentence, "for {typed}");
    }
    // A directory that can be searched but not listed lets the file be looked at, but not found
    // among its entries.
    let listless = failure_of("listless/hello.stella");
    assert!(matches!(listless, SearchFailure::DirectoryUnreadable(_)), "{listless:?}");
    let sentence =
        "a directory on the way to this file can't be read: Permission denied (os error 13).";
    assert_eq!(listless.to_string(), sentence);
}

#[cfg(target_os = "macos")]
#[test]
fn a_path_through_a_mount_point_is_found() {
    // A mount point is listed in its directory with the inode of the directory it covers, not of
    // what's mounted there, so it can't be found by matching the inode a listing gives. It's
    // found by its name. Here a small disk image is mounted at `volume`, and paths through it are
    // found as they're stored, on a volume that tells names apart by case.
    let scratch = Scratch::of("stored");
    let parent = scratch.path();
    // A run that was killed may have left its image mounted, which can't be removed until it's
    // unmounted. Nothing is mounted there otherwise, so this is allowed to find nothing.
    _ = unmount(&parent.join("mount-point/volume"));
    let directory = fresh_directory("mount-point");
    let mounted = Mount::in_directory(&directory);
    write(&mounted.mount_point, "hello.stella", GOOD);
    write(&mounted.mount_point, "programs/hello.stella", GOOD);
    write(&mounted.mount_point, "Shout.stella", GOOD);
    // The listing really does give the mount point another inode than looking at it does, which
    // is what this test is here for.
    let listed = fs::read_dir(&directory)
        .expect("the directory can be listed")
        .filter_map(Result::ok)
        .find(|entry| entry.file_name() == "volume")
        .expect("the mount point is listed");
    let looked_at = fs::symlink_metadata(directory.join("volume")).expect("it can be looked at");
    assert_ne!(
        std::os::unix::fs::DirEntryExt::ino(&listed),
        std::os::unix::fs::MetadataExt::ino(&looked_at),
        "a mount point is listed with the inode of the directory it covers"
    );
    stores(&directory, "volume/hello.stella", "volume/hello.stella");
    stores(&directory, "volume/programs/hello.stella", "volume/programs/hello.stella");
    // The volume tells names apart by case, so a name in another case reaches nothing there.
    let missing = stored_of(&directory, "volume/shout.stella").expect_err("nothing is there");
    assert!(matches!(missing, SearchFailure::Access(AccessFailure::Missing)), "{missing:?}");
}
