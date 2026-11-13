// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for examples. They come back in the order they were added, each with the line that
//! added it, and a test can't pass by adding examples and never checking them.

#![forbid(unsafe_code)]

use stella_test_support::{EXAMPLES_MAX, Example, Examples};

#[test]
fn examples_come_back_in_order_with_where_they_were_added() {
    // Two examples are added on lines two apart, and taken back. They come back in the order
    // they went in, each with what was expected of it and the line that added it.
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
    let _: Vec<Example<()>> = Examples::<()>::new().take();
}

#[test]
#[should_panic(expected = "examples were added but never checked")]
fn dropping_examples_unchecked_fails_the_test() {
    // The example is added and never taken, so the panic comes from the drop at the end of this
    // function.
    let mut examples = Examples::new();
    examples.add("// a\n", ());
}

#[test]
#[should_panic(expected = "a test tries at most")]
fn adding_past_the_most_examples_is_a_bug() {
    // One more than the most a test can hold. The examples are forgotten once it's caught, so
    // the only panic is the one from `add`.
    let mut examples = Examples::new();
    for _ in 0..=EXAMPLES_MAX {
        examples.add("", ());
    }
}

#[test]
fn dropping_no_examples_is_fine() {
    // Nothing was added, so there's nothing left unchecked.
    drop(Examples::<()>::new());
}
