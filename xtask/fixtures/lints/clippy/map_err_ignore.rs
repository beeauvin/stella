// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An error replaced by another, with the first thrown away.

/// Returns the number `text` spells.
///
/// # Errors
///
/// Returns a reason when `text` is not a number.
pub fn number(text: &str) -> Result<u32, String> {
    text.parse().map_err(|_| String::from("not a number"))
}
