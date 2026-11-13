// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for running a command. Each runs the shell with a script whose status and output
//! are plain to read, and checks the run says exactly that.

#![forbid(unsafe_code)]

use std::process::{Command, Stdio};

use stella_test_support::Run;

/// Returns the shell, about to run `script`.
fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    command
}

#[test]
fn a_run_keeps_the_status_and_each_stream_apart() {
    // A script that writes one word to each stream and exits with 3. The run has each word in
    // its own stream, and the status.
    let run = Run::of(&mut shell("printf out; printf err >&2; exit 3"));
    let expected = Run { status: 3, stdout: String::from("out"), stderr: String::from("err") };
    assert_eq!(run, expected);
    assert_ne!(run, Run::exits(3, "err"), "what's on stdout is part of a run");
    assert_eq!(Run::of(&mut shell("printf err >&2; exit 3")), Run::exits(3, "err"));
    assert_eq!(Run::of(&mut shell("true")), Run::exits(0, ""));
}

#[test]
fn a_run_has_nothing_on_stdin() {
    // `cat` copies stdin to stdout until stdin ends. With nothing on stdin it ends at once, so
    // the run comes back, with nothing written.
    assert_eq!(Run::of(&mut shell("cat")), Run::exits(0, ""));
}

#[test]
fn a_stream_the_command_was_given_isnt_captured() {
    // A command whose stdout already goes somewhere keeps it, so nothing of it is in the run.
    let mut command = shell("printf out; printf err >&2");
    command.stdout(Stdio::null());
    assert_eq!(Run::of(&mut command), Run::exits(0, "err"));
}
