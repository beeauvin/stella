// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Running a command, the way an end-to-end test does, and keeping everything it did.

use std::process::{Command, Output, Stdio};

/// A run is what one run of a command did: how it exited, and all it wrote.
///
/// Two runs are equal when all three are, so a test pins everything a command did at once, and
/// a difference shows all of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    /// The status the command exited with.
    pub status: i32,
    /// Everything the command wrote to stdout.
    pub stdout: String,
    /// Everything the command wrote to stderr.
    pub stderr: String,
}

impl Run {
    /// Runs `command` with nothing on stdin, waits for it, and returns what it did. Its stdout
    /// and stderr are captured, unless `command` already says where they go.
    ///
    /// # Panics
    ///
    /// Panics if the command can't be run, if a signal ends it, or if what it wrote isn't UTF-8.
    #[must_use]
    pub fn of(command: &mut Command) -> Self {
        let output = command.stdin(Stdio::null()).output();
        Self::from_output(output.expect("the command a test runs is there to run"))
    }

    /// Returns what a command did, from the `output` it left when it was waited for. It's for
    /// a command a test started itself, to run beside others.
    ///
    /// # Panics
    ///
    /// Panics if a signal ended the command, or if what it wrote isn't UTF-8.
    #[must_use]
    pub fn from_output(output: Output) -> Self {
        Self {
            status: output.status.code().expect("the command exits, and isn't ended by a signal"),
            stdout: String::from_utf8(output.stdout).expect("what a command writes is UTF-8"),
            stderr: String::from_utf8(output.stderr).expect("what a command writes is UTF-8"),
        }
    }

    /// Returns the run of a command that exits with `status`, writes nothing to stdout, and
    /// writes `stderr`.
    #[must_use]
    pub fn exits(status: i32, stderr: &str) -> Self {
        Self { status, stdout: String::new(), stderr: String::from(stderr) }
    }
}
