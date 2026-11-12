// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An `#[expect]` of a lint that does not fire.

/// Returns `count`, unchanged.
#[expect(clippy::as_conversions, reason = "nothing here is cast, on purpose")]
#[must_use]
pub const fn same(count: u32) -> u32 {
    count
}
