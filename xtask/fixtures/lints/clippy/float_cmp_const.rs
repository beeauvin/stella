// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A float compared for equality with a constant.

/// A tenth.
pub const TENTH: f64 = 0.1;

/// Says whether `value` is a tenth.
#[must_use]
pub fn is_tenth(value: f64) -> bool {
    value == TENTH
}
