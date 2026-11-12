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

use std::ops::Range;

use stella_linter_text::Text;
use stella_source::{Position, SOURCE_BYTES_MAX, u32_from_usize, usize_from_u32};

use crate::problem::{Kind, Problem};

/// This is the most tokens a source can hold. Every token is at least one byte, and a source's
/// last byte is a `\n`, which is in no token.
pub const TOKENS_MAX: u32 = SOURCE_BYTES_MAX - 1;

/// A token is one piece of code, a name, a keyword, a symbol or a comment, and where it is.
///
/// It holds where its text is rather than the text, so a buffer of tokens can serve one source
/// after another. `Tokens::text` gives the text back. Only `lex` makes one, so a token always
/// names text that's in some source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// The kind says what the token is.
    kind: TokenKind,
    /// The position is the token's first character, by line and column.
    position: Position,
    /// The start is the index of the token's first byte in the source.
    start_index: u32,
    /// The end is the index just past the token's last byte in the source. A comment ends just
    /// before its `\n`.
    end_index: u32,
}

impl Token {
    /// Returns what the token is.
    #[must_use]
    pub const fn kind(&self) -> TokenKind {
        self.kind
    }

    /// Returns where the token's first character is, by line and column.
    #[must_use]
    pub const fn position(&self) -> Position {
        self.position
    }

    /// Returns where the token's text is in its source, as a range of bytes. A comment's runs
    /// from its `//` to just before the `\n`.
    ///
    /// # Panics
    ///
    /// Never, for a token `lex` made, and nothing else makes one. The assertion checks the
    /// lexer's own work.
    #[must_use]
    pub const fn span(&self) -> Range<usize> {
        assert!(self.start_index < self.end_index, "a token is at least one byte");
        usize_from_u32(self.start_index)..usize_from_u32(self.end_index)
    }
}

const _: () = assert!(
    size_of::<Token>() == 20,
    "a buffer is budgeted at `TOKENS_MAX` tokens of 20 bytes each, about 320 KiB"
);

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
    /// The symbol `(`.
    ParenthesisOpen,
    /// The symbol `)`.
    ParenthesisClose,
    /// The symbol `{`.
    BraceOpen,
    /// The symbol `}`.
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
    ///
    /// # Panics
    ///
    /// Panics if there isn't the memory for it, as any allocation does.
    #[must_use]
    pub fn new() -> Self {
        let tokens = Vec::with_capacity(usize_from_u32(TOKENS_MAX));
        assert!(
            tokens.capacity() >= usize_from_u32(TOKENS_MAX),
            "`with_capacity` gives at least the room it's asked for"
        );
        Self { tokens }
    }
}

impl Default for TokenBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// The tokens of a source that lexed, in the order they're in the source, and the source itself.
///
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
    #[must_use]
    pub const fn as_slice(&self) -> &'buffer [Token] {
        self.tokens
    }

    /// Returns `token`'s text, as it is in the source. The text outlives the buffer, since it's
    /// the source's own.
    ///
    /// # Panics
    ///
    /// Panics if `token` isn't in this source: one from another source can reach past this
    /// source's end or into the middle of a character. One that happens to fit gives some other
    /// text, which nothing here can tell.
    #[must_use]
    pub fn text(&self, token: &Token) -> &'source str {
        let source = self.text.as_str();
        let span = token.span();
        assert!(span.end <= source.len(), "a token is in its source");
        source.get(span).expect("a token starts and ends between characters")
    }
}

/// Turns `text` into tokens in `buffer`. Whatever the buffer held before is replaced.
///
/// # Errors
///
/// Returns the first problem in the source, when some character in its code is in no token and
/// isn't a space, or a name breaks its shape.
///
/// # Panics
///
/// Never, whatever the text. The assertions check what the text lint guarantees and the lexer's
/// own work, so one that fails is a bug in the compiler.
pub fn lex<'source, 'buffer>(
    text: Text<'source>,
    buffer: &'buffer mut TokenBuffer,
) -> Result<Tokens<'source, 'buffer>, Problem> {
    let source = text.as_str();
    // The text lint guarantees these, and the bounds below lean on them.
    assert!(
        source.len() <= usize_from_u32(SOURCE_BYTES_MAX),
        "the text lint refuses a larger source"
    );
    assert!(source.ends_with('\n'), "the text lint refuses a last line without its `\\n`");
    let capacity_before = buffer.tokens.capacity();
    buffer.tokens.clear();
    let mut line_start: usize = 0;
    for (content, number) in source.split_terminator('\n').zip(1_u32..) {
        lex_line(Line { content, number, start: line_start }, &mut buffer.tokens)?;
        line_start += content.len() + 1;
    }
    assert_eq!(line_start, source.len(), "every line was lexed");
    assert!(
        buffer.tokens.len() <= usize_from_u32(TOKENS_MAX),
        "every token is at least a byte, and the last byte is in none"
    );
    assert_eq!(buffer.tokens.capacity(), capacity_before, "the buffer never grows");
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

impl Line<'_> {
    /// Returns a token of `kind` whose text is the bytes at `span` in the line.
    fn token(self, kind: TokenKind, span: Range<usize>) -> Token {
        assert!(span.start < span.end, "a token is at least one byte");
        assert!(span.end <= self.content.len(), "a token is on its line");
        Token {
            kind,
            position: self.position(span.start),
            start_index: u32_from_usize(self.start + span.start),
            end_index: u32_from_usize(self.start + span.end),
        }
    }

    /// Returns the problem `kind`, at the byte at `index` in the line.
    fn problem(self, kind: Kind, index: usize) -> Problem {
        Problem { kind, position: self.position(index) }
    }

    /// Returns the position of the byte at `index` in the line. Everything before it on the line
    /// is ASCII, either code or indentation, so its column is one more than its index.
    fn position(self, index: usize) -> Position {
        assert!(index < self.content.len(), "the byte is on the line");
        Position::on_line(self.number).at_column(u32_from_usize(index) + 1)
    }
}

/// Lexes one line, adding its tokens to `tokens`, or returns the line's first problem.
fn lex_line(line: Line<'_>, tokens: &mut Vec<Token>) -> Result<(), Problem> {
    let bytes = line.content.as_bytes();
    assert!(!bytes.contains(&b'\n'), "a line's content stops before its `\\n`");
    let indentation = bytes.iter().take_while(|&&byte| byte == b' ').count();
    // A comment is a line that starts with `//` after its indentation, as the text lint says. Its
    // text can be any Unicode, and none of it is code, so it's one token.
    if bytes[indentation..].starts_with(b"//") {
        tokens.push(line.token(TokenKind::Comment, indentation..bytes.len()));
        return Ok(());
    }
    assert!(line.content.is_ascii(), "the text lint lets only ASCII into code");
    // In code, then, a byte is a character, and a column.
    let mut index = indentation;
    while index < bytes.len() {
        let (kind, length) = match bytes[index] {
            b' ' => (None, 1),
            b'(' => (Some(TokenKind::ParenthesisOpen), 1),
            b')' => (Some(TokenKind::ParenthesisClose), 1),
            b'{' => (Some(TokenKind::BraceOpen), 1),
            b'}' => (Some(TokenKind::BraceClose), 1),
            b'a'..=b'z' | b'A'..=b'Z' => {
                let length = word_length(&bytes[index..]);
                let word = &bytes[index..index + length];
                let kind = word_kind(word).map_err(|(kind, at)| line.problem(kind, index + at))?;
                (Some(kind), length)
            }
            byte => return Err(line.problem(Kind::Unexpected(char::from(byte)), index)),
        };
        assert!(length >= 1, "every step moves forward, so the loop ends within the line");
        if let Some(kind) = kind {
            tokens.push(line.token(kind, index..index + length));
        }
        index += length;
    }
    assert_eq!(index, bytes.len(), "the last token or space ends where the line does");
    Ok(())
}

/// Returns how many bytes long the word at the start of `bytes` is: a letter, then any letters,
/// digits and `_`.
fn word_length(bytes: &[u8]) -> usize {
    let in_word = |byte: &&u8| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_');
    let length = bytes.iter().take_while(in_word).count();
    assert!(length >= 1, "a word is lexed where a letter is, and a letter is in a word");
    assert!(bytes[0].is_ascii_alphabetic(), "a word starts with a letter");
    length
}

/// Returns what `word` is: a keyword, a name or a type's name. A name that breaks its shape is a
/// problem instead: its kind, and the index in the word of the first byte that breaks it.
fn word_kind(word: &[u8]) -> Result<TokenKind, (Kind, usize)> {
    if let Some((kind, at)) = broken_shape(word) {
        assert!(at < word.len(), "a shape breaks at one of the word's own bytes");
        return Err((kind, at));
    }
    let kind = match word {
        b"function" => TokenKind::Function,
        b"returns" => TokenKind::Returns,
        [b'A'..=b'Z', ..] => TokenKind::TypeName,
        [b'a'..=b'z', ..] => TokenKind::Name,
        _ => unreachable!("a word starts with a letter"),
    };
    Ok(kind)
}

/// Returns where `word` first breaks its shape, and the kind of problem that is, or `None` when
/// it keeps it. A word that starts with a lowercase letter is a name, in snake case, and one that
/// starts with a capital is a type's name, in Pascal case.
fn broken_shape(word: &[u8]) -> Option<(Kind, usize)> {
    assert!(!word.is_empty(), "a word is at least its first letter");
    assert!(word[0].is_ascii_alphabetic(), "a word starts with a letter");
    // Only a capital or `_` can break a shape, and the first byte is a letter, which is the start
    // of its shape either way. So whatever breaks it has a byte before it.
    if word[0].is_ascii_lowercase() {
        let broken = (1..word.len()).find(|&index| breaks_snake_case(word, index));
        broken.map(|index| (Kind::NameNotSnakeCase, index))
    } else {
        let broken = (1..word.len()).find(|&index| breaks_pascal_case(word, index));
        broken.map(|index| (Kind::TypeNameNotPascalCase, index))
    }
}

/// Says whether the byte at `index` in a name breaks snake case: lowercase letters and digits,
/// with single underscores between them. A capital breaks it, and so does an `_` after another,
/// or at the end.
fn breaks_snake_case(word: &[u8], index: usize) -> bool {
    assert!(index > 0, "a name's first byte is a lowercase letter, which keeps its shape");
    assert!(index < word.len(), "the byte is in the word");
    match word[index] {
        b'A'..=b'Z' => true,
        b'_' if word[index - 1] == b'_' => true,
        b'_' => index == word.len() - 1,
        b'a'..=b'z' | b'0'..=b'9' => false,
        other => unreachable!("a word holds only letters, digits and `_`, not byte {other}"),
    }
}

/// Says whether the byte at `index` in a type's name breaks Pascal case: words that each start
/// with one capital, then lowercase letters and digits. An `_` breaks it, and so does a capital
/// after another.
fn breaks_pascal_case(word: &[u8], index: usize) -> bool {
    assert!(index > 0, "a type's name's first byte is a capital, which keeps its shape");
    assert!(index < word.len(), "the byte is in the word");
    match word[index] {
        b'_' => true,
        b'A'..=b'Z' => word[index - 1].is_ascii_uppercase(),
        b'a'..=b'z' | b'0'..=b'9' => false,
        other => unreachable!("a word holds only letters, digits and `_`, not byte {other}"),
    }
}
