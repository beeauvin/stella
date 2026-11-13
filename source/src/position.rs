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
    /// Starts a position on `line`, which `OnLine::at_column` finishes. The two numbers are given
    /// one at a time, each by name, so they can't be given the wrong way round.
    ///
    /// # Panics
    ///
    /// Panics if no source has that line: if it's 0, or more than `SOURCE_BYTES_MAX`.
    pub const fn on_line(line: u32) -> OnLine {
        assert!(line >= 1, "lines count from 1");
        assert!(line <= SOURCE_BYTES_MAX, "every line but the last ends with a byte of its own");
        OnLine { line }
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

/// A line of a source that's waiting for its column, on the way to being a position.
///
/// Only `Position::on_line` makes one, so its line is one a source can have.
#[derive(Clone, Copy, Debug)]
#[must_use = "a line is half a position until it's given its column"]
pub struct OnLine {
    line: u32,
}

impl OnLine {
    /// Returns the position at `column` on this line.
    ///
    /// # Panics
    ///
    /// Panics if no line has that column: if it's 0, or more than one past `SOURCE_BYTES_MAX`.
    #[must_use]
    pub const fn at_column(self, column: u32) -> Position {
        assert!(self.line >= 1, "`Position::on_line` checked the line");
        assert!(column >= 1, "columns count from 1");
        assert!(
            column <= SOURCE_BYTES_MAX + 1,
            "a column is in a line, which is no longer than its source, or just past its end"
        );
        Position { line: self.line, column }
    }
}
