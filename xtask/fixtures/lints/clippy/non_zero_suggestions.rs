// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A number known not to be zero, widened into one that forgets it.

use std::num::NonZeroU32;

/// Returns `count`, widened.
#[must_use]
pub fn widened(count: NonZeroU32) -> u64 {
    u64::from(count.get())
}
