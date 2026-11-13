// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A file of more than 64 KiB read into the program.

/// How long the big file is.
pub const BIG_BYTES: usize = include_bytes!("big.txt").len();
