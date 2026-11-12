// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The tokens lint checks that code is laid out the one way, in three steps. Each step leaves the
//! next less to worry about. The first step that fails is the one reported, and within a step,
//! the earliest problem in the file:
//!
//! 1. The lines: `{` ends its line and doesn't start it, `}` is the only token on its line, and a
//!    line of code ends with `{` or `}`. Each is found at the first token at fault, or for a line
//!    that ends some other way, just past its last token. A `{` that neither ends its line nor
//!    has a token before it is refused for not ending it. Which tokens can come before a `{` is
//!    for the grammar to say, once it's built.
//! 2. The braces: every `}` closes a `{` before it, and every `{` is closed. A `}` with nothing
//!    open always comes before any `{` left open, so the first such `}` is the earliest problem.
//!    Otherwise it's the outermost `{` left open.
//! 3. Within lines, in one pass: a line is indented one level, `LEVEL_SPACES` spaces, for each
//!    `{` still open on it, and a line that starts with `}` counts its own brace as closed.
//!    Between two tokens on a line goes exactly one space, except that none goes after `(`, and
//!    none goes before `(` or `)`. A problem is at the first character that differs from the one
//!    layout.
//!
//! After the first two steps, a line opens or closes at most one brace, so the depth moves by one
//! at most from line to line and is never negative. No two tokens on a line are then a pair with
//! `}` in it, or with `{` first.
//!
//! The lexer guarantees that everything outside the tokens is spaces and line ends, and that a
//! comment is a whole line after its indentation. So the gaps between tokens are counted from
//! where they start and end, which in code are bytes.

use std::cmp::Ordering;

use stella_lexer::TokenKind::{
    BraceClose, BraceOpen, Comment, Function, Name, ParenClose, ParenOpen, Returns, TypeName,
};
use stella_lexer::{TOKENS_MAX, Token, Tokens};
use stella_linter_text::LEVEL_SPACES;
use stella_source::{Position, SOURCE_BYTES_MAX, usize_from_u32};

use crate::problem::{Kind, Problem};

/// A layout is tokens laid out the one way, whose braces pair up.
///
/// Only `lint_tokens` makes one, so holding a `Layout` proves the source passed the text lint,
/// the lexer and the tokens lint.
#[derive(Clone, Copy, Debug)]
pub struct Layout<'source, 'buffer> {
    tokens: Tokens<'source, 'buffer>,
}

impl<'source, 'buffer> Layout<'source, 'buffer> {
    /// Returns the tokens, just as the lexer made them.
    #[must_use]
    pub const fn tokens(&self) -> Tokens<'source, 'buffer> {
        self.tokens
    }
}

/// Checks that `tokens` are laid out the one way, step by step, and stops at the first step that
/// fails. Returns them as a `Layout`.
///
/// # Errors
///
/// Returns the earliest problem in the first step that fails.
///
/// # Panics
///
/// Never, whatever the tokens. The assertions check what the lexer guarantees and the lint's own
/// work, so one that fails is a bug in the compiler.
pub fn lint_tokens<'source, 'buffer>(
    tokens: Tokens<'source, 'buffer>,
) -> Result<Layout<'source, 'buffer>, Problem> {
    let slice = tokens.as_slice();
    assert!(
        slice.len() <= usize_from_u32(TOKENS_MAX),
        "the lexer makes no more tokens than a source can hold"
    );
    lint_lines(slice)?;
    lint_braces(slice)?;
    lint_within_lines(slice)?;
    Ok(Layout { tokens })
}

/// The first step: `{` ends its line and doesn't start it, `}` is the only token on its line, and
/// a line of code ends with `{` or `}`. The tokens are looked at in order, so the first problem
/// found is the earliest.
fn lint_lines(tokens: &[Token]) -> Result<(), Problem> {
    for (index, token) in tokens.iter().enumerate() {
        assert!(token.length >= 1, "a token is at least one character");
        // Whether a token comes before this one on its line, and whether one comes after it.
        let before =
            index.checked_sub(1).is_some_and(|previous| same_line(&tokens[previous], token));
        let after = tokens.get(index + 1).is_some_and(|next| same_line(token, next));
        let kind = match (token.kind, before, after) {
            // Of the two problems a `{` can have, not ending its line is the one reported.
            (BraceOpen, true | false, true) => Some(Kind::BraceOpenNotLast),
            (BraceOpen, false, false) => Some(Kind::BraceOpenStartsLine),
            (BraceClose, true, true | false) | (BraceClose, false, true) => {
                Some(Kind::BraceCloseNotAlone)
            }
            (BraceOpen, true, false) | (BraceClose, false, false) => None,
            (Comment, comes_before, comes_after) => {
                assert!(!comes_before, "the lexer makes a comment of a whole line");
                assert!(!comes_after, "the lexer makes a comment of a whole line");
                None
            }
            (Function | Returns | Name | TypeName | ParenOpen | ParenClose, true | false, true) => {
                None
            }
            (
                Function | Returns | Name | TypeName | ParenOpen | ParenClose,
                true | false,
                false,
            ) => {
                // The line ends with a token that isn't a brace. It's code, so it's ASCII, and
                // the column just past it is its column and its length.
                let column = token.position.column() + token.length;
                let position = Position::new(token.position.line(), column);
                return Err(at(Kind::LineEndsWithoutBrace, position));
            }
        };
        if let Some(kind) = kind {
            return Err(at(kind, token.position));
        }
    }
    Ok(())
}

/// The second step: every `}` closes a `{` before it, and every `{` is closed. A `}` with nothing
/// open is found in order, and it always comes before any `{` left open, so it's the earliest
/// problem. Otherwise the earliest is the outermost `{` left open.
fn lint_braces(tokens: &[Token]) -> Result<(), Problem> {
    // Every `{` is a token, so the depth is at most `TOKENS_MAX`. That's looser than any source
    // reaches, since a line that opens a block is at least three bytes, but it's true.
    let mut depth: u32 = 0;
    // This is where the outermost `{` still open is, whenever one is.
    let mut outermost: Option<Position> = None;
    for token in tokens {
        match token.kind {
            BraceOpen => {
                if depth == 0 {
                    outermost = Some(token.position);
                }
                depth += 1;
            }
            BraceClose => {
                if depth == 0 {
                    return Err(at(Kind::BraceClosesNothing, token.position));
                }
                depth -= 1;
                if depth == 0 {
                    outermost = None;
                }
            }
            Function | Returns | Name | TypeName | ParenOpen | ParenClose | Comment => {}
        }
        assert!(depth <= TOKENS_MAX, "every `{{` that's open is a token");
        assert_eq!(outermost.is_some(), depth > 0, "an outermost `{{` is open when any is");
    }
    match outermost {
        Some(position) => Err(at(Kind::BraceNeverClosed, position)),
        None => Ok(()),
    }
}

/// The last step: each line's indentation, and the spaces between its tokens, in one pass, so the
/// first problem found is the earliest. The first two steps passed, so a `}` always closes a `{`,
/// and two tokens on a line are never a pair with `}` in it, or with `{` first.
fn lint_within_lines(tokens: &[Token]) -> Result<(), Problem> {
    // This is how many braces are open before the token being looked at.
    let mut depth: u32 = 0;
    for (index, token) in tokens.iter().enumerate() {
        let previous = index.checked_sub(1).map(|previous| &tokens[previous]);
        let found = match previous.filter(|previous| same_line(previous, token)) {
            None => indentation_problem(token, depth),
            Some(previous) => gap_problem(previous, token),
        };
        if let Some(problem) = found {
            return Err(problem);
        }
        match token.kind {
            BraceOpen => depth += 1,
            BraceClose => {
                assert!(depth > 0, "the braces pair up, so a `}}` closes a `{{` before it");
                depth -= 1;
            }
            Function | Returns | Name | TypeName | ParenOpen | ParenClose | Comment => {}
        }
        assert!(depth <= TOKENS_MAX, "every `{{` that's open is a token");
    }
    assert_eq!(depth, 0, "the braces pair up, so none is left open");
    Ok(())
}

/// Returns the problem with the indentation of `token`, the first on its line, when `depth`
/// braces are open before it. The line is indented one level for each `{` still open on it, and
/// a `}` counts its own brace as closed.
fn indentation_problem(token: &Token, depth: u32) -> Option<Problem> {
    let level = if token.kind == BraceClose {
        assert!(depth > 0, "the braces pair up, so a `}}` closes a `{{` before it");
        depth - 1
    } else {
        depth
    };
    let wanted = level * LEVEL_SPACES + 1;
    if token.position.column() == wanted {
        return None;
    }
    // Too deep, the first space past the right indentation is at fault. Too shallow, the token
    // is. Indentation is spaces, so either way the column is the lesser.
    let column = token.position.column().min(wanted);
    let position = Position::new(token.position.line(), column);
    Some(at(Kind::IndentationNotDepth(level), position))
}

/// Returns the problem with the spaces between `previous` and `token`, one right after the other
/// on a line: none when there are as many as go there.
fn gap_problem(previous: &Token, token: &Token) -> Option<Problem> {
    assert!(same_line(previous, token), "a gap is between two tokens on one line");
    // A comment is a whole line, so both of these are code. Code is ASCII, so a column is a
    // byte, and the gap can be counted either way.
    assert_ne!(previous.kind, Comment, "the lexer makes a comment of a whole line");
    assert_ne!(token.kind, Comment, "the lexer makes a comment of a whole line");
    assert_ne!(previous.kind, BraceOpen, "the first step passed, so a `{{` ends its line");
    assert_ne!(previous.kind, BraceClose, "the first step passed, so a `}}` is alone on its line");
    assert_ne!(token.kind, BraceClose, "the first step passed, so a `}}` is alone on its line");
    let end = previous.start + previous.length;
    assert!(token.start >= end, "tokens don't overlap");
    let gap = token.start - end;
    assert_eq!(
        token.position.column() - previous.position.column(),
        token.start - previous.start,
        "code is ASCII, so on a line columns and bytes move together"
    );
    let wanted = spaces_wanted(previous, token);
    let line = token.position.line();
    let first_space = previous.position.column() + previous.length;
    let (first, second) = (previous.kind, token.kind);
    match gap.cmp(&wanted) {
        Ordering::Equal => None,
        // The second token is where the space should be.
        Ordering::Less => Some(at(Kind::SpaceMissing(first, second), token.position)),
        Ordering::Greater if wanted == 0 => {
            let position = Position::new(line, first_space);
            Some(at(Kind::SpaceUnwanted(first, second), position))
        }
        Ordering::Greater => {
            let position = Position::new(line, first_space + 1);
            Some(at(Kind::SpacesExtra(first, second), position))
        }
    }
}

/// Returns how many spaces go between `previous` and `token`, one right after the other on a
/// line: none after `(`, and none before `(` or `)`, and otherwise one.
fn spaces_wanted(previous: &Token, token: &Token) -> u32 {
    assert!(same_line(previous, token), "a gap is between two tokens on one line");
    match (previous.kind, token.kind) {
        (ParenOpen, _) | (_, ParenOpen | ParenClose) => 0,
        (
            Function | Returns | Name | TypeName | ParenClose | BraceOpen | BraceClose | Comment,
            Function | Returns | Name | TypeName | BraceOpen | BraceClose | Comment,
        ) => 1,
    }
}

/// Says whether `first` and `second`, which come one right after the other, are on the same line.
fn same_line(first: &Token, second: &Token) -> bool {
    assert!(first.position.line() <= second.position.line(), "tokens come in order");
    first.position.line() == second.position.line()
}

/// Returns the problem `kind` at `position`, which is a character on a line of the source, or
/// just past the last token of one.
fn at(kind: Kind, position: Position) -> Problem {
    assert!(
        position.column() <= SOURCE_BYTES_MAX,
        "every line ends with a `\\n`, so just past a line's last token is still in the source"
    );
    Problem { kind, position }
}
