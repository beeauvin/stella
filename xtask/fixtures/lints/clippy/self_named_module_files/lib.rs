// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A module with files of its own, kept beside a file of its name.

mod outer;

/// Returns two.
#[must_use]
pub const fn two() -> u32 {
    outer::inner::two()
}
