// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An import of `{self}` alone.

use std::fmt::{self};

/// A marker that holds nothing.
pub struct Marker;

impl fmt::Debug for Marker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Marker")
    }
}
