// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the walk. They're a crate of their own, so the compiler lets them reach only
//! what the crate makes public: a directory in, everything found under it out.
//!
//! Every test works on real files, under the directory cargo gives the tests to write in. The
//! command's own tests try the walk's bound, which takes ten thousand files, through the binary.

use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use stella_cli::{Found, TooManyEntries, walk};
use stella_test_support::{Lock, fifo, write};

/// Returns a new, empty directory for one test to work in.
fn fresh_directory(name: &str) -> PathBuf {
    let parent = Path::new(env!("CARGO_TARGET_TMPDIR")).join("walk");
    stella_test_support::fresh_directory(&parent, name)
}

/// Returns what the walk found under `root`, each as its path and whether it's a source, in the
/// order the walk gave them.
fn found(root: &Path) -> Vec<(String, bool)> {
    let found = walk(root).expect("the directory holds few entries");
    found
        .iter()
        .map(|find| match find {
            Found::Source(path) => (String::from_utf8_lossy(path).into_owned(), true),
            Found::Unreadable { directory, .. } => {
                (String::from_utf8_lossy(directory).into_owned(), false)
            }
        })
        .collect()
}

#[test]
fn the_walk_finds_every_source_in_the_order_of_its_path() {
    // Sources at every depth are found, by paths relative to where the walk started, sorted by
    // their bytes: capitals before lowercase, and `programs-2/` before `programs/`.
    let directory = fresh_directory("sources");
    for path in [
        "hello.stella",
        "Shout.stella",
        "0.stella",
        "programs/a.stella",
        "programs/deep/c.stella",
        "programs-2/b.stella",
    ] {
        write(&directory, path, b"");
    }
    let expected = [
        "0.stella",
        "Shout.stella",
        "hello.stella",
        "programs-2/b.stella",
        "programs/a.stella",
        "programs/deep/c.stella",
    ];
    assert_eq!(found(&directory), expected.map(|path| (String::from(path), true)));
}

#[test]
fn the_walk_skips_what_isnt_source() {
    // None of these is found: a file that isn't named `.stella`, a directory and a FIFO that
    // are, anything whose name starts with `.`, anything under `target`, and a symlink to a
    // source or to a directory of them.
    let directory = fresh_directory("skipped");
    write(&directory, "hello.stella", b"");
    write(&directory, "notes.txt", b"");
    write(&directory, "hello.stella.txt", b"");
    write(&directory, "other.Stella", b"");
    fs::create_dir(directory.join("folder.stella")).expect("the directory is ours to make");
    fifo(&directory.join("pipe.stella"));
    write(&directory, ".hidden/bad.stella", b"");
    write(&directory, ".bad.stella", b"");
    write(&directory, "target/bad.stella", b"");
    write(&directory, "programs/target/bad.stella", b"");
    write(&directory, "programs/a.stella", b"");
    symlink("hello.stella", directory.join("link.stella")).expect("make a file symlink");
    symlink("programs", directory.join("linked")).expect("make a directory symlink");
    symlink(".", directory.join("loop")).expect("make a symlink to the directory itself");
    let expected = ["hello.stella", "programs/a.stella"];
    assert_eq!(found(&directory), expected.map(|path| (String::from(path), true)));
    // A directory named like a source is still walked into.
    write(&directory, "folder.stella/inside.stella", b"");
    let expected = ["folder.stella/inside.stella", "hello.stella", "programs/a.stella"];
    assert_eq!(found(&directory), expected.map(|path| (String::from(path), true)));
}

#[test]
fn a_directory_that_cant_be_read_is_found_too() {
    // A directory the walk can't list is a find of its own, with why, in order among the
    // sources. Everything else is still found.
    let directory = fresh_directory("unreadable");
    write(&directory, "hello.stella", b"");
    write(&directory, "locked/inside.stella", b"");
    write(&directory, "programs/a.stella", b"");
    let lock = Lock::new(directory.join("locked"), 0o000);
    let walked = walk(&directory).expect("the directory holds few entries");
    let [Found::Source(first), Found::Unreadable { directory: locked, error }, Found::Source(last)] =
        walked.as_slice()
    else {
        unreachable!("the walk finds a source, the locked directory and a source: {walked:?}");
    };
    assert_eq!(
        (first.as_slice(), last.as_slice()),
        (&b"hello.stella"[..], &b"programs/a.stella"[..])
    );
    assert_eq!(locked, b"locked");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    drop(lock);
    // When it's the directory the walk starts in, it's called `.`.
    let _lock = Lock::new(directory.clone(), 0o100);
    assert_eq!(found(&directory), [(String::from("."), false)]);
}

#[test]
fn an_empty_directory_holds_nothing_to_find() {
    // No entries, no finds, and no error: saying there's nothing to lint is the command's to do.
    let directory = fresh_directory("empty");
    assert_eq!(found(&directory), []);
}

#[test]
fn too_many_entries_has_its_sentence() {
    // The sentence names the bound, and is pinned exactly, because people and tools read it.
    let sentence = "there are more than 10,000 files and directories under this directory.";
    assert_eq!(TooManyEntries.to_string(), sentence);
}
