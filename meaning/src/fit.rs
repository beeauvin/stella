// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Fitting checks that what each function's body gives back fits the type it returns.
//!
//! - **A function that returns `Void`** returns when its body ends, so it has no `return`. One
//!   that has one is refused at its `return`.
//! - **A function that returns an integer type** ends with a `return` and an integer, since
//!   there's nothing else it could give back. One with no `return` is refused at the `}` that
//!   closes its body. The grammar proved a body holds at most one `return`, as its last line of
//!   code, so that's the only one to check.
//! - **An integer** fits its function's integer type when it's no greater than the type holds,
//!   and is refused at the integer when it's greater. It's read digit by digit with checked
//!   arithmetic, so an integer of any length is read without overflowing: the first digit that
//!   takes it past the type is where reading stops.
//!
//! The functions are checked in the order they're in the source, and each has at most one fault,
//! so the first fault in the file is the one reported. The value each function gives back is
//! kept as its type, so the code generator never reads an integer's text again.

use stella_ast::{Ast, FUNCTIONS_MAX, Function};
use stella_source::usize_from_u32;

use crate::problem::{Kind, Problem};
use crate::resolve::{IntegerType, Type, Types};

/// A returned value is what a function's body gives back, as its type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Returned {
    /// The function returns `Void`, so it gives nothing back: it returns when its body ends.
    Void,
    /// The function returns `U8`, and gives back this value.
    U8(u8),
}

impl Returned {
    /// Returns the type of what's given back.
    #[must_use]
    pub const fn type_of(self) -> Type {
        match self {
            Self::Void => Type::Void,
            Self::U8(_) => Type::Integer(IntegerType::U8),
        }
    }
}

const _: () = assert!(
    size_of::<Returned>() == 2,
    "what a function gives back takes two bytes, so a source's take twice as many as its functions"
);

/// What each of a source's functions gives back, every one fitting the type it returns.
///
/// Only `check_fit` makes it, so holding `Fitted` proves the source passed every check through
/// the types, and that every function's body agrees with its type.
#[derive(Debug)]
pub struct Fitted<'source> {
    types: Types<'source>,
    returned: Vec<Returned>,
}

impl<'source> Fitted<'source> {
    /// Returns what the types check found, just as `resolve` made it.
    #[must_use]
    pub const fn types(&self) -> &Types<'source> {
        &self.types
    }

    /// Returns what each function gives back, one for each of the AST's functions, in the same
    /// order.
    #[must_use]
    pub const fn as_slice(&self) -> &[Returned] {
        self.returned.as_slice()
    }
}

/// Checks that what each of `types`'s functions gives back fits the type it returns, and returns
/// them as `Fitted`.
///
/// # Errors
///
/// Returns the first problem in the file: a `return` in a function that returns `Void`, a
/// function that returns an integer type with no `return`, or an integer greater than its type
/// holds.
///
/// # Panics
///
/// Never, whatever was resolved. The assertions check what the types check proved and the
/// fitting's own work, so one that fails is a bug in the compiler.
///
/// If there isn't the memory for what's given back, Rust ends the process, which isn't a panic.
pub fn check_fit(types: Types<'_>) -> Result<Fitted<'_>, Problem> {
    let ast = types.ast();
    let functions = ast.functions();
    assert_eq!(types.as_slice().len(), functions.len(), "each function has one type");
    assert!(
        functions.len() <= usize_from_u32(FUNCTIONS_MAX),
        "no source holds more than `FUNCTIONS_MAX` functions"
    );
    // Each function gives back one thing, so this is room for all of them, made once.
    let mut returned = Vec::with_capacity(functions.len());
    let capacity_before = returned.capacity();
    for (function, &found) in functions.iter().zip(types.as_slice()) {
        returned.push(fit_function(ast, function, found)?);
    }
    assert_eq!(returned.capacity(), capacity_before, "the room made for them never grows");
    assert_fitted(&types, &returned);
    Ok(Fitted { types, returned })
}

/// Returns what `function`, one of `ast`'s, gives back as `found`, the type it returns, or the
/// problem with its body.
fn fit_function(ast: &Ast<'_>, function: &Function, found: Type) -> Result<Returned, Problem> {
    let statements = ast.statements(function);
    assert!(statements.len() <= 1, "the grammar lets a body hold one `return` at most");
    match (found, statements.first()) {
        (Type::Void, None) => Ok(Returned::Void),
        (Type::Void, Some(statement)) => {
            Err(Problem::at(Kind::ReturnInVoid, ast.keyword_of(statement).position()))
        }
        (Type::Integer(integer), None) => {
            Err(Problem::at(Kind::MissingReturn(integer), ast.close(function).position()))
        }
        (Type::Integer(IntegerType::U8), Some(statement)) => {
            let value = ast.value(statement);
            match read_u8(ast.text(value)) {
                Some(read) => Ok(Returned::U8(read)),
                None => Err(Problem::at(Kind::DoesntFit(IntegerType::U8), value.position())),
            }
        }
    }
}

/// Returns the value `digits`, an integer's text, has as a `U8`, or `None` when it's greater
/// than a `U8` holds. It's read a digit at a time, and stops at the first that takes it past.
fn read_u8(digits: &str) -> Option<u8> {
    assert!(!digits.is_empty(), "an integer is at least one digit");
    let mut value: u8 = 0;
    for digit in digits.bytes() {
        assert!(digit.is_ascii_digit(), "the lexer makes an integer of digits only");
        value = value.checked_mul(10)?.checked_add(digit - b'0')?;
    }
    // The lexer refused every other spelling that starts with `0`, so only `0` itself reads 0.
    if value == 0 {
        assert_eq!(digits, "0", "only `0` reads as 0");
    }
    Some(value)
}

/// Asserts that `returned`, found for `types`'s functions, hold together, as the checks after
/// fitting will trust: one for each function, in the same order, each of the type that function
/// returns, and an integer exactly when the function has a `return`.
fn assert_fitted(types: &Types<'_>, returned: &[Returned]) {
    let ast = types.ast();
    let functions = ast.functions();
    assert_eq!(returned.len(), functions.len(), "each function gives back one thing");
    for ((function, &found), &given) in functions.iter().zip(types.as_slice()).zip(returned) {
        assert_eq!(given.type_of(), found, "what a function gives back is of the type it returns");
        let has_return = !ast.statements(function).is_empty();
        assert_eq!(has_return, given != Returned::Void, "only a function with a value returns");
    }
}
