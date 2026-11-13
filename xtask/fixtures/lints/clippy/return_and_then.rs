// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An `and_then` as a function's whole result, where `?` reads more plainly.

/// Returns one more than twice `count`, when all of that fits.
#[must_use]
pub fn next(count: Option<u32>) -> Option<u32> {
    count.and_then(|count| {
        let doubled = count.checked_mul(2)?;
        doubled.checked_add(1)
    })
}
