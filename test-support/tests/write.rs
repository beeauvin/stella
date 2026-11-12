// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the ways tests write and show source. The oracles lean on these, so each is
//! pinned against text written out by hand.

#![forbid(unsafe_code)]

use stella_test_support::{excerpt, indent, joined, nested_blocks, slice, source_of};

#[test]
fn indent_is_four_spaces_a_level() {
    // No level is no spaces, and each level after adds four, as the layout page says.
    assert_eq!(indent(0), "");
    assert_eq!(indent(1), "    ");
    assert_eq!(indent(3), "            ");
}

#[test]
fn source_of_ends_every_line() {
    // Each line gets its `\n`, a blank line included, and no lines make an empty source.
    let lines = [String::from("a {"), String::new(), String::from("}")];
    assert_eq!(source_of(&lines), "a {\n\n}\n");
    assert_eq!(source_of(&[]), "");
}

#[test]
fn joined_writes_a_line_the_one_way() {
    // One space between two tokens, except none after `(`, and none before `(` or `)`.
    for (texts, line) in [
        (
            &["function", "main", "(", ")", "returns", "Void", "{"][..],
            "function main() returns Void {",
        ),
        (&["a", "(", "b", ")", "(", "c", ")", "{"], "a(b)(c) {"),
        (&["(", "{"], "({"),
        (&[")", "x"], ") x"),
        (&["}"], "}"),
        (&[], ""),
    ] {
        assert_eq!(joined(texts), line, "for {texts:?}");
    }
}

#[test]
fn nested_blocks_open_and_close_in_order() {
    // Each block is opened by `({`, a level deeper than the one it's in, and closed by a `}` at
    // its own depth, innermost first. The lines for depths 1 and 3 are written out by hand.
    assert_eq!(nested_blocks(0), Vec::<String>::new());
    assert_eq!(nested_blocks(1), ["({", "}"]);
    assert_eq!(nested_blocks(3), ["({", "    ({", "        ({", "        }", "    }", "}"]);
}

#[test]
fn an_excerpt_cuts_a_long_source_short() {
    // A source of up to 120 bytes is shown whole, as a string literal. A longer one is its first
    // 60 characters and its length.
    assert_eq!(excerpt("a {\n}\n"), "\"a {\\n}\\n\"");
    let longest_whole = "x".repeat(120);
    assert_eq!(excerpt(&longest_whole), format!("{longest_whole:?}"));
    let long = "x".repeat(121);
    assert_eq!(excerpt(&long), format!("{:?}, and on to 121 bytes", "x".repeat(60)));
}

#[test]
fn slice_returns_the_part_at_a_range() {
    // Any range between characters gives that part, an empty one included.
    assert_eq!(slice("a é b", 0..1), "a");
    assert_eq!(slice("a é b", 2..4), "é");
    assert_eq!(slice("a é b", 5..5), "");
}

#[test]
#[should_panic(expected = "starts and ends between characters")]
fn slicing_inside_a_character_is_a_bug() {
    // `é` is two bytes, so byte 3 is in the middle of it.
    let _: &str = slice("a é b", 2..3);
}
