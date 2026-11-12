// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `&str` turned into a `String` with `to_string`.

/// Returns `text` as a string of its own.
#[must_use]
pub fn owned(text: &str) -> String {
    text.to_string()
}
