// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `stella lint` checks programs without building them.
//!
//! A program goes through the checks in a fixed order: its path, the path as it's stored, its
//! text, its tokens, their layout, and then its blocks. Each check takes what only the one before
//! it makes, so they can't run in another order, and an earlier check's problem always comes
//! first. A linter holds the buffers the checks work in. They're made once, and lent to each
//! program in turn, so checking every program in a tree takes the same room as checking one.

use std::io::{self, Write};
use std::path::Path;

use stella_blocks::{LineBuffer, nest};
use stella_lexer::{TokenBuffer, lex};
use stella_linter_path::lint_path;
use stella_linter_text::lint;
use stella_linter_tokens::lint_tokens;
use stella_source::Position;

use crate::message::{Place, message, reason};
use crate::read::{SourceBuffer, read};
use crate::status::Status;
use crate::stored::{Unstored, stored};
use crate::walk::{Found, walk};

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

/// A linter lints the programs under one directory, in buffers it makes once.
#[derive(Debug)]
pub struct Linter<'root> {
    /// The directory the command runs in, which every path is relative to.
    root: &'root Path,
    /// Each source is read into this.
    source: SourceBuffer,
    /// Each source's tokens go in this.
    tokens: TokenBuffer,
    /// Each source's lines go in this.
    lines: LineBuffer,
}

impl<'root> Linter<'root> {
    /// Returns a linter for the programs under `root`, the directory the command runs in.
    #[must_use]
    pub fn new(root: &'root Path) -> Self {
        let (source, tokens, lines) = (SourceBuffer::new(), TokenBuffer::new(), LineBuffer::new());
        Self { root, source, tokens, lines }
    }

    /// Lints the program at `path`, as it was typed. The path is checked before the file is read.
    /// Its message shows the path as it was typed, except where the path as it's stored is what's
    /// refused.
    pub fn one(&mut self, path: &[u8]) -> Outcome {
        // A path that isn't UTF-8 can't be kebab, so it's refused like any other bad name.
        let typed = String::from_utf8_lossy(path);
        let source_path = match lint_path(&typed) {
            Ok(source_path) => source_path,
            Err(problem) => return Outcome::Refused(message(path, Place::File, problem)),
        };
        // On macOS, which ignores case, `hello.stella` can name `Hello.stella`, so the path is
        // found as it's stored before it's checked again.
        match stored(self.root, source_path) {
            Ok(as_stored) => self.stored(path, &as_stored),
            Err(unstored @ Unstored::Symlink) => {
                Outcome::Refused(message(path, Place::File, unstored))
            }
            Err(
                unstored @ (Unstored::Missing
                | Unstored::Unreadable(_)
                | Unstored::DirectoryUnreadable(_)
                | Unstored::TooManyEntries),
            ) => Outcome::Failed(message(path, Place::File, unstored)),
        }
    }

    /// Lints every program the walk finds, writing each message to `stderr` as it's made, with a
    /// blank line between messages. So linting every file holds one message at a time, however
    /// many there are. Returns the worst status any program gets.
    ///
    /// # Errors
    ///
    /// Returns the error from `stderr` when a message can't be written.
    ///
    /// # Panics
    ///
    /// Never, whatever is under the directory. The assertion checks this function's own work.
    pub fn every(&mut self, stderr: &mut impl Write) -> io::Result<Status> {
        let found = match walk(self.root) {
            Ok(found) => found,
            Err(too_many) => {
                writeln!(stderr, "stella: {too_many}")?;
                return Ok(Status::Failed);
            }
        };
        if found.is_empty() {
            writeln!(stderr, "stella: there are no `.stella` files under this directory.")?;
            return Ok(Status::Failed);
        }
        let mut status = Status::Passed;
        let mut messages_count: u32 = 0;
        for find in &found {
            let outcome = match find {
                // The walk finds each name as it's stored, and it never follows a symlink.
                Found::Source(path) => self.stored(path, path),
                Found::Unreadable { directory, error } => {
                    let reason = reason(error);
                    let sentence = format_args!("this directory can't be read: {reason}.");
                    Outcome::Failed(message(directory, Place::File, sentence))
                }
            };
            status = status.max(outcome.status());
            if let Some(text) = outcome.message() {
                if messages_count > 0 {
                    stderr.write_all(b"\n")?;
                }
                stderr.write_all(text.as_bytes())?;
                messages_count += 1;
            }
        }
        let silent = messages_count == 0;
        assert_eq!(status == Status::Passed, silent, "only a program that passes says nothing");
        Ok(status)
    }

    /// Lints the program stored at `stored`, which `path` names: its path, then its text, its
    /// tokens, their layout and its blocks. A problem with the path is shown at `stored`, and any
    /// other at `path`, as it was given.
    fn stored(&mut self, path: &[u8], stored: &[u8]) -> Outcome {
        let stored_text = String::from_utf8_lossy(stored);
        let stored_path = match lint_path(&stored_text) {
            Ok(stored_path) => stored_path,
            Err(problem) => return Outcome::Refused(message(stored, Place::File, problem)),
        };
        let file = self.root.join(stored_path.normalized());
        let source = match read(&file, &mut self.source) {
            Ok(source) => source,
            Err(unread) => return Outcome::Failed(message(path, Place::File, unread)),
        };
        match check(source, &mut self.tokens, &mut self.lines) {
            Ok(()) => Outcome::Passed,
            Err(Refusal { position, sentence }) => {
                let place = match position {
                    Some(position) => Place::At { source, position },
                    None => Place::File,
                };
                Outcome::Refused(message(path, place, sentence))
            }
        }
    }
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

/// Checks `source`'s text, then its tokens, lexed into `tokens`, then their layout, and nests its
/// lines into `lines`. Stops at the first check that fails, and returns its refusal.
fn check(source: &[u8], tokens: &mut TokenBuffer, lines: &mut LineBuffer) -> Result<(), Refusal> {
    let text = lint(source).map_err(|problem| Refusal {
        position: problem.position(),
        sentence: problem.to_string(),
    })?;
    let tokens = lex(text, tokens)
        .map_err(|problem| Refusal::at(problem.position, problem.kind.to_string()))?;
    let layout = lint_tokens(tokens)
        .map_err(|problem| Refusal::at(problem.position, problem.kind.to_string()))?;
    // Nothing reads the blocks yet, since the grammar isn't built. They're still nested, so every
    // program that's linted goes through nesting's assertions, and a layout that nesting can't
    // hold stops here, where it's found, rather than when the grammar first meets it.
    _ = nest(layout, lines);
    Ok(())
}
