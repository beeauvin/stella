// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Ways to write Stella source the one way, and to show it when a test fails.
//!
//! These state the layout as the reference words it, on their own: four spaces to a level, and
//! one space between two tokens, except none after `(` and none before `(` or `)`.

use std::ops::Range;

use stella_source::usize_from_u32;

/// This is the most sequences `sequences` makes at once, so a test that asks for too many fails
/// where it asked, and not when the memory runs out.
pub const SEQUENCES_MAX: usize = 1_000_000;

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

/// Returns every sequence of `length` numbers below `choices_count`, in counting order.
///
/// Each number is an index a test gives its own meaning. The last number changes fastest, and
/// there are `choices_count` to the power of `length` sequences.
///
/// # Panics
///
/// Panics if there would be more than `SEQUENCES_MAX` of them.
#[must_use]
pub fn sequences(choices_count: usize, length: u32) -> Vec<Vec<usize>> {
    let count = choices_count.checked_pow(length).filter(|&count| count <= SEQUENCES_MAX);
    let count = count.expect("a test tries at most `SEQUENCES_MAX` sequences at once");
    let mut all: Vec<Vec<usize>> = vec![Vec::new()];
    for _ in 0..length {
        all = all
            .into_iter()
            .flat_map(|sequence| {
                (0..choices_count).map(move |choice| {
                    let mut longer = sequence.clone();
                    longer.push(choice);
                    longer
                })
            })
            .collect();
    }
    assert_eq!(all.len(), count, "every sequence, once");
    all
}

/// Returns every program of one to `items_count_max` of `items`, as its source, shortest first.
///
/// Each item is a line outside every block: a comment, or a function's first line, which is
/// followed by the `}` that closes its body.
///
/// # Panics
///
/// Panics if the programs of one length would be more than `SEQUENCES_MAX`.
#[must_use]
pub fn programs_of(items: &[&str], items_count_max: u32) -> Vec<String> {
    let mut sources = Vec::new();
    for items_count in 1..=items_count_max {
        for sequence in sequences(items.len(), items_count) {
            let mut lines = Vec::new();
            for item in sequence {
                let line = items[item];
                lines.push(String::from(line));
                if line.starts_with("function ") {
                    lines.push(String::from("}"));
                }
            }
            sources.push(source_of(&lines));
        }
    }
    sources
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
