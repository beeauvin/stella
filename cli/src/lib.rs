// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `stella` command, as a library, so every part of it can be tested without a process.
//!
//! `run` is the whole command: it takes the arguments, the directory to work in and the `Streams`
//! to write to, and returns the status to exit with. The binary only hands it the process's own.
//! The rest are its parts:
//!
//! - `command` works out what the arguments ask for, or the `Misuse` they are.
//! - `lint_one` lints one program, and `lint_every` every program `walk` finds. `build_one`
//!   builds one program, and returns its report. A program that's read goes through every check
//!   there is, in the one order they run, whichever command reads it. A program the command goes
//!   no further with comes back as a `Stop`.
//! - `save` puts a binary's bytes in `.build/`, or says where and why it couldn't, as a
//!   `SaveFailure`.
//! - `search_for_stored` finds a path as its filesystem stores it, and `read` reads a file, as
//!   much of it as a source can be. They, `walk` and `save` ask the filesystem through one
//!   module, which turns each of its failures into an `AccessFailure` or a `Reason`.
//! - `message` turns a problem into what the compiler prints, and `shown` makes any bytes safe to
//!   print.
//! - `Status` is how a run ends.

#![forbid(unsafe_code)]

pub use arguments::{Command, Misuse, USAGE, command};
pub use build::build_one;
pub use filesystem::{AccessFailure, Reason};
pub use lint::{lint_every, lint_one};
pub use message::{Place, message, shown};
pub use read::{READ_INTERRUPTIONS_MAX, ReadFailure, fill, read};
pub use run::{Streams, run};
pub use save::{BUILD_DIRECTORY, PARTS_MAX, SaveFailure, SaveFailureKind, save};
pub use status::Status;
pub use stop::Stop;
pub use stored::{SEARCH_ENTRIES_MAX, SearchFailure, search_for_stored};
pub use walk::{Find, TooManyEntries, WALK_ENTRIES_MAX, walk};

mod arguments;
mod build;
mod check;
mod filesystem;
mod lint;
mod message;
mod read;
mod run;
mod save;
mod source;
mod status;
mod stop;
mod stored;
mod walk;
