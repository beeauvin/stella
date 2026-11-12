// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A path made empty and then pushed onto.

use std::path::PathBuf;

/// Returns the path of the programs.
#[must_use]
pub fn programs() -> PathBuf {
    let mut path = PathBuf::new();
    path.push("programs");
    path
}
