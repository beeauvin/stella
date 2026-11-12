// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Cargo, as the tools run it.
//!
//! This is the one module that starts Cargo, so there's one place that says how CI runs it, and
//! one sentence for when it can't be started.

use std::io::Read as _;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::files::create;

/// These have Clippy look at every target of every crate in a workspace.
pub(crate) const CLIPPY: [&str; 3] = ["clippy", "--workspace", "--all-targets"];

/// These come after Cargo's own arguments, and have Clippy refuse every warning.
pub(crate) const REFUSE_WARNINGS: [&str; 3] = ["--", "-D", "warnings"];

/// These have Cargo build what it can when a crate fails, and report in JSON, a message a line.
pub(crate) const REPORT_ALL_AS_JSON: [&str; 2] = ["--keep-going", "--message-format=json"];

/// This is the most Cargo may write to its stdout when its messages are read. It's many times
/// what a hundred small crates make it say, and it bounds what a tool holds in memory.
const MESSAGES_BYTES_MAX: u64 = 64 * 1024 * 1024;

/// A report is what came of a run of Cargo whose messages were read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    /// What Cargo wrote to its stdout.
    pub messages: String,
    /// Whether Cargo said it succeeded.
    pub succeeded: bool,
}

/// A run is one way of running Cargo: where, and what it's asked.
#[derive(Clone, Copy)]
pub(crate) struct Run<'run> {
    /// The workspace Cargo runs in.
    pub directory: &'run Path,
    /// Every argument Cargo is given.
    pub arguments: &'run [&'run str],
}

impl Run<'_> {
    /// Runs Cargo, with its output going where the tool's does, and says whether it succeeded.
    ///
    /// # Errors
    ///
    /// Returns why when Cargo can't be started.
    pub(crate) fn succeeds(self) -> Result<bool, String> {
        let status = self.command().status().map_err(|error| self.unstarted(&error))?;
        Ok(status.success())
    }

    /// Runs Cargo in a scratch workspace, which builds beside its own crates, and returns what
    /// it wrote to stdout and whether it succeeded. What it wrote to stderr goes to the file at
    /// `log`. A run that fails isn't an error here, since a run whose messages are read is often
    /// one that's meant to find problems: the caller says what a failure means.
    ///
    /// # Errors
    ///
    /// Returns why when Cargo can't be started, the log can't be made, or what Cargo wrote
    /// isn't UTF-8 or is more than `MESSAGES_BYTES_MAX`.
    pub(crate) fn report(self, log: &Path) -> Result<Report, String> {
        let stderr = create(log)?;
        let mut child = self
            .command()
            .env("CARGO_TARGET_DIR", self.directory.join("target"))
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn()
            .map_err(|error| self.unstarted(&error))?;
        let stdout = child.stdout.take().expect("the child's stdout was asked for as a pipe");
        let mut messages = String::new();
        // One byte past the most is asked for, so more is told from exactly the most.
        let read = stdout.take(MESSAGES_BYTES_MAX + 1).read_to_string(&mut messages);
        let is_bounded =
            u64::try_from(messages.len()).is_ok_and(|length| length <= MESSAGES_BYTES_MAX);
        if !is_bounded {
            // Cargo is still writing to a pipe nobody reads, so it's stopped, not waited for. If
            // it can't be stopped, the error below is still the one that matters.
            let _: std::io::Result<()> = child.kill();
        }
        let waited = child.wait();
        read.map_err(|error| format!("`{}` wrote something unreadable: {error}.", self.shown()))?;
        let status = waited
            .map_err(|error| format!("`{}` couldn't be waited for: {error}.", self.shown()))?;
        if !is_bounded {
            return Err(format!("`{}` wrote more than a tool reads.", self.shown()));
        }
        Ok(Report { messages, succeeded: status.success() })
    }

    /// Returns the run as a person would type it.
    pub(crate) fn shown(self) -> String {
        format!("cargo {}", self.arguments.join(" "))
    }

    /// Returns Cargo, ready to run this way. A warning in documentation is refused, as CI
    /// refuses it, whichever command builds the documentation.
    fn command(self) -> Command {
        assert!(!self.arguments.is_empty(), "Cargo is always asked for something");
        let mut command = Command::new("cargo");
        command.args(self.arguments).current_dir(self.directory).env("RUSTDOCFLAGS", "-D warnings");
        command
    }

    /// Returns the sentence that says Cargo couldn't be started, for `error`.
    fn unstarted(self, error: &std::io::Error) -> String {
        format!("`{}` can't be run: {error}.", self.shown())
    }
}
