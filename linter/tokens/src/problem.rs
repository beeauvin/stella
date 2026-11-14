// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This module holds what the tokens lint reports when it refuses a program.

use std::fmt;

use stella_lexer::TokenKind;
use stella_linter_text::LEVEL_SPACES;
use stella_source::Position;

const _: () = assert!(
    LEVEL_SPACES == 4,
    "`IndentationNotDepth`'s sentence names a level's four spaces, so a new level needs a new \
     sentence"
);

/// A problem is the one thing the tokens lint refused, and where it is.
///
/// The tokens lint only reads tokens and the spaces between them, so a problem is always at a
/// position, never with the file as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The kind says what's wrong.
    pub kind: Kind,
    /// The position says where it's wrong.
    pub position: Position,
}

/// A kind is one of the problems the tokens lint can find.
///
/// They're grouped by the step that finds them, in the order the steps run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    // The lines.
    /// A `{` isn't the last token on its line.
    BraceOpenNotLast,
    /// A `{` is the first token on its line, so nothing it could open comes before it. A `{`
    /// that's both first and not last is `BraceOpenNotLast`.
    BraceOpenStartsLine,
    /// A `}` isn't the only token on its line.
    BraceCloseNotAlone,

    // The braces.
    /// A `}` comes when no `{` is open, so there's nothing for it to close.
    BraceClosesNothing,
    /// A `{` is still open at the end of the file. It's the outermost one left open.
    BraceNeverClosed,

    // Within lines.
    /// A line isn't indented one level for each `{` still open on it. This is how many are.
    IndentationNotDepth(u32),
    /// Two tokens on a line need one space between them, and have none.
    SpaceMissing(Neighbors),
    /// Two tokens on a line have a space between them, and need none: the first is `(`, or the
    /// second is `(` or `)`.
    SpaceUnwanted(Neighbors),
    /// Two tokens on a line have more than one space between them, and need one.
    SpacesExtra(Neighbors),
}

/// Neighbors are two tokens on a line, one right after the other, by their kinds. Each is named,
/// so the two can't be given the wrong way round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Neighbors {
    /// The before is the kind of the token that comes first.
    pub before: TokenKind,
    /// The after is the kind of the token that comes right after it.
    pub after: TokenKind,
}

/// A kind displays as the sentence that says what's wrong. A message prints it after the file,
/// line and column.
impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sentence = match *self {
            Self::BraceOpenNotLast => "`{` must end its line.",
            Self::BraceOpenStartsLine => "`{` can't start a line.",
            Self::BraceCloseNotAlone => "`}` must be on a line of its own.",
            Self::BraceClosesNothing => "this `}` has no `{` to close.",
            Self::BraceNeverClosed => "this `{` is never closed.",
            Self::IndentationNotDepth(0) => {
                "this line can't be indented, since no `{` is still open on it."
            }
            Self::IndentationNotDepth(levels) => {
                // Counted wide, so any number of levels has its spaces.
                let spaces = u64::from(levels) * u64::from(LEVEL_SPACES);
                return write!(
                    f,
                    "this line must be indented {spaces} spaces, one level of four for each `{{` \
                     still open on it."
                );
            }
            Self::SpaceMissing(Neighbors { before, after }) => {
                let (before, after) = (named(before), named(after));
                return write!(f, "one space must go between {before} and {after}.");
            }
            Self::SpaceUnwanted(Neighbors { before, after }) => {
                let (before, after) = (named(before), named(after));
                return write!(f, "no space can go between {before} and {after}.");
            }
            Self::SpacesExtra(Neighbors { before, after }) => {
                let (before, after) = (named(before), named(after));
                return write!(f, "only one space can go between {before} and {after}.");
            }
        };
        f.write_str(sentence)
    }
}

/// Returns how a sentence names a token of `kind`: a keyword or a symbol by its own spelling, in
/// backticks, and any other kind in words.
const fn named(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Function => "`function`",
        TokenKind::Return => "`return`",
        TokenKind::Returns => "`returns`",
        TokenKind::Name => "a name",
        TokenKind::TypeName => "a type's name",
        TokenKind::Integer => "an integer",
        TokenKind::ParenthesisOpen => "`(`",
        TokenKind::ParenthesisClose => "`)`",
        TokenKind::BraceOpen => "`{`",
        TokenKind::BraceClose => "`}`",
        TokenKind::Comment => "a comment",
    }
}
