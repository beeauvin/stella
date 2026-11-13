// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for messages. They're a crate of their own, so the compiler lets them reach only
//! what the crate makes public: a path, a place and a sentence in, the text the compiler prints
//! out.
//!
//! Every code point is shown, against an oracle that words the rule as the compiler page does,
//! and every code point is put before a problem to check the caret still lands under it. The rest
//! is every byte that isn't UTF-8, two tables of whole messages, and the two positions that are
//! a bug.

#![forbid(unsafe_code)]

use stella_cli::{Place, message, shown};
use stella_linter_path::Problem::NameNotKebab;
use stella_linter_text::Kind::{
    BlankLineAtEnd, CarriageReturn, CommentSharesLine, NoFinalNewline, NotUtf8, Tab, TrailingSpace,
};
use stella_linter_text::Problem::FileTooBig;
use stella_source::Position;

/// Returns the place at `line` and `column` in `source`.
// KS-DISTINCT-ARGS: a line then a column is the order every message prints them in, and an
// expectation with the two swapped fails its test.
const fn at(source: &[u8], line: u32, column: u32) -> Place<'_> {
    Place::At { source, position: Position::on_line(line).at_column(column) }
}

/// Returns how `character` should be shown, worked out the way the compiler page words it.
fn shown_oracle(character: char) -> String {
    match character {
        '\t' => String::from("\\t"),
        '\r' => String::from("\\r"),
        ' '..='~' => character.to_string(),
        _ => format!("\\u{{{:X}}}", u32::from(character)),
    }
}

#[test]
fn every_code_point_is_shown_safely() {
    // Each code point, alone, is shown as the oracle says: as itself, or as an escape.
    for character in char::MIN..=char::MAX {
        let mut encoded = [0; 4];
        let bytes = character.encode_utf8(&mut encoded).as_bytes();
        assert_eq!(shown(bytes), shown_oracle(character), "for {character:?}");
    }
}

#[test]
fn every_byte_that_isnt_utf8_is_shown_safely() {
    // Alone, every byte from 0x80 up is invalid.
    for byte in 0x80..=0xFF_u8 {
        assert_eq!(shown(&[byte]), format!("\\x{byte:02X}"), "for {byte:#04X}");
    }
    // A sequence cut short shows each of its bytes.
    assert_eq!(shown(b"a\xE2\x82b"), "a\\xE2\\x82b");
}

#[test]
fn the_caret_lands_after_every_code_point() {
    // Each code point is put before a trailing space on a line. However wide it's shown, the
    // caret must still land under the space.
    for character in char::MIN..=char::MAX {
        // `\n` would end the line.
        if character == '\n' {
            continue;
        }
        let source = format!("// a{character}b \n");
        let place = at(source.as_bytes(), 1, 7);
        let echo = format!("// a{}b ", shown_oracle(character));
        // The caret goes after `// a`, the character as it's shown, and `b`.
        let caret = " ".repeat(5 + shown_oracle(character).len());
        let expected =
            format!("x.stella:1:7: a line can't end in a space.\n    {echo}\n    {caret}^\n");
        assert_eq!(message(b"x.stella", place, TrailingSpace), expected, "for {character:?}");
    }
}

#[test]
fn whole_file_messages() {
    // A problem with the file as a whole is one line: the path, shown safely, then the sentence.
    for (path, sentence, expected) in [
        (
            &b"Hello.stella"[..],
            NameNotKebab.to_string(),
            "Hello.stella: a source file's name, before `.stella`, must be lowercase letters and \
             digits joined by single hyphens, like `hello-world`.\n",
        ),
        (
            b"big.stella",
            FileTooBig.to_string(),
            "big.stella: this file is over 16,384 bytes, the most a source file can hold.\n",
        ),
        (
            b"a\xFF\n\xE2\x80\xAEb.stella",
            NameNotKebab.to_string(),
            "a\\xFF\\u{A}\\u{202E}b.stella: a source file's name, before `.stella`, must be \
             lowercase letters and digits joined by single hyphens, like `hello-world`.\n",
        ),
    ] {
        assert_eq!(message(path, Place::File, &sentence), expected, "for {sentence:?}");
    }
}

#[test]
fn messages() {
    // A problem at a position shows its line, and a caret under its column.
    for (source, (kind, line, column), expected) in [
        (
            &b"// a\n// b\nfunction main() returns Void { // hi\n}\n"[..],
            (CommentSharesLine, 3, 32),
            "x.stella:3:32: a comment can't share a line with code.\n    \
             function main() returns Void { // hi\n                                   ^\n",
        ),
        (
            b"function main() returns Void {\n\t// x\n}\n",
            (Tab, 2, 1),
            "x.stella:2:1: a tab can't appear in a source file; indent with four spaces.\n    \
             \\t// x\n    ^\n",
        ),
        // The caret counts an escape at its shown width.
        (
            "// é\t\n".as_bytes(),
            (Tab, 1, 5),
            "x.stella:1:5: a tab can't appear in a source file; indent with four spaces.\n    \
             // \\u{E9}\\t\n             ^\n",
        ),
        (
            b"function main() returns Void {\r\n}\n",
            (CarriageReturn, 1, 31),
            "x.stella:1:31: `\\r` can't appear in a source file; lines end with `\\n` alone.\n    \
             function main() returns Void {\\r\n                                  ^\n",
        ),
        (
            b"// a\xFFb\n",
            (NotUtf8, 1, 5),
            "x.stella:1:5: this isn't valid UTF-8.\n    // a\\xFFb\n        ^\n",
        ),
        (
            b"// \xE2\x82\n",
            (NotUtf8, 1, 4),
            "x.stella:1:4: this isn't valid UTF-8.\n    // \\xE2\\x82\n       ^\n",
        ),
        // A caret can sit just past the end of the line, and a line can be empty.
        (
            b"}",
            (NoFinalNewline, 1, 2),
            "x.stella:1:2: the file must end with a newline.\n    }\n     ^\n",
        ),
        (
            b"}\n\n",
            (BlankLineAtEnd, 2, 1),
            "x.stella:2:1: a file can't end with a blank line.\n    \n    ^\n",
        ),
    ] {
        let place = at(source, line, column);
        assert_eq!(message(b"x.stella", place, kind), expected, "for {kind:?}");
    }
}

#[test]
#[should_panic(expected = "a problem's line is in its source")]
fn a_position_past_the_last_line_is_a_bug() {
    // A source of one line has no third line to show.
    let _: String = message(b"x.stella", at(b"}", 3, 1), Tab);
}

#[test]
#[should_panic(expected = "a problem's column is in its line or just past it")]
fn a_position_past_the_end_of_its_line_is_a_bug() {
    // The line is one character, so column 2 is just past it, and column 3 is nowhere.
    let _: String = message(b"x.stella", at(b"}", 1, 3), Tab);
}
