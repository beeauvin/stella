// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The lexer turns source text into tokens, one line at a time.
//!
//! The text lint has already passed the source, so every line of code is ASCII, no line of code
//! holds `//`, and the source ends in `\n`. In code, then, a column is a byte.
//!
//! - **A comment** is a line that starts with `//` after its indentation, as the text lint says.
//!   It's one `Comment` token, from its `//` to the end of the line, without the `\n`.
//! - **A blank line** has no tokens.
//! - **In code,** spaces separate tokens and aren't tokens. How many there are and where they go
//!   is the tokens lint's to check.
//! - **A word** is an ASCII letter, then any ASCII letters, digits and `_`. The longest run is one
//!   word. A word that's exactly `function` or `returns` is that keyword, and keywords match by
//!   case.
//! - **Every other word is a name.** One that starts with a lowercase letter, like `main`, is a
//!   `Name`, and it must be snake case: lowercase letters and digits, with single underscores
//!   between them. One that starts with a capital, like `Void`, is a `TypeName`, and it must be
//!   Pascal case: words that each start with one capital, then lowercase letters and digits. A
//!   name that breaks its shape is refused at the first character that breaks it.
//! - **A symbol** is `(`, `)`, `{` or `}`.
//! - **Anything else** in code is refused. Lexing stops at the first character in the file that
//!   isn't in a token or a space: one that can't start a token where one would start, like `;`,
//!   or a digit or `_` with no word for it to go on.
//!
//! The tokens go in a `TokenBuffer`, made once with room for the most tokens a source can hold.
//! Each source the buffer is lent to replaces the last one's tokens, so lexing every program in a
//! tree uses the same memory as lexing one.

use stella_linter_text::{Position, SOURCE_BYTES_MAX, Text};

use crate::problem::{Kind, Problem};

/// This is the most tokens a source can hold. Every token is at least one byte, and a source's
/// last byte is a `\n`, which is in no token.
pub const TOKENS_MAX: u32 = SOURCE_BYTES_MAX - 1;

/// A token is one piece of code, a name, a keyword, a symbol or a comment, and where it is. It
/// holds where its text is rather than the text, so a buffer of tokens can serve one source after
/// another. `Tokens::text` gives the text back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// The kind says what the token is.
    pub kind: TokenKind,
    /// The position is the token's first character, by line and column.
    pub position: Position,
    /// The start is the token's first byte in the source, counted from 0.
    pub start: u32,
    /// The length is how many bytes the token has. A comment's length runs from its `//` to just
    /// before the `\n`.
    pub length: u32,
}

// A buffer holds `TOKENS_MAX` tokens of 20 bytes each, about 320 KiB, and it's made once.
const _: () = assert!(size_of::<Token>() == 20);

/// A token kind says what a token is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// The keyword `function`, which begins a function.
    Function,
    /// The keyword `returns`, which comes before a function's return type.
    Returns,
    /// A name that starts with a lowercase letter, like `main`. It's snake case.
    Name,
    /// A name that starts with a capital, like `Void`. It's Pascal case.
    TypeName,
    /// `(`
    ParenOpen,
    /// `)`
    ParenClose,
    /// `{`
    BraceOpen,
    /// `}`
    BraceClose,
    /// A comment, which is a whole line after its indentation.
    Comment,
}

/// A token buffer is where `lex` puts the tokens it finds. It's made once, with room for
/// `TOKENS_MAX` tokens, and it never grows.
#[derive(Debug)]
pub struct TokenBuffer {
    tokens: Vec<Token>,
}

impl TokenBuffer {
    /// Returns a buffer with room for `TOKENS_MAX` tokens.
    pub fn new() -> TokenBuffer {
        let tokens = Vec::with_capacity(TOKENS_MAX as usize);
        assert!(tokens.capacity() >= TOKENS_MAX as usize);
        TokenBuffer { tokens }
    }
}

impl Default for TokenBuffer {
    fn default() -> TokenBuffer {
        TokenBuffer::new()
    }
}

/// The tokens of a source that lexed, in the order they're in the source, and the source itself.
/// Only `lex` makes them, so holding `Tokens` proves the source passed the text lint and the
/// lexer. Everything in the source outside its tokens is spaces and line ends, so the spaces
/// between two tokens on a line can be counted from their columns.
#[derive(Clone, Copy, Debug)]
pub struct Tokens<'source, 'buffer> {
    text: Text<'source>,
    tokens: &'buffer [Token],
}

impl<'source, 'buffer> Tokens<'source, 'buffer> {
    /// Returns the tokens, in the order they're in the source.
    pub fn as_slice(&self) -> &'buffer [Token] {
        self.tokens
    }

    /// Returns `token`'s text, as it is in the source. The text outlives the buffer, since it's
    /// the source's own. The token must be one of these tokens: one from another source, or made
    /// by hand, gives some other text, or panics.
    pub fn text(&self, token: &Token) -> &'source str {
        let source = self.text.as_str();
        let start = token.start as usize;
        let end = start + token.length as usize;
        assert!(start < end, "a token is at least one byte");
        assert!(end <= source.len(), "a token is in its source");
        &source[start..end]
    }
}

/// Turns `text` into tokens in `buffer`, or returns the first problem in it. Whatever the buffer
/// held before is replaced.
pub fn lex<'source, 'buffer>(
    text: Text<'source>,
    buffer: &'buffer mut TokenBuffer,
) -> Result<Tokens<'source, 'buffer>, Problem> {
    let source = text.as_str();
    // The text lint guarantees these, and the bounds below lean on them.
    assert!(source.len() <= SOURCE_BYTES_MAX as usize);
    assert!(source.ends_with('\n'));
    let capacity = buffer.tokens.capacity();
    buffer.tokens.clear();
    let mut line_start: usize = 0;
    for (content, number) in source.split_terminator('\n').zip(1_u32..) {
        lex_line(Line { content, number, start: line_start }, &mut buffer.tokens)?;
        line_start += content.len() + 1;
    }
    assert_eq!(line_start, source.len(), "every line was lexed");
    assert!(buffer.tokens.len() <= TOKENS_MAX as usize);
    assert_eq!(buffer.tokens.capacity(), capacity, "the buffer never grows");
    Ok(Tokens { text, tokens: &buffer.tokens })
}

/// A line is one line of the source, without its `\n`, and where it is.
#[derive(Clone, Copy)]
struct Line<'source> {
    /// The content is the line's text, without its `\n`.
    content: &'source str,
    /// The number is the line's, counted from 1.
    number: u32,
    /// The start is the line's first byte in the source, counted from 0.
    start: usize,
}

/// Lexes one line, adding its tokens to `tokens`, or returns the line's first problem.
fn lex_line(line: Line<'_>, tokens: &mut Vec<Token>) -> Result<(), Problem> {
    assert!(line.number >= 1);
    assert!(line.number <= SOURCE_BYTES_MAX);
    let bytes = line.content.as_bytes();
    assert!(!bytes.contains(&b'\n'));
    let indentation = bytes.iter().take_while(|&&byte| byte == b' ').count();
    // A comment is a line that starts with `//` after its indentation, as the text lint says. Its
    // text can be any Unicode, and none of it is code, so it's one token.
    if bytes[indentation..].starts_with(b"//") {
        tokens.push(token(TokenKind::Comment, line, indentation, bytes.len() - indentation));
        return Ok(());
    }
    // The text lint lets only ASCII into code, so in code a byte is a character, and a column.
    assert!(line.content.is_ascii());
    let mut index = indentation;
    while index < bytes.len() {
        let (kind, length) = match bytes[index] {
            b' ' => (None, 1),
            b'(' => (Some(TokenKind::ParenOpen), 1),
            b')' => (Some(TokenKind::ParenClose), 1),
            b'{' => (Some(TokenKind::BraceOpen), 1),
            b'}' => (Some(TokenKind::BraceClose), 1),
            b'a'..=b'z' | b'A'..=b'Z' => {
                let length = word_length(&bytes[index..]);
                (Some(word_kind(line, index, length)?), length)
            }
            byte => return Err(problem(Kind::Unexpected(char::from(byte)), line, index)),
        };
        // Every step moves forward, so the loop ends within the line.
        assert!(length >= 1);
        if let Some(kind) = kind {
            tokens.push(token(kind, line, index, length));
        }
        index += length;
    }
    assert_eq!(index, bytes.len());
    Ok(())
}

/// Returns how many bytes long the word at the start of `bytes` is: a letter, then any letters,
/// digits and `_`.
fn word_length(bytes: &[u8]) -> usize {
    assert!(bytes[0].is_ascii_alphabetic());
    let in_word = |byte: &&u8| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_');
    let length = bytes.iter().take_while(in_word).count();
    assert!(length >= 1);
    length
}

/// Returns what the word at `index` in `line`, `length` bytes long, is: a keyword, a name or a
/// type's name. A name that breaks its shape is the problem at the first byte that breaks it.
fn word_kind(line: Line<'_>, index: usize, length: usize) -> Result<TokenKind, Problem> {
    assert!(length >= 1);
    assert!(index + length <= line.content.len());
    let word = &line.content[index..index + length];
    if let Some((kind, at)) = broken_shape(word) {
        assert!(at < length);
        return Err(problem(kind, line, index + at));
    }
    let kind = match word {
        "function" => TokenKind::Function,
        "returns" => TokenKind::Returns,
        _ if word.as_bytes()[0].is_ascii_uppercase() => TokenKind::TypeName,
        _ => TokenKind::Name,
    };
    Ok(kind)
}

/// Returns where `word` first breaks its shape, and the kind of problem that is, or `None` when
/// it keeps it. A word that starts with a lowercase letter is a name, in snake case, and one that
/// starts with a capital is a type's name, in Pascal case.
fn broken_shape(word: &str) -> Option<(Kind, usize)> {
    let bytes = word.as_bytes();
    assert!(bytes[0].is_ascii_alphabetic());
    // Only a capital or `_` can break a shape, and the first byte is a letter, which is the start
    // of its shape either way. So whatever breaks it has a byte before it.
    if bytes[0].is_ascii_lowercase() {
        let broken = (1..bytes.len()).find(|&index| breaks_snake_case(bytes, index));
        broken.map(|index| (Kind::NameNotSnakeCase, index))
    } else {
        let broken = (1..bytes.len()).find(|&index| breaks_pascal_case(bytes, index));
        broken.map(|index| (Kind::TypeNameNotPascalCase, index))
    }
}

/// Says whether the byte at `index` in a name breaks snake case: lowercase letters and digits,
/// with single underscores between them. A capital breaks it, and so does an `_` after another,
/// or at the end.
fn breaks_snake_case(bytes: &[u8], index: usize) -> bool {
    assert!(index > 0);
    match bytes[index] {
        b'A'..=b'Z' => true,
        b'_' if bytes[index - 1] == b'_' => true,
        b'_' => index == bytes.len() - 1,
        _ => false,
    }
}

/// Says whether the byte at `index` in a type's name breaks Pascal case: words that each start
/// with one capital, then lowercase letters and digits. An `_` breaks it, and so does a capital
/// after another.
fn breaks_pascal_case(bytes: &[u8], index: usize) -> bool {
    assert!(index > 0);
    match bytes[index] {
        b'_' => true,
        b'A'..=b'Z' => bytes[index - 1].is_ascii_uppercase(),
        _ => false,
    }
}

/// Returns a token of `kind`, `length` bytes long, at `index` in `line`.
fn token(kind: TokenKind, line: Line<'_>, index: usize, length: usize) -> Token {
    assert!(length >= 1);
    assert!(index + length <= line.content.len());
    let start = offset(line.start + index);
    Token { kind, position: position(line, index), start, length: offset(length) }
}

/// Returns the problem `kind`, at `index` in `line`.
fn problem(kind: Kind, line: Line<'_>, index: usize) -> Problem {
    assert!(index < line.content.len());
    Problem { kind, position: position(line, index) }
}

/// Returns the position of the byte at `index` in `line`. Everything before it on the line is
/// ASCII, either code or indentation, so its column is one more than its index.
fn position(line: Line<'_>, index: usize) -> Position {
    assert!(index < line.content.len());
    Position { line: line.number, column: offset(index) + 1 }
}

/// Returns `count`, which is within a source, as a `u32`.
fn offset(count: usize) -> u32 {
    assert!(count <= SOURCE_BYTES_MAX as usize);
    u32::try_from(count).expect("a count within a source fits in a `u32`")
}
