// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A negative literal with a method call, which reads two ways.

/// Returns minus one squared, or is it minus the square of one.
#[must_use]
pub const fn squared() -> i32 {
    -1_i32.pow(2)
}
