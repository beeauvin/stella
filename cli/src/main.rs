// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `stella` binary. It hands the process's arguments, its working directory, its stdout and
//! its stderr to `stella_cli::run`, and exits with the status that comes back.
//!
//! Every message the command says goes to stderr, and only a build's report goes to stdout. If
//! stderr can't be written, there's nowhere left to say so, and the command exits with 2. It
//! writes through its own copy of each, because std's own quietly treats one that isn't open
//! for writing as written. The copy of stderr is buffered, so a message that fits in the buffer
//! is one write, however many pieces it's made of. A stream that's closed can be written,
//! though: Rust opens `/dev/null` in its place before `main` runs, so what's written to it is
//! lost, and the status stands.

#![forbid(unsafe_code)]

use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::os::fd::AsFd as _;
use std::path::Path;
use std::process::ExitCode;

use stella_cli::{Status, Streams, run};

/// This is how many bytes of a message are held before they're written. Most messages are
/// shorter, so most are one write. A longer one is written as it fills.
const STDERR_BUFFER_BYTES: usize = 8 * 1024;

/// The command's own copy of stdout, or why it couldn't be made.
///
/// Only a build's report is written to stdout, so a copy that couldn't be made matters only
/// once there's a report to write. Then it's the report that can't be written, for the reason
/// the copy couldn't be made.
struct Stdout(io::Result<File>);

impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.0 {
            Ok(file) => file.write(buf),
            Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.0 {
            Ok(file) => file.flush(),
            Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
        }
    }
}

fn main() -> ExitCode {
    // KS-BOUNDED: the operating system bounds a process's arguments, and std has copied every
    // one by the time `args_os` returns, so no limit here would save any memory.
    let arguments: Vec<OsString> = env::args_os().skip(1).collect();
    let Ok(stderr) = io::stderr().as_fd().try_clone_to_owned() else {
        return ExitCode::from(Status::Failed.code());
    };
    let stdout = io::stdout().as_fd().try_clone_to_owned().map(File::from);
    let mut streams = Streams {
        stdout: Stdout(stdout),
        stderr: BufWriter::with_capacity(STDERR_BUFFER_BYTES, File::from(stderr)),
    };
    // Every path is relative to the directory the command runs in, and is never resolved to an
    // absolute one, which can be too long to resolve.
    let status = match run(&arguments, Path::new("."), &mut streams) {
        Ok(status) => status,
        Err(_) => Status::Failed,
    };
    ExitCode::from(status.code())
}
