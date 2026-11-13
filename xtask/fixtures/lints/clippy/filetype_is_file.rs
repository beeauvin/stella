// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A check for a regular file, which a symlink to one fails.

use std::fs::FileType;

/// Says whether `kind` is a regular file's.
#[must_use]
pub fn is_plain(kind: FileType) -> bool {
    kind.is_file()
}
