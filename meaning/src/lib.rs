// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks what Stella code means, in two checks after the AST.
//!
//! The grammar proved only the order of the tokens, so a function's return type is some type's
//! name, and a source holds some number of functions. What those names name, and whether the
//! functions make a program, are these checks':
//!
//! 1. **The types.** `resolve` takes the `Ast` that `build` made, and finds the type each
//!    function's return type names. A name that no type has is refused at it. It returns the
//!    types as `Types`, which only `resolve` can make.
//! 2. **The program.** `check_program` takes `Types`, so every type is known, and checks the
//!    functions make a program: exactly one function, named `main`. It returns `Program`, which
//!    only `check_program` can make, and which knows which function is `main`.
//!
//! The types come first because the program reads `main`'s type, which they found. Each check
//! returns the first problem in the file, and a source with no function at all is refused as a
//! whole, since there's no function to point at.

#![forbid(unsafe_code)]

pub use problem::{Kind, Problem};
pub use program::{Program, check_program};
pub use resolve::{Type, Types, resolve};

mod problem;
mod program;
mod resolve;
