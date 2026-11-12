// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for examples. They come back in the order they were added, each with the line that
//! added it, and a test can't pass by adding examples and never checking them.

use stella_test_support::Examples;

#[test]
fn examples_come_back_in_order_with_where_they_were_added() {
    let mut examples = Examples::new();
    examples.add("a {\n}\n", 1_u32);
    let first_line = line!() - 1;
    examples.add("// b\n", 2);
    let taken = examples.take();
    let found: Vec<(&str, u32)> =
        taken.iter().map(|example| (example.source.as_str(), example.expected)).collect();
    assert_eq!(found, [("a {\n}\n", 1), ("// b\n", 2)]);
    // Each example points at the line of the test that added it, not at `add`.
    assert!(taken[0].at.file().ends_with("examples.rs"));
    assert_eq!((taken[0].at.line(), taken[1].at.line()), (first_line, first_line + 2));
}

#[test]
#[should_panic(expected = "a test tries at least one example")]
fn taking_no_examples_is_a_bug() {
    _ = Examples::<()>::new().take();
}

#[test]
#[should_panic(expected = "examples were added but never checked")]
fn dropping_examples_unchecked_fails_the_test() {
    let mut examples = Examples::new();
    examples.add("// a\n", ());
}

#[test]
fn dropping_no_examples_is_fine() {
    // Nothing was added, so there's nothing left unchecked.
    drop(Examples::<()>::new());
}
