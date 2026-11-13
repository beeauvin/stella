// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `stella lint` checks programs without building them.
//!
//! A program goes through the checks in a fixed order: its path, the path as it's stored, its
//! text, its tokens, their layout, and then its blocks. Each check takes what only the one before
//! it makes, so they can't run in another order, and an earlier check's problem always comes
//! first. Each check owns what it makes, and all of it is dropped before the next program is
//! read, so checking every program in a tree takes the same room as checking one.

use std::io::{self, Write};
use std::path::Path;
use std::str;

use stella_blocks::{Blocks, nest};
use stella_lexer::lex;
use stella_linter_path::{Problem, SourcePath, lint_path};
use stella_linter_tokens::lint_tokens;
use stella_source::{Position, usize_from_u32};

use crate::message::{Place, message};
use crate::read::read;
use crate::status::Status;
use crate::stored::{SearchFailure, search_for_stored};
use crate::walk::{Find, walk};

/// An outcome is what linting one program came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The program passed every check. A program that passes says nothing.
    Passed,
    /// The program was refused, with this message.
    Refused(String),
    /// Something outside the program went wrong, with this message.
    Failed(String),
}

impl Outcome {
    /// Returns the status the outcome ends a run with.
    #[must_use]
    pub const fn status(&self) -> Status {
        match self {
            Self::Passed => Status::Passed,
            Self::Refused(_) => Status::Refused,
            Self::Failed(_) => Status::Failed,
        }
    }

    /// Returns the outcome's message, or nothing for a program that passed.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Passed => None,
            Self::Refused(text) | Self::Failed(text) => Some(text),
        }
    }
}

/// A linter lints the programs under one directory.
#[derive(Clone, Copy, Debug)]
pub struct Linter<'root> {
    /// The directory the command runs in, which every path is relative to.
    root: &'root Path,
}

impl<'root> Linter<'root> {
    /// Returns a linter for the programs under `root`, the directory the command runs in.
    #[must_use]
    pub const fn new(root: &'root Path) -> Self {
        Self { root }
    }

    /// Lints the program at `typed`, a path as it was typed. The path is checked before the file
    /// is read. Its message shows the path as it was typed, except where the path as it's stored
    /// is what's refused.
    #[must_use]
    pub fn lint_one(self, typed: &[u8]) -> Outcome {
        let typed_path = match lint_path_bytes(typed) {
            Ok(typed_path) => typed_path,
            Err(problem) => return Outcome::Refused(message(typed, Place::File, problem)),
        };
        // On macOS, which ignores case, `hello.stella` can name `Hello.stella`, so the path is
        // found as it's stored before it's checked again.
        match search_for_stored(self.root, typed_path) {
            Ok(as_stored) => self.lint_stored(ProgramPaths { typed, stored: &as_stored }),
            Err(failure @ SearchFailure::Symlink) => {
                Outcome::Refused(message(typed, Place::File, failure))
            }
            Err(
                failure @ (SearchFailure::Access(_)
                | SearchFailure::DirectoryUnreadable(_)
                | SearchFailure::TooManyEntries),
            ) => Outcome::Failed(message(typed, Place::File, failure)),
        }
    }

    /// Lints every program the walk finds, writing each message to `stderr` as it's made, with a
    /// blank line between messages, and flushing after each. So linting every file holds one
    /// message at a time, however many there are, and each is seen as soon as it's made. Returns
    /// the worst status any program gets.
    ///
    /// # Errors
    ///
    /// Returns the error from `stderr` when a message can't be written.
    ///
    /// # Panics
    ///
    /// Never, whatever is under the directory. The assertion checks this function's own work.
    pub fn lint_every(self, stderr: &mut impl Write) -> io::Result<Status> {
        let finds = match walk(self.root) {
            Ok(finds) => finds,
            Err(too_many) => {
                writeln!(stderr, "stella: {too_many}")?;
                return Ok(Status::Failed);
            }
        };
        if finds.is_empty() {
            writeln!(stderr, "stella: there are no `.stella` files under this directory.")?;
            return Ok(Status::Failed);
        }
        let mut status = Status::Passed;
        let mut messages_count: u32 = 0;
        for find in &finds {
            let outcome = match find {
                // The walk finds each name as it's stored, and it never follows a symlink.
                Find::Source(path) => self.lint_stored(ProgramPaths { typed: path, stored: path }),
                Find::Unreadable { directory, error } => {
                    let sentence = format_args!("this directory can't be read: {error}.");
                    Outcome::Failed(message(directory, Place::File, sentence))
                }
            };
            status = status.max(outcome.status());
            if let Some(text) = outcome.message() {
                if messages_count > 0 {
                    stderr.write_all(b"\n")?;
                }
                stderr.write_all(text.as_bytes())?;
                stderr.flush()?;
                messages_count += 1;
            }
        }
        let said_once_each = usize_from_u32(messages_count) <= finds.len();
        assert!(said_once_each, "each find says one message at most");
        let silent = messages_count == 0;
        assert_eq!(status == Status::Passed, silent, "only a program that passes says nothing");
        Ok(status)
    }

    /// Lints `program`: its path as it's stored, then its text, its tokens, their layout and its
    /// blocks. A problem with the path is shown at the path as it's stored, and any other at the
    /// path as it was typed.
    fn lint_stored(self, program: ProgramPaths<'_>) -> Outcome {
        let ProgramPaths { typed, stored } = program;
        let stored_path = match lint_path_bytes(stored) {
            Ok(stored_path) => stored_path,
            Err(problem) => return Outcome::Refused(message(stored, Place::File, problem)),
        };
        // Joining a path that's absolute would put the root aside, and one that climbs with `..`
        // would leave it. The path lint refuses both, and the file is read on its word.
        let normalized = stored_path.normalized();
        assert!(!normalized.starts_with('/'), "a path that passed the lint isn't absolute");
        let climbs = normalized.split('/').any(|part| part == "..");
        assert!(!climbs, "a path that passed the lint never climbs with `..`");
        let file = self.root.join(normalized);
        let source = match read(&file) {
            Ok(source) => source,
            Err(failure) => return Outcome::Failed(message(typed, Place::File, failure)),
        };
        match lint_stored_source(&source) {
            Ok(()) => Outcome::Passed,
            Err(Refusal { position, sentence }) => {
                let place = match position {
                    Some(position) => Place::At { source: &source, position },
                    None => Place::File,
                };
                Outcome::Refused(message(typed, place, sentence))
            }
        }
    }
}

/// Lints `path`, in the bytes it was typed or stored in.
///
/// A path that passes was UTF-8 as it came, so the path that's returned is those very bytes. One
/// that isn't UTF-8 is linted with U+FFFD in place of each byte that isn't. No name may hold
/// that, so the path is refused by whichever check meets it first, like any other bad name.
fn lint_path_bytes(path: &[u8]) -> Result<SourcePath<'_>, Problem> {
    if let Ok(text) = str::from_utf8(path) {
        return lint_path(text);
    }
    let replaced = String::from_utf8_lossy(path);
    Err(lint_path(&replaced).expect_err("no path that passes holds U+FFFD"))
}

/// A program's paths are the two it goes by. Each is named, so the two can't be given the wrong
/// way round.
#[derive(Clone, Copy)]
struct ProgramPaths<'path> {
    /// The program's path as it was typed, or as the walk found it.
    typed: &'path [u8],
    /// The program's path as it's stored, which the file is read by.
    stored: &'path [u8],
}

/// A refusal is what a check found wrong with a source: where, when it's at a position, and the
/// sentence that says what.
struct Refusal {
    position: Option<Position>,
    sentence: String,
}

impl Refusal {
    /// Returns a refusal at `position`, which `sentence` says.
    const fn at(position: Position, sentence: String) -> Self {
        Self { position: Some(position), sentence }
    }
}

/// Checks `source`'s text, then its tokens, then their layout, and nests them. Stops at the first
/// check that fails, and returns its refusal.
fn lint_stored_source(source: &[u8]) -> Result<(), Refusal> {
    let text = stella_linter_text::lint(source).map_err(|problem| Refusal {
        position: problem.position(),
        sentence: problem.to_string(),
    })?;
    let tokens =
        lex(text).map_err(|problem| Refusal::at(problem.position, problem.kind.to_string()))?;
    let layout = lint_tokens(tokens)
        .map_err(|problem| Refusal::at(problem.position, problem.kind.to_string()))?;
    // Nothing reads the blocks yet, since the grammar isn't built. They're still nested, so every
    // program that's linted goes through nesting's assertions, and a layout that nesting can't
    // hold stops here, where it's found, rather than when the grammar first meets it.
    let _: Blocks<'_> = nest(layout);
    Ok(())
}
