// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for the ways tests write and show source. The oracles lean on these, so each is
//! pinned against text written out by hand.

#![forbid(unsafe_code)]

use stella_test_support::{
    SEQUENCES_MAX, excerpt, indent, joined, nested_blocks, programs_of, sequences, slice, source_of,
};

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
fn sequences_count_through_every_choice() {
    // Every sequence of two numbers below three, in counting order, stated by hand. A length of
    // none is the one sequence that holds nothing, and no choices at all make no sequence.
    let pairs = sequences(3, 2);
    let expected: [[usize; 2]; 9] =
        [[0, 0], [0, 1], [0, 2], [1, 0], [1, 1], [1, 2], [2, 0], [2, 1], [2, 2]];
    assert_eq!(pairs, expected);
    assert_eq!(sequences(3, 0), [Vec::<usize>::new()]);
    assert!(sequences(0, 2).is_empty(), "nothing to choose from makes no sequence");
    assert_eq!(sequences(6, 7).len(), 279_936, "six to the seventh");
    assert_eq!(SEQUENCES_MAX, 1_000_000);
}

#[test]
#[should_panic(expected = "a test tries at most `SEQUENCES_MAX` sequences at once")]
fn too_many_sequences_is_a_bug() {
    // Ten to the seventh is more than the most, so it fails where it's asked for.
    _ = sequences(10, 7);
}

#[test]
fn programs_of_closes_each_function() {
    // Every program of one or two items, from a comment and a function, stated by hand: the
    // shorter programs first, and a `}` after each function's first line.
    let programs = programs_of(&["// c", "function f() returns Void {"], 2);
    let expected = [
        "// c\n",
        "function f() returns Void {\n}\n",
        "// c\n// c\n",
        "// c\nfunction f() returns Void {\n}\n",
        "function f() returns Void {\n}\n// c\n",
        "function f() returns Void {\n}\nfunction f() returns Void {\n}\n",
    ];
    assert_eq!(programs, expected);
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
