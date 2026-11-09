// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The text lint checks every rule about a source file's bytes, before the lexer sees them.
//!
//! The steps run in a fixed order, each leaving the next less to worry about. The first step that
//! fails is the one reported, and within a step, the earliest problem in the file:
//!
//! 1. The size: at least one byte, and at most `SOURCE_BYTES_MAX`.
//! 2. The characters, in one pass: valid UTF-8, with no U+FEFF, `\r`, tab, other control
//!    character, `Bidi_Control` character, U+2028 or U+2029.
//! 3. The lines: the file ends with exactly one `\n` and no empty line before it, no line ends
//!    in a space, and indentation, the spaces a line starts with, is a multiple of four. A line
//!    of only spaces is a trailing space at its first column, not a blank line.
//! 4. The comments: a line that starts with `//` after its indentation is a comment, and its
//!    `//` must then be followed by a space or the end of the line. Every other line is code,
//!    and code never holds `//`.
//! 5. ASCII: every character on a line of code is ASCII.

use crate::problem::Problem;

/// This is the largest source file, in bytes. Anything larger is refused before its characters
/// are read.
pub const SOURCE_BYTES_MAX: u32 = 16_384;

/// A text is source that passed every step of the text lint. Only `lint` makes one, so holding a
/// `Text` proves the source passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Text<'source> {
    source: &'source str,
}

impl<'source> Text<'source> {
    /// Returns the source, unchanged.
    pub fn as_str(&self) -> &'source str {
        self.source
    }
}

/// Checks a source file's bytes, step by step, and stops at the first step that fails.
pub fn lint(bytes: &[u8]) -> Result<Text<'_>, Problem> {
    todo!("lint {} bytes", bytes.len())
}
