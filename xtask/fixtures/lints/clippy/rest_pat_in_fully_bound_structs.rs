// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `..` in a pattern that already names every field.

/// A pair.
#[derive(Clone, Copy, Debug)]
pub struct Pair {
    /// The left of the pair.
    pub left: u32,
    /// The right of the pair.
    pub right: u32,
}

/// Returns the sum of the pair, wrapping.
#[must_use]
pub const fn sum(pair: Pair) -> u32 {
    let Pair { left, right, .. } = pair;
    left.wrapping_add(right)
}
