// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Nests Stella code's lines in their blocks.
//!
//! The blocks are the step after the tokens lint. `nest` takes the `Layout` the tokens lint made,
//! so it can trust that every line of code ends with `{` or `}`, that a `{` never starts a line
//! and a `}` is alone on its line, that the braces pair up, and that each line is indented one
//! level for each block it's in. That leaves nothing for it to refuse. It returns the lines as
//! `Blocks`, each in the block it's in, and anything that goes wrong on the way is a bug in the
//! compiler, caught by an assertion. Only `nest` can make `Blocks`, so the grammar can take them
//! and trust them in turn.

#![forbid(unsafe_code)]

mod nest;

pub use nest::{Block, Blocks, Children, LINES_MAX, Line, LineBuffer, nest};
