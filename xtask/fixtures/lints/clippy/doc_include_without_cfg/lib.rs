// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A file made the crate's documentation whether or not documentation is being built.
#![doc = include_str!("readme.md")]

/// Returns `count`, unchanged.
#[must_use]
pub const fn same(count: u32) -> u32 {
    count
}
