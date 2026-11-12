// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A position is where something is in a source: its line and its column.

use crate::count::SOURCE_BYTES_MAX;

/// A position is a line and a column in a source, both counted from 1.
///
/// A column counts code points, not bytes. A position is a character's, or it's just past the
/// end of a line, where something is missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    line: u32,
    column: u32,
}

impl Position {
    /// Returns the position at `line` and `column`.
    ///
    /// # Panics
    ///
    /// Panics if no source has that position: if either is 0, if `line` is more than
    /// `SOURCE_BYTES_MAX`, or if `column` is more than one past it.
    #[must_use]
    pub const fn new(line: u32, column: u32) -> Self {
        assert!(line >= 1, "lines count from 1");
        assert!(line <= SOURCE_BYTES_MAX, "every line but the last ends with a byte of its own");
        assert!(column >= 1, "columns count from 1");
        assert!(
            column <= SOURCE_BYTES_MAX + 1,
            "a column is in a line, which is no longer than its source, or just past its end"
        );
        Self { line, column }
    }

    /// Returns the line, counted from 1.
    #[must_use]
    pub const fn line(self) -> u32 {
        self.line
    }

    /// Returns the column, counted in code points from 1.
    #[must_use]
    pub const fn column(self) -> u32 {
        self.column
    }
}
