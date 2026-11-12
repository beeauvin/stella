// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A function cast to a number.

/// Does nothing.
pub const fn nothing() {}

/// Returns where `nothing` is, as a number.
#[must_use]
pub fn address() -> usize {
    nothing as usize
}
