// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The arguments say what the command is asked to do.
//!
//! There are two commands: `build`, which takes one path, and `lint`, which takes one path or
//! none. There are no options. Anything else is a misuse, which has a sentence that says what's
//! wrong. An argument needn't be UTF-8, so each is read as bytes.

use std::ffi::OsString;
use std::fmt;
use std::os::unix::ffi::OsStrExt as _;

use crate::message::shown;

/// These are the lines every misuse ends with.
pub const USAGE: &str = "usage: stella build <path>\n       stella lint [path]\n";

/// A command is what the arguments ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command<'arguments> {
    /// Build the one program at this path, which isn't empty.
    Build(&'arguments [u8]),
    /// Lint the one program at this path, which isn't empty.
    LintOne(&'arguments [u8]),
    /// Lint every program under the directory the command runs in.
    LintEvery,
}

/// A misuse is what's wrong with arguments that ask for nothing the command does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Misuse<'arguments> {
    /// An argument starts with `-`, as an option would. There are none.
    NotAnOption(&'arguments [u8]),
    /// There's no command, or it's empty.
    NoCommand,
    /// The first argument isn't a command.
    NotACommand(&'arguments [u8]),
    /// `build` was given no path, or more than one.
    BuildNeedsOnePath,
    /// `lint` was given more than one path.
    LintTooManyPaths,
    /// The path a command was given is empty.
    EmptyPath,
}

/// A misuse displays as the sentence that says what's wrong. An argument in it is shown safely,
/// like anything else that reaches the terminal.
impl fmt::Display for Misuse<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::NotAnOption(option) => {
                write!(f, "`{}` isn't an option; there are none.", shown(option))
            }
            Self::NoCommand => f.write_str("a command is needed."),
            Self::NotACommand(command) => write!(f, "`{}` isn't a command.", shown(command)),
            Self::BuildNeedsOnePath => f.write_str("`build` takes one path."),
            Self::LintTooManyPaths => {
                f.write_str("`lint` takes one path, or none to lint every file.")
            }
            Self::EmptyPath => f.write_str("the path is empty."),
        }
    }
}

/// Works out what `arguments` ask for. They're the command's own, without the program's name.
///
/// # Errors
///
/// Returns the misuse, when the arguments ask for nothing the command does.
///
/// # Panics
///
/// Never, whatever the arguments. The assertions check this function's own work.
pub fn command(arguments: &[OsString]) -> Result<Command<'_>, Misuse<'_>> {
    // There are no options, so any argument that looks like one is a misuse, wherever it is.
    if let Some(option) = arguments.iter().find(|argument| argument.as_bytes().starts_with(b"-")) {
        return Err(Misuse::NotAnOption(option.as_bytes()));
    }
    // An empty command is no command at all.
    let Some((command, paths)) = arguments.split_first().filter(|(command, _)| !command.is_empty())
    else {
        return Err(Misuse::NoCommand);
    };
    let command = match (command.as_bytes(), paths) {
        (b"build", [path]) => Command::Build(path.as_bytes()),
        (b"build", [] | [_, _, ..]) => return Err(Misuse::BuildNeedsOnePath),
        (b"lint", []) => return Ok(Command::LintEvery),
        (b"lint", [path]) => Command::LintOne(path.as_bytes()),
        (b"lint", [_, _, ..]) => return Err(Misuse::LintTooManyPaths),
        (other, _) => return Err(Misuse::NotACommand(other)),
    };
    let (Command::Build(path) | Command::LintOne(path)) = command else {
        unreachable!("linting every program was returned above, since it takes no path");
    };
    if path.is_empty() {
        return Err(Misuse::EmptyPath);
    }
    assert!(!path.starts_with(b"-"), "an argument that looks like an option was refused first");
    Ok(command)
}
