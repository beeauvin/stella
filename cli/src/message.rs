// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A message is what the compiler prints on stderr when it refuses a program.
//!
//! A problem with the file as a whole is one line: the path, then the sentence. A problem at a
//! position is three: the path, line and column, then the sentence; the line itself, indented
//! four spaces; and a caret under the problem's column.
//!
//! The path and the line are shown safely. Every character outside printable ASCII, U+0020 to
//! U+007E, is shown as an escape: `\t`, `\r`, or `\u{E9}` for any other, and a byte that isn't
//! UTF-8 is shown as `\xFF`. So nothing unsafe reaches the terminal, and since everything shown
//! is ASCII, the caret is always exactly under the problem.

use std::fmt::{self, Write as _};

use stella_source::{Position, usize_from_u32};

/// A place is where a problem is: the file as a whole, or one position in its source.
///
/// A position comes with the source it's in, since its line is looked up there to be shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place<'source> {
    /// The problem is with the file as a whole, such as its name or its size.
    File,
    /// The problem is at one position: a character, or just past the end of a line.
    At {
        /// The source the position is in.
        source: &'source [u8],
        /// The position, by line and column.
        position: Position,
    },
}

/// Returns the message for a problem at `place` in the file at `path`, which `sentence` says.
/// Every line of it ends in `\n`.
///
/// # Panics
///
/// Panics if a position isn't in the source it comes with: if the source has no such line, or
/// the column is past the end of the line by more than one. A check only reports positions in
/// the source it was given, so that's a bug in the compiler.
#[must_use]
pub fn message(path: &[u8], place: Place<'_>, sentence: impl fmt::Display) -> String {
    let path = shown(path);
    let text = match place {
        Place::File => format!("{path}: {sentence}\n"),
        Place::At { source, position } => {
            let line_index = usize_from_u32(position.line() - 1);
            let source_line = source
                .split(|&byte| byte == b'\n')
                .nth(line_index)
                .expect("a problem's line is in its source");
            let (echo, caret_width) = echo(source_line, position.column());
            let caret = " ".repeat(caret_width);
            let (line, column) = (position.line(), position.column());
            format!("{path}:{line}:{column}: {sentence}\n    {echo}\n    {caret}^\n")
        }
    };
    assert!(text.ends_with('\n'), "every line of a message ends in `\\n`");
    text
}

/// Returns `bytes` shown safely: printable ASCII as itself, and everything else as an escape.
///
/// # Panics
///
/// Never, whatever the bytes. The assertion checks this function's own work.
#[must_use]
pub fn shown(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for unit in units(bytes) {
        push_shown(&mut text, unit);
    }
    assert!(text.is_ascii(), "printable ASCII and escapes are all ASCII");
    text
}

/// Returns `line` shown safely, and how far into it the caret goes to sit under `column`.
fn echo(line: &[u8], column: u32) -> (String, usize) {
    let mut echo = String::with_capacity(line.len());
    let mut caret = None;
    let mut units_count: u32 = 0;
    for unit in units(line) {
        units_count += 1;
        if units_count == column {
            caret = Some(echo.len());
        }
        push_shown(&mut echo, unit);
    }
    // Past every unit is one column more, just past the end of the line.
    let caret = caret.unwrap_or_else(|| {
        assert_eq!(column, units_count + 1, "a problem's column is in its line or just past it");
        echo.len()
    });
    assert!(caret <= echo.len(), "the caret is under the line, or just past its end");
    (echo, caret)
}

/// A unit is one column's worth of bytes: a character, or a byte that isn't UTF-8.
#[derive(Clone, Copy)]
enum Unit {
    Character(char),
    Byte(u8),
}

/// Returns the units of `bytes`, in order.
fn units(bytes: &[u8]) -> impl Iterator<Item = Unit> + '_ {
    bytes.utf8_chunks().flat_map(|chunk| {
        let characters = chunk.valid().chars().map(Unit::Character);
        characters.chain(chunk.invalid().iter().map(|&byte| Unit::Byte(byte)))
    })
}

/// Adds `unit` to `text`, shown safely: printable ASCII as itself, and the rest as an escape.
fn push_shown(text: &mut String, unit: Unit) {
    let written = match unit {
        Unit::Character('\t') => write!(text, "\\t"),
        Unit::Character('\r') => write!(text, "\\r"),
        Unit::Character(character @ ' '..='~') => write!(text, "{character}"),
        Unit::Character(character) => write!(text, "\\u{{{:X}}}", u32::from(character)),
        Unit::Byte(byte) => write!(text, "\\x{byte:02X}"),
    };
    written.expect("writing to a string can't fail");
}

/// Returns why `error` happened, shown safely like anything else that reaches the terminal.
pub(crate) fn reason(error: &std::io::Error) -> String {
    let reason = shown(error.to_string().as_bytes());
    assert!(!reason.is_empty(), "every error says something");
    reason
}
