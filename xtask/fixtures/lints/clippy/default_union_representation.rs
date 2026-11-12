// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A union with no `repr`, so its layout is not fixed.

use std::fmt;

/// Four bytes, as one number or as four.
#[derive(Clone, Copy)]
pub union Bits {
    /// The four bytes as one number.
    pub whole: u32,
    /// The four bytes one by one.
    pub parts: [u8; 4],
}

impl fmt::Debug for Bits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Bits")
    }
}
