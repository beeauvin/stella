// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An `unsafe` block with no comment to say why it is sound.

/// Returns the byte `byte` refers to.
#[must_use]
pub const fn read(byte: &u8) -> u8 {
    let pointer: *const u8 = byte;
    unsafe { *pointer }
}
