// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A whole-range slice used where a dereference says the same.

/// A bag of numbers.
#[derive(Debug)]
pub struct Bag {
    items: Vec<u32>,
}

impl Bag {
    /// Returns every number in the bag.
    #[must_use]
    pub fn all(&self) -> &[u32] {
        &self.items[..]
    }
}
