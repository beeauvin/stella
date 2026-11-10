// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Checks Stella source text before it's lexed.
//!
//! The text lint is the first check a program meets. `lint_path` checks the path it was named
//! by, and `lint` checks its bytes. A source that passes comes back as a `Text`, which only
//! `lint` can make, so the lexer can take one and trust everything the lint guarantees.

#![warn(missing_docs)]

mod path;
mod problem;
mod text;

pub use path::{SourcePath, lint_path};
pub use problem::{Kind, Place, Position, Problem};
pub use text::{SOURCE_BYTES_MAX, Text, lint};
