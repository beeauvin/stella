// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This module holds what the text lint reports when it refuses a program.

use std::fmt;

use stella_source::{Position, SOURCE_BYTES_MAX};

use crate::text::LEVEL_SPACES;

const _: () = assert!(
    SOURCE_BYTES_MAX == 16_384,
    "`FileTooBig`'s sentence names the limit, so a new limit needs a new sentence"
);

const _: () = assert!(
    LEVEL_SPACES == 4,
    "`Tab`'s and `IndentationNotFour`'s sentences name a level's four spaces, so a new level \
     needs new sentences"
);

/// A problem is the one thing the text lint refused about a source.
///
/// It's either with the file as a whole, which is its size, or at one position in it. The size
/// is checked first, so a file of the wrong size has no positions to speak of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The file has no bytes at all.
    FileEmpty,
    /// The file is larger than `SOURCE_BYTES_MAX`.
    FileTooBig,
    /// Something at one position is refused.
    At {
        /// The kind says what's wrong.
        kind: Kind,
        /// The position says where it's wrong: a character, or just past the end of a line.
        position: Position,
    },
}

impl Problem {
    /// Returns a problem of `kind` at `position`.
    pub(crate) const fn at(kind: Kind, position: Position) -> Self {
        Self::At { kind, position }
    }

    /// Returns where the problem is, or `None` when it's with the file as a whole.
    #[must_use]
    pub const fn position(&self) -> Option<Position> {
        match *self {
            Self::FileEmpty | Self::FileTooBig => None,
            Self::At { position, .. } => Some(position),
        }
    }
}

/// A problem displays as the sentence that says what's wrong. A message prints it after the file,
/// and after the position when there is one.
impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::FileEmpty => f.write_str("this file is empty."),
            Self::FileTooBig => {
                f.write_str("this file is over 16,384 bytes, the most a source file can hold.")
            }
            Self::At { kind, .. } => kind.fmt(f),
        }
    }
}

/// A kind is one of the problems the text lint can find at a position. They're grouped by the
/// step that finds them, in the order the steps run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    // The characters, in one pass, where the earliest problem wins.
    /// The bytes here don't decode as UTF-8.
    NotUtf8,
    /// U+FEFF appears, which is refused anywhere in the file.
    ByteOrderMark,
    /// `\r` appears, which is refused anywhere, because only `\n` ends a line.
    CarriageReturn,
    /// A tab appears, which is refused anywhere.
    Tab,
    /// A control character other than `\n`, `\r` or a tab appears. These are U+0000 to U+001F
    /// and U+007F to U+009F.
    ControlCharacter(char),
    /// One of Unicode's twelve `Bidi_Control` characters appears.
    BidiControl(char),
    /// U+2028 or U+2029 appears. Some editors draw either one as a line break.
    LineSeparator(char),

    // The lines.
    /// The last line doesn't end in `\n`.
    NoFinalNewline,
    /// The file ends with an empty line.
    BlankLineAtEnd,
    /// A line ends in a space, or holds nothing but spaces. A line of only spaces isn't blank.
    TrailingSpace,
    /// A line's indentation isn't a multiple of four spaces.
    IndentationNotFour,

    // The comments.
    /// A line of code holds `//`.
    CommentSharesLine,
    /// A comment's `//` is followed by something other than a space or the end of the line.
    CommentNeedsSpace,

    // ASCII.
    /// A character outside a comment isn't ASCII.
    NotAscii(char),
}

/// A kind displays as the sentence that says what's wrong.
impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Most sentences are fixed. The four kinds that hold a character name it by its code point.
        let sentence = match *self {
            Self::NotUtf8 => "this isn't valid UTF-8.",
            Self::ByteOrderMark => "a byte-order mark (U+FEFF) can't appear in a source file.",
            Self::CarriageReturn => {
                "`\\r` can't appear in a source file; lines end with `\\n` alone."
            }
            Self::Tab => "a tab can't appear in a source file; indent with four spaces.",
            Self::ControlCharacter(character) => {
                let code_point = CodePoint(character);
                return write!(
                    f,
                    "the control character {code_point} can't appear in a source file."
                );
            }
            Self::BidiControl(character) => {
                let code_point = CodePoint(character);
                return write!(
                    f,
                    "the bidirectional control character {code_point} can't appear in a source \
                     file."
                );
            }
            Self::LineSeparator(character) => {
                let code_point = CodePoint(character);
                return write!(
                    f,
                    "{code_point} can't appear in a source file; lines end with `\\n` alone."
                );
            }
            Self::NoFinalNewline => "the file must end with a newline.",
            Self::BlankLineAtEnd => "a file can't end with a blank line.",
            Self::TrailingSpace => "a line can't end in a space.",
            Self::IndentationNotFour => "indentation must be a multiple of four spaces.",
            Self::CommentSharesLine => "a comment can't share a line with code.",
            Self::CommentNeedsSpace => "`//` must be followed by a space or the end of the line.",
            Self::NotAscii(character) => {
                let code_point = CodePoint(character);
                return write!(
                    f,
                    "outside comments, only ASCII can appear, and {code_point} isn't ASCII."
                );
            }
        };
        f.write_str(sentence)
    }
}

/// A code point displays as `U+` and at least four uppercase hex digits, like `U+00E9`.
struct CodePoint(char);

impl fmt::Display for CodePoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U+{:04X}", u32::from(self.0))
    }
}
