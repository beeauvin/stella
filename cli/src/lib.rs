// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `stella` command, as a library, so every part of it can be tested without a process.
//!
//! `run` is the whole command: it takes the arguments, the directory to work in and somewhere to
//! write, and returns the status to exit with. The binary only hands it the process's own. The
//! rest are its parts:
//!
//! - `command` works out what the arguments ask for, or the `Misuse` they are.
//! - `Linter` lints one program, or every program `walk` finds, and says how each came out.
//! - `stored` finds a path as its filesystem stores it, and `read` reads a file into a buffer.
//! - `message` turns a problem into what the compiler prints, and `shown` makes any bytes safe to
//!   print.
//! - `Status` is how a run ends.

#![forbid(unsafe_code)]

mod arguments;
mod lint;
mod message;
mod read;
mod run;
mod status;
mod stored;
mod unreached;
mod walk;

pub use arguments::{Command, Misuse, USAGE, command};
pub use lint::{Linter, Outcome};
pub use message::{Place, message, shown};
pub use read::{READ_INTERRUPTIONS_MAX, SourceBuffer, Unread, fill, read};
pub use run::run;
pub use status::Status;
pub use stored::{STORED_ENTRIES_MAX, Unstored, stored};
pub use unreached::Unreached;
pub use walk::{Find, TooManyEntries, WALK_ENTRIES_MAX, walk};
