// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The parts of the `stella` command that don't need a process to test.
//!
//! `message` turns a problem into what the compiler prints, and `shown` makes any bytes safe to
//! print. `walk` finds every source file `stella lint` checks when it's given no path.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod message;
mod walk;

pub use message::{Place, message, shown};
pub use walk::{Found, TooManyEntries, WALK_ENTRIES_MAX, walk};
