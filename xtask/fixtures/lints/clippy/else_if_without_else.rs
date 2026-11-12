// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An `else if` with no `else` after it.

/// Returns `count`, but never one or two.
#[must_use]
pub const fn skipped(count: u32) -> u32 {
    let mut skipped = count;
    if count == 1 {
        skipped = 3;
    } else if count == 2 {
        skipped = 4;
    }
    skipped
}
