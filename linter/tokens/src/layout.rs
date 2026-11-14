// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The tokens lint checks that code is laid out the one way, in three steps. Each step leaves the
//! next less to worry about. The first step that fails is the one reported, and within a step,
//! the earliest problem in the file:
//!
//! 1. The lines: `{` ends its line and doesn't start it, and `}` is the only token on its line.
//!    Each is found at the first token at fault. A `{` that neither ends its line nor has a token
//!    before it is refused for not ending it. Which tokens can come before a `{`, and how a line
//!    that doesn't open a block ends, are for the grammar to say.
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
    BraceClose, BraceOpen, Comment, Function, Name, ParenthesisClose, ParenthesisOpen, Returns,
    TypeName,
};
use stella_lexer::{TOKENS_MAX, Token, Tokens};
use stella_linter_text::LEVEL_SPACES;
use stella_source::{Position, SOURCE_BYTES_MAX, u32_from_usize, usize_from_u32};

use crate::problem::{Kind, Neighbors, Problem};

/// A pair is two tokens, one right after the other in the source. Each is named, so the two
/// can't be given the wrong way round.
#[derive(Clone, Copy)]
struct Pair<'tokens> {
    /// The token that comes first.
    first: &'tokens Token,
    /// The token right after it.
    second: &'tokens Token,
}

impl Pair<'_> {
    /// Says whether the two tokens are on the same line.
    fn is_on_one_line(self) -> bool {
        let (first, second) = (self.first.position().line(), self.second.position().line());
        assert!(first <= second, "tokens come in the order they're in the source");
        first == second
    }
}

/// A layout is tokens laid out the one way, whose braces pair up.
///
/// Only `lint_tokens` makes one, so holding a `Layout` proves the source passed the text lint,
/// the lexer and the tokens lint.
#[derive(Debug)]
pub struct Layout<'source> {
    tokens: Tokens<'source>,
}

impl<'source> Layout<'source> {
    /// Returns the tokens, just as the lexer made them.
    #[must_use]
    pub const fn tokens(&self) -> &Tokens<'source> {
        &self.tokens
    }
}

/// Checks that `lexed`, a source's tokens, are laid out the one way, step by step, and stops at
/// the first step that fails. Returns them as a `Layout`.
///
/// # Errors
///
/// Returns the earliest problem in the first step that fails.
///
/// # Panics
///
/// Never, whatever the tokens. The assertions check what the lexer guarantees and the lint's own
/// work, so one that fails is a bug in the compiler.
pub fn lint_tokens(lexed: Tokens<'_>) -> Result<Layout<'_>, Problem> {
    let tokens = lexed.as_slice();
    assert!(
        tokens.len() <= usize_from_u32(TOKENS_MAX),
        "the lexer makes no more tokens than a source can hold"
    );
    lint_tokens_lines(tokens)?;
    lint_tokens_braces(tokens)?;
    lint_tokens_within_lines(tokens)?;
    Ok(Layout { tokens: lexed })
}

/// The first step: `{` ends its line and doesn't start it, and `}` is the only token on its line.
/// The tokens are looked at in order, so the first problem found is the earliest.
fn lint_tokens_lines(tokens: &[Token]) -> Result<(), Problem> {
    for (index, token) in tokens.iter().enumerate() {
        assert!(!token.span().is_empty(), "a token is at least one character");
        // Whether this token comes after another on its line, and whether it comes before one.
        let comes_after_one = index.checked_sub(1).is_some_and(|previous| {
            Pair { first: &tokens[previous], second: token }.is_on_one_line()
        });
        let comes_before_one = tokens
            .get(index + 1)
            .is_some_and(|next| Pair { first: token, second: next }.is_on_one_line());
        let kind = match (token.kind(), comes_after_one, comes_before_one) {
            // Of the two problems a `{` can have, not ending its line is the one reported.
            (BraceOpen, true | false, true) => Some(Kind::BraceOpenNotLast),
            (BraceOpen, false, false) => Some(Kind::BraceOpenStartsLine),
            (BraceClose, true, true | false) | (BraceClose, false, true) => {
                Some(Kind::BraceCloseNotAlone)
            }
            // A `{` that ends its line and doesn't start it, a `}` alone on its line, and any
            // other code, wherever it is on its line, are all as they should be. How a line that
            // doesn't open a block ends is the grammar's to say.
            (BraceOpen, true, false)
            | (BraceClose, false, false)
            | (
                Function | Returns | Name | TypeName | ParenthesisOpen | ParenthesisClose,
                true | false,
                true | false,
            ) => None,
            (Comment, after_one, before_one) => {
                assert!(!after_one, "the lexer makes a comment of a whole line");
                assert!(!before_one, "the lexer makes a comment of a whole line");
                None
            }
        };
        if let Some(kind) = kind {
            return Err(problem(kind, token.position()));
        }
    }
    Ok(())
}

/// The second step: every `}` closes a `{` before it, and every `{` is closed. A `}` with nothing
/// open is found in order, and it always comes before any `{` left open, so it's the earliest
/// problem. Otherwise the earliest is the outermost `{` left open.
fn lint_tokens_braces(tokens: &[Token]) -> Result<(), Problem> {
    // Every `{` is a token, so the depth is at most `TOKENS_MAX`. That's looser than any source
    // reaches, since a line that opens a block is at least three bytes, but it's true.
    let mut depth: u32 = 0;
    // This is where the outermost `{` still open is, whenever one is.
    let mut outermost: Option<Position> = None;
    for token in tokens {
        match token.kind() {
            BraceOpen => {
                if depth == 0 {
                    outermost = Some(token.position());
                }
                depth += 1;
            }
            BraceClose => {
                if depth == 0 {
                    return Err(problem(Kind::BraceClosesNothing, token.position()));
                }
                depth -= 1;
                if depth == 0 {
                    outermost = None;
                }
            }
            Function | Returns | Name | TypeName | ParenthesisOpen | ParenthesisClose | Comment => {
            }
        }
        assert!(depth <= TOKENS_MAX, "every `{{` that's open is a token");
        assert_eq!(outermost.is_some(), depth > 0, "an outermost `{{` is open when any is");
    }
    match outermost {
        Some(position) => Err(problem(Kind::BraceNeverClosed, position)),
        None => Ok(()),
    }
}

/// The last step: each line's indentation, and the spaces between its tokens, in one pass, so the
/// first problem found is the earliest. The first two steps passed, so a `}` always closes a `{`,
/// and two tokens on a line are never a pair with `}` in it, or with `{` first.
fn lint_tokens_within_lines(tokens: &[Token]) -> Result<(), Problem> {
    // This is how many braces are open before the token being looked at.
    let mut depth: u32 = 0;
    for (index, token) in tokens.iter().enumerate() {
        let pair =
            index.checked_sub(1).map(|previous| Pair { first: &tokens[previous], second: token });
        let found = match pair.filter(|pair| pair.is_on_one_line()) {
            None => indentation_problem(token, depth),
            Some(pair) => gap_problem(pair),
        };
        if let Some(problem) = found {
            return Err(problem);
        }
        match token.kind() {
            BraceOpen => depth += 1,
            BraceClose => {
                assert!(depth > 0, "the braces pair up, so a `}}` closes a `{{` before it");
                depth -= 1;
            }
            Function | Returns | Name | TypeName | ParenthesisOpen | ParenthesisClose | Comment => {
            }
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
    let level = if token.kind() == BraceClose {
        assert!(depth > 0, "the braces pair up, so a `}}` closes a `{{` before it");
        depth - 1
    } else {
        depth
    };
    let wanted = level * LEVEL_SPACES + 1;
    if token.position().column() == wanted {
        return None;
    }
    // Too deep, the first space past the right indentation is at fault. Too shallow, the token
    // is. Indentation is spaces, so either way the column is the lesser.
    let column = token.position().column().min(wanted);
    let position = Position::on_line(token.position().line()).at_column(column);
    Some(problem(Kind::IndentationNotDepth(level), position))
}

/// Returns the problem with the spaces between the tokens of `pair`, which are on one line: none
/// when there are as many as go there.
fn gap_problem(pair: Pair<'_>) -> Option<Problem> {
    assert!(pair.is_on_one_line(), "a gap is between two tokens on one line");
    let Pair { first: previous, second: token } = pair;
    // A comment is a whole line, so both of these are code. Code is ASCII, so a column is a
    // byte, and the gap can be counted either way.
    assert_ne!(previous.kind(), Comment, "the lexer makes a comment of a whole line");
    assert_ne!(token.kind(), Comment, "the lexer makes a comment of a whole line");
    assert_ne!(previous.kind(), BraceOpen, "the first step passed, so a `{{` ends its line");
    assert_ne!(
        previous.kind(),
        BraceClose,
        "the first step passed, so a `}}` is alone on its line"
    );
    assert_ne!(token.kind(), BraceClose, "the first step passed, so a `}}` is alone on its line");
    let (previous_span, token_span) = (previous.span(), token.span());
    assert!(token_span.start >= previous_span.end, "tokens don't overlap");
    let gap_count = u32_from_usize(token_span.start - previous_span.end);
    assert_eq!(
        usize_from_u32(token.position().column() - previous.position().column()),
        token_span.start - previous_span.start,
        "code is ASCII, so on a line columns and bytes move together"
    );
    let neighbors = Neighbors { before: previous.kind(), after: token.kind() };
    let wanted_count = spaces_wanted(neighbors);
    let line = token.position().line();
    let first_space_column = column_after(previous);
    match gap_count.cmp(&wanted_count) {
        Ordering::Equal => None,
        // The second token is where the space should be.
        Ordering::Less => Some(problem(Kind::SpaceMissing(neighbors), token.position())),
        Ordering::Greater if wanted_count == 0 => {
            let position = Position::on_line(line).at_column(first_space_column);
            Some(problem(Kind::SpaceUnwanted(neighbors), position))
        }
        Ordering::Greater => {
            let position = Position::on_line(line).at_column(first_space_column + 1);
            Some(problem(Kind::SpacesExtra(neighbors), position))
        }
    }
}

/// Returns the column just past `token`, which is code. Code is ASCII, so a token is as many
/// columns wide as it is bytes long.
fn column_after(token: &Token) -> u32 {
    assert_ne!(token.kind(), Comment, "a comment can hold any character, so it isn't measured");
    token.position().column() + u32_from_usize(token.span().len())
}

/// Returns how many spaces go between `neighbors`: none after `(`, and none before `(` or `)`,
/// and otherwise one. Every pair of kinds is named, so a new kind has to be given its place.
const fn spaces_wanted(neighbors: Neighbors) -> u32 {
    match (neighbors.before, neighbors.after) {
        (
            ParenthesisOpen,
            Function | Returns | Name | TypeName | ParenthesisOpen | ParenthesisClose | BraceOpen
            | BraceClose | Comment,
        )
        | (
            Function | Returns | Name | TypeName | ParenthesisClose | BraceOpen | BraceClose
            | Comment,
            ParenthesisOpen | ParenthesisClose,
        ) => 0,
        (
            Function | Returns | Name | TypeName | ParenthesisClose | BraceOpen | BraceClose
            | Comment,
            Function | Returns | Name | TypeName | BraceOpen | BraceClose | Comment,
        ) => 1,
    }
}

/// Returns the problem `kind` at `position`, which is a character on a line of the source.
fn problem(kind: Kind, position: Position) -> Problem {
    assert!(
        position.column() < SOURCE_BYTES_MAX,
        "every line ends with a `\\n`, so a character on one is never a source's last byte"
    );
    Problem { kind, position }
}
