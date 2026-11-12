// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A string sliced by bytes, which can land inside a character.

/// Returns `text` without its first byte.
#[must_use]
pub fn rest(text: &str) -> &str {
    &text[1..]
}
