// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A visibility written the long way, as `pub(in crate)`.

mod inner {
    /// Returns two.
    pub(in crate) const fn two() -> u32 {
        2
    }
}

/// Returns two.
#[must_use]
pub const fn two() -> u32 {
    inner::two()
}
