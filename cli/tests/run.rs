// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for running the command without a process. `run` is given arguments, a directory
//! and the streams to write to, so what it says on each and how it ends can be seen directly,
//! and so can what it does when it can't write.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use stella_cli::{Status, Streams, run};
use stella_test_support::{Scratch, write};

/// This is the smallest program, which passes.
const GOOD: &[u8] = b"function main() returns Void {\n}\n";

/// This program is refused, because a line in it ends in a space.
const TRAILING_SPACE: &[u8] = b"function main() returns Void { \n}\n";

/// This is the message for `TRAILING_SPACE`, after its path.
const TRAILING_SPACE_MESSAGE: &str = ":1:31: a line can't end in a space.\n    \
     function main() returns Void { \n                                  ^\n";

/// This is the usage, which every misuse ends with.
const USAGE: &str = "usage: stella build <path>\n       stella lint [path]\n";

/// This is what a build says when it builds `hello` from `GOOD`.
const HELLO_REPORT: &str = "hello needs 0 bytes of running memory and 44 bytes of code, on top \
                            of what macOS needs to run any program.\n";

/// Returns a new, empty directory for one test to work in.
fn fresh_directory(name: &str) -> PathBuf {
    Scratch::of("run").fresh(name)
}

/// A writer that takes a few writes and then fails every one, and counts those it took.
struct ClosingWriter {
    writes_left: u32,
    writes_taken: u32,
}

impl Write for ClosingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.writes_left == 0 {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }
        self.writes_left -= 1;
        self.writes_taken += 1;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A writer that holds what it's given until it's flushed, as a buffered stream does. A flush
/// can be made to fail, as it does when the stream can't be written.
#[derive(Default)]
struct HoldingWriter {
    held: Vec<u8>,
    written: Vec<u8>,
    flushes_count: u32,
    flush_fails: bool,
}

impl Write for HoldingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.held.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.flush_fails {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }
        self.written.append(&mut self.held);
        self.flushes_count += 1;
        Ok(())
    }
}

/// A writer that fails every write with an error that has no words.
struct WordlessWriter;

impl Write for WordlessWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other(""))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Returns `arguments` as the command is given them.
fn given(arguments: &[&str]) -> Vec<OsString> {
    arguments.iter().map(OsString::from).collect()
}

/// Returns streams that write `stderr`, and lose what's written to stdout.
const fn streams_to<Stderr>(stderr: Stderr) -> Streams<io::Sink, Stderr> {
    Streams { stdout: io::sink(), stderr }
}

/// What one run of the command did: how it ended, and what it wrote to each stream.
#[derive(Debug, PartialEq, Eq)]
struct Ran {
    status: Status,
    stdout: String,
    stderr: String,
}

/// Returns a run that ended with `status`, wrote nothing to stdout, and wrote `stderr`.
fn says(status: Status, stderr: &str) -> Ran {
    Ran { status, stdout: String::new(), stderr: String::from(stderr) }
}

/// Runs the command with `arguments` under `root`, and returns what it did.
fn ran(arguments: &[&str], root: &Path) -> Ran {
    let mut streams = Streams { stdout: Vec::new(), stderr: Vec::new() };
    let status = run(&given(arguments), root, &mut streams).expect("writing to memory can't fail");
    let Streams { stdout, stderr } = streams;
    Ran {
        status,
        stdout: String::from_utf8(stdout).expect("everything the command writes is ASCII"),
        stderr: String::from_utf8(stderr).expect("everything the command writes is ASCII"),
    }
}

#[test]
fn one_program_ends_with_its_status_and_its_message() {
    // A program that passes says nothing. One that's refused says why, at its path as given, and
    // one that isn't there is a failure. Each ends with its own status.
    let directory = fresh_directory("one");
    write(&directory, "hello.stella", GOOD);
    write(&directory, "programs/bad.stella", TRAILING_SPACE);
    assert_eq!(ran(&["lint", "hello.stella"], &directory), says(Status::Passed, ""));
    let refused = format!("./programs/bad.stella{TRAILING_SPACE_MESSAGE}");
    let run = ran(&["lint", "./programs/bad.stella"], &directory);
    assert_eq!(run, says(Status::Refused, &refused));
    let missing = "gone.stella: this file doesn't exist.\n";
    assert_eq!(ran(&["lint", "gone.stella"], &directory), says(Status::Failed, missing));
}

#[test]
fn a_build_reports_on_stdout_and_says_the_rest_on_stderr() {
    // A program that's built says what it needs, on stdout, and nothing on stderr, and its
    // binary is in `.build/`. One that's refused says why on stderr, as a lint of it does, and
    // nothing on stdout, and nothing is made for it. One that isn't there says so on stderr,
    // and nothing on stdout.
    let directory = fresh_directory("build");
    write(&directory, "hello.stella", GOOD);
    write(&directory, "programs/bad.stella", TRAILING_SPACE);
    let built =
        Ran { status: Status::Passed, stdout: String::from(HELLO_REPORT), stderr: String::new() };
    assert_eq!(ran(&["build", "hello.stella"], &directory), built);
    assert!(directory.join(".build/hello").is_file(), "the binary is in `.build/`");
    let refused = format!("./programs/bad.stella{TRAILING_SPACE_MESSAGE}");
    let run = ran(&["build", "./programs/bad.stella"], &directory);
    assert_eq!(run, says(Status::Refused, &refused));
    assert_eq!(run, ran(&["lint", "./programs/bad.stella"], &directory));
    let missing = "gone.stella: this file doesn't exist.\n";
    assert_eq!(ran(&["build", "gone.stella"], &directory), says(Status::Failed, missing));
    assert!(!directory.join(".build/programs").exists(), "nothing is made for a refused program");
}

#[test]
fn a_report_that_cant_be_written_is_a_failure_that_says_so() {
    // A stdout that fails when it's written, and one that fails only when it's flushed. Either
    // way the binary is already in place, so the run says the report can't be written, with
    // the stream's own reason, and ends as a failure.
    let directory = fresh_directory("report");
    write(&directory, "hello.stella", GOOD);
    let said = "stella: the report can't be written: broken pipe.\n";
    let closing = ClosingWriter { writes_left: 0, writes_taken: 0 };
    let mut streams = Streams { stdout: closing, stderr: Vec::new() };
    let status = run(&given(&["build", "hello.stella"]), &directory, &mut streams);
    assert_eq!(status.expect("stderr takes the message"), Status::Failed);
    assert_eq!(String::from_utf8_lossy(&streams.stderr), said);
    assert!(directory.join(".build/hello").is_file(), "the binary is in place");
    let holding = HoldingWriter { flush_fails: true, ..HoldingWriter::default() };
    let mut streams = Streams { stdout: holding, stderr: Vec::new() };
    let status = run(&given(&["build", "hello.stella"]), &directory, &mut streams);
    assert_eq!(status.expect("stderr takes the message"), Status::Failed);
    assert_eq!(String::from_utf8_lossy(&streams.stderr), said);
    assert_eq!(streams.stdout.written, b"", "a report that wasn't flushed wasn't written");
    // A stream is whoever runs the command's own, so its error is too, and needn't have any
    // words. The message still ends as a sentence.
    let mut streams = Streams { stdout: WordlessWriter, stderr: Vec::new() };
    let status = run(&given(&["build", "hello.stella"]), &directory, &mut streams);
    assert_eq!(status.expect("stderr takes the message"), Status::Failed);
    let said = "stella: the report can't be written: no reason was given.\n";
    assert_eq!(String::from_utf8_lossy(&streams.stderr), said);
}

#[test]
fn a_report_is_flushed_before_the_run_ends() {
    // A stdout that holds what it's given until it's flushed. When the run returns, the whole
    // report is out and nothing is still held.
    let directory = fresh_directory("report-flushed");
    write(&directory, "hello.stella", GOOD);
    let mut streams = Streams { stdout: HoldingWriter::default(), stderr: Vec::new() };
    let status = run(&given(&["build", "hello.stella"]), &directory, &mut streams);
    assert_eq!(status.expect("writing to memory can't fail"), Status::Passed);
    assert_eq!(streams.stdout.held, b"");
    assert_eq!(String::from_utf8_lossy(&streams.stdout.written), HELLO_REPORT);
    assert_eq!(streams.stderr, b"", "a program that's built says nothing on stderr");
}

#[test]
fn every_program_ends_with_the_worst_status() {
    // With no path, every program is linted. Messages come in the order of their paths with a
    // blank line between them, and the run ends with the worst status any program gets.
    let directory = fresh_directory("every");
    write(&directory, "a.stella", GOOD);
    assert_eq!(ran(&["lint"], &directory), says(Status::Passed, ""));
    write(&directory, "b.stella", TRAILING_SPACE);
    write(&directory, "c.stella", GOOD);
    write(&directory, "d.stella", TRAILING_SPACE);
    let refused = format!("b.stella{TRAILING_SPACE_MESSAGE}\nd.stella{TRAILING_SPACE_MESSAGE}");
    assert_eq!(ran(&["lint"], &directory), says(Status::Refused, &refused));
}

#[test]
fn nothing_to_lint_is_a_failure() {
    // A directory with no source in it has nothing to lint, which the command says.
    let directory = fresh_directory("nothing");
    let said = "stella: there are no `.stella` files under this directory.\n";
    assert_eq!(ran(&["lint"], &directory), says(Status::Failed, said));
}

#[test]
fn a_misuse_says_whats_wrong_and_how_to_use_it() {
    // A misuse is a failure, and ends with the usage, in a directory that holds nothing.
    let directory = fresh_directory("misuse");
    let said = format!("stella: `bild` isn't a command.\n{USAGE}");
    assert_eq!(ran(&["bild"], &directory), says(Status::Failed, &said));
    let said = format!("stella: `build` takes one path.\n{USAGE}");
    assert_eq!(ran(&["build"], &directory), says(Status::Failed, &said));
}

#[test]
fn a_message_that_cant_be_written_is_an_error() {
    // When stderr stops taking writes, `run` returns its error, whichever command was running,
    // so the binary can exit with a failure. Linting every file writes each message as it's
    // made, so the messages before the one that failed were already written.
    let directory = fresh_directory("unwritable");
    write(&directory, "a.stella", TRAILING_SPACE);
    write(&directory, "b.stella", TRAILING_SPACE);
    write(&directory, "c.stella", TRAILING_SPACE);
    for (arguments, writes_left, writes_taken) in [
        (&["lint", "a.stella"][..], 0, 0),
        (&["build", "a.stella"], 0, 0),
        (&["bild"], 0, 0),
        // The first message, then the blank line before the second, which is where it fails.
        (&["lint"], 1, 1),
        // Two messages and the blank line between them, then the blank line before the third.
        (&["lint"], 3, 3),
    ] {
        let mut streams = streams_to(ClosingWriter { writes_left, writes_taken: 0 });
        let error = run(&given(arguments), &directory, &mut streams).expect_err("the write fails");
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe, "for {arguments:?}");
        assert_eq!(streams.stderr.writes_taken, writes_taken, "for {arguments:?}");
    }
    // When neither stream can be written, a build's report is lost and so is the message that
    // says so, which is the same error.
    write(&directory, "good.stella", GOOD);
    let closed = || ClosingWriter { writes_left: 0, writes_taken: 0 };
    let mut streams = Streams { stdout: closed(), stderr: closed() };
    let error = run(&given(&["build", "good.stella"]), &directory, &mut streams);
    assert_eq!(error.expect_err("the write fails").kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn everything_said_is_flushed_before_the_run_ends() {
    // The binary's stderr is buffered, so what's written isn't out until it's flushed. Whatever
    // the command, nothing is still held when `run` returns. Linting every file flushes after
    // each message, so each is seen as soon as it's made.
    let directory = fresh_directory("flushed");
    write(&directory, "a.stella", TRAILING_SPACE);
    write(&directory, "b.stella", TRAILING_SPACE);
    let one = format!("a.stella{TRAILING_SPACE_MESSAGE}");
    let both = format!("a.stella{TRAILING_SPACE_MESSAGE}\nb.stella{TRAILING_SPACE_MESSAGE}");
    let misuse = format!("stella: `bild` isn't a command.\n{USAGE}");
    for (arguments, said, flushes_least) in [
        (&["lint", "a.stella"][..], one.clone(), 1),
        (&["build", "a.stella"], one, 1),
        (&["bild"], misuse, 1),
        (&["lint"], both, 2),
    ] {
        let mut streams = streams_to(HoldingWriter::default());
        let status = run(&given(arguments), &directory, &mut streams);
        let status = status.expect("the writer takes it all");
        assert_ne!(status, Status::Passed, "for {arguments:?}");
        assert_eq!(streams.stderr.held, b"", "for {arguments:?}");
        assert_eq!(String::from_utf8_lossy(&streams.stderr.written), said, "for {arguments:?}");
        assert!(streams.stderr.flushes_count >= flushes_least, "for {arguments:?}");
    }
}

#[test]
fn a_flush_that_fails_is_an_error() {
    // A buffered stderr says it can't be written only when it's flushed. That's still an error
    // from `run`, whichever command was running, and even when there was nothing to say.
    let directory = fresh_directory("unflushable");
    write(&directory, "a.stella", TRAILING_SPACE);
    write(&directory, "good.stella", GOOD);
    for arguments in [
        &["lint", "a.stella"][..],
        &["bild"],
        &["lint"],
        &["lint", "good.stella"],
        &["build", "a.stella"],
        &["build", "good.stella"],
    ] {
        let holding = HoldingWriter { flush_fails: true, ..HoldingWriter::default() };
        let mut streams = streams_to(holding);
        let error = run(&given(arguments), &directory, &mut streams).expect_err("the flush fails");
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe, "for {arguments:?}");
    }
}
