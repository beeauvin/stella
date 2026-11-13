// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Reading a source file, as much of it as a source can be.
//!
//! A file is read into room for one byte more than the largest source, so a file that's too big
//! is seen to be, without being read whole. What comes back is the file's own bytes and nothing
//! else, and it belongs to whoever asked.

use std::fmt;
use std::io::{self, Read};
use std::path::Path;

use stella_source::{SOURCE_BYTES_MAX, usize_from_u32};

use crate::filesystem::{AccessFailure, look_through, open};

/// This is the most bytes of a file that are read: one more than the largest source.
const READ_BYTES_MAX: usize = usize_from_u32(SOURCE_BYTES_MAX) + 1;

/// This is how many times a read that a signal interrupted is tried again, over one file.
///
/// Interruptions are rare and each is harmless, so a few are waited out. More than that is
/// reported, so a reader that only ever says it was interrupted can't hold the command forever.
pub const READ_INTERRUPTIONS_MAX: u32 = 8;

/// A read failure is why a file couldn't be read.
#[derive(Debug)]
pub enum ReadFailure {
    /// The file couldn't be reached: nothing is there, or it can't be looked at, opened or read.
    Access(AccessFailure),
    /// What's at the path isn't a regular file: it's a directory, say, or a FIFO.
    NotAFile,
}

/// It displays as the sentence that says what's wrong.
impl fmt::Display for ReadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Access(failure) => failure.fmt(f),
            Self::NotAFile => f.write_str("this isn't a file."),
        }
    }
}

/// Reads the file at `path`, up to `SOURCE_BYTES_MAX` bytes and one more, and returns what was
/// read.
///
/// What's returned is one byte longer than the largest source when the file is too big, so the
/// text lint can refuse it for its size.
///
/// # Errors
///
/// Returns why the file couldn't be read: nothing is there, it isn't a regular file, or it
/// can't be looked at, opened or read.
///
/// # Panics
///
/// Never, whatever the file holds. The assertion checks `fill`'s own work.
///
/// If there isn't the memory to read it into, Rust ends the process, which isn't a panic.
pub fn read(path: &Path) -> Result<Vec<u8>, ReadFailure> {
    // KS-CLOSE-USE: the file is checked before it's opened, and not after, since opening
    // something that isn't a file, like a FIFO, can wait forever. Something swapped in between
    // the two could still do that, or be a symlink by the time it's opened. Opening without
    // waiting or following takes flags std doesn't name, whose numbers differ from one system to
    // the next, and even then a directory on the way could be swapped. Only someone who can
    // already write where the program is could do either, so the gap is left open rather than
    // closed halfway.
    let metadata = look_through(path).map_err(ReadFailure::Access)?;
    if !metadata.is_file() {
        return Err(ReadFailure::NotAFile);
    }
    let mut file = open(path).map_err(ReadFailure::Access)?;
    let mut bytes = vec![0; READ_BYTES_MAX];
    let length = fill(&mut file, &mut bytes)
        .map_err(|error| ReadFailure::Access(AccessFailure::from_reading(error)))?;
    assert!(length <= READ_BYTES_MAX, "a fill writes no more than it's given room for");
    // Only what the read wrote is returned, never the zeros the room was made with.
    bytes.truncate(length);
    Ok(bytes)
}

/// Reads from `reader` into `buffer` until the buffer is full or the reader ends, and returns how
/// many bytes that was.
///
/// Only that many were written, so only that many are the reader's.
///
/// # Errors
///
/// Returns the reader's error when a read fails, or when a signal interrupts reads more than
/// `READ_INTERRUPTIONS_MAX` times.
///
/// # Panics
///
/// Panics if the reader says it read more than it was given room for, which `Read` promises it
/// never does.
pub fn fill(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut length: usize = 0;
    let mut interruptions_count: u32 = 0;
    // Every read adds bytes, ends the reader, fails, or is one of a few interruptions, so this
    // stops.
    while length < buffer.len() {
        match reader.read(&mut buffer[length..]) {
            Ok(0) => break,
            Ok(count) => {
                let room = buffer.len() - length;
                assert!(count <= room, "a reader reads no more than it's given room for");
                length += count;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                interruptions_count += 1;
                if interruptions_count > READ_INTERRUPTIONS_MAX {
                    return Err(error);
                }
            }
            Err(error) => return Err(error),
        }
    }
    assert!(length <= buffer.len(), "no more was read than the buffer holds");
    Ok(length)
}
