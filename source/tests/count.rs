// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for counts. A count within a source goes to a `u32` and back unchanged, up to the
//! size of the largest source, and one more is a bug, caught where it's converted.

#![forbid(unsafe_code)]

use stella_source::{SOURCE_BYTES_MAX, u32_from_usize, usize_from_u32};

#[test]
fn counts_within_a_source_convert_both_ways() {
    // Every count a source can hold, from none to its whole size, comes back as itself.
    for count in 0..=SOURCE_BYTES_MAX {
        assert_eq!(u32_from_usize(usize_from_u32(count)), count);
    }
    // A `usize` holds every `u32`, not only the counts within a source.
    assert_eq!(u32::try_from(usize_from_u32(u32::MAX)), Ok(u32::MAX));
}

#[test]
#[should_panic(expected = "a count within a source is at most its size, and 16385 is more")]
fn a_count_past_the_largest_source_is_a_bug() {
    // One more than the largest source holds can't count anything within one.
    _ = u32_from_usize(usize_from_u32(SOURCE_BYTES_MAX) + 1);
}
