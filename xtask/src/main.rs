// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The workspace's tools, run as `cargo xtask <command>`.
//!
//! - `ci` runs everything that must pass before anything merges, which is what CI runs.
//! - `lints` proves that the workspace's lints bite: it compiles one small fixture for each lint
//!   the configuration claims to enforce, and fails unless that lint is what's reported.
//!
//! A tool here is a Rust program built by the pinned toolchain under the workspace's own lints,
//! so it runs the same way on every machine, and there's no script beside the code.

#![forbid(unsafe_code)]

use std::env;
use std::ffi::OsString;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod cargo;
mod ci;
mod diagnostics;
mod files;
mod fixtures;
mod lints;
mod table;

/// This is what the tool says when it's asked for something it doesn't do.
const USAGE: &str = "usage: cargo xtask <ci|lints>";

fn main() -> ExitCode {
    // TS-BOUNDED: the operating system bounds a process's arguments, and std has copied every
    // one by the time `args_os` returns, so no limit here would save any memory.
    let arguments: Vec<OsString> = env::args_os().skip(1).collect();
    let outcome = match root() {
        Ok(root) => dispatch(&arguments, &root),
        Err(reason) => Err(reason),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            // If stderr can't be written there's nowhere left to say so, and the status still
            // says the tool failed.
            let _: io::Result<()> = writeln!(io::stderr().lock(), "xtask: {reason}");
            ExitCode::FAILURE
        }
    }
}

/// Hands the workspace at `root` to the command `arguments` name.
fn dispatch(arguments: &[OsString], root: &Path) -> Result<(), String> {
    let [command] = arguments else {
        return Err(format!("one command is needed.\n{USAGE}"));
    };
    match command.to_str() {
        Some("ci") => ci::run_steps(root),
        Some("lints") => lints::prove(root),
        Some(_) | None => Err(format!("`{}` isn't a command.\n{USAGE}", command.to_string_lossy())),
    }
}

/// Returns the workspace's root, which holds this crate's directory. Cargo says where the crate
/// is when it runs the tool.
fn root() -> Result<PathBuf, String> {
    let manifest_directory = env::var_os("CARGO_MANIFEST_DIR")
        .ok_or("`CARGO_MANIFEST_DIR` isn't set. Run the tool as `cargo xtask <command>`.")?;
    let root = Path::new(&manifest_directory)
        .parent()
        .ok_or("the tool's crate has no directory above it to be the workspace's root.")?;
    if !files::is_file(&root.join("Cargo.lock"))? {
        return Err(format!("{} holds no `Cargo.lock`, so it isn't the root.", root.display()));
    }
    Ok(root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;

    use super::{USAGE, dispatch};

    #[test]
    fn anything_but_one_known_command_is_refused_with_the_usage() {
        // No command, two, and one the tool doesn't have. Each is refused before anything is
        // run, so the root is never looked at, and each refusal ends with the usage.
        let root = Path::new("/workspace");
        let refused = |arguments: &[&str]| {
            let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
            dispatch(&arguments, root).expect_err("only `ci` and `lints` are commands")
        };
        let one_needed = format!("one command is needed.\n{USAGE}");
        assert_eq!(refused(&[]), one_needed);
        assert_eq!(refused(&["ci", "lints"]), one_needed);
        assert_eq!(refused(&["lint"]), format!("`lint` isn't a command.\n{USAGE}"));
        assert_eq!(refused(&[""]), format!("`` isn't a command.\n{USAGE}"));
        assert!(USAGE.contains("ci"), "the usage names `ci`");
        assert!(USAGE.contains("lints"), "the usage names `lints`");
    }
}
