// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A number whose type is left to the default.

/// Returns a count as text.
#[must_use]
pub fn counted() -> String {
    let count = 7;
    format!("{count}")
}
