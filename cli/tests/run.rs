// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for running the command without a process. `run` is given arguments, a directory
//! and somewhere to write, so what it says and how it ends can be seen directly, and so can what
//! it does when it can't write.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use stella_cli::{Status, run};
use stella_test_support::{Scratch, write};

/// This is the smallest program, which passes.
const GOOD: &[u8] = b"function main() returns Void {\n}\n";

/// This program is refused, because a line in it ends in a space.
const TRAILING_SPACE: &[u8] = b"function main() returns Void { \n}\n";

/// This is the message for `TRAILING_SPACE`, after its path.
const TRAILING_SPACE_MESSAGE: &str = ":1:31: a line can't end in a space.\n    \
     function main() returns Void { \n                                  ^\n";

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

/// A writer that holds what it's given until it's flushed, as a buffered stderr does. A flush can
/// be made to fail, as it does when stderr can't be written.
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

/// Runs the command with `arguments` under `root`, and returns its status and what it wrote.
fn ran(arguments: &[&str], root: &Path) -> (Status, String) {
    let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
    let mut written = Vec::new();
    let status = run(&arguments, root, &mut written).expect("writing to memory can't fail");
    (status, String::from_utf8(written).expect("everything the command writes is ASCII"))
}

#[test]
fn one_program_ends_with_its_status_and_its_message() {
    // A program that passes says nothing. One that's refused says why, at its path as given, and
    // one that isn't there is a failure. Each ends with its own status.
    let directory = fresh_directory("one");
    write(&directory, "hello.stella", GOOD);
    write(&directory, "programs/bad.stella", TRAILING_SPACE);
    assert_eq!(ran(&["lint", "hello.stella"], &directory), (Status::Passed, String::new()));
    let refused = format!("./programs/bad.stella{TRAILING_SPACE_MESSAGE}");
    assert_eq!(ran(&["lint", "./programs/bad.stella"], &directory), (Status::Refused, refused));
    let missing = String::from("gone.stella: this file doesn't exist.\n");
    assert_eq!(ran(&["lint", "gone.stella"], &directory), (Status::Failed, missing));
}

#[test]
fn every_program_ends_with_the_worst_status() {
    // With no path, every program is linted. Messages come in the order of their paths with a
    // blank line between them, and the run ends with the worst status any program gets.
    let directory = fresh_directory("every");
    write(&directory, "a.stella", GOOD);
    assert_eq!(ran(&["lint"], &directory), (Status::Passed, String::new()));
    write(&directory, "b.stella", TRAILING_SPACE);
    write(&directory, "c.stella", GOOD);
    write(&directory, "d.stella", TRAILING_SPACE);
    let refused = format!("b.stella{TRAILING_SPACE_MESSAGE}\nd.stella{TRAILING_SPACE_MESSAGE}");
    assert_eq!(ran(&["lint"], &directory), (Status::Refused, refused));
}

#[test]
fn nothing_to_lint_is_a_failure() {
    // A directory with no source in it has nothing to lint, which the command says.
    let directory = fresh_directory("nothing");
    let said = String::from("stella: there are no `.stella` files under this directory.\n");
    assert_eq!(ran(&["lint"], &directory), (Status::Failed, said));
}

#[test]
fn a_misuse_says_whats_wrong_and_how_to_use_it() {
    // A misuse is a failure, and ends with the usage line, whatever the directory holds.
    let directory = fresh_directory("misuse");
    let said = String::from("stella: `bild` isn't a command.\nusage: stella lint [path]\n");
    assert_eq!(ran(&["bild"], &directory), (Status::Failed, said));
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
        (&["bild"], 0, 0),
        // The first message, then the blank line before the second, which is where it fails.
        (&["lint"], 1, 1),
        // Two messages and the blank line between them, then the blank line before the third.
        (&["lint"], 3, 3),
    ] {
        let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
        let mut stderr = ClosingWriter { writes_left, writes_taken: 0 };
        let error = run(&arguments, &directory, &mut stderr).expect_err("the write fails");
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe, "for {arguments:?}");
        assert_eq!(stderr.writes_taken, writes_taken, "for {arguments:?}");
    }
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
    let misuse = String::from("stella: `bild` isn't a command.\nusage: stella lint [path]\n");
    for (arguments, said, flushes_least) in
        [(&["lint", "a.stella"][..], one, 1), (&["bild"], misuse, 1), (&["lint"], both, 2)]
    {
        let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
        let mut stderr = HoldingWriter::default();
        let status = run(&arguments, &directory, &mut stderr).expect("the writer takes it all");
        assert_ne!(status, Status::Passed, "for {arguments:?}");
        assert_eq!(stderr.held, b"", "for {arguments:?}");
        assert_eq!(String::from_utf8_lossy(&stderr.written), said, "for {arguments:?}");
        assert!(stderr.flushes_count >= flushes_least, "for {arguments:?}");
    }
}

#[test]
fn a_flush_that_fails_is_an_error() {
    // A buffered stderr says it can't be written only when it's flushed. That's still an error
    // from `run`, whichever command was running, and even when there was nothing to say.
    let directory = fresh_directory("unflushable");
    write(&directory, "a.stella", TRAILING_SPACE);
    write(&directory, "good.stella", GOOD);
    for arguments in [&["lint", "a.stella"][..], &["bild"], &["lint"], &["lint", "good.stella"]] {
        let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
        let mut stderr = HoldingWriter { flush_fails: true, ..HoldingWriter::default() };
        let error = run(&arguments, &directory, &mut stderr).expect_err("the flush fails");
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe, "for {arguments:?}");
    }
}
