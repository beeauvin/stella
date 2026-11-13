// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `stella lint` checks programs without building them.
//!
//! A program's path is checked first, as it was typed and then as it's stored, before the file is
//! read. What's read then goes through every check a build runs. Each check owns what it makes,
//! and all of it is dropped before the next program is read, so checking every program in a tree
//! takes the same room as checking one.

use std::io::{self, Write};
use std::path::Path;

use stella_source::usize_from_u32;

use crate::check::check;
use crate::message::{Place, message};
use crate::source::ProgramPaths;
use crate::status::Status;
use crate::stop::Stop;
use crate::walk::{Find, walk};

/// Lints the program at `typed`, a path as it was typed, under `root`, the directory the command
/// runs in. The path is checked before the file is read. A program that passes says nothing.
///
/// # Errors
///
/// Returns the stop, whose message shows the path as it was typed, except where the path as
/// it's stored is what's refused.
///
/// # Panics
///
/// Never, whatever the program and whatever is on the filesystem. The assertions check the
/// compiler's own work.
pub fn lint_one(root: &Path, typed: &[u8]) -> Result<(), Stop> {
    lint_stored(root, &ProgramPaths::find(root, typed)?)
}

/// Lints every program the walk finds under `root`, the directory the command runs in, and
/// returns the worst status any program or unreadable directory gets.
///
/// When there are too many entries to walk, or no programs, it says so and returns a failure.
/// Each message is written to `stderr` as it's made, with a blank line between messages, and
/// `stderr` is flushed after each. So linting every file holds one message at a time, however
/// many there are, and each is seen as soon as it's made.
///
/// # Errors
///
/// Returns the error from `stderr` when a message can't be written.
///
/// # Panics
///
/// Never, whatever is under the directory. The assertions check this function's own work.
pub fn lint_every(root: &Path, stderr: &mut impl Write) -> io::Result<Status> {
    let finds = match walk(root) {
        Ok(finds) => finds,
        Err(too_many) => {
            writeln!(stderr, "stella: {too_many}")?;
            return Ok(Status::Failed);
        }
    };
    if finds.is_empty() {
        writeln!(stderr, "stella: there are no `.stella` files under this directory.")?;
        return Ok(Status::Failed);
    }
    let mut status = Status::Passed;
    let mut messages_count: u32 = 0;
    for find in &finds {
        let linted = match find {
            Find::Source(path) => lint_stored(root, &ProgramPaths::found(path)),
            Find::Unreadable { directory, error } => {
                let sentence = format_args!("this directory can't be read: {error}.");
                Err(Stop::Failed(message(directory, Place::File, sentence)))
            }
        };
        if let Err(stop) = linted {
            status = status.max(stop.status());
            if messages_count > 0 {
                stderr.write_all(b"\n")?;
            }
            stderr.write_all(stop.message().as_bytes())?;
            stderr.flush()?;
            messages_count += 1;
        }
    }
    let said_once_each = usize_from_u32(messages_count) <= finds.len();
    assert!(said_once_each, "each find says one message at most");
    let silent = messages_count == 0;
    assert_eq!(status == Status::Passed, silent, "only a program that passes says nothing");
    Ok(status)
}

/// Lints `program` under `root`: its path as it's stored, and then what's read from it, with
/// every check a build runs.
fn lint_stored(root: &Path, program: &ProgramPaths<'_>) -> Result<(), Stop> {
    let (_, source) = program.read(root)?;
    match check(&source) {
        // A program that passes every check is one a build would build, which is all a lint is
        // asked.
        Ok(_) => Ok(()),
        Err(refusal) => Err(Stop::Refused(refusal.message(program.typed()))),
    }
}
