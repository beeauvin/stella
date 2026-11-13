// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! End-to-end tests for `stella lint`. Each one runs the real binary in a directory of its own and
//! pins exactly what it does: its exit status, its stdout and its stderr. The two tests of a
//! stderr that can't be written or is closed pin the status alone.

#![forbid(unsafe_code)]

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Read as _;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use stella_cli::{SEARCH_ENTRIES_MAX, WALK_ENTRIES_MAX};
use stella_source::{SOURCE_BYTES_MAX, usize_from_u32};
use stella_test_support::{ModeGuard, Run, Scratch, fifo, write};

/// This is the smallest program, which passes.
const GOOD: &[u8] = b"function main() returns Void {\n}\n";

/// This program is refused at 1:32, because its comment shares a line with code.
const COMMENT_SHARES_LINE: &[u8] = b"function main() returns Void { // hi\n}\n";

/// This is the message for `COMMENT_SHARES_LINE`, after its path.
const COMMENT_SHARES_LINE_MESSAGE: &str = ":1:32: a comment can't share a line with code.\n    \
     function main() returns Void { // hi\n                                   ^\n";

/// This program passes the text lint, and the lexer refuses it at 1:32, because `;` can't start a
/// token.
const UNEXPECTED: &[u8] = b"function main() returns Void { ;\n}\n";

/// This is the message for `UNEXPECTED`, after its path.
const UNEXPECTED_MESSAGE: &str = ":1:32: `;` can't start a token.\n    \
     function main() returns Void { ;\n                                   ^\n";

/// This program passes the text lint and the lexer, and the tokens lint refuses it at 1:30,
/// because `{` doesn't end its line.
const EMPTY_BODY: &[u8] = b"function main() returns Void {}\n";

/// This is the message for `EMPTY_BODY`, after its path.
const EMPTY_BODY_MESSAGE: &str = ":1:30: `{` must end its line.\n    \
     function main() returns Void {}\n                                 ^\n";

/// This is the sentence for a type that doesn't exist.
const NO_SUCH_TYPE: &str = "this type doesn't exist; for now, the only type is `Void`.";

/// This is the sentence for a source with no `main`.
const NO_MAIN: &str = "a program needs a function named `main`.";

/// These are the lines every misuse ends with.
const USAGE: &str = "usage: stella build <path>\n       stella lint [path]\n";

/// This is the sentence for a directory's name that isn't kebab.
const DIRECTORY_NOT_KEBAB: &str = "a directory's name must be lowercase letters and digits \
                                   joined by single hyphens, like `hello-world`.";

/// This is the sentence for a source file's name that isn't kebab before `.stella`.
const NAME_NOT_KEBAB: &str = "a source file's name, before `.stella`, must be lowercase letters \
                              and digits joined by single hyphens, like `hello-world`.";

/// This is the sentence for a path that goes through a symlink.
const PATH_SYMLINK: &str =
    "the build path can't be established from a path that goes through a symlink.";

/// Runs `stella` in `directory` with `arguments`, and nothing on stdin.
fn stella(directory: &Path, arguments: &[&str]) -> Run {
    let arguments: Vec<&OsStr> = arguments.iter().map(OsStr::new).collect();
    stella_with(directory, &arguments)
}

/// Returns the path of the `stella` binary Cargo built for these tests.
fn stella_binary() -> OsString {
    env::var_os("CARGO_BIN_EXE_stella").expect("Cargo tells a test where the binary it built is")
}

/// Runs `stella` in `directory` with `arguments`, which needn't be UTF-8.
fn stella_with(directory: &Path, arguments: &[&OsStr]) -> Run {
    Run::of(Command::new(stella_binary()).args(arguments).current_dir(directory))
}

/// Returns a new, empty directory for one test, or one row of a table, to work in.
fn fresh_directory(name: &str) -> PathBuf {
    Scratch::of("lint").fresh(name)
}

/// Returns a new directory for `name` that holds `files`, each a path and its contents, for
/// `stella lint` to be given one path in.
fn directory_with(name: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let directory = fresh_directory(&format!("one-path-{name}"));
    for (file, contents) in files {
        write(&directory, file, contents);
    }
    directory
}

/// Returns what `stella lint hello.stella` does in a new directory for `name`, where
/// `hello.stella` holds `source`.
fn lints_hello(name: &str, source: &[u8]) -> Run {
    stella(&directory_with(name, &[("hello.stella", source)]), &["lint", "hello.stella"])
}

/// Returns the message for a problem at `column` of line 1 of `hello.stella`, which `sentence`
/// says, with `line` echoed under it and a caret under the column. Code is ASCII, so the caret
/// goes as many spaces in as the column, past the echo's four.
fn message_on_first_line(column: usize, sentence: &str, line: &str) -> String {
    assert!(column >= 1, "columns count from 1");
    let caret = " ".repeat(4 + column - 1);
    format!("hello.stella:1:{column}: {sentence}\n    {line}\n{caret}^\n")
}

#[test]
fn one_path() {
    // Given one path, the command lints that program: it passes in silence, is refused with a
    // message at the path as it was given, or isn't there.
    for (name, files, path, expected) in [
        ("passes", &[("hello.stella", GOOD)][..], "hello.stella", Run::exits(0, "")),
        (
            "refused",
            &[("hello.stella", COMMENT_SHARES_LINE)],
            "hello.stella",
            Run::exits(1, &format!("hello.stella{COMMENT_SHARES_LINE_MESSAGE}")),
        ),
        // The path is shown as it was given.
        (
            "as-given",
            &[("programs/hello.stella", COMMENT_SHARES_LINE)],
            "./programs/hello.stella",
            Run::exits(1, &format!("./programs/hello.stella{COMMENT_SHARES_LINE_MESSAGE}")),
        ),
        // The path is checked before the file is read, so a refused one needn't exist.
        (
            "refused-name",
            &[],
            "Hello.stella",
            Run::exits(
                1,
                "Hello.stella: a source file's name, before `.stella`, must be lowercase letters \
                 and digits joined by single hyphens, like `hello-world`.\n",
            ),
        ),
        ("missing", &[], "hello.stella", Run::exits(2, "hello.stella: this file doesn't exist.\n")),
        (
            "missing-as-given",
            &[],
            "./hello.stella",
            Run::exits(2, "./hello.stella: this file doesn't exist.\n"),
        ),
    ] {
        assert_eq!(stella(&directory_with(name, files), &["lint", path]), expected, "for {name}");
    }
}

#[test]
fn one_path_sizes() {
    // The largest source passes, and an empty file and one a byte too big are refused whole.
    let too_big_message =
        "big.stella: this file is over 16,384 bytes, the most a source file can hold.\n";
    // The largest is a program: `main`, then the longest comment that fits after it.
    let comment_bytes = usize_from_u32(SOURCE_BYTES_MAX) - GOOD.len() - "// \n".len();
    let largest =
        format!("function main() returns Void {{\n}}\n// {}\n", "x".repeat(comment_bytes));
    assert_eq!(largest.len(), usize_from_u32(SOURCE_BYTES_MAX), "the largest source is the limit");
    let too_big = vec![b'x'; usize_from_u32(SOURCE_BYTES_MAX) + 1];
    for (name, files, path, expected) in [
        ("largest", &[("hello.stella", largest.as_bytes())][..], "hello.stella", Run::exits(0, "")),
        (
            "empty",
            &[("hello.stella", b"")],
            "hello.stella",
            Run::exits(1, "hello.stella: this file is empty.\n"),
        ),
        ("too-big", &[("big.stella", &too_big)], "big.stella", Run::exits(1, too_big_message)),
    ] {
        assert_eq!(stella(&directory_with(name, files), &["lint", path]), expected, "for {name}");
    }
}

#[test]
fn a_path_that_isnt_utf8() {
    // A byte that isn't UTF-8, in each part a path has: the file's name, its extension, a
    // directory at the start, in the middle and after `./`, and behind a `/` or beside a `..`.
    // Each is refused by the check that would refuse any other bad name there, and shown with
    // the byte escaped. None stops the command any other way.
    let name = "a source file's name, before `.stella`, must be lowercase letters and digits \
                joined by single hyphens, like `hello-world`.";
    let directory_name = "a directory's name must be lowercase letters and digits joined by \
                          single hyphens, like `hello-world`.";
    let extension = "a source file's name must end in `.stella`.";
    let absolute = "the build path can't be established from an absolute path.";
    let parent = "the build path can't be established from a path that uses `..`.";
    let directory = fresh_directory("not-utf8");
    for (path, shown, sentence) in [
        (&b"a\xFF.stella"[..], "a\\xFF.stella", name),
        (b"\xFF.stella", "\\xFF.stella", name),
        (b"a.stella\xFF", "a.stella\\xFF", extension),
        (b"\xFF", "\\xFF", extension),
        (b"\xFF/a.stella", "\\xFF/a.stella", directory_name),
        (b"a/\xFF/b.stella", "a/\\xFF/b.stella", directory_name),
        (b"./\xFF/a.stella", "./\\xFF/a.stella", directory_name),
        (b"/\xFF.stella", "/\\xFF.stella", absolute),
        (b"\xFF/../a.stella", "\\xFF/../a.stella", parent),
    ] {
        let expected = Run::exits(1, &format!("{shown}: {sentence}\n"));
        let arguments = [OsStr::new("lint"), OsStr::from_bytes(path)];
        assert_eq!(stella_with(&directory, &arguments), expected, "for {shown}");
    }
}

#[test]
fn not_a_file() {
    // A directory and a FIFO named as programs aren't files, and the command says so at once.
    let directory = fresh_directory("not-a-file");
    fs::create_dir(directory.join("folder.stella")).expect("make a directory");
    // A FIFO isn't a file either, and the answer comes at once, with nothing writing to it.
    fifo(&directory.join("pipe.stella"));
    for path in ["folder.stella", "pipe.stella"] {
        let expected = Run::exits(2, &format!("{path}: this isn't a file.\n"));
        assert_eq!(stella(&directory, &["lint", path]), expected, "for {path}");
    }
}

#[test]
fn an_unreadable_file() {
    // A file whose permissions forbid reading it is a failure, not a refusal, with the reason
    // the system gives.
    let directory = fresh_directory("unreadable-file");
    write(&directory, "hello.stella", GOOD);
    let _guard = ModeGuard::new(directory.join("hello.stella"), 0o000);
    // The path is shown as it was given.
    for path in ["hello.stella", "./hello.stella"] {
        let sentence = "this file can't be read: Permission denied (os error 13).";
        let expected = Run::exits(2, &format!("{path}: {sentence}\n"));
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
    let _locked = ModeGuard::new(directory.join("locked"), 0o000);
    let _listless = ModeGuard::new(directory.join("listless"), 0o100);
    let listless =
        "a directory on the way to this file can't be read: Permission denied (os error 13).";
    for (path, sentence) in [
        ("locked/hello.stella", "this file can't be read: Permission denied (os error 13)."),
        ("file/hello.stella", "this file can't be read: Not a directory (os error 20)."),
        ("listless/hello.stella", listless),
    ] {
        let expected = Run::exits(2, &format!("{path}: {sentence}\n"));
        assert_eq!(stella(&directory, &["lint", path]), expected, "for {path}");
    }
}

#[test]
fn names_are_checked_as_stored() {
    // macOS ignores case, so a path typed in lowercase reaches a file stored with capitals. The
    // path is checked again as it's stored, so a named file is held to the names the walk holds
    // every source to, and a file the walk would pass over for its suffix is refused.
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
            Run::exits(1, &format!("Hello.stella: {NAME_NOT_KEBAB}\n")),
        ),
        (
            "directory",
            &[("Programs/hello.stella", GOOD)],
            "programs/hello.stella",
            Run::exits(1, &format!("Programs/hello.stella: {DIRECTORY_NOT_KEBAB}\n")),
        ),
        (
            "suffix",
            &[("HELLO.STELLA", GOOD)],
            "hello.stella",
            Run::exits(1, "HELLO.STELLA: a source file's name must end in `.stella`.\n"),
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
            Run::exits(1, &format!("\\u{{212A}}ey.stella: {NAME_NOT_KEBAB}\n")),
        ),
    ] {
        let directory = directory_with(&format!("as-stored-{name}"), files);
        assert_eq!(stella(&directory, &["lint", path]), expected, "for {name}");
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
        ("hello-2.stella", Run::exits(0, "")),
        ("hello.stella", Run::exits(1, &format!("Hello.stella: {NAME_NOT_KEBAB}\n"))),
        ("shout-2.stella", Run::exits(0, "")),
        ("shout.stella", Run::exits(1, &format!("Shout.stella: {NAME_NOT_KEBAB}\n"))),
        ("a.stella", Run::exits(0, "")),
        ("key.stella", Run::exits(1, &format!("\\u{{212A}}ey.stella: {NAME_NOT_KEBAB}\n"))),
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
        let expected = Run::exits(1, &format!("{path}: {PATH_SYMLINK}\n"));
        assert_eq!(stella(&inside, &["lint", path]), expected, "for {path}");
    }
    // The directory the command runs in may be reached through a symlink, since that's no part of
    // the path.
    symlink("inside", directory.join("via")).expect("make a symlink");
    assert_eq!(stella(&directory.join("via"), &["lint", "hello.stella"]), Run::exits(0, ""));
}

#[test]
fn a_named_path_is_bounded() {
    // Each name is looked up among the entries of its directory, and no more than 10,000 entries
    // are looked at in all. Here the start directory holds one, `programs`, and `programs` holds
    // the rest.
    let directory = fresh_directory("named-bounded");
    let entries_max = usize_from_u32(SEARCH_ENTRIES_MAX);
    write(&directory, "programs/hello.stella", GOOD);
    for index in 0..entries_max - 2 {
        write(&directory, &format!("programs/{index}.txt"), b"");
    }
    assert_eq!(stella(&directory, &["lint", "programs/hello.stella"]), Run::exits(0, ""));
    write(&directory, "programs/one-more.txt", b"");
    let sentence = "the directories on the way to this file hold more than 10,000 entries.";
    let expected = Run::exits(2, &format!("programs/hello.stella: {sentence}\n"));
    assert_eq!(stella(&directory, &["lint", "programs/hello.stella"]), expected);
}

#[test]
fn every_program_in_a_tree() {
    // Given no path, the command lints every `.stella` file under the directory, skips what
    // isn't source, and prints each refused file's first problem in the order of their paths.
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
    let expected = Run::exits(
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
        let _guard = ModeGuard::new(directory.join(unreadable), 0o000);
        let refusal = format!("{refused}{COMMENT_SHARES_LINE_MESSAGE}");
        let failure =
            format!("{unreadable}: this file can't be read: Permission denied (os error 13).\n");
        let stderr = if refused < unreadable {
            format!("{refusal}\n{failure}")
        } else {
            format!("{failure}\n{refusal}")
        };
        assert_eq!(stella(&directory, &["lint"]), Run::exits(2, &stderr), "for {refused} refused");
    }
}

#[test]
fn code_that_cant_be_lexed() {
    // Code that passes the text lint is lexed, and a character that can't start a token is
    // refused at its line and column.
    let message = format!("hello.stella{UNEXPECTED_MESSAGE}");
    assert_eq!(
        stella(
            &directory_with("lexed", &[("hello.stella", UNEXPECTED)]),
            &["lint", "hello.stella"]
        ),
        Run::exits(1, &message),
        "for lexed"
    );
    // The path is shown as it was given.
    let message = format!("./hello.stella{UNEXPECTED_MESSAGE}");
    assert_eq!(
        stella(
            &directory_with("lexed-as-given", &[("hello.stella", UNEXPECTED)]),
            &["lint", "./hello.stella"]
        ),
        Run::exits(1, &message),
        "for lexed-as-given"
    );
    // A name that breaks its shape is refused at the character that breaks it.
    let camel: &[u8] = b"function myFunction() returns Void {\n}\n";
    let message = "hello.stella:1:12: a name must be lowercase letters and digits joined by \
                   single underscores, like `my_function`.\n    \
                   function myFunction() returns Void {\n               ^\n";
    assert_eq!(
        stella(
            &directory_with("camel-case", &[("hello.stella", camel)]),
            &["lint", "hello.stella"]
        ),
        Run::exits(1, message),
        "for camel-case"
    );
    // The text lint runs first, so its problem wins, even when the lexer's is earlier in the file.
    let both: &[u8] = b"function main() returns Void { ;\n} \n";
    let text_first = "hello.stella:2:2: a line can't end in a space.\n    } \n     ^\n";
    assert_eq!(
        stella(&directory_with("text-first", &[("hello.stella", both)]), &["lint", "hello.stella"]),
        Run::exits(1, text_first),
        "for text-first"
    );
    // Linting every file lexes each one, and the worst status still wins.
    let directory = fresh_directory("lexed-tree");
    write(&directory, "a.stella", GOOD);
    write(&directory, "b.stella", UNEXPECTED);
    write(&directory, "c.stella", GOOD);
    write(&directory, "d.stella", GOOD);
    let _guard = ModeGuard::new(directory.join("d.stella"), 0o000);
    let expected = Run::exits(
        2,
        &format!(
            "b.stella{UNEXPECTED_MESSAGE}\n\
             d.stella: this file can't be read: Permission denied (os error 13).\n"
        ),
    );
    assert_eq!(stella(&directory, &["lint"]), expected);
}

#[test]
fn code_in_another_layout() {
    // Code that lexes is checked for the one layout, and refused at the first character at fault.
    let message = format!("hello.stella{EMPTY_BODY_MESSAGE}");
    assert_eq!(
        stella(
            &directory_with("laid-out", &[("hello.stella", EMPTY_BODY)]),
            &["lint", "hello.stella"]
        ),
        Run::exits(1, &message),
        "for laid-out"
    );
    // The path is shown as it was given.
    let message = format!("./hello.stella{EMPTY_BODY_MESSAGE}");
    assert_eq!(
        stella(
            &directory_with("laid-out-as-given", &[("hello.stella", EMPTY_BODY)]),
            &["lint", "./hello.stella"]
        ),
        Run::exits(1, &message),
        "for laid-out-as-given"
    );
    // A space where none goes is refused at the space.
    let spaced: &[u8] = b"function main () returns Void {\n}\n";
    let message = "hello.stella:1:14: no space can go between a name and `(`.\n    \
                   function main () returns Void {\n                 ^\n";
    assert_eq!(
        stella(&directory_with("spaced", &[("hello.stella", spaced)]), &["lint", "hello.stella"]),
        Run::exits(1, message),
        "for spaced"
    );
    // A space that's missing is refused at the token that needs one before it, as the layout
    // page shows.
    let unspaced: &[u8] = b"function main() returns Void{\n}\n";
    let message = "hello.stella:1:29: one space must go between a type's name and `{`.\n    \
                   function main() returns Void{\n                                ^\n";
    assert_eq!(
        stella(
            &directory_with("unspaced", &[("hello.stella", unspaced)]),
            &["lint", "hello.stella"]
        ),
        Run::exits(1, message),
        "for unspaced"
    );
    // The lexer runs first, so its problem wins, even when the layout's is earlier in the line.
    let both: &[u8] = b"function main() returns Void { ;}\n";
    let lexed_first = "hello.stella:1:32: `;` can't start a token.\n    \
                       function main() returns Void { ;}\n                                   ^\n";
    assert_eq!(
        stella(
            &directory_with("lexed-first", &[("hello.stella", both)]),
            &["lint", "hello.stella"]
        ),
        Run::exits(1, lexed_first),
        "for lexed-first"
    );
    // Linting every file checks each one's layout, and the worst status still wins.
    let directory = fresh_directory("laid-out-tree");
    write(&directory, "a.stella", GOOD);
    write(&directory, "b.stella", EMPTY_BODY);
    write(&directory, "c.stella", UNEXPECTED);
    write(&directory, "d.stella", GOOD);
    let _guard = ModeGuard::new(directory.join("d.stella"), 0o000);
    let expected = Run::exits(
        2,
        &format!(
            "b.stella{EMPTY_BODY_MESSAGE}\n\
             c.stella{UNEXPECTED_MESSAGE}\n\
             d.stella: this file can't be read: Permission denied (os error 13).\n"
        ),
    );
    assert_eq!(stella(&directory, &["lint"]), expected);
}

#[test]
fn code_that_breaks_the_grammar() {
    // Code that's laid out the one way is checked against the grammar, and refused at the first
    // token at fault.
    let no_return_type: &[u8] = b"function main() {\n}\n";
    let sentence = "`returns` and the function's return type must come after `()`.";
    let message = message_on_first_line(17, sentence, "function main() {");
    assert_eq!(lints_hello("grammar", no_return_type), Run::exits(1, &message));
    // The path is shown as it was given.
    assert_eq!(
        stella(
            &directory_with("grammar-as-given", &[("hello.stella", no_return_type)]),
            &["lint", "./hello.stella"]
        ),
        Run::exits(1, &format!("./{message}")),
    );
    // The layout runs first, so its problem wins, even when the grammar's is earlier in the line.
    let both: &[u8] = b"function Main {}\n";
    let message = message_on_first_line(15, "`{` must end its line.", "function Main {}");
    assert_eq!(lints_hello("laid-out-first", both), Run::exits(1, &message));
    // Linting every file checks each one's grammar, and the worst status still wins. Comments in
    // and around `main` pass.
    let directory = fresh_directory("grammar-tree");
    write(&directory, "a.stella", GOOD);
    write(&directory, "b.stella", b"function main() returns Void {\n    main {\n    }\n}\n");
    write(&directory, "c.stella", EMPTY_BODY);
    write(&directory, "d.stella", b"// before\nfunction main() returns Void {\n    // in it\n}\n");
    write(&directory, "e.stella", GOOD);
    let _guard = ModeGuard::new(directory.join("e.stella"), 0o000);
    let expected = Run::exits(
        2,
        &format!(
            "b.stella:2:5: a function's body can hold only comments, for now.\n        \
             main {{\n        ^\n\n\
             c.stella{EMPTY_BODY_MESSAGE}\n\
             e.stella: this file can't be read: Permission denied (os error 13).\n"
        ),
    );
    assert_eq!(stella(&directory, &["lint"]), expected);
}

#[test]
fn code_that_isnt_a_program() {
    // Code that passes the grammar is checked for what it means: first its types, then whether
    // its functions make a program. Each is refused at the first fault, and a source with no
    // function is refused as a whole, in one line.
    let unknown: &[u8] = b"function main() returns Bar {\n}\n";
    let no_such_type = message_on_first_line(25, NO_SUCH_TYPE, "function main() returns Bar {");
    assert_eq!(lints_hello("no-such-type", unknown), Run::exits(1, &no_such_type));
    let misnamed: &[u8] = b"function start() returns Void {\n}\n";
    let sentence = "a program's one function must be named `main`, for now.";
    let message = message_on_first_line(10, sentence, "function start() returns Void {");
    assert_eq!(lints_hello("not-main", misnamed), Run::exits(1, &message));
    let two: &[u8] = b"function main() returns Void {\n}\nfunction main() returns Void {\n}\n";
    let another = "hello.stella:3:1: a program holds only one function, for now.\n    \
                   function main() returns Void {\n    ^\n";
    assert_eq!(lints_hello("another", two), Run::exits(1, another));
    let comments: &[u8] = b"// only a comment\n";
    let whole_file = format!("hello.stella: {NO_MAIN}\n");
    assert_eq!(lints_hello("no-main", comments), Run::exits(1, &whole_file));
    // The types are checked before the program, so a type that doesn't exist wins, even when
    // the function's name is earlier in the line.
    let both: &[u8] = b"function start() returns Bar {\n}\n";
    let message = message_on_first_line(26, NO_SUCH_TYPE, "function start() returns Bar {");
    assert_eq!(lints_hello("types-first", both), Run::exits(1, &message));
    // And the grammar is checked before them both.
    let unparsed: &[u8] = b"function start() {\n}\n";
    let sentence = "`returns` and the function's return type must come after `()`.";
    let message = message_on_first_line(18, sentence, "function start() {");
    assert_eq!(lints_hello("grammar-first", unparsed), Run::exits(1, &message));
    // Every line passes the grammar before any type is looked for. So a line that breaks the
    // grammar wins over a type that doesn't exist, even one on a line before it.
    let later: &[u8] = b"function main() returns Bar {\n}\nfunction other() {\n}\n";
    let message = format!(
        "hello.stella:3:18: {sentence}\n    function other() {{\n{}^\n",
        " ".repeat(4 + 17)
    );
    assert_eq!(lints_hello("grammar-before-types", later), Run::exits(1, &message));
    // Linting every file checks what each one means, and the worst status still wins.
    let directory = fresh_directory("meaning-tree");
    write(&directory, "a.stella", GOOD);
    write(&directory, "b.stella", comments);
    write(&directory, "c.stella", two);
    write(&directory, "d.stella", unknown);
    write(&directory, "e.stella", GOOD);
    let _guard = ModeGuard::new(directory.join("e.stella"), 0o000);
    let expected = Run::exits(
        2,
        &format!(
            "b.stella: {NO_MAIN}\n\n{}\n{}\n\
             e.stella: this file can't be read: Permission denied (os error 13).\n",
            another.replace("hello.stella", "c.stella"),
            no_such_type.replace("hello.stella", "d.stella"),
        ),
    );
    assert_eq!(stella(&directory, &["lint"]), expected);
}

#[test]
fn every_program_passes() {
    // When every program in a tree passes, the command says nothing and exits with 0.
    let directory = fresh_directory("all-pass");
    write(&directory, "hello.stella", GOOD);
    write(&directory, "programs/hello.stella", GOOD);
    assert_eq!(stella(&directory, &["lint"]), Run::exits(0, ""));
}

#[test]
fn no_programs() {
    // A tree with no source in it, apart from what the walk skips, is a failure with a message.
    let directory = fresh_directory("none");
    write(&directory, "notes.txt", b"");
    write(&directory, "target/hello.stella", GOOD);
    let expected = Run::exits(2, "stella: there are no `.stella` files under this directory.\n");
    assert_eq!(stella(&directory, &["lint"]), expected);
}

#[test]
fn symlinks_are_skipped() {
    // The walk never follows a symlink, to a file or to a directory, so neither a bad file
    // behind one nor a loop is reached.
    let directory = fresh_directory("symlinks");
    write(&directory, "hello.stella", GOOD);
    write(&directory, ".hidden/bad.stella", b"\t");
    symlink(".hidden/bad.stella", directory.join("link.stella")).expect("make a file symlink");
    symlink(".", directory.join("loop")).expect("make a directory symlink");
    assert_eq!(stella(&directory, &["lint"]), Run::exits(0, ""));
}

#[test]
fn unreadable_directories() {
    // A directory that can't be read gets a message of its own, and every other file is still
    // checked.
    let directory = fresh_directory("unreadable-directory");
    write(&directory, "hello.stella", COMMENT_SHARES_LINE);
    write(&directory, "locked/hello.stella", GOOD);
    let guard = ModeGuard::new(directory.join("locked"), 0o000);
    let expected = Run::exits(
        2,
        &format!(
            "hello.stella{COMMENT_SHARES_LINE_MESSAGE}\n\
             locked: this directory can't be read: Permission denied (os error 13).\n"
        ),
    );
    assert_eq!(stella(&directory, &["lint"]), expected);
    drop(guard);
    // When it's the directory the walk starts in, it's called `.`.
    let _guard = ModeGuard::new(directory.clone(), 0o100);
    let expected =
        Run::exits(2, ".: this directory can't be read: Permission denied (os error 13).\n");
    assert_eq!(stella(&directory, &["lint"]), expected);
}

#[test]
fn the_walk_is_bounded() {
    // The walk looks at exactly 10,000 entries and passes, and at one more and fails. What's
    // under a directory it skips doesn't count.
    let directory = fresh_directory("bounded");
    let entries_max = usize_from_u32(WALK_ENTRIES_MAX);
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
    assert_eq!(stella(&directory, &["lint"]), Run::exits(0, ""));
    write(&directory, "b/one-more.txt", b"");
    let expected = Run::exits(
        2,
        "stella: there are more than 10,000 files and directories under this directory.\n",
    );
    assert_eq!(stella(&directory, &["lint"]), expected);
}

#[test]
fn stderr_that_cant_be_written() {
    // Each of these characters is shown as a 7-byte escape, so the message is about 115 KB, far
    // more than a pipe holds. Once the reader has gone, the rest can't be written. Neither can a
    // stderr that's open only for reading, or one that's full and doesn't wait for its reader.
    let directory = fresh_directory("unwritable-stderr");
    let source = format!("// {}\t", "\u{7FF}".repeat(8_189));
    write(&directory, "wide.stella", source.as_bytes());
    for arguments in [&["lint", "wide.stella"][..], &["lint"]] {
        let mut child = Command::new(stella_binary())
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
        assert_eq!(child.wait().expect("wait for stella").code(), Some(2_i32), "for {arguments:?}");
    }
    // A stderr open only for reading can't be written either.
    let read_only = fs::File::open("/dev/null").expect("open /dev/null");
    let status = Command::new(stella_binary())
        .args(["lint", "Hello.stella"])
        .current_dir(&directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(read_only)
        .status()
        .expect("run stella");
    assert_eq!(status.code(), Some(2_i32));
    // A stderr that doesn't wait for its reader can't be written once it's full. Std makes a
    // socket that doesn't wait, where it can't make such a pipe, and a write to either fails
    // the same way. Nothing reads this one, so it fills long before the message is written.
    let (unread, full) = UnixStream::pair().expect("make a pair of sockets");
    full.set_nonblocking(true).expect("a socket can be told not to wait");
    let status = Command::new(stella_binary())
        .args(["lint", "wide.stella"])
        .current_dir(&directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(OwnedFd::from(full))
        .status()
        .expect("run stella");
    assert_eq!(status.code(), Some(2_i32), "for a stderr that doesn't wait");
    drop(unread);
}

#[test]
fn a_closed_stderr() {
    // Rust opens `/dev/null` in place of a closed stderr before `main` runs, so a message to it is
    // written and lost, and the status is the lint's. Std can't close a child's stderr, so the
    // shell does.
    let directory = fresh_directory("closed-stderr");
    write(&directory, "hello.stella", GOOD);
    for (path, expected) in [("hello.stella", 0_i32), ("Hello.stella", 1_i32)] {
        // Only a shell can start a program with its stderr closed, which std can't do without
        // `unsafe`. The script is fixed, and the binary and the path are handed to it as
        // arguments, never written into it.
        let status = Command::new("/bin/sh")
            .args(["-c", "exec \"$0\" lint \"$1\" 2>&-"])
            .arg(stella_binary())
            .arg(path)
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
    // Each wrong way to call the command exits with 2, says what's wrong, and ends with the
    // usage lines.
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
        let expected = Run::exits(2, &format!("{sentence}{USAGE}"));
        assert_eq!(stella(&directory, arguments), expected, "for {arguments:?}");
    }
}
