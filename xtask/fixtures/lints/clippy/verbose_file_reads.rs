// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A file opened and then read whole, where one call does both.

use std::fs::File;
use std::io::{self, Read as _};
use std::path::Path;

/// Returns every byte of the file at `path`.
///
/// # Errors
///
/// Returns the error from opening or reading the file.
pub fn bytes(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}
