// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A result turned into an option only to be dropped.

use std::fmt::Write as _;

/// Adds a word to `text`.
pub fn add(text: &mut String) {
    text.write_str("word").ok();
}
