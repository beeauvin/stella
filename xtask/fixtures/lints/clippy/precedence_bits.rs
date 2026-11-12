// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A mask and a shift with no parentheses to say which is first.

/// Returns the second-lowest four bits of `flags`.
#[must_use]
pub const fn nibble(flags: u32) -> u32 {
    0x0F & flags >> 4
}
