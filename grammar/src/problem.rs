// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This module holds what the grammar reports when it refuses a program.

use std::fmt;

use stella_source::Position;

/// A problem is the one thing the grammar refused, and where it is.
///
/// The grammar only reads tokens, so a problem is always on a line, never with the file as a
/// whole: at a token, or just past a line's last token when the line ends before its rule does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The kind says what's wrong.
    pub kind: Kind,
    /// The position is the first character of the first token at fault, or the column just past
    /// a line's last token, when the line ends before its rule does.
    pub position: Position,
}

/// A kind is one of the problems the grammar can find.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A line of code outside every block doesn't start with `function`. It's at the line's first
    /// token.
    NotAFunction,
    /// A line of code in a function's body doesn't start with `return`, which for now is the only
    /// line of code a body holds. It's at the line's first token, whatever follows it.
    NotAReturn,
    /// A line of code comes after its body's `return`, where nothing could run. It's at the
    /// line's first token.
    AfterReturn,
    /// A `return` line goes on after its value. It's at the first token after the value.
    AfterReturnValue,
    /// A line has some other token where this part goes, and it's at that token. Or the line
    /// ends where this part goes, and it's just past the line's last token.
    Expected(Part),
}

/// A part is one piece of a line's rule, after its keyword: of a function's first line, in the
/// order they go, or of a `return` line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The function's name, a name that starts with a lowercase letter.
    Name,
    /// The `(` that starts the parameters.
    ParenthesisOpen,
    /// The `)` that ends the parameters, which for now are empty.
    ParenthesisClose,
    /// The keyword `returns`.
    Returns,
    /// The function's return type, a type's name.
    ReturnType,
    /// The `{` that opens the function's body.
    BraceOpen,
    /// The value a `return` line gives back, which for now is an integer.
    ReturnValue,
}

/// A kind displays as the sentence that says what's wrong. A message prints it after the file,
/// line and column.
impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sentence = match *self {
            Self::NotAFunction => "outside a block, a line of code must start with `function`.",
            Self::NotAReturn => {
                "in a function's body, a line of code must start with `return`, for now."
            }
            Self::AfterReturn => "nothing can come after `return` in a body.",
            Self::AfterReturnValue => "a line with `return` ends with its value.",
            Self::Expected(part) => part.sentence(),
        };
        f.write_str(sentence)
    }
}

impl Part {
    /// Returns the sentence for a line with some other token where this part goes, or that ends
    /// there. It names the part, and what comes before it.
    const fn sentence(self) -> &'static str {
        match self {
            Self::Name => "the function's name must come after `function`.",
            Self::ParenthesisOpen => "`(` must come after the function's name.",
            Self::ParenthesisClose => {
                "`)` must come after `(`, since a function takes no parameters, for now."
            }
            Self::Returns => "`returns` and the function's return type must come after `()`.",
            Self::ReturnType => "the function's return type must come after `returns`.",
            Self::BraceOpen => "`{` must come after the function's return type.",
            Self::ReturnValue => "an integer must come after `return`.",
        }
    }
}
