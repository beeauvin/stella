// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A cast written with `as`.

/// Returns `count` as a `u64`.
#[must_use]
pub const fn widened(count: usize) -> u64 {
    count as u64
}
