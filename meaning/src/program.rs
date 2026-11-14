// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checking the program checks that a source's functions make a program.
//!
//! For now a program is exactly one function, named `main`, so:
//!
//! - **A source with no function** has no `main`, and is refused as a whole.
//! - **A first function not named `main`** is refused at its name.
//! - **A function after `main`** is refused at its `function`.
//!
//! The functions are checked in the order they're in the source, so the first fault in the file
//! is the one reported. `main` returns `Void`, and the program exits with 0, or `U8`, and the
//! program exits with the value `main` gives back. Those are every type there is, so nothing is
//! refused for `main`'s type here. A type that's added stops the build at this check's `match`,
//! until it says whether `main` may return it.

use stella_source::usize_from_u32;

use crate::fit::Fitted;
use crate::problem::{Kind, Problem};
use crate::resolve::{IntegerType, Type};

/// This is the name of the function that running a program runs.
const MAIN_NAME: &str = "main";

/// This is the index of `main`, in the AST's functions: a program's one function is its first.
const MAIN_FUNCTION_INDEX: u32 = 0;

/// A program is a source whose types all exist and whose functions make a program, and which of
/// them is `main`.
///
/// Only `check_program` makes it, so holding a `Program` proves the source passed every check
/// the compiler has, and that running it runs `main`.
#[derive(Debug)]
pub struct Program<'source> {
    fitted: Fitted<'source>,
    /// The index of `main`, in the AST's functions.
    main_function_index: u32,
}

impl<'source> Program<'source> {
    /// Returns what the fit check found, just as `check_fit` made it.
    #[must_use]
    pub const fn fitted(&self) -> &Fitted<'source> {
        &self.fitted
    }

    /// Returns the index of `main`, in the AST's functions.
    #[must_use]
    pub const fn main_function_index(&self) -> u32 {
        self.main_function_index
    }
}

/// Checks that `fitted`'s functions make a program, and returns it.
///
/// # Errors
///
/// Returns the first problem in the file: no function at all, a first function that isn't named
/// `main`, or a function after it.
///
/// # Panics
///
/// Never, whatever was resolved. The assertions check what the types check proved and this
/// check's own work, so one that fails is a bug in the compiler.
pub fn check_program(fitted: Fitted<'_>) -> Result<Program<'_>, Problem> {
    let types = fitted.types();
    let ast = types.ast();
    let functions = ast.functions();
    assert_eq!(types.as_slice().len(), functions.len(), "each function has one type");
    // The faults are looked for in the order they come in the file: a missing `main` has no
    // place, the first function's name comes before any second function, and that before any
    // third.
    let Some(first) = functions.first() else {
        return Err(Problem::NoMain);
    };
    let name = ast.name(first);
    if ast.text(name) != MAIN_NAME {
        return Err(Problem::at(Kind::NotMain, name.position()));
    }
    if let Some(second) = functions.get(1) {
        return Err(Problem::at(Kind::AnotherFunction, ast.keyword(second).position()));
    }
    assert_eq!(functions.len(), 1, "a program is exactly one function");
    // `main` may return `Void` or `U8`, every type there is. A type added to `Type` stops the
    // build at this `match`, which is where the refusal of a `main` that returns it goes.
    match types.as_slice()[usize_from_u32(MAIN_FUNCTION_INDEX)] {
        Type::Void | Type::Integer(IntegerType::U8) => {}
    }
    Ok(Program { fitted, main_function_index: MAIN_FUNCTION_INDEX })
}
