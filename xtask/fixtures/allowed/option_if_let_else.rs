// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An `if let` with an `else`, which `option_if_let_else` would turn into `map_or`.

/// Returns the number after `count`, or zero when there is none.
#[must_use]
pub const fn next(count: Option<u32>) -> u32 {
    if let Some(count) = count { count.saturating_add(1) } else { 0 }
}
