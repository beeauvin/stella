// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks that Stella code is laid out the one way.
//!
//! The tokens lint is the check after the lexer. `lint_tokens` takes the `Tokens` the lexer made,
//! so it can trust everything the text lint and the lexer guarantee, and returns them as a
//! `Layout`, or the first problem in them. Only `lint_tokens` can make a `Layout`, so nesting
//! can take one and trust that the code is laid out the one way and that its braces pair up.

#![forbid(unsafe_code)]

mod layout;
mod problem;

pub use layout::{Layout, lint_tokens};
pub use problem::{Kind, Problem};
