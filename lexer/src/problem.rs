// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This module holds what the lexer reports when it refuses a program.

use std::fmt;

use stella_source::Position;

/// A problem is the one thing the lexer refused, and where it is.
///
/// The lexer only reads characters, so a problem is always at a position, never with the file as
/// a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The kind says what's wrong.
    pub kind: Kind,
    /// The position says where it's wrong.
    pub position: Position,
}

/// A kind is one of the problems the lexer can find.
///
/// A name's kinds are at the first character that breaks its shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A character in code can't start a token, and the token before it, if any, has ended. The
    /// text lint lets only printable ASCII into code, and a space separates tokens, so this
    /// character is always printable ASCII other than a space.
    Unexpected(char),
    /// A name that starts with a lowercase letter isn't snake case: lowercase letters and digits,
    /// with single underscores between them. A capital is refused, so camel case is too, and so
    /// are two underscores in a row and an underscore at the end.
    NameNotSnakeCase,
    /// A name that starts with a capital isn't Pascal case: words that each start with one
    /// capital, then lowercase letters and digits. An underscore is refused, and so are two
    /// capitals in a row.
    TypeNameNotPascalCase,
    /// An integer starts with `0` and goes on, as in `03`. Each value has one spelling, so only
    /// `0` starts with `0`. It's at the `0`.
    IntegerStartsWithZero,
    /// A letter or `_` comes right after an integer, as in `3x`. An integer is digits only, and
    /// it's at the letter or `_`.
    IntegerRunsIntoWord,
}

/// A kind displays as the sentence that says what's wrong. A message prints it after the file,
/// line and column.
impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            // A backtick can't sit between backticks, so it's written the way Markdown writes one.
            Self::Unexpected('`') => f.write_str("`` ` `` can't start a token."),
            Self::Unexpected(character) => {
                write!(f, "`{character}` can't start a token.")
            }
            Self::NameNotSnakeCase => f.write_str(
                "a name must be lowercase letters and digits joined by single underscores, like \
                 `my_function`.",
            ),
            Self::TypeNameNotPascalCase => f.write_str(
                "a type's name must be words that each start with one capital, then lowercase \
                 letters and digits, like `Void` or `MessageQueue`.",
            ),
            Self::IntegerStartsWithZero => {
                f.write_str("an integer can't start with `0`, unless it's `0`.")
            }
            Self::IntegerRunsIntoWord => {
                f.write_str("an integer is digits only, so a letter or `_` can't follow one.")
            }
        }
    }
}
