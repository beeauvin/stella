// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A function that could be `const`, and is not.

/// Returns twice `count`, wrapping.
#[must_use]
pub fn doubled(count: u32) -> u32 {
    count.wrapping_mul(2)
}
