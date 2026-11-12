// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A library that keeps every rule.

#![forbid(unsafe_code)]

/// Returns twice `count`, or `None` when that would not fit.
#[must_use]
pub const fn doubled(count: u32) -> Option<u32> {
    count.checked_mul(2)
}
