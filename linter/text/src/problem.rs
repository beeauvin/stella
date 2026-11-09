// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This module holds what the text lint reports when it refuses a program.

use std::fmt;

/// A problem is one thing the text lint refused, and where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The kind says what's wrong.
    pub kind: Kind,
    /// The place says where it's wrong.
    pub place: Place,
}

/// A place is either the file as a whole or one character in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// The problem is with the file as a whole, such as its name or its size.
    File,
    /// The problem is at one character.
    At(Position),
}

/// A position is a character's line and column, both counted from 1. A column counts code points,
/// not bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    /// The line is counted from 1.
    pub line: u32,
    /// The column is counted in code points from 1.
    pub column: u32,
}

/// A kind is one of the problems the text lint can find. They're grouped by the step that finds
/// them, in the order the steps run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    // The path, checked before the file is read. The checks run in this order: absolute, parent,
    // empty parts, `.` parts, directories' names, and then the file's name.
    /// The path starts at the root, so there's no way to mirror it under `.build/`.
    PathAbsolute,
    /// A part of the path is `..`, so it could leave the directory the compiler runs in.
    PathParent,
    /// A part of the path is empty, as `//` or a trailing `/` makes.
    PathEmptyPart,
    /// A part of the path is `.`, other than one leading `./`.
    PathDotPart,
    /// A directory's name isn't lowercase letters and digits with single hyphens between words.
    DirectoryNotKebab,
    /// The file's name doesn't end in `.stella`. Case matters, so `hello.Stella` doesn't.
    NameNotStella,
    /// The file's name is `.stella` and nothing more.
    NameEmpty,
    /// The file's name before `.stella` isn't lowercase letters and digits with single hyphens
    /// between words.
    NameNotKebab,

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

/// A kind displays as the sentence that says what's wrong. A message prints it after the file,
/// and after the position when there is one.
impl fmt::Display for Kind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = formatter;
        todo!("the sentence for {self:?}")
    }
}
