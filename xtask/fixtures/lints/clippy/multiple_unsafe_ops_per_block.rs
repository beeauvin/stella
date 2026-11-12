// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Two unsafe operations in one `unsafe` block.

/// Returns the sum of the bytes `left` and `right` refer to, wrapping.
#[must_use]
pub const fn sum(left: &u8, right: &u8) -> u8 {
    let (left, right): (*const u8, *const u8) = (left, right);
    // SAFETY: both pointers were made from references just above.
    unsafe { (*left).wrapping_add(*right) }
}
