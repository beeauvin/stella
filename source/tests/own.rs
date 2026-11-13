// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for telling a value that's a slice's own from one that only equals it. Each value
//! here is equal to every other, so only where a value is can tell them apart.

#![forbid(unsafe_code)]

use stella_source::is_own;

#[test]
fn only_the_very_elements_of_a_slice_are_its_own() {
    // Two vectors of equal numbers, and a copy of one of them. Each element of a slice is its
    // own, at every index, and of any part of the slice that holds it. An equal element of the
    // other vector isn't, and neither is the copy, though it equals every one of them.
    let (first, second) = (vec![7_u32; 5], vec![7_u32; 5]);
    for (index, element) in first.iter().enumerate() {
        assert!(is_own(&first, element), "element {index} of the first");
        assert!(!is_own(&second, element), "element {index} of the first, asked of the second");
        assert_eq!(is_own(&first[..3], element), index < 3, "element {index}, of the first three");
        assert_eq!(is_own(&first[3..], element), index >= 3, "element {index}, of the last two");
    }
    let copy = first[0];
    assert_eq!(copy, first[0], "a copy equals what it's a copy of");
    assert!(!is_own(&first, &copy), "a copy is somewhere else");
    let nothing: [u32; 0] = [];
    assert!(!is_own(&nothing, &copy), "a slice of nothing holds nothing");
}

#[test]
fn a_value_across_two_elements_is_neither() {
    // Three pairs, read as six numbers in a row. The two numbers in the middle of the first
    // two pairs make a pair too, and it lies inside the slice. But it starts half-way through
    // one element and ends half-way through the next, so it's none of them.
    let pairs = [[7_u16; 2]; 3];
    let numbers = pairs.as_flattened();
    assert_eq!(numbers.len(), 6, "three pairs are six numbers");
    let across: &[u16; 2] = numbers[1..3].try_into().expect("two numbers make a pair");
    assert_eq!(*across, pairs[0], "it equals each of the pairs");
    assert!(!is_own(&pairs, across), "a value across two elements is neither of them");
    let whole: &[u16; 2] = numbers[2..4].try_into().expect("two numbers make a pair");
    assert!(is_own(&pairs, whole), "the second pair, reached through the numbers, is itself");
}
