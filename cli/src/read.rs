// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Reading a source file into a buffer made once.
//!
//! A source is read into one `SourceBuffer`, lent to each file in turn, so checking every program
//! in a tree takes the same room as checking one. The buffer has room for one byte more than the
//! largest source, so a file that's too big is seen to be, without being read whole.

use std::fmt;
use std::io::{self, Read};
use std::path::Path;

use stella_source::{SOURCE_BYTES_MAX, usize_from_u32};

use crate::filesystem::{AccessFailure, look_through, open};

/// This is how many bytes a source buffer holds: one more than the largest source.
const BUFFER_BYTES: usize = usize_from_u32(SOURCE_BYTES_MAX) + 1;

/// This is how many times a read that a signal interrupted is tried again, over one file.
///
/// Interruptions are rare and each is harmless, so a few are waited out. More than that is
/// reported, so a reader that only ever says it was interrupted can't hold the command forever.
pub const READ_INTERRUPTIONS_MAX: u32 = 8;

/// A source buffer is where `read` puts a file's bytes. It's made once, and it never grows.
#[derive(Debug)]
pub struct SourceBuffer {
    bytes: Box<[u8]>,
}

impl SourceBuffer {
    /// Returns a buffer with room for one byte more than the largest source.
    ///
    /// # Panics
    ///
    /// Panics if there isn't the memory for it, as any allocation does.
    #[must_use]
    pub fn new() -> Self {
        let bytes = vec![0; BUFFER_BYTES].into_boxed_slice();
        assert_eq!(bytes.len(), BUFFER_BYTES, "the buffer holds one byte more than a source");
        Self { bytes }
    }
}

impl Default for SourceBuffer {
    fn default() -> Self {
        Self::new()
    }
}

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

/// Reads the file at `path` into `buffer`, as much of it as fits, and returns what was read.
///
/// Whatever the buffer held before is replaced. What's returned is one byte longer than the
/// largest source when the file is too big.
///
/// # Errors
///
/// Returns why the file couldn't be read: nothing is there, it isn't a regular file, or it
/// can't be looked at, opened or read.
///
/// # Panics
///
/// Never, whatever the file holds. The assertion checks `fill`'s own work.
pub fn read<'buffer>(
    path: &Path,
    buffer: &'buffer mut SourceBuffer,
) -> Result<&'buffer [u8], ReadFailure> {
    // TS-CLOSE-USE: the file is checked before it's opened, and not after, since opening
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
    let length = fill(&mut file, &mut buffer.bytes)
        .map_err(|error| ReadFailure::Access(AccessFailure::from_reading(error)))?;
    assert!(length <= BUFFER_BYTES, "a fill writes no more than the buffer holds");
    // Only what this read wrote is returned, never what an earlier file left further on.
    Ok(&buffer.bytes[..length])
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
