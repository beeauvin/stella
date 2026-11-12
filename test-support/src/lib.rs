// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! What the tests of more than one crate share.
//!
//! `Rng` is the seeded generator every fuzz test draws from. `Examples` is a table of sources a
//! test tries, which can't be dropped unchecked. `Scratch`, `write`, `fifo` and `ModeGuard` make
//! the files a test works in. The rest write and show Stella source the way the tests do.
//! Nothing here depends on a check, so an oracle built from it states a rule on its own, and
//! can't agree with the code it checks by sharing that code.

#![forbid(unsafe_code)]

pub use examples::{EXAMPLES_MAX, Example, Examples};
pub use files::{ModeGuard, Scratch, fifo, write};
pub use rng::Rng;
pub use write::{excerpt, indent, joined, nested_blocks, slice, source_of};

mod examples;
mod files;
mod rng;
mod write;
