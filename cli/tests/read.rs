// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for reading a source. They're a crate of their own, so the compiler lets them reach
//! only what the crate makes public.
//!
//! `fill` is tried on a reader that follows a script, so every way a reader can answer is seen:
//! bytes a few at a time, the end, an interruption and a failure. `read` is tried on real files
//! under the directory cargo gives the tests to write in.

#![forbid(unsafe_code)]

use std::io::{self, Read};
use std::iter::repeat_n;
use std::path::PathBuf;

use stella_cli::{AccessFailure, READ_INTERRUPTIONS_MAX, ReadFailure, SourceBuffer, fill, read};
use stella_source::{SOURCE_BYTES_MAX, usize_from_u32};
use stella_test_support::{ModeGuard, Scratch, fifo, write};

/// One answer a scripted reader gives to a read.
#[derive(Clone, Copy)]
enum Step {
    /// These bytes, or as many of them as there's room for.
    Bytes(&'static [u8]),
    /// A signal interrupted the read.
    Interrupted,
    /// The read failed.
    Fails,
}

/// A reader that answers each read with the next step of a script, and then ends.
struct ScriptedReader {
    steps: Vec<Step>,
    next: usize,
}

impl ScriptedReader {
    /// Returns a reader that follows `steps`.
    fn new(steps: &[Step]) -> Self {
        Self { steps: steps.to_vec(), next: 0 }
    }
}

impl Read for ScriptedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Some(&step) = self.steps.get(self.next) else {
            return Ok(0);
        };
        self.next += 1;
        match step {
            Step::Bytes(bytes) => {
                let count = bytes.len().min(buf.len());
                buf[..count].copy_from_slice(&bytes[..count]);
                Ok(count)
            }
            Step::Interrupted => Err(io::Error::from(io::ErrorKind::Interrupted)),
            Step::Fails => Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        }
    }
}

/// Returns a new, empty directory for one test to work in.
fn fresh_directory(name: &str) -> PathBuf {
    Scratch::of("read").fresh(name)
}

#[test]
fn fill_reads_until_the_reader_ends() {
    // A reader may give its bytes a few at a time. `fill` keeps reading until it ends, and says
    // how many bytes that was. Only those bytes were written.
    let mut reader =
        ScriptedReader::new(&[Step::Bytes(b"ab"), Step::Bytes(b"c"), Step::Bytes(b"de")]);
    let mut buffer = [b'-'; 8];
    assert_eq!(fill(&mut reader, &mut buffer).expect("the reader ends"), 5);
    assert_eq!(&buffer, b"abcde---");
    // A reader with nothing to give fills nothing.
    assert_eq!(fill(&mut ScriptedReader::new(&[]), &mut buffer).expect("the reader ends"), 0);
}

#[test]
fn fill_stops_when_the_buffer_is_full() {
    // A reader with more than the buffer holds is read only as far as the buffer goes, and no
    // read is made once it's full.
    let mut reader =
        ScriptedReader::new(&[Step::Bytes(b"abc"), Step::Bytes(b"defgh"), Step::Fails]);
    let mut buffer = [b'-'; 5];
    assert_eq!(fill(&mut reader, &mut buffer).expect("the buffer fills"), 5);
    assert_eq!(&buffer, b"abcde");
    assert_eq!(reader.next, 2, "the failing read is never made");
}

#[test]
fn fill_waits_out_a_few_interruptions() {
    // An interrupted read is tried again, up to the most there may be, and nothing is lost.
    let mut steps = vec![Step::Bytes(b"ab")];
    steps.extend(repeat_n(Step::Interrupted, usize_from_u32(READ_INTERRUPTIONS_MAX)));
    steps.push(Step::Bytes(b"cd"));
    let mut buffer = [b'-'; 8];
    assert_eq!(fill(&mut ScriptedReader::new(&steps), &mut buffer).expect("the reader ends"), 4);
    assert_eq!(&buffer, b"abcd----");
}

#[test]
fn fill_gives_up_on_too_many_interruptions() {
    // One interruption more than there may be is an error, so a reader that only ever says it
    // was interrupted can't hold the command forever.
    let steps = vec![Step::Interrupted; usize_from_u32(READ_INTERRUPTIONS_MAX) + 1];
    let mut reader = ScriptedReader::new(&steps);
    let error = fill(&mut reader, &mut [0; 8]).expect_err("the reader interrupts too often");
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(reader.next, steps.len(), "every interruption was tried");
}

#[test]
fn fill_returns_the_readers_error() {
    // A read that fails stops the fill with the reader's own error.
    let mut reader = ScriptedReader::new(&[Step::Bytes(b"ab"), Step::Fails, Step::Bytes(b"cd")]);
    let error = fill(&mut reader, &mut [0; 8]).expect_err("the reader fails");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn read_returns_a_files_bytes() {
    // A file comes back whole, whatever its bytes are, and an empty file is no bytes.
    let directory = fresh_directory("bytes");
    let mut buffer = SourceBuffer::new();
    for (name, contents) in [
        ("hello.stella", &b"function main() returns Void {\n}\n"[..]),
        ("empty.stella", b""),
        ("not-utf8.stella", b"\xFF\x00\r"),
    ] {
        write(&directory, name, contents);
        let found = read(&directory.join(name), &mut buffer).expect("the file can be read");
        assert_eq!(found, contents, "for {name}");
    }
}

#[test]
fn read_stops_one_byte_past_the_largest_source() {
    // The largest source comes back whole. Anything larger comes back one byte longer than
    // that, however large it is, so it's seen to be too big without being read whole.
    let directory = fresh_directory("sizes");
    let largest = usize_from_u32(SOURCE_BYTES_MAX);
    let mut buffer = SourceBuffer::new();
    for (name, size, read_bytes) in [
        ("largest.stella", largest, largest),
        ("one-more.stella", largest + 1, largest + 1),
        ("huge.stella", largest * 4, largest + 1),
    ] {
        write(&directory, name, &vec![b'x'; size]);
        let found = read(&directory.join(name), &mut buffer).expect("the file can be read");
        assert_eq!(found.len(), read_bytes, "for {name}");
        assert!(found.iter().all(|&byte| byte == b'x'), "for {name}");
    }
}

#[test]
fn one_buffer_serves_every_file() {
    // The same buffer is lent to file after file. Each file's bytes are its own, however much
    // more the file before it left in the buffer.
    let directory = fresh_directory("reuse");
    write(&directory, "long.stella", &vec![b'x'; usize_from_u32(SOURCE_BYTES_MAX)]);
    write(&directory, "short.stella", b"// y\n");
    let mut buffer = SourceBuffer::new();
    for (name, length) in [
        ("long.stella", usize_from_u32(SOURCE_BYTES_MAX)),
        ("short.stella", 5),
        ("long.stella", usize_from_u32(SOURCE_BYTES_MAX)),
        ("short.stella", 5),
    ] {
        let found = read(&directory.join(name), &mut buffer).expect("the file can be read");
        assert_eq!(found.len(), length, "for {name}");
    }
    let short = read(&directory.join("short.stella"), &mut buffer).expect("the file can be read");
    assert_eq!(short, b"// y\n");
}

#[test]
fn what_cant_be_read_says_why() {
    // Each thing that isn't a readable file, with what `read` says of it and the sentence that
    // prints. A FIFO is answered at once, with nothing writing to it.
    let directory = fresh_directory("not-read");
    std::fs::create_dir(directory.join("folder.stella")).expect("the directory is ours to make");
    fifo(&directory.join("pipe.stella"));
    write(&directory, "locked.stella", b"// x\n");
    write(&directory, "file", b"");
    let _guard = ModeGuard::new(directory.join("locked.stella"), 0o000);
    let mut buffer = SourceBuffer::new();
    let mut failure_of = |name: &str| {
        read(&directory.join(name), &mut buffer).map(<[u8]>::len).expect_err("it can't be read")
    };
    let missing = failure_of("missing.stella");
    assert!(matches!(missing, ReadFailure::Access(AccessFailure::Missing)), "{missing:?}");
    assert_eq!(missing.to_string(), "this file doesn't exist.");
    for name in ["folder.stella", "pipe.stella"] {
        let not_a_file = failure_of(name);
        assert!(matches!(not_a_file, ReadFailure::NotAFile), "for {name}: {not_a_file:?}");
        assert_eq!(not_a_file.to_string(), "this isn't a file.", "for {name}");
    }
    let locked = failure_of("locked.stella");
    assert!(matches!(locked, ReadFailure::Access(AccessFailure::Unreadable(_))), "{locked:?}");
    assert_eq!(locked.to_string(), "this file can't be read: Permission denied (os error 13).");
    // A file where a directory should be can't be looked through.
    let through_a_file = failure_of("file/hello.stella");
    assert!(
        matches!(through_a_file, ReadFailure::Access(AccessFailure::Unreadable(_))),
        "{through_a_file:?}"
    );
    let sentence = "this file can't be read: Not a directory (os error 20).";
    assert_eq!(through_a_file.to_string(), sentence);
}
