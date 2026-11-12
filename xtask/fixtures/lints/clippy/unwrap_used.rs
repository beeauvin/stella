// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An `unwrap`, which says nothing of why the value is there.

/// Returns the count.
///
/// # Panics
///
/// Panics if there is no count.
#[must_use]
pub const fn count(count: Option<u32>) -> u32 {
    count.unwrap()
}
