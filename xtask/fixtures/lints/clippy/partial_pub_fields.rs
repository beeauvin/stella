// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A struct with one field public and one not.

/// A pair.
#[derive(Debug)]
pub struct Pair {
    /// The left of the pair.
    pub left: u32,
    right: u32,
}

impl Pair {
    /// Returns the right of the pair.
    #[must_use]
    pub const fn right(&self) -> u32 {
        self.right
    }
}
