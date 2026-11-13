// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! What every check shares about a Stella source.
//!
//! A source is one file's bytes, and it's small: at most `SOURCE_BYTES_MAX`. So everything
//! counted within one, its bytes, characters, tokens and lines, fits in a `u32`, and that's how
//! the checks hold a count. `u32_from_usize` and `usize_from_u32` are the only ways between a
//! count that's held and one that indexes memory. A `Position` is where something is in a source,
//! by line and column. `is_own` says whether a value is the very one a source holds.

#![forbid(unsafe_code)]

pub use count::{SOURCE_BYTES_MAX, u32_from_usize, usize_from_u32};
pub use own::is_own;
pub use position::{OnLine, Position};

mod count;
mod own;
mod position;
