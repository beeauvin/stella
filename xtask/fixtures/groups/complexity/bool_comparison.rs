// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `bool` compared to `true`.

/// Says whether `flag` is set.
#[must_use]
pub const fn is_set(flag: bool) -> bool {
    flag == true
}
