// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `pub(crate)` item in a private module, which `redundant_pub_crate` would refuse.

mod inner {
    /// Returns two.
    pub(crate) const fn two() -> u32 {
        2
    }
}

/// Returns two.
#[must_use]
pub const fn two() -> u32 {
    inner::two()
}
