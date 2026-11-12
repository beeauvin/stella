// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! What the tests of more than one crate share.
//!
//! `Rng` is the seeded generator every fuzz test draws from. `Examples` is a table of sources a
//! test tries, which can't be dropped unchecked. The rest write and show Stella source the way
//! the tests do. Nothing here depends on a check, so an oracle built from it states a rule on its
//! own, and can't agree with the code it checks by sharing that code.

#![forbid(unsafe_code)]

mod examples;
mod rng;
mod write;

pub use examples::{Example, Examples};
pub use rng::Rng;
pub use write::{indent, joined, nested_blocks, shown, source_of};
