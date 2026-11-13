// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An assertion that does not say why it holds.

/// Returns `count`, which is more than none.
///
/// # Panics
///
/// Panics if `count` is none.
#[must_use]
pub const fn checked(count: u32) -> u32 {
    assert!(count > 0);
    count
}
