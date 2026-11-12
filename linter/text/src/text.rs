// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The text lint checks every rule about a source file's bytes, before the lexer sees them.
//!
//! The steps run in a fixed order, each leaving the next less to worry about. The first step that
//! fails is the one reported, and within a step, the earliest problem in the file:
//!
//! 1. The size: at least one byte, and at most `SOURCE_BYTES_MAX`.
//! 2. The characters, in one pass: valid UTF-8, with no U+FEFF, `\r`, tab, other control
//!    character, `Bidi_Control` character, U+2028 or U+2029.
//! 3. The lines: the file ends with exactly one `\n` and no empty line before it, no line ends
//!    in a space, and indentation, the spaces a line starts with, is a multiple of four. A line
//!    of only spaces is a trailing space at its first column, not a blank line.
//! 4. The comments: a line that starts with `//` after its indentation is a comment, and its
//!    `//` must then be followed by a space or the end of the line. Every other line is code,
//!    and code never holds `//`.
//! 5. ASCII: every character on a line of code is ASCII.

use stella_source::{Position, SOURCE_BYTES_MAX, u32_from_usize, usize_from_u32};

use crate::problem::{Kind, Problem};

/// A level of indentation is four spaces, for now. The tokens lint indents one level for each `{`
/// still open on a line, so this is its level too.
pub const LEVEL_SPACES: u32 = 4;

/// A text is source that passed every step of the text lint. Only `lint` makes one, so holding a
/// `Text` proves the source passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Text<'source> {
    source: &'source str,
}

impl<'source> Text<'source> {
    /// Returns the source, unchanged.
    #[must_use]
    pub const fn as_str(&self) -> &'source str {
        self.source
    }
}

/// Checks a source file's bytes, step by step, and stops at the first step that fails.
///
/// # Errors
///
/// Returns the earliest problem in the first step that fails.
///
/// # Panics
///
/// Never, whatever the bytes. The assertions check the lint's own work, so one that fails is a
/// bug in the compiler.
pub fn lint(bytes: &[u8]) -> Result<Text<'_>, Problem> {
    lint_size(bytes)?;
    let source = lint_characters(bytes)?;
    lint_lines(source)?;
    lint_comments(source)?;
    lint_ascii(source)?;
    // The lexer relies on what the steps found, so the plainest of it is checked on the way out.
    assert!(
        source.len() <= usize_from_u32(SOURCE_BYTES_MAX),
        "the size step refused anything larger"
    );
    assert!(source.ends_with('\n'), "the lines step refused a last line without its `\\n`");
    Ok(Text { source })
}

/// The first step: the file has at least one byte, and at most `SOURCE_BYTES_MAX`.
const fn lint_size(bytes: &[u8]) -> Result<(), Problem> {
    if bytes.is_empty() {
        Err(Problem::FileEmpty)
    } else if bytes.len() <= usize_from_u32(SOURCE_BYTES_MAX) {
        Ok(())
    } else {
        Err(Problem::FileTooBig)
    }
}

/// The second step: the bytes are UTF-8, and every character in them may appear in a source file.
/// Whichever problem comes first is the one reported, whatever its kind. Returns the bytes as
/// text.
fn lint_characters(bytes: &[u8]) -> Result<&str, Problem> {
    assert!(!bytes.is_empty(), "the size step refused an empty file");
    assert!(
        bytes.len() <= usize_from_u32(SOURCE_BYTES_MAX),
        "the size step refused anything larger"
    );
    // The characters before the first invalid byte come before it, so a refused one among them
    // is the earlier problem.
    let valid = bytes.utf8_chunks().next().expect("bytes that aren't empty have a chunk").valid();
    let mut line: u32 = 1;
    let mut column: u32 = 1;
    for character in valid.chars() {
        if let Some(kind) = character_kind(character) {
            return Err(Problem::at(kind, Position::on_line(line).at_column(column)));
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    if valid.len() == bytes.len() {
        Ok(valid)
    } else {
        Err(Problem::at(Kind::NotUtf8, Position::on_line(line).at_column(column)))
    }
}

/// Returns the kind of problem `character` is, wherever it appears, or `None` when a source file
/// may hold it.
fn character_kind(character: char) -> Option<Kind> {
    match character {
        '\n' => None,
        '\t' => Some(Kind::Tab),
        '\r' => Some(Kind::CarriageReturn),
        '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}' => {
            assert!(character.is_control(), "these two ranges are Unicode's control characters");
            Some(Kind::ControlCharacter(character))
        }
        // These are Unicode's twelve `Bidi_Control` characters.
        '\u{061C}'
        | '\u{200E}'
        | '\u{200F}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2066}'..='\u{2069}' => Some(Kind::BidiControl(character)),
        '\u{2028}' | '\u{2029}' => Some(Kind::LineSeparator(character)),
        '\u{FEFF}' => Some(Kind::ByteOrderMark),
        _ => {
            assert!(
                !character.is_control(),
                "everything Unicode calls a control character has a kind above, or is `\\n`"
            );
            None
        }
    }
}

/// The third step: no line ends in a space, every line's indentation is a multiple of four
/// spaces, and the file ends with exactly one `\n`, with no empty line before it.
fn lint_lines(source: &str) -> Result<(), Problem> {
    assert!(!source.is_empty(), "the size step refused an empty file");
    // This is the first of the empty lines the file ends with so far, if it ends with any.
    let mut blank_line_first: Option<u32> = None;
    for (piece, line) in source.split_inclusive('\n').zip(1_u32..) {
        assert!(line <= SOURCE_BYTES_MAX, "every line before this one ended with its own `\\n`");
        let content = piece.strip_suffix('\n').unwrap_or(piece);
        // A line of only spaces trims to nothing, so it has no indentation, and its first column
        // starts a trailing space.
        let trimmed = content.trim_end_matches(' ');
        let indentation = trimmed.len() - trimmed.trim_start_matches(' ').len();
        if !indentation.is_multiple_of(usize_from_u32(LEVEL_SPACES)) {
            return Err(Problem::at(
                Kind::IndentationNotFour,
                Position::on_line(line).at_column(1),
            ));
        }
        if trimmed.len() < content.len() {
            return Err(Problem::at(
                Kind::TrailingSpace,
                Position::on_line(line).at_column(column_after(trimmed)),
            ));
        }
        if piece.ends_with('\n') {
            if content.is_empty() {
                blank_line_first = blank_line_first.or(Some(line));
            } else {
                blank_line_first = None;
            }
        } else {
            // Only the file's last piece can be missing its `\n`.
            return Err(Problem::at(
                Kind::NoFinalNewline,
                Position::on_line(line).at_column(column_after(content)),
            ));
        }
    }
    match blank_line_first {
        Some(line) => Err(Problem::at(Kind::BlankLineAtEnd, Position::on_line(line).at_column(1))),
        None => Ok(()),
    }
}

/// The fourth step: a comment's `//` is followed by a space or the end of its line, and a line of
/// code never holds `//`.
fn lint_comments(source: &str) -> Result<(), Problem> {
    assert!(source.ends_with('\n'), "the lines step refused a last line without its `\\n`");
    for (content, line) in source.split_terminator('\n').zip(1_u32..) {
        assert!(line <= SOURCE_BYTES_MAX, "every line ends with its own `\\n`");
        // This is the kind of problem the line has, if any, and what comes before it on the line.
        let problem = match comment_text(content) {
            Some(comment) => match comment.chars().next() {
                None | Some(' ') => None,
                Some(_) => {
                    let before = content.strip_suffix(comment);
                    let before = before.expect("a comment's text runs to the end of its line");
                    Some((Kind::CommentNeedsSpace, before))
                }
            },
            None => content.split_once("//").map(|(before, _)| (Kind::CommentSharesLine, before)),
        };
        if let Some((kind, before)) = problem {
            return Err(Problem::at(kind, Position::on_line(line).at_column(column_after(before))));
        }
    }
    Ok(())
}

/// The last step: every character on a line of code is ASCII.
fn lint_ascii(source: &str) -> Result<(), Problem> {
    assert!(source.ends_with('\n'), "the lines step refused a last line without its `\\n`");
    for (content, line) in source.split_terminator('\n').zip(1_u32..) {
        assert!(line <= SOURCE_BYTES_MAX, "every line ends with its own `\\n`");
        if comment_text(content).is_some() {
            continue;
        }
        // A column counts code points, so the first one that isn't ASCII is at one more than the
        // number before it.
        let found = content.chars().zip(1_u32..).find(|&(character, _)| !character.is_ascii());
        if let Some((character, column)) = found {
            return Err(Problem::at(
                Kind::NotAscii(character),
                Position::on_line(line).at_column(column),
            ));
        }
    }
    Ok(())
}

/// Returns a comment's text, everything after its `//`, or `None` when the line is code. A line
/// is a comment when it starts with `//` after its indentation.
fn comment_text(content: &str) -> Option<&str> {
    assert!(!content.contains('\n'), "a line's content stops before its `\\n`");
    content.trim_start_matches(' ').strip_prefix("//")
}

/// Returns the column just after `prefix`, which starts a line: one more than its code points.
fn column_after(prefix: &str) -> u32 {
    assert!(!prefix.contains('\n'), "a line's content stops before its `\\n`");
    u32_from_usize(prefix.chars().count()) + 1
}
