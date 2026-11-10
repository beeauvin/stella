// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for messages. They're a crate of their own, so the compiler lets them reach only
//! what the crate makes public: a path, a source and a problem in, the text the compiler prints
//! out.
//!
//! Every code point is shown, against an oracle that words the rule as the compiler page does,
//! and every code point is put before a problem to check the caret still lands under it. The rest
//! is one table of whole messages.

use stella_linter_text::Kind::*;
use stella_linter_text::{Kind, Place, Position, Problem};

use stella_cli::{message, shown};

/// Returns a problem at `line` and `column`.
fn at(kind: Kind, line: u32, column: u32) -> Problem {
    Problem { kind, place: Place::At(Position { line, column }) }
}

/// Returns a problem with the file as a whole.
fn whole_file(kind: Kind) -> Problem {
    Problem { kind, place: Place::File }
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
    for character in char::MIN..=char::MAX {
        // `\n` would end the line.
        if character == '\n' {
            continue;
        }
        let source = format!("// a{character}b \n");
        let problem = at(TrailingSpace, 1, 7);
        let echo = format!("// a{}b ", shown_oracle(character));
        // The caret goes after `// a`, the character as it's shown, and `b`.
        let caret = " ".repeat(5 + shown_oracle(character).len());
        let expected =
            format!("x.stella:1:7: a line can't end in a space.\n    {echo}\n    {caret}^\n");
        assert_eq!(
            message(b"x.stella", source.as_bytes(), problem.place, problem.kind),
            expected,
            "for {character:?}"
        );
    }
}

#[test]
fn whole_file_messages() {
    // A problem with the file as a whole is one line, whatever the source holds.
    for (path, source, problem, expected) in [
        (
            &b"Hello.stella"[..],
            &b""[..],
            whole_file(NameNotKebab),
            "Hello.stella: a source file's name, before `.stella`, must be lowercase letters and \
             digits joined by single hyphens, like `hello-world`.\n",
        ),
        (
            b"big.stella",
            b"// x\n",
            whole_file(FileTooBig),
            "big.stella: this file is over 16,384 bytes, the most a source file can hold.\n",
        ),
        // A path is shown safely too.
        (
            b"a\xFF\n\xE2\x80\xAEb.stella",
            b"",
            whole_file(NameNotKebab),
            "a\\xFF\\u{A}\\u{202E}b.stella: a source file's name, before `.stella`, must be \
             lowercase letters and digits joined by single hyphens, like `hello-world`.\n",
        ),
    ] {
        assert_eq!(message(path, source, problem.place, problem.kind), expected, "for {problem:?}");
    }
}

#[test]
fn messages() {
    // A problem at a position shows its line, and a caret under its column.
    for (source, problem, expected) in [
        (
            &b"// a\n// b\nfunction main() returns Void { // hi\n}\n"[..],
            at(CommentSharesLine, 3, 32),
            "x.stella:3:32: a comment can't share a line with code.\n    \
             function main() returns Void { // hi\n                                   ^\n",
        ),
        (
            b"function main() returns Void {\n\t// x\n}\n",
            at(Tab, 2, 1),
            "x.stella:2:1: a tab can't appear in a source file; indent with four spaces.\n    \
             \\t// x\n    ^\n",
        ),
        // The caret counts an escape at its shown width.
        (
            "// é\t\n".as_bytes(),
            at(Tab, 1, 5),
            "x.stella:1:5: a tab can't appear in a source file; indent with four spaces.\n    \
             // \\u{E9}\\t\n             ^\n",
        ),
        (
            b"function main() returns Void {\r\n}\n",
            at(CarriageReturn, 1, 31),
            "x.stella:1:31: `\\r` can't appear in a source file; lines end with `\\n` alone.\n    \
             function main() returns Void {\\r\n                                  ^\n",
        ),
        (
            b"// a\xFFb\n",
            at(NotUtf8, 1, 5),
            "x.stella:1:5: this isn't valid UTF-8.\n    // a\\xFFb\n        ^\n",
        ),
        (
            b"// \xE2\x82\n",
            at(NotUtf8, 1, 4),
            "x.stella:1:4: this isn't valid UTF-8.\n    // \\xE2\\x82\n       ^\n",
        ),
        // A caret can sit just past the end of the line, and a line can be empty.
        (
            b"}",
            at(NoFinalNewline, 1, 2),
            "x.stella:1:2: the file must end with a newline.\n    }\n     ^\n",
        ),
        (
            b"}\n\n",
            at(BlankLineAtEnd, 2, 1),
            "x.stella:2:1: a file can't end with a blank line.\n    \n    ^\n",
        ),
    ] {
        assert_eq!(
            message(b"x.stella", source, problem.place, problem.kind),
            expected,
            "for {problem:?}"
        );
    }
}
