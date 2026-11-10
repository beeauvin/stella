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

use std::fmt::Write;

use stella_linter_text::{Place, Problem};

/// Returns the message for `problem`, found in `source`, the file at `path`. Every line of it
/// ends in `\n`. A problem at a position must be one the lint found in `source`, since its line
/// is looked up there; one that isn't is a programmer's error, and panics.
pub fn message(path: &[u8], source: &[u8], problem: Problem) -> String {
    let path = shown(path);
    let text = match problem.place {
        Place::File => format!("{path}: {}\n", problem.kind),
        Place::At(position) => {
            assert!(position.line >= 1);
            let source_line = source
                .split(|&byte| byte == b'\n')
                .nth(position.line as usize - 1)
                .expect("the problem's line is in the source");
            let (echo, caret_width) = echo(source_line, position.column);
            let caret = " ".repeat(caret_width);
            let (line, column) = (position.line, position.column);
            format!("{path}:{line}:{column}: {}\n    {echo}\n    {caret}^\n", problem.kind)
        }
    };
    assert!(text.ends_with('\n'));
    text
}

/// Returns `bytes` shown safely, as the module doc says.
pub fn shown(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for unit in units(bytes) {
        push_shown(&mut text, unit);
    }
    assert!(text.is_ascii());
    text
}

/// Returns `line` shown safely, and how far into it the caret goes to sit under `column`.
fn echo(line: &[u8], column: u32) -> (String, usize) {
    assert!(column >= 1);
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
        assert_eq!(column, units_count + 1, "the column is in the line or just past it");
        echo.len()
    });
    assert!(caret <= echo.len());
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
