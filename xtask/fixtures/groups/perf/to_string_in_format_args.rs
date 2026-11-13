// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `to_string` on a value that is formatted anyway.

/// Returns `count` in a sentence.
#[must_use]
pub fn counted(count: u32) -> String {
    format!("there are {}", count.to_string())
}
