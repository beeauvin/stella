// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for saving a binary. They're a crate of their own, so the compiler lets them reach
//! only what the crate makes public: a path that passed the path lint and some bytes in, and the
//! bytes in `.build/` out, or where and why they couldn't be put there.
//!
//! Every test works on real files, under the directory cargo gives the tests to write in, since
//! what's on the filesystem is the thing being checked. The bytes saved are plain text, not a
//! binary, so nothing here depends on what the compiler makes. One kind of failure isn't seen:
//! `Changing` takes something else writing in `.build/` between two looks at one path.

#![forbid(unsafe_code)]

use std::fs;
use std::io;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::process;
use std::thread;

use stella_cli::{BUILD_DIRECTORY, PARTS_MAX, SaveFailure, SaveFailureKind, save};
use stella_linter_path::lint_path;
use stella_test_support::{ModeGuard, Scratch, fifo, write};

/// Returns a new, empty directory for one test, or one row of a table, to work in.
fn fresh_directory(name: &str) -> PathBuf {
    Scratch::of("save").fresh(name)
}

/// Saves `bytes` as the binary of the source at `source`, which must pass the path lint, under
/// `root`.
fn saved(root: &Path, source: &str, bytes: &[u8]) -> Result<(), SaveFailure> {
    save(root, lint_path(source).expect("the source's path should pass the path lint"), bytes)
}

/// This is the number of a process there never is, to name a part no save of these tests
/// makes: a part some other build left.
const NO_PROCESS: u32 = u32::MAX;

/// Returns the path of the part a save by this process makes in `directory`, a path from the
/// directory the build runs in, when the counts before `count` are taken:
/// `.<this process's number>.<count>.part`.
fn part_in(directory: &str, count: u32) -> String {
    format!("{directory}/.{}.{count}.part", process::id())
}

/// Returns the names of what's in `directory`, in order.
fn names_in(directory: &Path) -> Vec<String> {
    let entries = fs::read_dir(directory).expect("the tests' own directory can be listed");
    let mut names: Vec<String> = entries
        .map(|entry| entry.expect("each entry can be read").file_name())
        .map(|name| name.into_string().expect("the tests' names are ASCII"))
        .collect();
    names.sort();
    names
}

/// A summary is a save failure as a test compares it: where it was, which kind it is, and the
/// operating system's kind of error if it gave one.
#[derive(Debug, PartialEq, Eq)]
struct Summary {
    path: String,
    kind: &'static str,
    system_kind: Option<io::ErrorKind>,
}

/// Returns the summary of `failure`.
fn summary_of(failure: &SaveFailure) -> Summary {
    let (kind, system_kind) = match failure.kind() {
        SaveFailureKind::NotADirectory => ("not a directory", None),
        SaveFailureKind::NotAFile => ("not a file", None),
        SaveFailureKind::TooManyParts => ("too many parts", None),
        SaveFailureKind::Changing => ("changing", None),
        SaveFailureKind::Unwritable(reason) => ("unwritable", Some(reason.kind())),
    };
    Summary { path: String::from(failure.path()), kind, system_kind }
}

/// Returns the summary of a failure of `kind` at `path`, which the operating system didn't give.
fn summary(path: &str, kind: &'static str) -> Summary {
    Summary { path: String::from(path), kind, system_kind: None }
}

/// Makes `kind` of thing at `path` under `directory`: a file that holds `kept`, a directory, a
/// FIFO, a symlink to a file named `target` that holds `kept`, or a symlink to nothing.
fn put(directory: &Path, path: &str, kind: &str) {
    if kind == "file" {
        return write(directory, path, b"kept");
    }
    let path = directory.join(path);
    match kind {
        "directory" => fs::create_dir(&path).expect("a directory can be made"),
        "fifo" => fifo(&path),
        "symlink" => {
            write(directory, "target", b"kept");
            symlink(directory.join("target"), &path).expect("a symlink can be made");
        }
        "dangling symlink" => {
            symlink(directory.join("absent"), &path).expect("a symlink can be made");
        }
        _ => unreachable!("{kind} isn't a kind of thing these tests make"),
    }
}

#[test]
fn a_binary_is_saved_at_its_sources_path_without_the_suffix() {
    // Sources at the top and two directories down, and one whose path is written with `./`.
    // Each binary holds the bytes it was given, is for anyone to run, and is alone in a
    // directory made for it. The build directory is named `.build`, so the walk skips it.
    assert_eq!(BUILD_DIRECTORY, ".build");
    let directory = fresh_directory("saved");
    for (source, binary) in [
        ("hello.stella", ".build/hello"),
        ("programs/first/hello-world.stella", ".build/programs/first/hello-world"),
        ("./tools/x2.stella", ".build/tools/x2"),
    ] {
        let bytes = format!("the bytes of {source}");
        assert!(saved(&directory, source, bytes.as_bytes()).is_ok(), "for {source}");
        let path = directory.join(binary);
        assert_eq!(fs::read(&path).expect("the binary is there"), bytes.as_bytes());
        let metadata = fs::symlink_metadata(&path).expect("the binary is there");
        assert!(metadata.is_file(), "{binary} should be a regular file");
        assert_eq!(metadata.permissions().mode() & 0o7777, 0o755, "for {binary}");
        let parent = path.parent().expect("a binary is in a directory");
        let name = path.file_name().expect("a binary has a name").to_str();
        let name = name.expect("the tests' names are ASCII");
        assert_eq!(names_in(parent), [name], "{binary} is alone when it's made");
    }
    assert_eq!(names_in(&directory.join(".build")), ["hello", "programs", "tools"]);
    assert_eq!(names_in(&directory), [".build"], "nothing is made outside `.build/`");
}

#[test]
fn saving_again_puts_a_new_file_in_place() {
    // A second save to one path. The binary holds the new bytes, in a new file, and the old
    // file, still held open, holds the old bytes: it was never rewritten in place.
    let directory = fresh_directory("again");
    assert!(saved(&directory, "hello.stella", b"first").is_ok());
    let binary = directory.join(".build/hello");
    let old_file = fs::File::open(&binary).expect("the binary can be opened");
    let inode_first = old_file.metadata().expect("an open file can be looked at").ino();
    assert!(saved(&directory, "hello.stella", b"second, and longer").is_ok());
    assert_eq!(fs::read(&binary).expect("the binary is there"), b"second, and longer");
    let inode_second = fs::metadata(&binary).expect("the binary is there").ino();
    assert_ne!(inode_second, inode_first, "a save should put a new file in place");
    let old_bytes = io::read_to_string(old_file).expect("an open file can be read");
    assert_eq!(old_bytes, "first", "the old file is never written again");
    assert_eq!(names_in(&directory.join(".build")), ["hello"], "no part is left");
}

#[test]
fn something_on_the_way_that_isnt_a_directory() {
    // A file, a FIFO and a symlink to a directory, each as `.build` and as a directory in it.
    // The save stops at it, says which path it is, and writes nothing through or over it.
    for in_the_way in [".build", ".build/programs", ".build/programs/first"] {
        for kind in ["file", "fifo", "symlink"] {
            let name = format!("way-{}-{kind}", in_the_way.replace(['.', '/'], "-"));
            let directory = fresh_directory(&name);
            fs::create_dir(directory.join("elsewhere")).expect("a directory can be made");
            let parent = Path::new(in_the_way).parent().expect("it's under the directory");
            fs::create_dir_all(directory.join(parent)).expect("directories can be made");
            match kind {
                "file" => write(&directory, in_the_way, b"kept"),
                "fifo" => fifo(&directory.join(in_the_way)),
                _ => symlink(directory.join("elsewhere"), directory.join(in_the_way))
                    .expect("a symlink can be made"),
            }
            let failure = saved(&directory, "programs/first/hello.stella", b"bytes")
                .expect_err("something is in the way");
            let row = format!("a {kind} at {in_the_way}");
            assert_eq!(summary_of(&failure), summary(in_the_way, "not a directory"), "for {row}");
            let sentence = "this isn't a directory, so the build can't go through it.";
            assert_eq!(failure.message(), format!("{in_the_way}: {sentence}\n"), "for {row}");
            assert!(names_in(&directory.join("elsewhere")).is_empty(), "for {row}");
            if kind == "file" {
                let kept = fs::read(directory.join(in_the_way)).expect("the file can be read");
                assert_eq!(kept, b"kept", "for {row}");
            }
        }
    }
}

#[test]
fn something_at_the_binarys_path_that_isnt_a_file() {
    // A directory, a FIFO, a symlink to a file and a symlink to nothing, each where the binary
    // goes. The save stops, says which path it is, leaves it there, and never writes what a
    // symlink leads to. No part is left beside it.
    let at = ".build/hello";
    for kind in ["directory", "fifo", "symlink", "dangling symlink"] {
        let directory = fresh_directory(&format!("at-binary-{}", kind.replace(' ', "-")));
        fs::create_dir(directory.join(".build")).expect("a directory can be made");
        put(&directory, at, kind);
        let before = fs::symlink_metadata(directory.join(at)).expect("it's there");
        let failure = saved(&directory, "hello.stella", b"bytes").expect_err("it's in the way");
        let row = format!("a {kind} at {at}");
        assert_eq!(summary_of(&failure), summary(at, "not a file"), "for {row}");
        let sentence = "this isn't a regular file, so the build left it alone.";
        assert_eq!(failure.message(), format!("{at}: {sentence}\n"), "for {row}");
        let after = fs::symlink_metadata(directory.join(at)).expect("it's still there");
        assert_eq!(after.file_type(), before.file_type(), "for {row}");
        assert_eq!(after.ino(), before.ino(), "for {row}");
        assert_eq!(names_in(&directory.join(".build")), ["hello"], "for {row}");
        if kind == "symlink" {
            let target = fs::read(directory.join("target")).expect("the target can be read");
            assert_eq!(target, b"kept", "for {row}");
        }
        assert!(!directory.join("absent").exists(), "nothing is made through a symlink");
    }
}

#[test]
fn whatever_is_where_a_part_would_go_is_left_alone() {
    // A file, a directory, a FIFO, a symlink to a file and a symlink to nothing, each under
    // the first name this save would give its part. Whatever it is, the save didn't make it,
    // so it isn't read, written, removed or renamed. The save makes its part at the next count
    // and saves beside it, and what a symlink leads to is never written.
    let first = part_in(".build", 0);
    let first_name = first.strip_prefix(".build/").expect("it's in `.build/`");
    for kind in ["file", "directory", "fifo", "symlink", "dangling symlink"] {
        let directory = fresh_directory(&format!("part-{}", kind.replace(' ', "-")));
        fs::create_dir(directory.join(".build")).expect("a directory can be made");
        put(&directory, &first, kind);
        let before = fs::symlink_metadata(directory.join(&first)).expect("it's there");
        assert!(saved(&directory, "hello.stella", b"bytes").is_ok(), "for a {kind}");
        let binary = fs::read(directory.join(".build/hello")).expect("the binary is there");
        assert_eq!(binary, b"bytes", "for a {kind}");
        let after = fs::symlink_metadata(directory.join(&first)).expect("it's still there");
        assert_eq!(after.file_type(), before.file_type(), "for a {kind}");
        assert_eq!(after.ino(), before.ino(), "for a {kind}");
        assert_eq!(names_in(&directory.join(".build")), [first_name, "hello"], "for a {kind}");
        if kind == "file" {
            let kept = fs::read(directory.join(&first)).expect("the file can be read");
            assert_eq!(kept, b"kept", "a file that's there isn't written");
        }
        if kind == "symlink" {
            let target = fs::read(directory.join("target")).expect("the target can be read");
            assert_eq!(target, b"kept", "what a symlink leads to isn't written");
        }
        assert!(!directory.join("absent").exists(), "nothing is made through a symlink");
    }
}

#[test]
fn another_builds_part_is_left_alone() {
    // A part with another process's number is another build's, which may be writing it now.
    // The save never reads it, removes it or renames it, and saves beside it.
    let directory = fresh_directory("others-part");
    let other = format!(".{NO_PROCESS}.0.part");
    write(&directory, &format!(".build/{other}"), b"another build's");
    let inode = fs::metadata(directory.join(".build").join(&other)).expect("it's there").ino();
    assert!(saved(&directory, "hello.stella", b"bytes").is_ok());
    assert_eq!(fs::read(directory.join(".build/hello")).expect("the binary is there"), b"bytes");
    assert_eq!(names_in(&directory.join(".build")), [other.as_str(), "hello"]);
    let kept = directory.join(".build").join(&other);
    assert_eq!(fs::read(&kept).expect("the part is there"), b"another build's");
    assert_eq!(fs::metadata(&kept).expect("the part is there").ino(), inode);
}

#[test]
fn saves_at_once_in_one_process_each_keep_their_own_part() {
    // Eight threads of this one process, each saving a program of its own into one directory,
    // at once, round after round. They share the process's number, so only the count keeps
    // their parts apart. Every save succeeds, every binary holds its own program's bytes and
    // never another's, and no part is left.
    let directory = fresh_directory("threads");
    let programs: Vec<(String, Vec<u8>)> = (0_u8..8)
        .map(|index| (format!("program-{index}.stella"), vec![b'a' + index; 16_640]))
        .collect();
    for round in 0_u32..50 {
        thread::scope(|scope| {
            for (source, bytes) in &programs {
                let directory = &directory;
                scope.spawn(move || {
                    let outcome = saved(directory, source, bytes);
                    assert!(outcome.is_ok(), "round {round}, {source}: {outcome:?}");
                });
            }
        });
        for (index, (_, bytes)) in programs.iter().enumerate() {
            let binary = directory.join(format!(".build/program-{index}"));
            let held = fs::read(&binary).expect("the binary is there");
            assert!(held == *bytes, "round {round}: program {index} holds another's bytes");
        }
    }
    let names = names_in(&directory.join(".build"));
    assert_eq!(names.len(), programs.len(), "only the binaries are left: {names:?}");
}

#[test]
fn too_many_parts_stop_a_save() {
    // Every count this process's parts can have is taken, as by saves at once or by builds
    // that were killed. The save makes no part, says so at the directory, and touches nothing.
    // With one count free, it saves there.
    let directory = fresh_directory("too-many");
    for count in 0..PARTS_MAX {
        write(&directory, &part_in(".build", count), b"left behind");
    }
    let failure = saved(&directory, "hello.stella", b"bytes").expect_err("no count is free");
    assert_eq!(summary_of(&failure), summary(".build", "too many parts"));
    let sentence = "this holds too many `.part` files, so the build can't make its own.";
    assert_eq!(failure.message(), format!(".build: {sentence}\n"));
    let names = names_in(&directory.join(".build"));
    assert_eq!(u32::try_from(names.len()), Ok(PARTS_MAX), "nothing is made or removed");
    for count in 0..PARTS_MAX {
        let part = fs::read(directory.join(part_in(".build", count))).expect("it's there");
        assert_eq!(part, b"left behind", "part {count} isn't written");
    }
    let last = directory.join(part_in(".build", PARTS_MAX - 1));
    fs::remove_file(&last).expect("the tests' own to remove");
    assert!(saved(&directory, "hello.stella", b"bytes").is_ok(), "the last count is free");
    assert_eq!(fs::read(directory.join(".build/hello")).expect("the binary is there"), b"bytes");
    assert!(!last.exists(), "the part made at the last count was moved");
}

#[test]
fn a_directory_that_cant_be_written() {
    // The directory the command runs in, `.build/`, and a directory in it, each without
    // permission to write. The save says where it stopped, with the operating system's kind of
    // error, and its words end the sentence. Nothing is made.
    let directory = fresh_directory("unwritable");
    let source = "programs/hello.stella";
    let denied = Some(io::ErrorKind::PermissionDenied);
    let sentence = "the binary can't be written: Permission denied (os error 13).";
    let part = part_in(".build/programs", 0);
    for (made_first, locked, stopped_at) in [
        ("", "", ".build"),
        (".build", ".build", ".build/programs"),
        (".build/programs", ".build/programs", part.as_str()),
    ] {
        fs::create_dir_all(directory.join(made_first)).expect("directories can be made");
        let guard = ModeGuard::new(directory.join(locked), 0o555);
        let failure = saved(&directory, source, b"bytes").expect_err("it can't be written");
        drop(guard);
        let expected =
            Summary { path: String::from(stopped_at), kind: "unwritable", system_kind: denied };
        assert_eq!(summary_of(&failure), expected, "with {locked:?} locked");
        assert_eq!(failure.message(), format!("{stopped_at}: {sentence}\n"));
        assert!(!directory.join(stopped_at).exists(), "nothing is made at {stopped_at}");
    }
    // A directory that can't be looked into hides what's at the binary's path, which is as far
    // as the save goes.
    fs::create_dir_all(directory.join(".build/programs")).expect("directories can be made");
    let guard = ModeGuard::new(directory.join(".build/programs"), 0o000);
    let failure = saved(&directory, source, b"bytes").expect_err("it can't be looked into");
    drop(guard);
    let expected = Summary {
        path: String::from(".build/programs/hello"),
        kind: "unwritable",
        system_kind: denied,
    };
    assert_eq!(summary_of(&failure), expected);
    assert!(names_in(&directory.join(".build/programs")).is_empty(), "nothing is made");
}

#[test]
fn a_failure_leaves_the_binary_that_was_there() {
    // A binary, then a save that stops: once with too many parts in the way, and once in a
    // directory that can't be written. Each time the binary keeps its bytes and its file.
    let directory = fresh_directory("kept");
    assert!(saved(&directory, "hello.stella", b"first").is_ok());
    let binary = directory.join(".build/hello");
    let inode_first = fs::metadata(&binary).expect("the binary is there").ino();
    for count in 0..PARTS_MAX {
        write(&directory, &part_in(".build", count), b"left behind");
    }
    let failure = saved(&directory, "hello.stella", b"second").expect_err("no count is free");
    assert_eq!(summary_of(&failure), summary(".build", "too many parts"));
    for count in 0..PARTS_MAX {
        let part = directory.join(part_in(".build", count));
        fs::remove_file(part).expect("the tests' own to remove");
    }
    let guard = ModeGuard::new(directory.join(".build"), 0o555);
    let failure = saved(&directory, "hello.stella", b"third").expect_err("it can't be written");
    drop(guard);
    assert_eq!(failure.path(), part_in(".build", 0));
    assert_eq!(fs::read(&binary).expect("the binary is there"), b"first");
    assert_eq!(fs::metadata(&binary).expect("the binary is there").ino(), inode_first);
    assert_eq!(names_in(&directory.join(".build")), ["hello"]);
}

#[test]
fn every_kind_has_its_sentence() {
    // Each kind of failure the filesystem gives no reason for displays as the sentence the
    // message ends with. `Changing` is made here, since only another writer makes it happen.
    for (kind, sentence) in [
        (
            SaveFailureKind::NotADirectory,
            "this isn't a directory, so the build can't go through it.",
        ),
        (SaveFailureKind::NotAFile, "this isn't a regular file, so the build left it alone."),
        (
            SaveFailureKind::TooManyParts,
            "this holds too many `.part` files, so the build can't make its own.",
        ),
        (SaveFailureKind::Changing, "something else is changing this, so the build left it alone."),
    ] {
        assert_eq!(kind.to_string(), sentence);
    }
}
