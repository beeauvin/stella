// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A type written out where the value already says it.

/// Returns an empty name.
#[must_use]
pub const fn name() -> String {
    let name: String = String::new();
    name
}
