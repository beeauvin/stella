// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This module holds what the text lint reports when it refuses a program.

use std::fmt;

use stella_source::{Position, SOURCE_BYTES_MAX};

use crate::text::LEVEL_SPACES;

// `FileTooBig`'s sentence names the limit, so a new limit needs a new sentence.
const _: () = assert!(SOURCE_BYTES_MAX == 16_384);

// `Tab`'s and `IndentationNotFour`'s sentences name a level's four spaces, so a new level needs new
// sentences.
const _: () = assert!(LEVEL_SPACES == 4);

/// A problem is one thing the text lint refused, and where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The kind says what's wrong.
    pub kind: Kind,
    /// The place says where it's wrong.
    pub place: Place,
}

impl Problem {
    /// Returns a problem with the file as a whole.
    pub(crate) fn whole_file(kind: Kind) -> Problem {
        assert!(kind.is_whole_file());
        Problem { kind, place: Place::File }
    }

    /// Returns a problem at `line` and `column`.
    pub(crate) fn at(kind: Kind, line: u32, column: u32) -> Problem {
        assert!(!kind.is_whole_file());
        assert!(line >= 1);
        assert!(column >= 1);
        // The largest source has at most `SOURCE_BYTES_MAX` lines, and a problem can be just past
        // the end of a line that long.
        assert!(line <= SOURCE_BYTES_MAX);
        assert!(column <= SOURCE_BYTES_MAX + 1);
        Problem { kind, place: Place::At(Position::new(line, column)) }
    }
}

/// A place is either the file as a whole or one character in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// The problem is with the file as a whole, such as its name or its size.
    File,
    /// The problem is at one position: a character, or just past the end of the last line.
    At(Position),
}

/// A kind is one of the problems the text lint can find. They're grouped by the step that finds
/// them, in the order the steps run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    // The size.
    /// The file has no bytes at all.
    FileEmpty,
    /// The file is larger than `SOURCE_BYTES_MAX`.
    FileTooBig,

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

impl Kind {
    /// Says whether this kind is about the file as a whole, as the size's are. Every other kind
    /// is at a position.
    fn is_whole_file(self) -> bool {
        use Kind::*;
        match self {
            FileEmpty | FileTooBig => true,
            NotUtf8 | ByteOrderMark | CarriageReturn | Tab | ControlCharacter(_)
            | BidiControl(_) | LineSeparator(_) | NoFinalNewline | BlankLineAtEnd
            | TrailingSpace | IndentationNotFour | CommentSharesLine | CommentNeedsSpace
            | NotAscii(_) => false,
        }
    }
}

/// A kind displays as the sentence that says what's wrong. A message prints it after the file,
/// and after the position when there is one.
impl fmt::Display for Kind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Kind::*;
        // Most sentences are fixed. The four kinds that hold a character name it by its code point.
        let sentence = match *self {
            FileEmpty => "this file is empty.",
            FileTooBig => "this file is over 16,384 bytes, the most a source file can hold.",
            NotUtf8 => "this isn't valid UTF-8.",
            ByteOrderMark => "a byte-order mark (U+FEFF) can't appear in a source file.",
            CarriageReturn => "`\\r` can't appear in a source file; lines end with `\\n` alone.",
            Tab => "a tab can't appear in a source file; indent with four spaces.",
            ControlCharacter(character) => {
                let code_point = CodePoint(character);
                return write!(
                    formatter,
                    "the control character {code_point} can't appear in a source file."
                );
            }
            BidiControl(character) => {
                let code_point = CodePoint(character);
                return write!(
                    formatter,
                    "the bidirectional control character {code_point} can't appear in a source \
                     file."
                );
            }
            LineSeparator(character) => {
                let code_point = CodePoint(character);
                return write!(
                    formatter,
                    "{code_point} can't appear in a source file; lines end with `\\n` alone."
                );
            }
            NoFinalNewline => "the file must end with a newline.",
            BlankLineAtEnd => "a file can't end with a blank line.",
            TrailingSpace => "a line can't end in a space.",
            IndentationNotFour => "indentation must be a multiple of four spaces.",
            CommentSharesLine => "a comment can't share a line with code.",
            CommentNeedsSpace => "`//` must be followed by a space or the end of the line.",
            NotAscii(character) => {
                let code_point = CodePoint(character);
                return write!(
                    formatter,
                    "outside comments, only ASCII can appear, and {code_point} isn't ASCII."
                );
            }
        };
        formatter.write_str(sentence)
    }
}

/// A code point displays as `U+` and at least four uppercase hex digits, like `U+00E9`.
struct CodePoint(char);

impl fmt::Display for CodePoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "U+{:04X}", u32::from(self.0))
    }
}
