// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A field that is public to the crate alone.

/// A count.
#[derive(Debug)]
pub struct Count {
    pub(crate) count: u32,
}

impl Count {
    /// Returns the count.
    #[must_use]
    pub const fn count(&self) -> u32 {
        self.count
    }
}
