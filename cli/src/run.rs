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
/// Everything it says goes to `stderr`, each message as soon as it's made. A misuse says what's
/// wrong and ends with a usage line.
///
/// # Errors
///
/// Returns the error from `stderr` when something can't be written to it. There's nowhere left
/// to say so, so the caller exits with the status of a failure.
pub fn run(arguments: &[OsString], root: &Path, stderr: &mut impl Write) -> io::Result<Status> {
    match command(arguments) {
        Err(misuse) => {
            write!(stderr, "stella: {misuse}\n{USAGE}")?;
            Ok(Status::Failed)
        }
        Ok(Command::LintEvery) => Linter::new(root).every(stderr),
        Ok(Command::LintOne(path)) => {
            let outcome = Linter::new(root).one(path);
            if let Some(text) = outcome.message() {
                stderr.write_all(text.as_bytes())?;
            }
            Ok(outcome.status())
        }
    }
}
