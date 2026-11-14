// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Builds the abstract syntax tree, the AST, of Stella code: what the code says, without how it's
//! written down.
//!
//! The AST is the step after the grammar. `build` takes the `Forms` that `parse` made, so it can
//! trust what every node is: a comment, a function's first line, outside every block and holding
//! exactly the tokens of its rule, or a `return` line, the last line of code in a function's
//! body. That leaves nothing for it to refuse. Anything that goes wrong on the way is a bug in
//! the compiler, caught by an assertion. It returns the source's functions and their statements
//! as an `Ast`, which only `build` can make, so the checks after it can take one and read what
//! the code says without working it out from the tokens again.
//!
//! The AST holds only what the code means, so comments aren't in it. Whether what it holds makes
//! a program, such as whether there's a `main` and only one function, is for the checks after it.

#![forbid(unsafe_code)]

pub use build::{Ast, FUNCTIONS_MAX, Function, STATEMENTS_MAX, Statement, build};

mod build;
