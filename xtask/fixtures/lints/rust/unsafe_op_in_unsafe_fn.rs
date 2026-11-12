// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An unsafe operation in an `unsafe fn`, outside any `unsafe` block.

/// Returns the byte `pointer` points at.
///
/// # Safety
///
/// `pointer` must point at a byte that can be read.
#[must_use]
pub const unsafe fn pointed_at(pointer: *const u8) -> u8 {
    *pointer
}
