// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A function with four parameters, one more than three.

/// Returns the sum of all four, wrapping.
#[must_use]
pub const fn total(first: u32, second: u32, third: u32, fourth: u32) -> u32 {
    first.wrapping_add(second).wrapping_add(third).wrapping_add(fourth)
}
