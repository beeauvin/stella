// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An import of a name the prelude already brings in.

use std::option::Option;

/// Returns `count`, or nothing when it is none.
#[must_use]
pub const fn some(count: u32) -> Option<u32> {
    if count > 0 { Some(count) } else { None }
}
