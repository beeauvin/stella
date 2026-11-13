// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `get` followed by an `unwrap`, where indexing says the same.

/// Returns the second of `items`.
///
/// # Panics
///
/// Panics if there is no second item.
#[must_use]
pub fn second(items: &[u32]) -> u32 {
    *items.get(1).unwrap()
}
