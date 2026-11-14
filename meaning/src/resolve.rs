// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Resolving finds the type each function's return type names.
//!
//! Each type there is has one name. For now there are two: `Void`, and the integer type `U8`.
//! Each function's return type is looked up by its name, in the order the functions are in the
//! source, and the first name no type has is refused, at that name. A name is found whole, so
//! `Voids` or `Vo` is no more `Void` than `Bar` is, and by case, so neither is `VoiD`, and `u8`
//! is no more `U8` than `U16` is.
//!
//! There's one type for each function, in the same order, so the checks after this one read a
//! function's type rather than look its name up again.

use stella_ast::{Ast, FUNCTIONS_MAX};
use stella_source::usize_from_u32;

use crate::problem::{Kind, Problem};

/// A type is one of the types there are. It's held as one byte, however few there are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    /// `Void` has exactly one value, so it carries no information.
    Void,
    /// An integer type, whose values are whole numbers in its range.
    Integer(IntegerType),
}

/// An integer type is one of the types whose values are whole numbers, each in its range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerType {
    /// `U8` holds the whole numbers from 0 to 255, so it fits in a byte.
    U8,
}

impl IntegerType {
    /// Returns the name that names this integer type.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::U8 => "U8",
        }
    }

    /// Returns the least value this integer type holds.
    #[must_use]
    pub const fn min(self) -> u64 {
        match self {
            Self::U8 => 0,
        }
    }

    /// Returns the greatest value this integer type holds.
    #[must_use]
    pub const fn max(self) -> u64 {
        match self {
            Self::U8 => 255,
        }
    }
}

const _: () = assert!(IntegerType::U8.max() == 255, "`U8` holds what a byte holds");

impl Type {
    /// These are the types there are, in the order the problems name them.
    pub const ALL: [Self; 2] = [Self::Void, Self::Integer(IntegerType::U8)];

    /// Returns the name that names this type. Every type has one, so a type that's added has to
    /// be given its name here before the compiler builds.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Void => "Void",
            Self::Integer(integer) => integer.name(),
        }
    }

    /// Returns the type `name` names, or `None` when no type has that name. A name is found
    /// whole, and by case.
    fn named(name: &str) -> Option<Self> {
        let found = match name {
            "Void" => Some(Self::Void),
            "U8" => Some(Self::Integer(IntegerType::U8)),
            // Any other text is a name no type has, which is for the caller to refuse.
            _ => None,
        };
        if let Some(found) = found {
            assert_eq!(found.name(), name, "a type found by a name has that name");
        }
        found
    }
}

const _: () = assert!(
    size_of::<Type>() == 1,
    "a type takes a byte, so a source's types take as many bytes as its functions number"
);

/// The type each of a source's functions returns, every one a type that exists.
///
/// Only `resolve` makes it, so holding `Types` proves the source passed every check through
/// the AST, and that every type it names exists.
#[derive(Debug)]
pub struct Types<'source> {
    ast: Ast<'source>,
    types: Vec<Type>,
}

impl<'source> Types<'source> {
    /// Returns the AST, just as `build` made it.
    #[must_use]
    pub const fn ast(&self) -> &Ast<'source> {
        &self.ast
    }

    /// Returns the type each function returns, one for each of the AST's functions, in the same
    /// order.
    #[must_use]
    pub const fn as_slice(&self) -> &[Type] {
        self.types.as_slice()
    }
}

/// Finds the type each of `ast`'s functions returns, and returns them as `Types`.
///
/// # Errors
///
/// Returns the problem with the first return type that names no type, at that name.
///
/// # Panics
///
/// Never, whatever the AST. The assertions check what the AST proved and the resolving's own work,
/// so one that fails is a bug in the compiler.
///
/// If there isn't the memory for its types, Rust ends the process, which isn't a panic.
pub fn resolve(ast: Ast<'_>) -> Result<Types<'_>, Problem> {
    let functions = ast.functions();
    assert!(
        functions.len() <= usize_from_u32(FUNCTIONS_MAX),
        "no source holds more than `FUNCTIONS_MAX` functions"
    );
    // Each function has one type, so this is room for every type, made once, before any is
    // found.
    let mut types = Vec::with_capacity(functions.len());
    let capacity_before = types.capacity();
    for function in functions {
        let name = ast.return_type(function);
        let Some(found) = Type::named(ast.text(name)) else {
            return Err(Problem::at(Kind::NoSuchType, name.position()));
        };
        types.push(found);
    }
    assert_eq!(types.capacity(), capacity_before, "the room made for the types never grows");
    assert_resolved(&ast, &types);
    Ok(Types { ast, types })
}

/// Asserts that `types`, found for `ast`'s functions, hold together, as the checks after the
/// types will trust: one type for each function, in the same order, each named by exactly the
/// text of that function's return type.
fn assert_resolved(ast: &Ast<'_>, types: &[Type]) {
    let functions = ast.functions();
    assert_eq!(types.len(), functions.len(), "each function has one type");
    for (function, &found) in functions.iter().zip(types) {
        let name = ast.text(ast.return_type(function));
        assert_eq!(found.name(), name, "a function's type is the one its return type names");
    }
}
