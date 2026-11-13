// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Running the command: working out what the arguments ask for, and doing it.

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;

use crate::arguments::{Command, USAGE, command};
use crate::lint::Linter;
use crate::status::Status;

/// Runs the command `arguments` ask for under `root`, the directory the command runs in, and
/// returns the status to exit with.
///
/// Everything it says goes to `stderr`, each message as soon as it's made, and `stderr` is
/// flushed before this returns. A misuse says what's wrong and ends with a usage line.
///
/// # Errors
///
/// Returns the error from `stderr` when something can't be written to it. There's nowhere left
/// to say so, so the caller exits with the status of a failure.
pub fn run(arguments: &[OsString], root: &Path, stderr: &mut impl Write) -> io::Result<Status> {
    let status = match command(arguments) {
        Err(misuse) => {
            write!(stderr, "stella: {misuse}\n{USAGE}")?;
            Status::Failed
        }
        Ok(Command::LintEvery) => Linter::new(root).lint_every(stderr)?,
        Ok(Command::LintOne(path)) => {
            let outcome = Linter::new(root).lint_one(path);
            if let Some(text) = outcome.message() {
                stderr.write_all(text.as_bytes())?;
            }
            outcome.status()
        }
    };
    // A writer that buffers holds what was written until it's flushed, and only then says
    // whether it could be written.
    stderr.flush()?;
    Ok(status)
}
