// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The checks a source goes through once it's read, in the one order they run.
//!
//! A source's text is checked, then its tokens, then their layout. Then its nodes are nested in
//! their blocks and checked against the grammar, its AST is built, and what it means is checked:
//! its types, and whether its functions make a program. Each check takes what only the one before
//! it makes, so they can't run in another order, and an earlier check's problem always comes
//! first. Nesting and building the AST can't refuse, since the checks before them proved all
//! they need.

use std::fmt;

use stella_ast::build;
use stella_blocks::nest;
use stella_grammar::parse;
use stella_lexer::lex;
use stella_linter_tokens::lint_tokens;
use stella_meaning::{Program, check_fit, check_program, resolve};
use stella_source::Position;

use crate::message::{Place, message};

/// A refusal is what a check found wrong with a source: where it is, and the sentence that says
/// what.
///
/// A position is held with the source it's in, so a refusal can't be shown against some other
/// source.
#[derive(Debug)]
pub(crate) struct Refusal<'source> {
    place: Place<'source>,
    sentence: String,
}

impl<'source> Refusal<'source> {
    /// Returns a refusal of `source` that `problem` says, at `position` when it has one, and
    /// with the file as a whole when it has none.
    fn new(source: &'source [u8], position: Option<Position>, problem: &impl fmt::Display) -> Self {
        let place = match position {
            Some(position) => Place::At { source, position },
            None => Place::File,
        };
        Self { place, sentence: problem.to_string() }
    }

    /// Returns the message that says this refusal, of the program at `path`.
    pub(crate) fn message(&self, path: &[u8]) -> String {
        message(path, self.place, &self.sentence)
    }
}

/// Runs every check on `source`, a file's bytes, and returns the program it is. Stops at the
/// first check that fails.
///
/// # Errors
///
/// Returns the refusal of the first check that fails.
pub(crate) fn check(source: &[u8]) -> Result<Program<'_>, Refusal<'_>> {
    let text = stella_linter_text::lint(source)
        .map_err(|problem| Refusal::new(source, problem.position(), &problem))?;
    let tokens =
        lex(text).map_err(|problem| Refusal::new(source, Some(problem.position), &problem.kind))?;
    let layout = lint_tokens(tokens)
        .map_err(|problem| Refusal::new(source, Some(problem.position), &problem.kind))?;
    let forms = parse(nest(layout))
        .map_err(|problem| Refusal::new(source, Some(problem.position), &problem.kind))?;
    let types = resolve(build(forms))
        .map_err(|problem| Refusal::new(source, problem.position(), &problem))?;
    let fitted =
        check_fit(types).map_err(|problem| Refusal::new(source, problem.position(), &problem))?;
    check_program(fitted).map_err(|problem| Refusal::new(source, problem.position(), &problem))
}
