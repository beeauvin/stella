// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A cast to a type left for the compiler to work out.

/// Returns `count`, widened.
#[must_use]
pub const fn widened(count: u8) -> u32 {
    count as _
}
