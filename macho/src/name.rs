// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A binary's name is what its code signature identifies it by.
//!
//! The name is written into the signature, and the binary's UUID is taken from it, so it's known
//! before a byte is written. It comes from the name of the source file, which the filesystem
//! bounds and the compiler doesn't make. So it's checked here, where it arrives, and what
//! refuses it is an error, not an assertion.

use std::fmt;

use stella_source::usize_from_u32;

/// This is the longest name a binary can have, in bytes.
///
/// A source file's name is at most 255 bytes, as macOS allows, and 7 of them are `.stella`. The
/// signature's size is worked out from this, so a longer name is refused rather than given
/// room.
pub const NAME_BYTES_MAX: u32 = 248;

/// This is `NAME_BYTES_MAX`, to measure a name in memory against.
const NAME_BYTES_MAX_USIZE: usize = usize_from_u32(NAME_BYTES_MAX);

const _: () = assert!(
    NAME_BYTES_MAX == 248,
    "`TooLong`'s sentence names the limit, so a new limit needs a new sentence"
);

/// A binary's name: at least one byte, at most `NAME_BYTES_MAX`, and all ASCII that leaves a
/// mark, so no space, other than `/`.
///
/// Only `BinaryName::new` makes one, so a name that's held is one a signature can identify a
/// binary by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinaryName<'name> {
    name: &'name str,
}

impl<'name> BinaryName<'name> {
    /// Returns `name` as a binary's name.
    ///
    /// # Errors
    ///
    /// Returns the problem with the name: it's empty, it's longer than `NAME_BYTES_MAX`, or it
    /// holds a byte that isn't ASCII that leaves a mark, such as a space, or a `/`.
    pub fn new(name: &'name str) -> Result<Self, BinaryNameProblem> {
        if name.is_empty() {
            return Err(BinaryNameProblem::Empty);
        }
        if name.len() > NAME_BYTES_MAX_USIZE {
            return Err(BinaryNameProblem::TooLong);
        }
        let is_plain = |byte: u8| byte.is_ascii_graphic() && byte != b'/';
        if !name.bytes().all(is_plain) {
            return Err(BinaryNameProblem::NotPlain);
        }
        Ok(Self { name })
    }

    /// Returns the name.
    #[must_use]
    pub const fn as_str(&self) -> &'name str {
        self.name
    }

    /// Returns how many bytes the name takes, from 1 to `NAME_BYTES_MAX`.
    ///
    /// # Panics
    ///
    /// Never, for a name `BinaryName::new` made, and nothing else makes one.
    #[must_use]
    pub fn size_bytes(&self) -> u32 {
        assert!(!self.name.is_empty(), "`BinaryName::new` refuses an empty name");
        assert!(self.name.len() <= NAME_BYTES_MAX_USIZE, "`BinaryName::new` refuses a long name");
        u32::try_from(self.name.len()).expect("a name of at most 248 bytes fits in a `u32`")
    }
}

/// A binary name problem is why some text can't be a binary's name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryNameProblem {
    /// The name has no bytes.
    Empty,
    /// The name is longer than `NAME_BYTES_MAX`.
    TooLong,
    /// The name holds a byte that isn't ASCII that leaves a mark, such as a space, or a `/`.
    NotPlain,
}

/// It displays as the sentence that says what's wrong.
impl fmt::Display for BinaryNameProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sentence = match *self {
            Self::Empty => "a binary's name can't be empty.",
            Self::TooLong => "a binary's name can be at most 248 bytes.",
            Self::NotPlain => "a binary's name can hold only ASCII, with no space and no `/`.",
        };
        f.write_str(sentence)
    }
}
