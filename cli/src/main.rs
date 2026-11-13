// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `stella` binary. It hands the process's arguments, its working directory and its stderr to
//! `stella_cli::run`, and exits with the status that comes back.
//!
//! Everything the command says goes to stderr. If stderr can't be written, there's nowhere left
//! to say so, and the command exits with 2. It writes through its own copy of stderr, because
//! std's own quietly treats a stderr that isn't open for writing as written. The copy is
//! buffered, so a message is one write, however many pieces it's made of. A stderr that's
//! closed can be written, though: Rust opens `/dev/null` in its place before `main` runs, so
//! what's written to it is lost, and the status stands.

#![forbid(unsafe_code)]

use std::env;
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufWriter};
use std::os::fd::AsFd as _;
use std::path::Path;
use std::process::ExitCode;

use stella_cli::{Status, run};

/// This is how many bytes of a message are held before they're written. Most messages are
/// shorter, so most are one write. A longer one is written as it fills.
const STDERR_BUFFER_BYTES: usize = 8 * 1024;

fn main() -> ExitCode {
    // KS-BOUNDED: the operating system bounds a process's arguments, and std has copied every
    // one by the time `args_os` returns, so no limit here would save any memory.
    let arguments: Vec<OsString> = env::args_os().skip(1).collect();
    let Ok(stderr) = io::stderr().as_fd().try_clone_to_owned() else {
        return ExitCode::from(Status::Failed.code());
    };
    let mut stderr = BufWriter::with_capacity(STDERR_BUFFER_BYTES, File::from(stderr));
    // Every path is relative to the directory the command runs in, and is never resolved to an
    // absolute one, which can be too long to resolve.
    let status = match run(&arguments, Path::new("."), &mut stderr) {
        Ok(status) => status,
        Err(_) => Status::Failed,
    };
    ExitCode::from(status.code())
}
