// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the sentences problems print. Each sentence is pinned exactly, because people
//! and tools both read them.

use stella_linter_text::Kind::*;

#[test]
fn sentences() {
    for (kind, sentence) in [
        (PathAbsolute, "the build path can't be established from an absolute path."),
        (PathParent, "the build path can't be established from a path that uses `..`."),
        (
            PathEmptyPart,
            "a path can't have an empty part, like the one `//` or a trailing `/` makes.",
        ),
        (PathDotPart, "`.` can't be a part of a path, except as one leading `./`."),
        (
            DirectoryNotKebab,
            "a directory's name must be lowercase letters and digits joined by single hyphens, \
             like `hello-world`.",
        ),
        (NameNotStella, "a source file's name must end in `.stella`."),
        (NameEmpty, "a source file needs a name before `.stella`."),
        (
            NameNotKebab,
            "a source file's name, before `.stella`, must be lowercase letters and digits joined \
             by single hyphens, like `hello-world`.",
        ),
        (FileEmpty, "this file is empty."),
        (FileTooBig, "this file is over 16,384 bytes, the most a source file can hold."),
        (NotUtf8, "this isn't valid UTF-8."),
        (ByteOrderMark, "a byte-order mark (U+FEFF) can't appear in a source file."),
        (CarriageReturn, "`\\r` can't appear in a source file; lines end with `\\n` alone."),
        (Tab, "a tab can't appear in a source file; indent with four spaces."),
        (NoFinalNewline, "the file must end with a newline."),
        (BlankLineAtEnd, "a file can't end with a blank line."),
        (TrailingSpace, "a line can't end in a space."),
        (IndentationNotFour, "indentation must be a multiple of four spaces."),
        (CommentSharesLine, "a comment can't share a line with code."),
        (CommentNeedsSpace, "`//` must be followed by a space or the end of the line."),
        // A code point prints as `U+` and at least four hex digits, and it's the one found.
        (ControlCharacter('\u{0}'), "the control character U+0000 can't appear in a source file."),
        (ControlCharacter('\u{85}'), "the control character U+0085 can't appear in a source file."),
        (
            BidiControl('\u{061C}'),
            "the bidirectional control character U+061C can't appear in a source file.",
        ),
        (
            BidiControl('\u{202E}'),
            "the bidirectional control character U+202E can't appear in a source file.",
        ),
        (
            LineSeparator('\u{2028}'),
            "U+2028 can't appear in a source file; lines end with `\\n` alone.",
        ),
        (
            LineSeparator('\u{2029}'),
            "U+2029 can't appear in a source file; lines end with `\\n` alone.",
        ),
        (NotAscii('é'), "outside comments, only ASCII can appear, and U+00E9 isn't ASCII."),
        (
            NotAscii('\u{1F3F3}'),
            "outside comments, only ASCII can appear, and U+1F3F3 isn't ASCII.",
        ),
    ] {
        assert_eq!(kind.to_string(), sentence, "for {kind:?}");
    }
}
