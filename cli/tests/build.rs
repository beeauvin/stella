// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! End-to-end tests for `stella build`. Each one runs the real binary in a directory of its own
//! and pins exactly what it does: its exit status, its stdout and its stderr, and what's in
//! `.build/` afterwards. Every binary it builds is checked by macOS's own `codesign`, and run.

#![forbid(unsafe_code)]

use std::env;
use std::ffi::OsString;
use std::fs;
use std::iter;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use stella_cli::PARTS_MAX;
use stella_test_support::{ModeGuard, Run, Scratch, fifo, write};

/// This is the smallest program, which passes.
const GOOD: &[u8] = b"function main() returns Void {\n}\n";

/// This is what `stella build` says when it builds `hello` from `GOOD`.
const HELLO_REPORT: &str = "hello needs 0 bytes of running memory and 44 bytes of code, on top \
                            of what macOS needs to run any program.\n";

/// This is how many bytes the binary of `GOOD` takes.
const HELLO_BYTES: u64 = 16_640;

/// Every byte of a binary before its signature is signed. This one is in the binary's first
/// page, which macOS reads to run it, so a binary with this byte changed doesn't run.
const SIGNED_BYTE_INDEX: usize = 1_024;

/// A build exits with this when the program is refused.
const REFUSED: i32 = 1;

/// A build exits with this when something outside the program went wrong.
const FAILED: i32 = 2;

/// These are the lines every misuse ends with.
const USAGE: &str = "usage: stella build <path>\n       stella lint [path]\n";

/// This is the sentence for a source file's name that isn't kebab before `.stella`.
const NAME_NOT_KEBAB: &str = "a source file's name, before `.stella`, must be lowercase letters \
                              and digits joined by single hyphens, like `hello-world`.";

/// This is the sentence for a path that goes through a symlink.
const PATH_SYMLINK: &str =
    "the build path can't be established from a path that goes through a symlink.";

/// This is the sentence for something on the way to a binary that isn't a directory.
const NOT_A_DIRECTORY: &str = "this isn't a directory, so the build can't go through it.";

/// This is the sentence for something at a binary's path that isn't a regular file.
const NOT_A_FILE: &str = "this isn't a regular file, so the build left it alone.";

/// This is the sentence for a directory where every name the build's part could have is taken.
const TOO_MANY_PARTS: &str = "this holds too many `.part` files, so the build can't make its own.";

/// This is the name of a part no build of these tests makes: one with the number of a process
/// there never is, as another build would leave.
const ANOTHER_BUILDS_PART: &str = ".4294967295.0.part";

/// Returns the path of the `stella` binary Cargo built for these tests.
fn stella_binary() -> OsString {
    env::var_os("CARGO_BIN_EXE_stella").expect("Cargo tells a test where the binary it built is")
}

/// Runs `stella` in `directory` with `arguments`, and nothing on stdin.
fn stella(directory: &Path, arguments: &[&str]) -> Run {
    Run::of(Command::new(stella_binary()).args(arguments).current_dir(directory))
}

/// Runs `stella build hello.stella` in `directory`, from a shell that has run `setup` first, so
/// the build runs under whatever `setup` sets.
fn stella_after(directory: &Path, setup: &str) -> Run {
    let script = format!("{setup} && exec \"$0\" build hello.stella");
    let mut shell = Command::new("/bin/sh");
    Run::of(shell.args(["-c", &script]).arg(stella_binary()).current_dir(directory))
}

/// Returns the run of a build that built its binary: exit 0, `report` on stdout, and nothing
/// on stderr.
fn built(report: &str) -> Run {
    Run { status: 0, stdout: String::from(report), stderr: String::new() }
}

/// Checks that `run` is a build that failed at its part in `directory`, a path from the
/// directory it ran in: it exits with 2, writes nothing to stdout, and says `sentence` at
/// `<directory>/.<number>.0.part`. The number is all digits, as a process's is, and the count
/// is 0, since nothing was where the part goes.
fn failed_at_its_part(run: &Run, directory: &str, sentence: &str) {
    assert_eq!((run.status, run.stdout.as_str()), (FAILED, ""), "{run:?}");
    let said = run.stderr.strip_prefix(&format!("{directory}/."));
    let said = said.and_then(|rest| rest.strip_suffix(&format!(".0.part: {sentence}\n")));
    let number = said.unwrap_or_else(|| unreachable!("{run:?} should fail at its part"));
    let is_number = !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit());
    assert!(is_number, "a part is named for a process's number, not {number:?}");
}

/// Returns the names of the parts in `directory`: what starts with a dot, as no binary does.
fn parts_in(directory: &Path) -> Vec<String> {
    names_in(directory).into_iter().filter(|name| name.starts_with('.')).collect()
}

/// Returns what a build says of a program named `name` built from `GOOD`.
fn report_of(name: &str) -> String {
    HELLO_REPORT.replacen("hello", name, 1)
}

/// Returns a new, empty directory for one test, or one row of a table, to work in.
fn fresh_directory(name: &str) -> PathBuf {
    Scratch::of("build").fresh(name)
}

/// Returns the names of what's in `directory`, in order.
fn names_in(directory: &Path) -> Vec<String> {
    let entries = fs::read_dir(directory).expect("a directory the build made can be listed");
    let mut names: Vec<String> = entries
        .map(|entry| entry.expect("each entry can be read").file_name())
        .map(|name| name.into_string().expect("the tests' names are ASCII"))
        .collect();
    names.sort();
    names
}

/// Checks that the binary at `path` is a regular file anyone can run, that `codesign` accepts
/// its signature strictly, and that it runs: it exits with 0 and says nothing.
fn runs(path: &Path) {
    let metadata = fs::symlink_metadata(path).expect("the binary should be there");
    assert!(metadata.is_file(), "{} should be a regular file", path.display());
    let mode = metadata.permissions().mode() & 0o7777;
    assert_eq!(mode, 0o755, "{} should be for anyone to run", path.display());
    let mut codesign = Command::new("/usr/bin/codesign");
    let verified = Run::of(codesign.args(["--verify", "--strict"]).arg(path));
    assert_eq!(verified, Run::exits(0, ""), "codesign should accept {}", path.display());
    assert_eq!(Run::of(&mut Command::new(path)), Run::exits(0, ""), "running {}", path.display());
}

#[test]
fn builds_a_program() {
    // A source two directories down. The build makes `.build/` and the directories in it, puts
    // the binary at the source's path without `.stella` and nothing else beside it, says what
    // the program needs, and the binary runs. A source at the top builds at the top of
    // `.build/`, with the same bytes, and `./` before a path changes nothing.
    let directory = fresh_directory("builds");
    write(&directory, "programs/first/hello.stella", GOOD);
    let source = "programs/first/hello.stella";
    assert_eq!(stella(&directory, &["build", source]), built(HELLO_REPORT));
    let binary = directory.join(".build/programs/first/hello");
    runs(&binary);
    assert_eq!(fs::metadata(&binary).expect("the binary is there").len(), HELLO_BYTES);
    assert_eq!(names_in(&directory.join(".build")), ["programs"]);
    assert_eq!(names_in(&directory.join(".build/programs")), ["first"]);
    assert_eq!(names_in(&directory.join(".build/programs/first")), ["hello"]);
    write(&directory, "hello.stella", GOOD);
    assert_eq!(stella(&directory, &["build", "./hello.stella"]), built(HELLO_REPORT));
    runs(&directory.join(".build/hello"));
    let first = fs::read(&binary).expect("the binary can be read");
    assert_eq!(fs::read(directory.join(".build/hello")).expect("the binary can be read"), first);
    assert_eq!(names_in(&directory.join(".build")), ["hello", "programs"]);
    // Linting every file never goes into `.build/`, so what's built there is never linted.
    assert_eq!(stella(&directory, &["lint"]), Run::exits(0, ""));
}

#[test]
fn the_report_names_the_program() {
    // A program is named for its file, without `.stella`, wherever the file is and however its
    // path was typed. The report starts with that name.
    let directory = fresh_directory("named");
    write(&directory, "tools/hello-world-2.stella", GOOD);
    let source = "./tools/hello-world-2.stella";
    assert_eq!(stella(&directory, &["build", source]), built(&report_of("hello-world-2")));
    runs(&directory.join(".build/tools/hello-world-2"));
}

#[test]
fn the_binary_is_signed_in_its_sources_name() {
    // A binary's signature names the program, and the name is its source file's, without
    // `.stella`, whatever directory the file is in. `codesign` shows the name each binary was
    // signed in. The name is part of what's signed, so two programs with the same code and
    // different names are different bytes.
    let directory = fresh_directory("signed-name");
    write(&directory, "hello.stella", GOOD);
    write(&directory, "programs/second.stella", GOOD);
    for (source, binary, name) in [
        ("hello.stella", ".build/hello", "hello"),
        ("programs/second.stella", ".build/programs/second", "second"),
    ] {
        assert_eq!(stella(&directory, &["build", source]), built(&report_of(name)));
        let binary = directory.join(binary);
        runs(&binary);
        let mut codesign = Command::new("/usr/bin/codesign");
        let shown = Run::of(codesign.args(["--display", "--verbose"]).arg(&binary));
        assert_eq!((shown.status, shown.stdout.as_str()), (0_i32, ""), "codesign shows {name}");
        let identifier = format!("Identifier={name}");
        let is_named = shown.stderr.lines().any(|line| line == identifier);
        assert!(is_named, "{name} should be signed in its own name: {}", shown.stderr);
    }
    let hello = fs::read(directory.join(".build/hello")).expect("the binary can be read");
    let second = fs::read(directory.join(".build/programs/second")).expect("it can be read");
    assert_eq!(hello.len(), second.len(), "the two hold the same code");
    assert!(hello != second, "a binary's name is in its bytes");
}

#[test]
fn the_mode_ignores_the_umask() {
    // Under a umask that takes away everything from everyone else, the binary is still for
    // anyone to run, since the build sets its mode after making the file.
    let directory = fresh_directory("umask");
    write(&directory, "hello.stella", GOOD);
    assert_eq!(stella_after(&directory, "umask 077"), built(HELLO_REPORT));
    runs(&directory.join(".build/hello"));
}

#[test]
fn rebuilding() {
    // Building again writes a new file, renamed over the old one, with the same bytes. That
    // holds even after the binary there was changed by hand and failed to run, which can leave
    // macOS remembering the old file as one it won't run.
    let directory = fresh_directory("rebuilding");
    write(&directory, "hello.stella", GOOD);
    let binary = directory.join(".build/hello");
    assert_eq!(stella(&directory, &["build", "hello.stella"]), built(HELLO_REPORT));
    let first = fs::read(&binary).expect("the binary can be read");
    let inode_first = fs::metadata(&binary).expect("the binary is there").ino();
    assert_eq!(stella(&directory, &["build", "hello.stella"]), built(HELLO_REPORT));
    assert_eq!(fs::read(&binary).expect("the binary can be read"), first);
    let inode_second = fs::metadata(&binary).expect("the binary is there").ino();
    assert_ne!(inode_second, inode_first, "a rebuild should put a new file in place");
    assert_eq!(names_in(&directory.join(".build")), ["hello"]);
    runs(&binary);
    let mut broken = first.clone();
    broken[SIGNED_BYTE_INDEX] ^= 0xFF;
    fs::write(&binary, &broken).expect("the binary is the tests' own to break");
    let failed = Command::new(&binary).output().expect("a broken binary is still there to run");
    assert!(!failed.status.success(), "a binary that no longer matches its signature fails");
    assert_eq!(stella(&directory, &["build", "hello.stella"]), built(HELLO_REPORT));
    assert_eq!(fs::read(&binary).expect("the binary can be read"), first);
    runs(&binary);
}

#[test]
fn builds_at_once() {
    // Eight builds of one program at once, twelve times over. Each writes a part of its own,
    // so none takes another's: every one builds, and says so. Whenever the binary is read
    // while they run, it's whole. And once they're done, nothing is left beside it.
    let directory = fresh_directory("at-once");
    write(&directory, "hello.stella", GOOD);
    assert_eq!(stella(&directory, &["build", "hello.stella"]), built(HELLO_REPORT));
    let binary = directory.join(".build/hello");
    let whole = fs::read(&binary).expect("the binary can be read");
    for round in 0_u32..12 {
        let start = || {
            let mut build = Command::new(stella_binary());
            build.args(["build", "hello.stella"]).current_dir(&directory);
            build.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
            build.spawn().expect("the binary Cargo built is there to run")
        };
        let builds: Vec<Child> = iter::repeat_with(start).take(8).collect();
        for _ in 0_u32..200 {
            let read = fs::read(&binary).expect("a binary is always at its path");
            assert!(read == whole, "the binary should be whole whenever it's read");
        }
        for build in builds {
            let output = build.wait_with_output().expect("a build that started can be waited for");
            assert_eq!(Run::from_output(output), built(HELLO_REPORT), "in round {round}");
        }
        assert_eq!(names_in(&directory.join(".build")), ["hello"], "in round {round}");
    }
    runs(&binary);
}

#[test]
fn a_refused_program_touches_nothing() {
    // One program for each check there is, from the text lint to what the program means. A
    // build refuses each with exactly `stella lint`'s status and message, writes nothing to
    // stdout, and makes nothing. Where an old binary is, it stays exactly as it was.
    let refused: [(&str, &[u8]); 6] = [
        ("text", b"function main() returns Void { // hi\n}\n"),
        ("lexer", b"function main() returns Void { ;\n}\n"),
        ("tokens", b"function main() returns Void {}\n"),
        ("grammar", b"function main() Void {\n}\n"),
        ("types", b"function main() returns Number {\n}\n"),
        ("program", b"function start() returns Void {\n}\n"),
    ];
    for (check, source) in refused {
        let directory = fresh_directory(&format!("refused-{check}"));
        write(&directory, "hello.stella", source);
        let linted = stella(&directory, &["lint", "hello.stella"]);
        assert_eq!(linted.status, REFUSED, "the {check} check should refuse its program");
        assert_eq!(linted.stdout, "", "a refusal says nothing on stdout, for {check}");
        assert!(linted.stderr.starts_with("hello.stella:"), "for {check}: {}", linted.stderr);
        assert_eq!(stella(&directory, &["build", "hello.stella"]), linted, "for {check}");
        // The path is shown as it was typed, by a build as by a lint.
        let typed = stella(&directory, &["lint", "./hello.stella"]);
        assert_eq!(typed.stderr, format!("./{}", linted.stderr), "for {check}");
        assert_eq!(stella(&directory, &["build", "./hello.stella"]), typed, "for {check}");
        assert_eq!(names_in(&directory), ["hello.stella"], "nothing is made for {check}");
        write(&directory, "hello.stella", GOOD);
        assert_eq!(stella(&directory, &["build", "hello.stella"]), built(HELLO_REPORT));
        runs(&directory.join(".build/hello"));
        let old = fs::read(directory.join(".build/hello")).expect("the binary can be read");
        write(&directory, "hello.stella", source);
        assert_eq!(stella(&directory, &["build", "hello.stella"]), linted, "for {check}");
        let kept = fs::read(directory.join(".build/hello")).expect("the binary can be read");
        assert_eq!(kept, old, "the old binary stays for {check}");
        assert_eq!(names_in(&directory.join(".build")), ["hello"], "for {check}");
    }
}

#[test]
fn sources_that_cant_be_built() {
    // A source that can't be read, or whose path is refused, stops the build before anything
    // is made, with the status and message `stella lint` gives it.
    let directory = fresh_directory("cant-be-built");
    write(&directory, "real.stella", GOOD);
    fs::create_dir(directory.join("directory.stella")).expect("a directory can be made");
    symlink("real.stella", directory.join("link.stella")).expect("a symlink can be made");
    write(&directory, "big.stella", &vec![b'a'; 16_385]);
    write(&directory, "Programs/hello.stella", GOOD);
    let too_big = "this file is over 16,384 bytes, the most a source file can hold.";
    let before = names_in(&directory);
    for (path, status, sentence) in [
        ("missing.stella", FAILED, "this file doesn't exist."),
        ("directory.stella", FAILED, "this isn't a file."),
        ("link.stella", REFUSED, PATH_SYMLINK),
        ("big.stella", REFUSED, too_big),
        ("hello.txt", REFUSED, "a source file's name must end in `.stella`."),
    ] {
        let expected = Run::exits(status, &format!("{path}: {sentence}\n"));
        assert_eq!(stella(&directory, &["build", path]), expected, "for {path}");
        assert_eq!(stella(&directory, &["lint", path]), expected, "for {path}");
        assert_eq!(names_in(&directory), before, "nothing is made for {path}");
    }
    // On a volume that ignores case, as macOS's do unless they're made otherwise,
    // `hello.stella` reaches `Hello.stella`, which is refused for its name as it's stored. On
    // one that doesn't, `hello.stella` isn't there, and there's nothing here to check.
    let directory = fresh_directory("cant-be-built-case");
    write(&directory, "Hello.stella", GOOD);
    if directory.join("hello.stella").exists() {
        let expected = Run::exits(REFUSED, &format!("Hello.stella: {NAME_NOT_KEBAB}\n"));
        assert_eq!(stella(&directory, &["build", "hello.stella"]), expected);
        assert_eq!(stella(&directory, &["lint", "hello.stella"]), expected);
        assert_eq!(names_in(&directory), ["Hello.stella"], "nothing is made");
    }
}

#[test]
fn a_binary_that_cant_be_written() {
    // When a directory the build writes in can't be written, the build stops with 2, and says
    // where and why, in the operating system's own words.
    let sentence = "the binary can't be written: Permission denied (os error 13).";
    let directory = fresh_directory("cant-be-written");
    write(&directory, "hello.stella", GOOD);
    fs::create_dir(directory.join(".build")).expect("a directory can be made");
    let guard = ModeGuard::new(directory.join(".build"), 0o555);
    failed_at_its_part(&stella(&directory, &["build", "hello.stella"]), ".build", sentence);
    drop(guard);
    assert!(names_in(&directory.join(".build")).is_empty(), "nothing was built");
    // The same where `.build/` itself can't be made, or a directory in it.
    let directory = fresh_directory("cant-be-made");
    write(&directory, "programs/hello.stella", GOOD);
    let guard = ModeGuard::new(directory.clone(), 0o555);
    let expected = Run::exits(FAILED, &format!(".build: {sentence}\n"));
    assert_eq!(stella(&directory, &["build", "programs/hello.stella"]), expected);
    drop(guard);
    fs::create_dir(directory.join(".build")).expect("a directory can be made");
    let guard = ModeGuard::new(directory.join(".build"), 0o555);
    let expected = Run::exits(FAILED, &format!(".build/programs: {sentence}\n"));
    assert_eq!(stella(&directory, &["build", "programs/hello.stella"]), expected);
    drop(guard);
    assert!(names_in(&directory.join(".build")).is_empty(), "nothing was built");
}

#[test]
fn a_binary_that_cant_be_written_whole() {
    // Under a limit on how big a file can be, smaller than a binary, the write fails part of
    // the way through. The build stops with 2, removes the part it had written, and leaves the
    // old binary exactly as it was. The shell ignores the signal that comes with the limit, so
    // the write fails as any other does.
    let directory = fresh_directory("cant-be-written-whole");
    write(&directory, "hello.stella", GOOD);
    assert_eq!(stella(&directory, &["build", "hello.stella"]), built(HELLO_REPORT));
    let old = fs::read(directory.join(".build/hello")).expect("the binary can be read");
    let limited = stella_after(&directory, "trap '' XFSZ && ulimit -f 8");
    let sentence = "the binary can't be written: File too large (os error 27).";
    failed_at_its_part(&limited, ".build", sentence);
    assert_eq!(names_in(&directory.join(".build")), ["hello"], "the part is removed");
    assert_eq!(fs::read(directory.join(".build/hello")).expect("the binary can be read"), old);
    runs(&directory.join(".build/hello"));
}

/// A file that can't be changed, renamed over or removed until this is dropped.
struct Immutable {
    path: PathBuf,
}

impl Immutable {
    /// Makes the file at `path` one that can't be changed, with `chflags uchg`.
    fn new(path: PathBuf) -> Self {
        let set = Run::of(Command::new("/usr/bin/chflags").arg("uchg").arg(&path));
        assert_eq!(set, Run::exits(0, ""), "the tests' own file can be made immutable");
        Self { path }
    }
}

impl Drop for Immutable {
    fn drop(&mut self) {
        // A test may have failed already, so this mustn't panic too. A file left immutable is
        // found by the next run, when it can't remove the directory.
        _ = Command::new("/usr/bin/chflags").arg("nouchg").arg(&self.path).status();
    }
}

#[test]
fn a_binary_that_cant_be_replaced() {
    // A binary macOS won't let anyone change can't be renamed over. The part is written whole,
    // and it's the rename that fails: the build stops with 2, says so at the binary's path,
    // removes its part, and leaves the old binary exactly as it was.
    let directory = fresh_directory("cant-be-replaced");
    write(&directory, "hello.stella", GOOD);
    assert_eq!(stella(&directory, &["build", "hello.stella"]), built(HELLO_REPORT));
    let old = fs::read(directory.join(".build/hello")).expect("the binary can be read");
    let immutable = Immutable::new(directory.join(".build/hello"));
    let sentence = "the binary can't be written: Operation not permitted (os error 1).";
    let expected = Run::exits(FAILED, &format!(".build/hello: {sentence}\n"));
    assert_eq!(stella(&directory, &["build", "hello.stella"]), expected);
    assert_eq!(names_in(&directory.join(".build")), ["hello"], "the part is removed");
    assert_eq!(fs::read(directory.join(".build/hello")).expect("the binary can be read"), old);
    drop(immutable);
    assert_eq!(stella(&directory, &["build", "hello.stella"]), built(HELLO_REPORT));
    runs(&directory.join(".build/hello"));
}

#[test]
fn a_report_that_cant_be_written() {
    // The report is written last, once the binary is in place. With a stdout that's open only
    // for reading, the build says the report can't be written, on stderr, and exits with 2.
    // The binary is there, and runs.
    let directory = fresh_directory("report");
    write(&directory, "hello.stella", GOOD);
    let read_only = fs::File::open("/dev/null").expect("`/dev/null` is there to open");
    let mut build = Command::new(stella_binary());
    build.args(["build", "hello.stella"]).current_dir(&directory).stdout(read_only);
    let said = "stella: the report can't be written: Bad file descriptor (os error 9).\n";
    assert_eq!(Run::of(&mut build), Run::exits(FAILED, said));
    runs(&directory.join(".build/hello"));
    // A stdout that's closed takes the report and loses it, as it does anything written to it.
    let closed = stella_after(&directory, "exec >&-");
    assert_eq!(closed, Run::exits(0, ""));
    // A program that's refused has no report, so a stdout that can't be written changes nothing.
    write(&directory, "hello.stella", b"function main() returns Number {\n}\n");
    let linted = stella(&directory, &["lint", "hello.stella"]);
    let read_only = fs::File::open("/dev/null").expect("`/dev/null` is there to open");
    let mut build = Command::new(stella_binary());
    build.args(["build", "hello.stella"]).current_dir(&directory).stdout(read_only);
    assert_eq!(Run::of(&mut build), linted);
}

#[test]
fn something_else_at_the_binarys_path() {
    // A directory, a symlink to a file and a FIFO, each where the binary goes. The build stops
    // with 2 and leaves each alone, and what the symlink leads to keeps its bytes. Nothing is
    // written beside it.
    for kind in ["directory", "symlink", "fifo"] {
        let directory = fresh_directory(&format!("in-the-way-{kind}"));
        write(&directory, "hello.stella", GOOD);
        fs::create_dir(directory.join(".build")).expect("a directory can be made");
        let binary = directory.join(".build/hello");
        match kind {
            "directory" => fs::create_dir(&binary).expect("a directory can be made"),
            "symlink" => {
                write(&directory, "target", b"kept");
                symlink("../target", &binary).expect("a symlink can be made");
            }
            _ => fifo(&binary),
        }
        let before = fs::symlink_metadata(&binary).expect("it's there").file_type();
        let expected = Run::exits(FAILED, &format!(".build/hello: {NOT_A_FILE}\n"));
        assert_eq!(stella(&directory, &["build", "hello.stella"]), expected, "for a {kind}");
        let after = fs::symlink_metadata(&binary).expect("it's still there").file_type();
        assert_eq!(after, before, "the {kind} is what it was");
        if kind == "symlink" {
            let target = fs::read(directory.join("target")).expect("the target can be read");
            assert_eq!(target, b"kept", "what the symlink leads to isn't written");
        }
        assert_eq!(names_in(&directory.join(".build")), ["hello"], "for a {kind}");
    }
}

#[test]
fn symlinks_on_the_way() {
    // A symlink to a directory, as `.build` or as a directory in it. The build never goes
    // through one, so nothing is written where it leads. The same for a file on the way.
    for in_the_way in [".build", ".build/programs"] {
        for kind in ["symlink", "file"] {
            let name = format!("on-the-way-{kind}-{}", in_the_way.replace(['.', '/'], "-"));
            let directory = fresh_directory(&name);
            write(&directory, "programs/hello.stella", GOOD);
            fs::create_dir(directory.join("elsewhere")).expect("a directory can be made");
            if in_the_way != ".build" {
                fs::create_dir(directory.join(".build")).expect("a directory can be made");
            }
            if kind == "symlink" {
                let link = directory.join(in_the_way);
                symlink(directory.join("elsewhere"), link).expect("a symlink can be made");
            } else {
                write(&directory, in_the_way, b"kept");
            }
            let expected = Run::exits(FAILED, &format!("{in_the_way}: {NOT_A_DIRECTORY}\n"));
            let run = stella(&directory, &["build", "programs/hello.stella"]);
            assert_eq!(run, expected, "for a {kind} at {in_the_way}");
            assert!(names_in(&directory.join("elsewhere")).is_empty(), "nothing went through");
            if kind == "file" {
                let kept = fs::read(directory.join(in_the_way)).expect("the file can be read");
                assert_eq!(kept, b"kept", "the file at {in_the_way} isn't written");
            }
        }
    }
}

#[test]
fn what_is_where_a_part_would_go() {
    // A file, a symlink to a file, a directory and a FIFO, each under the first name the build
    // would give its part. The shell makes it with its own number, and then becomes the build,
    // which keeps that number. The build didn't make it, so it's left exactly as it was, and
    // what the symlink leads to keeps its bytes. The build makes its part at the next count,
    // so the binary is built, and runs. Another build's part beside them is left alone too.
    let directory = fresh_directory("part-in-the-way");
    write(&directory, "hello.stella", GOOD);
    write(&directory, "target", b"kept");
    write(&directory, &format!(".build/{ANOTHER_BUILDS_PART}"), b"another build's");
    for (kind, setup) in [
        ("file", "printf 'left behind' > .build/.$$.0.part"),
        ("symlink", "ln -s ../target .build/.$$.0.part"),
        ("directory", "mkdir .build/.$$.0.part"),
        ("fifo", "mkfifo .build/.$$.0.part"),
    ] {
        assert_eq!(stella_after(&directory, setup), built(HELLO_REPORT), "for a {kind}");
        runs(&directory.join(".build/hello"));
        let mut parts = parts_in(&directory.join(".build"));
        parts.retain(|name| name != ANOTHER_BUILDS_PART);
        let [part] = parts.as_slice() else {
            unreachable!("the {kind} should still be there, and no part beside it: {parts:?}");
        };
        assert!(part.ends_with(".0.part"), "the {kind} is under the first count: {part}");
        let part = directory.join(".build").join(part);
        let metadata = fs::symlink_metadata(&part).expect("it's still there");
        let (is_file, is_dir) = (metadata.is_file(), metadata.is_dir());
        let kinds = (is_file, metadata.is_symlink(), is_dir);
        assert_eq!(kinds, (kind == "file", kind == "symlink", kind == "directory"));
        if is_file {
            let left = fs::read(&part).expect("the file can be read");
            assert_eq!(left, b"left behind", "a file that's there isn't written");
        }
        let target = fs::read(directory.join("target")).expect("the target can be read");
        assert_eq!(target, b"kept", "what the symlink leads to isn't written");
        let removed = if is_dir { fs::remove_dir(&part) } else { fs::remove_file(&part) };
        removed.expect("what the test put there is its own to remove");
    }
    let others = directory.join(".build").join(ANOTHER_BUILDS_PART);
    let others = fs::read(others).expect("another build's part is still there");
    assert_eq!(others, b"another build's", "another build's part isn't written");
}

#[test]
fn too_many_parts() {
    // Every name the build's part could have is taken, as by builds that were killed. The shell
    // makes them with its own number, and then becomes the build, which keeps that number. The
    // build stops with 2 and says so at `.build`. Each part keeps its bytes, and the binary
    // that was there stays as it was.
    let directory = fresh_directory("too-many-parts");
    write(&directory, "hello.stella", GOOD);
    assert_eq!(stella(&directory, &["build", "hello.stella"]), built(HELLO_REPORT));
    let built_bytes = fs::read(directory.join(".build/hello")).expect("the binary can be read");
    let setup = format!(
        "count=0; while [ $count -lt {PARTS_MAX} ]; do printf 'left behind' > \
         .build/.$$.$count.part || exit 9; count=$((count + 1)); done"
    );
    let stopped = stella_after(&directory, &setup);
    assert_eq!(stopped, Run::exits(FAILED, &format!(".build: {TOO_MANY_PARTS}\n")));
    let parts = parts_in(&directory.join(".build"));
    assert_eq!(u32::try_from(parts.len()), Ok(PARTS_MAX), "no part is made or removed");
    for part in parts {
        let left = fs::read(directory.join(".build").join(&part)).expect("it can be read");
        assert_eq!(left, b"left behind", "{part} isn't written");
    }
    let kept = fs::read(directory.join(".build/hello")).expect("the binary can be read");
    assert_eq!(kept, built_bytes, "the binary stays");
    runs(&directory.join(".build/hello"));
}

#[test]
fn the_longest_name() {
    // The longest name a file can have on macOS is 255 bytes, so the longest a source can have
    // is 248 before `.stella`. It builds, and its binary is shorter, so it had room. A name one
    // byte longer can't be a file at all.
    let directory = fresh_directory("longest");
    let name = "a".repeat(248);
    let source = format!("{name}.stella");
    assert_eq!(source.len(), 255, "the source's name is as long as a name can be");
    write(&directory, &source, GOOD);
    assert_eq!(stella(&directory, &["build", &source]), built(&report_of(&name)));
    runs(&directory.join(".build").join(&name));
    assert_eq!(names_in(&directory.join(".build")), [name]);
    let longer = directory.join(format!("a{source}"));
    assert!(fs::write(longer, GOOD).is_err(), "a name of 256 bytes can't be made");
}

#[test]
fn a_binary_and_a_directory_in_one_place() {
    // `programs/hello.stella` builds to `.build/programs/hello`, and a source under
    // `programs/hello/` needs a directory there. Whichever is built second stops, and what the
    // first built is left as it was.
    let directory = fresh_directory("one-place-binary-first");
    write(&directory, "programs/hello.stella", GOOD);
    write(&directory, "programs/hello/world.stella", GOOD);
    assert_eq!(stella(&directory, &["build", "programs/hello.stella"]), built(HELLO_REPORT));
    let binary = directory.join(".build/programs/hello");
    let first = fs::read(&binary).expect("the binary can be read");
    let expected = Run::exits(FAILED, &format!(".build/programs/hello: {NOT_A_DIRECTORY}\n"));
    assert_eq!(stella(&directory, &["build", "programs/hello/world.stella"]), expected);
    assert_eq!(fs::read(&binary).expect("the binary can be read"), first);
    assert_eq!(names_in(&directory.join(".build/programs")), ["hello"]);
    runs(&binary);

    let directory = fresh_directory("one-place-directory-first");
    write(&directory, "programs/hello.stella", GOOD);
    write(&directory, "programs/hello/world.stella", GOOD);
    let source = "programs/hello/world.stella";
    assert_eq!(stella(&directory, &["build", source]), built(&report_of("world")));
    let expected = Run::exits(FAILED, &format!(".build/programs/hello: {NOT_A_FILE}\n"));
    assert_eq!(stella(&directory, &["build", "programs/hello.stella"]), expected);
    assert!(directory.join(".build/programs/hello").is_dir(), "the directory is as it was");
    assert_eq!(names_in(&directory.join(".build/programs")), ["hello"]);
    runs(&directory.join(".build/programs/hello/world"));
}

#[test]
fn misuse() {
    // Each wrong way to call `build` exits with 2, says what's wrong, ends with the usage
    // lines, and makes nothing.
    let directory = fresh_directory("misuse");
    write(&directory, "hello.stella", GOOD);
    for (arguments, sentence) in [
        (&["build"][..], "stella: `build` takes one path.\n"),
        (&["build", "hello.stella", "hello.stella"], "stella: `build` takes one path.\n"),
        (&["build", ""], "stella: the path is empty.\n"),
        (&["build", "--release"], "stella: `--release` isn't an option; there are none.\n"),
        (&["build", "hello.stella", "-o"], "stella: `-o` isn't an option; there are none.\n"),
        (&["build", "-"], "stella: `-` isn't an option; there are none.\n"),
    ] {
        let expected = Run::exits(FAILED, &format!("{sentence}{USAGE}"));
        assert_eq!(stella(&directory, arguments), expected, "for {arguments:?}");
    }
    assert_eq!(names_in(&directory), ["hello.stella"], "nothing is made");
}
