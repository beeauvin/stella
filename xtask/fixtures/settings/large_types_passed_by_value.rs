// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A value of 200 bytes passed by value.

/// Returns the first byte of `block`.
#[must_use]
pub const fn first(block: [u8; 200]) -> u8 {
    block[0]
}
