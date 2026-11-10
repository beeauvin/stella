// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! End-to-end tests for `stella lint`. Each one runs the real binary in a directory of its own and
//! pins exactly what it does: its exit status, its stdout and its stderr.

use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use stella_cli::WALK_ENTRIES_MAX;
use stella_linter_text::SOURCE_BYTES_MAX;

/// This is the smallest program, which passes.
const GOOD: &[u8] = b"function main() returns Void {\n}\n";

/// This program is refused at 1:32, because its comment shares a line with code.
const COMMENT_SHARES_LINE: &[u8] = b"function main() returns Void { // hi\n}\n";

/// This is the message for `COMMENT_SHARES_LINE`, after its path.
const COMMENT_SHARES_LINE_MESSAGE: &str = ":1:32: a comment can't share a line with code.\n    \
     function main() returns Void { // hi\n                                   ^\n";

/// This is the line every misuse ends with.
const USAGE: &str = "usage: stella lint [path]\n";

/// This is the sentence for a directory's name that isn't kebab.
const DIRECTORY_NOT_KEBAB: &str = "a directory's name must be lowercase letters and digits \
                                   joined by single hyphens, like `hello-world`.";

/// This is the sentence for a source file's name that isn't kebab before `.stella`.
const NAME_NOT_KEBAB: &str = "a source file's name, before `.stella`, must be lowercase letters \
                              and digits joined by single hyphens, like `hello-world`.";

/// This is the sentence for a path that goes through a symlink.
const PATH_SYMLINK: &str =
    "the build path can't be established from a path that goes through a symlink.";

/// A run is what one run of `stella` did.
#[derive(Debug, PartialEq, Eq)]
struct Run {
    status: i32,
    stdout: String,
    stderr: String,
}

/// Returns a run that exited with `status`, wrote nothing to stdout, and wrote `stderr`.
fn exits(status: i32, stderr: &str) -> Run {
    Run { status, stdout: String::new(), stderr: String::from(stderr) }
}

/// Runs `stella` in `directory` with `arguments`, and nothing on stdin.
fn stella(directory: &Path, arguments: &[&str]) -> Run {
    let arguments: Vec<&OsStr> = arguments.iter().map(OsStr::new).collect();
    stella_with(directory, &arguments)
}

/// Runs `stella` in `directory` with `arguments`, which needn't be UTF-8.
fn stella_with(directory: &Path, arguments: &[&OsStr]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_stella"))
        .args(arguments)
        .current_dir(directory)
        .stdin(Stdio::null())
        .output()
        .expect("run stella");
    Run {
        status: output.status.code().expect("stella exits rather than being killed"),
        stdout: String::from_utf8(output.stdout).expect("stdout is UTF-8"),
        stderr: String::from_utf8(output.stderr).expect("stderr is UTF-8"),
    }
}

/// Returns a new, empty directory for one test, or one row of a table, to work in.
fn fresh_directory(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lint").join(name);
    if path.exists() {
        fs::remove_dir_all(&path).expect("remove an old test directory");
    }
    fs::create_dir_all(&path).expect("make a test directory");
    path
}

/// Writes `contents` to `path` under `directory`, making any directories it needs.
fn write(directory: &Path, path: &str, contents: &[u8]) {
    let path = directory.join(path);
    fs::create_dir_all(path.parent().expect("a file is in a directory")).expect("make directories");
    fs::write(path, contents).expect("write a file");
}

/// Makes a FIFO at `path`. Std has no way to, so this runs `mkfifo`.
fn fifo(path: &Path) {
    let status = Command::new("/usr/bin/mkfifo").arg(path).status().expect("run mkfifo");
    assert!(status.success(), "mkfifo {}", path.display());
}

/// A lock takes away a file's or directory's permissions until it's dropped. So even a test that
/// fails can't leave behind a directory the next run can't remove.
struct Lock {
    path: PathBuf,
}

impl Lock {
    /// Sets `path`'s permissions to `mode` until the lock is dropped.
    fn new(path: PathBuf, mode: u32) -> Lock {
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("lock");
        Lock { path }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // A test may have failed already, so this mustn't panic too.
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o755));
    }
}

/// Writes `files` into a new directory for `name`, then checks that `stella lint path` there does
/// what's `expected`.
fn lints_one(name: &str, files: &[(&str, &[u8])], path: &str, expected: Run) {
    let directory = fresh_directory(&format!("one-path-{name}"));
    for (file, contents) in files {
        write(&directory, file, contents);
    }
    assert_eq!(stella(&directory, &["lint", path]), expected, "for {name}");
}

#[test]
fn one_path() {
    for (name, files, path, expected) in [
        ("passes", &[("hello.stella", GOOD)][..], "hello.stella", exits(0, "")),
        (
            "refused",
            &[("hello.stella", COMMENT_SHARES_LINE)],
            "hello.stella",
            exits(1, &format!("hello.stella{COMMENT_SHARES_LINE_MESSAGE}")),
        ),
        // The path is shown as it was given.
        (
            "as-given",
            &[("programs/hello.stella", COMMENT_SHARES_LINE)],
            "./programs/hello.stella",
            exits(1, &format!("./programs/hello.stella{COMMENT_SHARES_LINE_MESSAGE}")),
        ),
        // The path is checked before the file is read, so a refused one needn't exist.
        (
            "refused-name",
            &[],
            "Hello.stella",
            exits(
                1,
                "Hello.stella: a source file's name, before `.stella`, must be lowercase letters \
                 and digits joined by single hyphens, like `hello-world`.\n",
            ),
        ),
        ("missing", &[], "hello.stella", exits(2, "hello.stella: this file doesn't exist.\n")),
        (
            "missing-as-given",
            &[],
            "./hello.stella",
            exits(2, "./hello.stella: this file doesn't exist.\n"),
        ),
    ] {
        lints_one(name, files, path, expected);
    }
}

#[test]
fn one_path_sizes() {
    let too_big_message =
        "big.stella: this file is over 16,384 bytes, the most a source file can hold.\n";
    let largest = format!("// {}\n", "x".repeat(SOURCE_BYTES_MAX as usize - "// \n".len()));
    let too_big = vec![b'x'; SOURCE_BYTES_MAX as usize + 1];
    for (name, files, path, expected) in [
        ("largest", &[("hello.stella", largest.as_bytes())][..], "hello.stella", exits(0, "")),
        (
            "empty",
            &[("hello.stella", b"")],
            "hello.stella",
            exits(1, "hello.stella: this file is empty.\n"),
        ),
        ("too-big", &[("big.stella", &too_big)], "big.stella", exits(1, too_big_message)),
    ] {
        lints_one(name, files, path, expected);
    }
}

#[test]
fn a_name_that_isnt_utf8() {
    let directory = fresh_directory("not-utf8");
    let path = OsStr::from_bytes(b"a\xFF.stella");
    let expected = exits(
        1,
        "a\\xFF.stella: a source file's name, before `.stella`, must be lowercase letters and \
         digits joined by single hyphens, like `hello-world`.\n",
    );
    assert_eq!(stella_with(&directory, &[OsStr::new("lint"), path]), expected);
}

#[test]
fn not_a_file() {
    let directory = fresh_directory("not-a-file");
    fs::create_dir(directory.join("folder.stella")).expect("make a directory");
    // A FIFO isn't a file either, and the answer comes at once, with nothing writing to it.
    fifo(&directory.join("pipe.stella"));
    for path in ["folder.stella", "pipe.stella"] {
        let expected = exits(2, &format!("{path}: this isn't a file.\n"));
        assert_eq!(stella(&directory, &["lint", path]), expected, "for {path}");
    }
}

#[test]
fn an_unreadable_file() {
    let directory = fresh_directory("unreadable-file");
    write(&directory, "hello.stella", GOOD);
    let _lock = Lock::new(directory.join("hello.stella"), 0o000);
    // The path is shown as it was given.
    for path in ["hello.stella", "./hello.stella"] {
        let sentence = "this file can't be read: Permission denied (os error 13).";
        let expected = exits(2, &format!("{path}: {sentence}\n"));
        assert_eq!(stella(&directory, &["lint", path]), expected, "for {path}");
    }
}

#[test]
fn unreadable_paths() {
    // A path is followed before the file is read, so a directory on the way that can't be searched
    // stops it, and so does a file where a directory should be. Each name is looked up where it's
    // stored, so a directory that can be searched but not read stops it too.
    let directory = fresh_directory("unreadable-paths");
    write(&directory, "locked/hello.stella", GOOD);
    write(&directory, "listless/hello.stella", GOOD);
    write(&directory, "file", b"");
    let _locked = Lock::new(directory.join("locked"), 0o000);
    let _listless = Lock::new(directory.join("listless"), 0o100);
    let listless =
        "a directory on the way to this file can't be read: Permission denied (os error 13).";
    for (path, sentence) in [
        ("locked/hello.stella", "this file can't be read: Permission denied (os error 13)."),
        ("file/hello.stella", "this file can't be read: Not a directory (os error 20)."),
        ("listless/hello.stella", listless),
    ] {
        let expected = exits(2, &format!("{path}: {sentence}\n"));
        assert_eq!(stella(&directory, &["lint", path]), expected, "for {path}");
    }
}

#[test]
fn names_are_checked_as_stored() {
    // macOS ignores case, so a path typed in lowercase reaches a file stored with capitals. The
    // path is checked again as it's stored, so a named file is held to the same names the walk is.
    let directory = fresh_directory("as-stored");
    write(&directory, "Hello.stella", GOOD);
    assert!(
        directory.join("hello.stella").exists(),
        "this test needs a filesystem that ignores case, as macOS's does by default"
    );
    for (name, files, path, expected) in [
        (
            "file",
            &[("Hello.stella", GOOD)][..],
            "hello.stella",
            exits(1, &format!("Hello.stella: {NAME_NOT_KEBAB}\n")),
        ),
        (
            "directory",
            &[("Programs/hello.stella", GOOD)],
            "programs/hello.stella",
            exits(1, &format!("Programs/hello.stella: {DIRECTORY_NOT_KEBAB}\n")),
        ),
        (
            "suffix",
            &[("HELLO.STELLA", GOOD)],
            "hello.stella",
            exits(1, "HELLO.STELLA: a source file's name must end in `.stella`.\n"),
        ),
        // Case goes past ASCII: the Kelvin sign is a capital `k`. The others are only beside it,
        // and are found by name or by inode as not being it.
        (
            "kelvin",
            &[
                ("a.stella", GOOD),
                ("\u{E9}.stella", GOOD),
                ("\u{212A}ey.stella", GOOD),
                ("\u{FC}.stella", GOOD),
                ("z.stella", GOOD),
            ],
            "key.stella",
            exits(1, &format!("\\u{{212A}}ey.stella: {NAME_NOT_KEBAB}\n")),
        ),
    ] {
        lints_one(&format!("as-stored-{name}"), files, path, expected);
    }
    // A file with two names is checked by the one it's named by, in whichever order they're
    // listed. A kebab name for the file never stands in for the name that was typed.
    let directory = fresh_directory("as-stored-hard-links");
    for (name, other) in [
        ("Hello.stella", "hello-2.stella"),
        ("shout-2.stella", "Shout.stella"),
        ("\u{212A}ey.stella", "a.stella"),
    ] {
        write(&directory, name, GOOD);
        fs::hard_link(directory.join(name), directory.join(other)).expect("make a hard link");
    }
    for (path, expected) in [
        ("hello-2.stella", exits(0, "")),
        ("hello.stella", exits(1, &format!("Hello.stella: {NAME_NOT_KEBAB}\n"))),
        ("shout-2.stella", exits(0, "")),
        ("shout.stella", exits(1, &format!("Shout.stella: {NAME_NOT_KEBAB}\n"))),
        ("a.stella", exits(0, "")),
        ("key.stella", exits(1, &format!("\\u{{212A}}ey.stella: {NAME_NOT_KEBAB}\n"))),
    ] {
        assert_eq!(stella(&directory, &["lint", path]), expected, "for {path}");
    }
}

#[test]
fn named_symlinks_are_refused() {
    // A named path can't go through a symlink anywhere, just as the walk never follows one, so
    // it's refused before anything it leads to is looked at.
    let directory = fresh_directory("named-symlinks");
    let inside = directory.join("inside");
    write(&directory, "outside/hello.stella", GOOD);
    write(&inside, "hello.stella", GOOD);
    write(&inside, "programs/hello.stella", GOOD);
    write(&inside, "Upper/hello.stella", GOOD);
    fifo(&inside.join("pipe.stella"));
    let absolute = directory.join("outside/hello.stella");
    for (link, target) in [
        (Path::new("link.stella"), Path::new("hello.stella")),
        (Path::new("linked"), Path::new("programs")),
        (Path::new("out.stella"), Path::new("../outside/hello.stella")),
        (Path::new("absolute.stella"), absolute.as_path()),
        (Path::new("gone.stella"), Path::new("missing.stella")),
        (Path::new("loop.stella"), Path::new("loop.stella")),
        (Path::new("here.stella"), Path::new(".")),
        (Path::new("pipe-link.stella"), Path::new("pipe.stella")),
        (Path::new("Upper/link.stella"), Path::new("hello.stella")),
    ] {
        symlink(target, inside.join(link)).expect("make a symlink");
    }
    for path in [
        "link.stella",
        "./link.stella",
        "linked/hello.stella",
        "out.stella",
        "absolute.stella",
        "gone.stella",
        "loop.stella",
        "here.stella",
        "pipe-link.stella",
        // The symlink is found before the names are checked as they're stored, so this isn't
        // refused for `Upper`.
        "upper/link.stella",
    ] {
        let expected = exits(1, &format!("{path}: {PATH_SYMLINK}\n"));
        assert_eq!(stella(&inside, &["lint", path]), expected, "for {path}");
    }
    // The directory the command runs in may be reached through a symlink, since that's no part of
    // the path.
    symlink("inside", directory.join("via")).expect("make a symlink");
    assert_eq!(stella(&directory.join("via"), &["lint", "hello.stella"]), exits(0, ""));
}

#[test]
fn a_named_path_is_bounded() {
    // Each name is looked up among the entries of its directory, and no more than 10,000 entries
    // are looked at in all. Here the start directory holds one, `programs`, and `programs` holds
    // the rest.
    let directory = fresh_directory("named-bounded");
    let entries_max = WALK_ENTRIES_MAX as usize;
    write(&directory, "programs/hello.stella", GOOD);
    for index in 0..entries_max - 2 {
        write(&directory, &format!("programs/{index}.txt"), b"");
    }
    assert_eq!(stella(&directory, &["lint", "programs/hello.stella"]), exits(0, ""));
    write(&directory, "programs/one-more.txt", b"");
    let sentence = "the directories on the way to this file hold more than 10,000 entries.";
    let expected = exits(2, &format!("programs/hello.stella: {sentence}\n"));
    assert_eq!(stella(&directory, &["lint", "programs/hello.stella"]), expected);
}

#[test]
fn every_program_in_a_tree() {
    let directory = fresh_directory("tree");
    // A program that passes comes first, so the first message has nothing before it.
    write(&directory, "0.stella", GOOD);
    write(&directory, "hello.stella", GOOD);
    write(&directory, "programs/a.stella", b"function main() returns Void { \n}\n");
    write(&directory, "programs/b.stella", GOOD);
    write(&directory, "programs/deep/c.stella", b"function main() returns Void {\n\t// x\n}\n");
    write(&directory, "programs-2/b.stella", COMMENT_SHARES_LINE);
    write(&directory, "targets/c.stella", b"function main() returns Void {\n}");
    // macOS ignores case, so these can't be `Programs/` and `Hello.stella` beside the others.
    write(&directory, "Other/x.stella", GOOD);
    write(&directory, "Shout.stella", GOOD);
    // None of these is checked. Some aren't `.stella` source files, and the rest are where source
    // never goes.
    write(&directory, "notes.txt", b"\t");
    write(&directory, "hello.stella.txt", b"\t");
    write(&directory, "other.Stella", b"\t");
    fs::create_dir(directory.join("folder.stella")).expect("make a directory");
    fifo(&directory.join("pipe.stella"));
    write(&directory, ".hidden/bad.stella", b"\t");
    write(&directory, ".bad.stella", b"\t");
    write(&directory, ".build/programs/bad.stella", b"\t");
    write(&directory, "target/bad.stella", b"\t");
    write(&directory, "programs/target/bad.stella", b"\t");
    symlink("programs", directory.join("linked")).expect("make a directory symlink");
    // Each refused file gets its first problem, with a blank line between messages. They come in
    // the order of their paths' bytes, so `programs-2/` comes before `programs/`.
    let expected = exits(
        1,
        &format!(
            "Other/x.stella: a directory's name must be lowercase letters and digits joined by \
             single hyphens, like `hello-world`.\n\
             \n\
             Shout.stella: a source file's name, before `.stella`, must be lowercase letters and \
             digits joined by single hyphens, like `hello-world`.\n\
             \n\
             programs-2/b.stella{COMMENT_SHARES_LINE_MESSAGE}\
             \n\
             programs/a.stella:1:31: a line can't end in a space.\n    \
             function main() returns Void {{ \n                                  ^\n\
             \n\
             programs/deep/c.stella:2:1: a tab can't appear in a source file; indent with four \
             spaces.\n    \\t// x\n    ^\n\
             \n\
             targets/c.stella:2:2: the file must end with a newline.\n    }}\n     ^\n"
        ),
    );
    assert_eq!(stella(&directory, &["lint"]), expected);
}

#[test]
fn the_worst_status_wins() {
    // A file that can't be read is exit 2, which is worse than a refusal's 1, whichever comes
    // first.
    for (refused, unreadable) in [("a.stella", "b.stella"), ("b.stella", "a.stella")] {
        let directory = fresh_directory(&format!("worst-{refused}"));
        write(&directory, refused, COMMENT_SHARES_LINE);
        write(&directory, unreadable, GOOD);
        let _lock = Lock::new(directory.join(unreadable), 0o000);
        let refusal = format!("{refused}{COMMENT_SHARES_LINE_MESSAGE}");
        let failure =
            format!("{unreadable}: this file can't be read: Permission denied (os error 13).\n");
        let stderr = if refused < unreadable {
            format!("{refusal}\n{failure}")
        } else {
            format!("{failure}\n{refusal}")
        };
        assert_eq!(stella(&directory, &["lint"]), exits(2, &stderr), "for {refused} refused");
    }
}

#[test]
fn every_program_passes() {
    let directory = fresh_directory("all-pass");
    write(&directory, "hello.stella", GOOD);
    write(&directory, "programs/hello.stella", GOOD);
    assert_eq!(stella(&directory, &["lint"]), exits(0, ""));
}

#[test]
fn no_programs() {
    let directory = fresh_directory("none");
    write(&directory, "notes.txt", b"");
    write(&directory, "target/hello.stella", GOOD);
    let expected = exits(2, "stella: there are no `.stella` files under this directory.\n");
    assert_eq!(stella(&directory, &["lint"]), expected);
}

#[test]
fn symlinks_are_skipped() {
    let directory = fresh_directory("symlinks");
    write(&directory, "hello.stella", GOOD);
    write(&directory, ".hidden/bad.stella", b"\t");
    symlink(".hidden/bad.stella", directory.join("link.stella")).expect("make a file symlink");
    symlink(".", directory.join("loop")).expect("make a directory symlink");
    assert_eq!(stella(&directory, &["lint"]), exits(0, ""));
}

#[test]
fn unreadable_directories() {
    // A directory that can't be read gets a message of its own, and every other file is still
    // checked.
    let directory = fresh_directory("unreadable-directory");
    write(&directory, "hello.stella", COMMENT_SHARES_LINE);
    write(&directory, "locked/hello.stella", GOOD);
    let lock = Lock::new(directory.join("locked"), 0o000);
    let expected = exits(
        2,
        &format!(
            "hello.stella{COMMENT_SHARES_LINE_MESSAGE}\n\
             locked: this directory can't be read: Permission denied (os error 13).\n"
        ),
    );
    assert_eq!(stella(&directory, &["lint"]), expected);
    drop(lock);
    // When it's the directory the walk starts in, it's called `.`.
    let _lock = Lock::new(directory.clone(), 0o100);
    let expected = exits(2, ".: this directory can't be read: Permission denied (os error 13).\n");
    assert_eq!(stella(&directory, &["lint"]), expected);
}

#[test]
fn the_walk_is_bounded() {
    let directory = fresh_directory("bounded");
    let entries_max = WALK_ENTRIES_MAX as usize;
    // The start directory holds five entries: a program, `.git`, `target`, `a` and `b`.
    write(&directory, "hello.stella", GOOD);
    // What's under `.git` and `target` doesn't count, however much there is.
    for index in 0..=entries_max {
        write(&directory, &format!(".git/{index}"), b"");
        write(&directory, &format!("target/{index}"), b"");
    }
    // The files in `a` and `b` bring the count to exactly the most the walk looks at.
    for index in 0..entries_max / 2 - 5 {
        write(&directory, &format!("a/{index}.txt"), b"");
    }
    for index in 0..entries_max / 2 {
        write(&directory, &format!("b/{index}.txt"), b"");
    }
    assert_eq!(stella(&directory, &["lint"]), exits(0, ""));
    write(&directory, "b/one-more.txt", b"");
    let expected = exits(
        2,
        "stella: there are more than 10,000 files and directories under this directory.\n",
    );
    assert_eq!(stella(&directory, &["lint"]), expected);
}

#[test]
fn stderr_that_cant_be_written() {
    // Each of these characters is shown as a 7-byte escape, so the message is about 115 KB, far
    // more than a pipe holds. Once the reader has gone, the rest can't be written.
    let directory = fresh_directory("unwritable-stderr");
    let source = format!("// {}\t", "\u{7FF}".repeat(8_189));
    write(&directory, "wide.stella", source.as_bytes());
    for arguments in [&["lint", "wide.stella"][..], &["lint"]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_stella"))
            .args(arguments)
            .current_dir(&directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run stella");
        let mut stderr = child.stderr.take().expect("stderr is piped");
        // The start of the message proves it's the lint's, not one of the other ways to exit 2.
        let mut start = [0_u8; 19];
        stderr.read_exact(&mut start).expect("read the start of the message");
        assert_eq!(&start, b"wide.stella:1:8193:", "for {arguments:?}");
        drop(stderr);
        assert_eq!(child.wait().expect("wait for stella").code(), Some(2), "for {arguments:?}");
    }
    // A stderr open only for reading can't be written either.
    let read_only = fs::File::open("/dev/null").expect("open /dev/null");
    let status = Command::new(env!("CARGO_BIN_EXE_stella"))
        .args(["lint", "Hello.stella"])
        .current_dir(&directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(read_only)
        .status()
        .expect("run stella");
    assert_eq!(status.code(), Some(2));
}

#[test]
fn a_closed_stderr() {
    // Rust opens `/dev/null` in place of a closed stderr before `main` runs, so a message to it is
    // written and lost, and the status is the lint's. Std can't close a child's stderr, so the
    // shell does.
    let directory = fresh_directory("closed-stderr");
    write(&directory, "hello.stella", GOOD);
    for (path, expected) in [("hello.stella", 0), ("Hello.stella", 1)] {
        let status = Command::new("/bin/sh")
            .args(["-c", "exec \"$0\" lint \"$1\" 2>&-", env!("CARGO_BIN_EXE_stella"), path])
            .current_dir(&directory)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .expect("run stella")
            .code();
        assert_eq!(status, Some(expected), "for {path}");
    }
}

#[test]
fn misuse() {
    let directory = fresh_directory("misuse");
    write(&directory, "hello.stella", GOOD);
    for (arguments, sentence) in [
        (&[][..], "stella: a command is needed.\n"),
        (&[""], "stella: a command is needed.\n"),
        (&["bild"], "stella: `bild` isn't a command.\n"),
        // An argument is shown safely, like a path.
        (&["bi\u{1B}[31mld"], "stella: `bi\\u{1B}[31mld` isn't a command.\n"),
        (&["--help"], "stella: `--help` isn't an option; there are none.\n"),
        (&["lint", "--fix"], "stella: `--fix` isn't an option; there are none.\n"),
        (&["lint", "hello.stella", "--fix"], "stella: `--fix` isn't an option; there are none.\n"),
        // Source can't be piped in, so `-` is an option like any other.
        (&["lint", "-"], "stella: `-` isn't an option; there are none.\n"),
        (
            &["lint", "hello.stella", "hello.stella"],
            "stella: `lint` takes one path, or none to lint every file.\n",
        ),
        (&["lint", ""], "stella: the path is empty.\n"),
    ] {
        let expected = exits(2, &format!("{sentence}{USAGE}"));
        assert_eq!(stella(&directory, arguments), expected, "for {arguments:?}");
    }
}
