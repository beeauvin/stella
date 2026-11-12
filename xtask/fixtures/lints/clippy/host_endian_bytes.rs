// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A number turned into bytes in whatever order the machine has.

/// Returns the bytes of `count`.
#[must_use]
pub const fn bytes(count: u32) -> [u8; 4] {
    count.to_ne_bytes()
}
