// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A remainder of numbers that can be negative.

/// Returns what is left of `left` after `right`.
#[must_use]
pub const fn remainder(left: i32, right: i32) -> i32 {
    left % right
}
