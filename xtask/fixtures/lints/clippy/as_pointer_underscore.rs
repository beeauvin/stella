// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A cast to a pointer whose target is left for the compiler to work out.

/// Returns `pointer` as a pointer to bytes.
#[must_use]
pub const fn bytes(pointer: *const u32) -> *const u8 {
    pointer as *const _
}
