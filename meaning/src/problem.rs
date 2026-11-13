// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This module holds what the meaning checks report when they refuse a program.

use std::fmt;

use stella_source::Position;

use crate::resolve::Type;

/// A problem is the one thing the meaning checks refused about a source.
///
/// It's either with the file as a whole, when there's no function to point at, or at one token
/// in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The source holds no function, so it has no `main`. The program check finds it.
    NoMain,
    /// Something at one token is refused.
    At {
        /// The kind says what's wrong.
        kind: Kind,
        /// The position is the first character of the token at fault.
        position: Position,
    },
}

impl Problem {
    /// Returns a problem of `kind` at `position`, the first character of the token at fault.
    pub(crate) const fn at(kind: Kind, position: Position) -> Self {
        Self::At { kind, position }
    }

    /// Returns where the problem is, or `None` when it's with the file as a whole.
    #[must_use]
    pub const fn position(&self) -> Option<Position> {
        match *self {
            Self::NoMain => None,
            Self::At { position, .. } => Some(position),
        }
    }
}

/// A problem displays as the sentence that says what's wrong. A message prints it after the
/// file, and after the position when there is one.
impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::NoMain => f.write_str("a program needs a function named `main`."),
            Self::At { kind, .. } => kind.fmt(f),
        }
    }
}

/// This is the only type there is, which `NoSuchType`'s sentence names. A type that's added
/// stops the build at this `match`, where the sentence needs new words.
const ONLY_TYPE: Type = match Type::Void {
    Type::Void => Type::Void,
};

/// A kind is one of the problems the meaning checks can find at a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A function's return type names no type there is. It's at the type's name. The types
    /// check finds it.
    NoSuchType,
    /// The source's first function isn't named `main`. It's at the function's name. The program
    /// check finds it.
    NotMain,
    /// The source holds a function after `main`. It's at that function's `function`. The
    /// program check finds it.
    AnotherFunction,
}

/// A kind displays as the sentence that says what's wrong.
impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sentence = match *self {
            Self::NoSuchType => {
                let only = ONLY_TYPE.name();
                return write!(f, "this type doesn't exist; for now, the only type is `{only}`.");
            }
            Self::NotMain => "a program's one function must be named `main`, for now.",
            Self::AnotherFunction => "a program holds only one function, for now.",
        };
        f.write_str(sentence)
    }
}
