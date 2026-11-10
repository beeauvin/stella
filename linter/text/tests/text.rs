// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the text lint. They're a crate of their own, so the compiler lets them reach
//! only what the crate makes public: bytes in, a `Text` or a `Problem` out.
//!
//! Every code point is tried, in a comment and in code, against the source text page's table.
//! The rest are tables, one per step: a source, and the problem it should get. The last table
//! checks the steps run in order, so an earlier step's problem wins even when a later step's
//! comes first in the file.

use stella_linter_text::Kind::*;
use stella_linter_text::{Kind, Place, Position, Problem, SOURCE_BYTES_MAX, lint};

/// Lints `source`, which must pass, and checks the `Text` holds it unchanged.
fn accepts(source: &str) {
    let text = lint(source.as_bytes()).expect("source should pass the text lint");
    assert_eq!(text.as_str(), source);
}

/// Lints `bytes`, which must be refused with `problem`.
fn refuses(bytes: &[u8], problem: Problem) {
    let found = lint(bytes).expect_err("source should be refused");
    assert_eq!(found, problem, "for {:?}", String::from_utf8_lossy(bytes));
}

/// Returns a problem at `line` and `column`.
fn at(kind: Kind, line: u32, column: u32) -> Problem {
    Problem { kind, place: Place::At(Position { line, column }) }
}

/// Returns a problem with the file as a whole.
fn whole_file(kind: Kind) -> Problem {
    Problem { kind, place: Place::File }
}

/// Returns a source of exactly `size` bytes: one long comment line.
fn source_of_size(size: u32) -> String {
    let source = format!("// {}\n", "x".repeat(size as usize - "// \n".len()));
    assert_eq!(source.len(), size as usize);
    source
}

/// Returns the problem the characters step finds in `character`, worked out the way the source
/// text page words it, or `None` when the character is fine in a comment. It isn't asked about
/// `\n`, which ends a line.
fn character_problem(character: char) -> Option<Kind> {
    assert_ne!(character, '\n');
    match character {
        '\t' => Some(Tab),
        '\r' => Some(CarriageReturn),
        '\u{0}'..='\u{1F}' | '\u{7F}'..='\u{9F}' => Some(ControlCharacter(character)),
        '\u{061C}'
        | '\u{200E}'
        | '\u{200F}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2066}'..='\u{2069}' => Some(BidiControl(character)),
        '\u{2028}' | '\u{2029}' => Some(LineSeparator(character)),
        '\u{FEFF}' => Some(ByteOrderMark),
        _ => None,
    }
}

#[test]
fn every_code_point_in_a_comment() {
    for character in char::MIN..=char::MAX {
        if character == '\n' {
            continue;
        }
        let source = format!("// a{character}b\n");
        match character_problem(character) {
            Some(kind) => refuses(source.as_bytes(), at(kind, 1, 5)),
            None => accepts(&source),
        }
    }
}

#[test]
fn every_code_point_in_code() {
    for character in char::MIN..=char::MAX {
        if character == '\n' {
            continue;
        }
        let source = format!("a{character}b\n");
        match character_problem(character) {
            Some(kind) => refuses(source.as_bytes(), at(kind, 1, 2)),
            None if character.is_ascii() => accepts(&source),
            None => refuses(source.as_bytes(), at(NotAscii(character), 1, 2)),
        }
    }
}

#[test]
fn accepted_sources() {
    for source in [
        "function main() returns Void {\n}\n",
        "// above\nfunction main() returns Void {\n    // inside\n}\n// below\n",
        "//\nfunction main() returns Void {\n    //\n}\n",
        "\n// a\n\nfunction main() returns Void {\n\n    // b\n\n}\n",
        "// café, 日本語, and a flag: \u{1F3F3}\u{FE0F}\u{200D}\u{1F308}\n",
        "// punctuation: ~ / // /// //! { } ( )\n",
        "function main() returns Void {\n    // café\n}\n",
        "function main() returns Void {\n        // eight\n            // twelve\n}\n",
        // The code itself is the lexer's to check.
        "functoin mian() returns Viod {\n}\n",
        "function main() returns Void { /\n}\n",
    ] {
        accepts(source);
    }
    accepts(&source_of_size(SOURCE_BYTES_MAX));
}

#[test]
fn size() {
    refuses(b"", whole_file(FileEmpty));
    let one_more = source_of_size(SOURCE_BYTES_MAX + 1);
    refuses(one_more.as_bytes(), whole_file(FileTooBig));
    // At the limit, every problem is still found and placed: on the last line a source can have,
    // just past the end of the longest line, or in a source of nothing but blank lines.
    let newlines = "\n".repeat(SOURCE_BYTES_MAX as usize - 1);
    refuses(format!("{newlines}\t").as_bytes(), at(Tab, SOURCE_BYTES_MAX, 1));
    refuses(&[newlines.as_bytes(), b"\xFF"].concat(), at(NotUtf8, SOURCE_BYTES_MAX, 1));
    refuses(format!("{newlines}\n").as_bytes(), at(BlankLineAtEnd, 1, 1));
    let longest = "x".repeat(SOURCE_BYTES_MAX as usize);
    refuses(longest.as_bytes(), at(NoFinalNewline, 1, SOURCE_BYTES_MAX + 1));
}

#[test]
fn characters() {
    for (source, problem) in [
        ("\u{FEFF}function main() returns Void {\n}\n", at(ByteOrderMark, 1, 1)),
        ("function main() returns Void {\r\n}\n", at(CarriageReturn, 1, 31)),
        ("function main() returns Void {\n\t// x\n}\n", at(Tab, 2, 1)),
        ("// one\u{2028}two\n", at(LineSeparator('\u{2028}'), 1, 7)),
        // Columns count code points: é is one, and the flag is four.
        ("// é\t\n", at(Tab, 1, 5)),
        ("// \u{1F3F3}\u{FE0F}\u{200D}\u{1F308}\t\n", at(Tab, 1, 8)),
        // The earliest problem wins, whatever its kind.
        ("// one\n// t\rwo\n\t// three\n", at(CarriageReturn, 2, 5)),
        ("// one\n\t// two\n// t\rhree\n", at(Tab, 2, 1)),
        ("// a\u{202E}\n// b\u{7}\n", at(BidiControl('\u{202E}'), 1, 5)),
        ("// a\u{7}\n// b\u{202E}\n", at(ControlCharacter('\u{7}'), 1, 5)),
    ] {
        refuses(source.as_bytes(), problem);
    }
}

#[test]
fn invalid_utf8() {
    for (bytes, problem) in [
        (&b"// a\xFFb\n"[..], at(NotUtf8, 1, 5)),
        (b"// \xC3\xA9\xFF\n", at(NotUtf8, 1, 5)),
        (b"// \xE2\x82\n", at(NotUtf8, 1, 4)),
        (b"// \xC0\xAF\n", at(NotUtf8, 1, 4)),
        (b"// \xED\xA0\x80\n", at(NotUtf8, 1, 4)),
        (b"// \xF4\x90\x80\x80\n", at(NotUtf8, 1, 4)),
        (b"// \x80\n", at(NotUtf8, 1, 4)),
        (b"function main() returns Void {\n    // \xFF\n}\n", at(NotUtf8, 2, 8)),
        (b"}\n\xE2\x82", at(NotUtf8, 2, 1)),
        // Bytes that aren't UTF-8 are part of the characters step, so the earliest problem wins.
        (b"\t// \xFF\n", at(Tab, 1, 1)),
        (b"// \xFF\t\n", at(NotUtf8, 1, 4)),
    ] {
        refuses(bytes, problem);
    }
}

#[test]
fn lines() {
    // Every indentation up to five levels: only whole levels of four spaces pass.
    for spaces in 0..=20 {
        let source = format!("function main() returns Void {{\n{}// x\n}}\n", " ".repeat(spaces));
        if spaces % 4 == 0 {
            accepts(&source);
        } else {
            refuses(source.as_bytes(), at(IndentationNotFour, 2, 1));
        }
    }
    for (source, problem) in [
        ("function main() returns Void {\n}", at(NoFinalNewline, 2, 2)),
        ("}", at(NoFinalNewline, 1, 2)),
        ("// é", at(NoFinalNewline, 1, 5)),
        ("}\n\n", at(BlankLineAtEnd, 2, 1)),
        ("}\n\n\n", at(BlankLineAtEnd, 2, 1)),
        ("\n", at(BlankLineAtEnd, 1, 1)),
        ("function main() returns Void { \n}\n", at(TrailingSpace, 1, 31)),
        ("function main() returns Void {   \n}\n", at(TrailingSpace, 1, 31)),
        ("// é \n", at(TrailingSpace, 1, 5)),
        // A line of only spaces is a trailing space, never a blank line.
        ("function main() returns Void {\n    \n}\n", at(TrailingSpace, 2, 1)),
        ("function main() returns Void {\n  \n}\n", at(TrailingSpace, 2, 1)),
        ("}\n    \n", at(TrailingSpace, 2, 1)),
        ("function main() returns Void {\n  }\n", at(IndentationNotFour, 2, 1)),
        // The last line is checked like any other, even when it has no `\n`.
        ("} ", at(TrailingSpace, 1, 2)),
        (" }", at(IndentationNotFour, 1, 1)),
        ("}\n  ", at(TrailingSpace, 2, 1)),
        // The earliest problem wins, the end of the file included.
        ("function main() returns Void {\n  // x \n}\n", at(IndentationNotFour, 2, 1)),
        ("function main() returns Void { \n  // x\n}\n", at(TrailingSpace, 1, 31)),
        ("function main() returns Void { \n}", at(TrailingSpace, 1, 31)),
        ("function main() returns Void {\n  // x\n}\n\n", at(IndentationNotFour, 2, 1)),
    ] {
        refuses(source.as_bytes(), problem);
    }
}

#[test]
fn comments() {
    for (source, problem) in [
        ("//x\n", at(CommentNeedsSpace, 1, 3)),
        ("//\u{A0}x\n", at(CommentNeedsSpace, 1, 3)),
        ("/// x\n", at(CommentNeedsSpace, 1, 3)),
        ("//! x\n", at(CommentNeedsSpace, 1, 3)),
        ("////\n", at(CommentNeedsSpace, 1, 3)),
        ("function main() returns Void {\n    //x\n}\n", at(CommentNeedsSpace, 2, 7)),
        ("function main() returns Void { // hi\n}\n", at(CommentSharesLine, 1, 32)),
        ("function main() returns Void {//\n}\n", at(CommentSharesLine, 1, 31)),
        ("function main() returns Void {\n} // end\n", at(CommentSharesLine, 2, 3)),
        // Only spaces indent, so a no-break space before `//` makes the line code.
        ("\u{A0}// x\n", at(CommentSharesLine, 1, 2)),
        // The earliest problem wins.
        ("//x\nfunction main() returns Void { // hi\n}\n", at(CommentNeedsSpace, 1, 3)),
        ("function main() returns Void { // hi\n//x\n}\n", at(CommentSharesLine, 1, 32)),
        ("function main() returns Void { // a // b\n}\n", at(CommentSharesLine, 1, 32)),
    ] {
        refuses(source.as_bytes(), problem);
    }
}

#[test]
fn ascii() {
    for (source, problem) in [
        ("function maín() returns Void {\n}\n", at(NotAscii('í'), 1, 12)),
        ("// a\nfunction maín() returns Void {\n}\n", at(NotAscii('í'), 2, 12)),
        ("function main() returns Void {\n}\u{A0}\n", at(NotAscii('\u{A0}'), 2, 2)),
        // The earliest problem wins.
        ("aé\u{A0}\n", at(NotAscii('é'), 1, 2)),
        ("é\nñ\n", at(NotAscii('é'), 1, 1)),
    ] {
        refuses(source.as_bytes(), problem);
    }
}

#[test]
fn the_steps_run_in_order() {
    refuses(&[0xFF; SOURCE_BYTES_MAX as usize + 1], whole_file(FileTooBig));
    for (source, problem) in [
        ("function main() returns Void { \n\t// x\n}\n", at(Tab, 2, 1)),
        ("//x \n", at(TrailingSpace, 1, 4)),
        ("function maín() returns Void { // hi\n}\n", at(CommentSharesLine, 1, 32)),
    ] {
        refuses(source.as_bytes(), problem);
    }
}
