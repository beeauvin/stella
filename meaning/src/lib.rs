// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks what Stella code means, in three checks after the AST.
//!
//! The grammar proved only the order of the tokens, so a function's return type is some type's
//! name, its body holds some `return` or none, and a source holds some number of functions. What
//! those names name, whether each body agrees with its type, and whether the functions make a
//! program, are these checks':
//!
//! 1. **The types.** `resolve` takes the `Ast` that `build` made, and finds the type each
//!    function's return type names. A name that no type has is refused at it. It returns the
//!    types as `Types`, which only `resolve` can make.
//! 2. **The fit.** `check_fit` takes `Types`, so every type is known, and checks what each
//!    function's body gives back fits the type it returns: no `return` for `Void`, and for an
//!    integer type, a `return` whose integer the type holds. It returns `Fitted`, which only
//!    `check_fit` can make, and which holds each value as its type.
//! 3. **The program.** `check_program` takes `Fitted`, and checks the functions make a program:
//!    exactly one function, named `main`, which returns `Void` or `U8`. It returns `Program`,
//!    which only `check_program` can make, and which knows which function is `main`.
//!
//! The types come first because both later checks read them. The fit comes before the program
//! because it's about each function alone, and the program about all of them. Each check
//! returns the first problem in the file, and a source with no function at all is refused as a
//! whole, since there's no function to point at.

#![forbid(unsafe_code)]

pub use fit::{Fitted, Returned, check_fit};
pub use problem::{Kind, Problem};
pub use program::{Program, check_program};
pub use resolve::{IntegerType, Type, Types, resolve};

mod fit;
mod problem;
mod program;
mod resolve;
