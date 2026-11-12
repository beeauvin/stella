// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for positions. A position holds the line and column it was made with, at every
//! corner of the largest source, and one that no source has is a bug, caught when it's made.

use stella_source::{Position, SOURCE_BYTES_MAX};

#[test]
fn a_position_holds_its_line_and_column() {
    // The corners: the first character, the last line a source can have, and just past the end
    // of the longest line.
    for (line, column) in [
        (1, 1),
        (3, 32),
        (SOURCE_BYTES_MAX, 1),
        (1, SOURCE_BYTES_MAX + 1),
        (SOURCE_BYTES_MAX, SOURCE_BYTES_MAX + 1),
    ] {
        let position = Position::new(line, column);
        assert_eq!((position.line(), position.column()), (line, column));
    }
    // Two positions are the same when both parts are.
    assert_eq!(Position::new(2, 5), Position::new(2, 5));
    assert_ne!(Position::new(2, 5), Position::new(5, 2));
}

#[test]
#[should_panic(expected = "lines count from 1")]
fn line_zero_is_a_bug() {
    _ = Position::new(0, 1);
}

#[test]
#[should_panic(expected = "columns count from 1")]
fn column_zero_is_a_bug() {
    _ = Position::new(1, 0);
}

#[test]
#[should_panic(expected = "every line but the last ends with a byte of its own")]
fn a_line_past_the_largest_source_is_a_bug() {
    _ = Position::new(SOURCE_BYTES_MAX + 1, 1);
}

#[test]
#[should_panic(expected = "a column is in a line")]
fn a_column_past_the_longest_line_is_a_bug() {
    _ = Position::new(1, SOURCE_BYTES_MAX + 2);
}
