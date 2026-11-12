// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A variable read and written in one expression, in an order nobody can see.

/// Returns three, or four if the write comes before the read.
#[must_use]
pub const fn mixed() -> u32 {
    let mut count = 1_u32;
    count + ({
        count = 2;
        count
    })
}
