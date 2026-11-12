// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Ways to write Stella source the one way, and to show it when a test fails.
//!
//! These state the layout as the reference words it, on their own: four spaces to a level, and
//! one space between two tokens, except none after `(` and none before `(` or `)`.

use std::ops::Range;

use stella_source::usize_from_u32;

/// This is how many spaces indent a line one level, as the layout page says.
const LEVEL_SPACES: usize = 4;

/// This is the longest source a failure shows whole, in bytes.
const EXCERPT_WHOLE_BYTES_MAX: usize = 120;

/// This is how many characters of a longer source a failure shows.
const EXCERPT_START_CHARACTERS: usize = 60;

/// Returns the spaces that indent a line `levels` levels.
#[must_use]
pub fn indent(levels: u32) -> String {
    " ".repeat(usize_from_u32(levels) * LEVEL_SPACES)
}

/// Returns `lines` as a source, each ending in `\n`.
#[must_use]
pub fn source_of(lines: &[String]) -> String {
    let mut source = String::new();
    for line in lines {
        source.push_str(line);
        source.push('\n');
    }
    source
}

/// Returns `texts`, the tokens of a line of code, written the one way: one space between two
/// tokens, except none after `(`, and none before `(` or `)`.
#[must_use]
pub fn joined(texts: &[&str]) -> String {
    let mut line = String::new();
    let mut previous: Option<&str> = None;
    for &text in texts {
        let spaced = match (previous, text) {
            (None | Some("("), _) | (Some(_), "(" | ")") => false,
            (Some(_), _) => true,
        };
        if spaced {
            line.push(' ');
        }
        line.push_str(text);
        previous = Some(text);
    }
    line
}

/// Returns the lines of blocks nested `depth` deep, each opened by `({` and closed by its own
/// `}`, all laid out the one way. A `{` can't start a line, so `({` is the shortest line that
/// opens a block.
#[must_use]
pub fn nested_blocks(depth: u32) -> Vec<String> {
    let mut lines: Vec<String> = (0..depth).map(|level| format!("{}({{", indent(level))).collect();
    lines.extend((0..depth).rev().map(|level| format!("{}}}", indent(level))));
    lines
}

/// Returns the part of `text` at `range`.
///
/// # Panics
///
/// Panics if `range` reaches past the end of `text`, or starts or ends inside a character.
#[must_use]
pub fn slice(text: &str, range: Range<usize>) -> &str {
    text.get(range).expect("the range is in the text, and starts and ends between characters")
}

/// Returns `source` as a failure shows it: whole when it's short, or else its start and its
/// length, since the largest sources are 16,384 bytes.
#[must_use]
pub fn excerpt(source: &str) -> String {
    if source.len() <= EXCERPT_WHOLE_BYTES_MAX {
        format!("{source:?}")
    } else {
        let start: String = source.chars().take(EXCERPT_START_CHARACTERS).collect();
        format!("{start:?}, and on to {} bytes", source.len())
    }
}
