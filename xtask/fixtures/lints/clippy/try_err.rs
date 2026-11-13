// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An error returned by `Err(..)?`.

/// Returns `count`.
///
/// # Errors
///
/// Returns a reason when `count` is none.
pub fn checked(count: u32) -> Result<u32, String> {
    if count == 0 {
        Err(String::from("none"))?;
    }
    Ok(count)
}
