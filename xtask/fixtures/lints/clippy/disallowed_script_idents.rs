// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A name written in a script other than Latin.

/// Returns the number after seven.
#[must_use]
pub const fn number() -> u32 {
    let число = 7_u32;
    число.wrapping_add(1)
}
