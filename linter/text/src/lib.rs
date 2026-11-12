// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks Stella source text before it's lexed.
//!
//! The text lint is the check after the path lint, and the first to read the file. `lint` checks
//! a source's bytes. A source that passes comes back as a `Text`, which only `lint` can make, so
//! the lexer can take one and trust everything the lint guarantees.

#![forbid(unsafe_code)]

pub use problem::{Kind, Problem};
pub use text::{LEVEL_SPACES, Text, lint};

mod problem;
mod text;
