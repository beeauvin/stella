// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Running the command: working out what the arguments ask for, and doing it.

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;

use crate::arguments::{Command, USAGE, command};
use crate::build::build_one;
use crate::filesystem::Reason;
use crate::lint::{lint_every, lint_one};
use crate::status::Status;
use crate::stop::Stop;

/// The streams are the two places the command writes. Each is named, so the two can't be given
/// the wrong way round.
#[derive(Debug)]
pub struct Streams<Stdout, Stderr> {
    /// Where a build's report goes, and nothing else.
    pub stdout: Stdout,
    /// Where every message goes.
    pub stderr: Stderr,
}

/// Runs the command `arguments` ask for under `root`, the directory the command runs in, and
/// returns the status to exit with.
///
/// Every message goes to `streams.stderr`, each as soon as it's made, and it's flushed before
/// this returns. A misuse says what's wrong and ends with the usage lines. Only a build's report
/// goes to `streams.stdout`, last, once the binary is in place. If the report can't be written
/// the binary stays, a message says why, and the run is a failure.
///
/// # Errors
///
/// Returns the error from `streams.stderr` when something can't be written to it. There's
/// nowhere left to say so, so the caller exits with the status of a failure.
pub fn run(
    arguments: &[OsString],
    root: &Path,
    streams: &mut Streams<impl Write, impl Write>,
) -> io::Result<Status> {
    let status = match command(arguments) {
        Err(misuse) => {
            write!(streams.stderr, "stella: {misuse}\n{USAGE}")?;
            Status::Failed
        }
        Ok(Command::Build(path)) => match build_one(root, path) {
            Ok(report) => run_report(&report, streams)?,
            Err(stop) => run_stop(&stop, &mut streams.stderr)?,
        },
        Ok(Command::LintEvery) => lint_every(root, &mut streams.stderr)?,
        Ok(Command::LintOne(path)) => match lint_one(root, path) {
            Ok(()) => Status::Passed,
            Err(stop) => run_stop(&stop, &mut streams.stderr)?,
        },
    };
    // A writer that buffers holds what was written until it's flushed, and only then says
    // whether it could be written.
    streams.stderr.flush()?;
    Ok(status)
}

/// Writes the message of `stop` to `stderr`, and returns the status it ends the run with.
fn run_stop(stop: &Stop, stderr: &mut impl Write) -> io::Result<Status> {
    stderr.write_all(stop.message().as_bytes())?;
    Ok(stop.status())
}

/// Writes `report`, what a build says of the program it built, to `streams.stdout`, and returns
/// the status the run ends with. If it can't be written, the binary is in place and only its
/// report is lost, so `streams.stderr` says why, and the run is a failure.
fn run_report(report: &str, streams: &mut Streams<impl Write, impl Write>) -> io::Result<Status> {
    let written = streams.stdout.write_all(report.as_bytes());
    match written.and_then(|()| streams.stdout.flush()) {
        Ok(()) => Ok(Status::Passed),
        Err(error) => {
            let reason = Reason::from_writing(error);
            writeln!(streams.stderr, "stella: the report can't be written: {reason}.")?;
            Ok(Status::Failed)
        }
    }
}
