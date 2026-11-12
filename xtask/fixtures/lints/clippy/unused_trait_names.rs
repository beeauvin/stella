// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A trait imported by name, when only its methods are used.

use std::fmt::Write;

/// Returns `count` as text.
#[must_use]
pub fn counted(count: u32) -> String {
    let mut text = String::new();
    // Writing to a string cannot fail.
    _ = write!(text, "{count}");
    text
}
