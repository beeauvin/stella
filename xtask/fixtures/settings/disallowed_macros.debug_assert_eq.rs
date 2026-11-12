// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An equality assertion that a release build removes.

/// Returns `count`, which is one.
#[must_use]
pub fn checked(count: u32) -> u32 {
    debug_assert_eq!(count, 1, "the count is one");
    count
}
