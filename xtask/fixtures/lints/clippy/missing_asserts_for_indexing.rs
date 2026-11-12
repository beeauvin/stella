// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A slice indexed three times with no assertion on its length first.

/// Returns the sum of the first three of `items`, wrapping.
#[must_use]
pub const fn total(items: &[u32]) -> u32 {
    items[0].wrapping_add(items[1]).wrapping_add(items[2])
}
