// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An `unimplemented!` left in.

/// Returns a count, one day.
#[must_use]
pub fn count() -> u32 {
    unimplemented!("nothing counts yet")
}
