// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This module holds what the meaning checks report when they refuse a program.

use std::fmt;

use stella_source::Position;

use crate::resolve::{IntegerType, Type};

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

const _: () = assert!(
    Type::ALL.len() == 2,
    "`NoSuchType`'s sentence names both types there are, so a type that's added needs new words"
);

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
    /// A function that returns `Void` has a `return`, though it returns when its body ends.
    /// It's at the `return`. The fit check finds it.
    ReturnInVoid,
    /// A function that returns an integer type has no `return`, so its body ends with nothing
    /// to give back. It's at the `}` that closes the body. The fit check finds it.
    MissingReturn(IntegerType),
    /// A `return`'s integer is greater than its function's integer type holds. It's at the
    /// integer. The fit check finds it.
    DoesntFit(IntegerType),
}

/// A kind displays as the sentence that says what's wrong.
impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sentence = match *self {
            Self::NoSuchType => {
                let [first, second] = Type::ALL.map(Type::name);
                return write!(
                    f,
                    "this type doesn't exist; for now, the types are `{first}` and `{second}`."
                );
            }
            Self::NotMain => "a program's one function must be named `main`, for now.",
            Self::AnotherFunction => "a program holds only one function, for now.",
            Self::ReturnInVoid => "a function that returns `Void` ends without `return`.",
            Self::MissingReturn(integer) => {
                let name = integer.name();
                return write!(
                    f,
                    "a function that returns `{name}` must end with `return` and a value."
                );
            }
            Self::DoesntFit(integer) => {
                let (name, min, max) = (integer.name(), integer.min(), integer.max());
                return write!(
                    f,
                    "this integer doesn't fit in `{name}`, which holds {min} to {max}."
                );
            }
        };
        f.write_str(sentence)
    }
}
