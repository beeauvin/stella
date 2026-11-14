// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks that Stella code's tokens come in the order its grammar allows.
//!
//! The grammar is the check after the blocks. `parse` takes the `Blocks` that `nest` made, so it
//! can trust that the code is laid out the one way, that a `{` only ever ends a node, since a `}`
//! isn't a node, and that each node knows the block it's in. It checks each node against the
//! rule for the block it's in, and returns the nodes' forms as `Forms`, or the first problem in
//! them. Only `parse` can make `Forms`, so the AST can take them and trust what each node is,
//! without deciding it again.
//!
//! The grammar checks only the order of tokens. Whether a name or a type means anything, such as
//! whether a program has a `main` or a type exists, is for the checks after it.

#![forbid(unsafe_code)]

pub use parse::{
    FUNCTION_NAME_TOKEN_INDEX, FUNCTION_RETURN_TYPE_TOKEN_INDEX_FROM_END, Form, Forms,
    RETURN_VALUE_TOKEN_INDEX, parse,
};
pub use problem::{Kind, Part, Problem};

mod parse;
mod problem;
